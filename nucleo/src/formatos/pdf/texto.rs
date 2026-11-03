//! Texto de las páginas: extracción con cajas, y salidas MD, TXT y DOCX
//! desde el texto ya anonimizado, sin maquetación.

use std::io::Cursor;

use pdfium_render::prelude::*;

use super::{PageText, PdfDocument, Rect, pdf_error};
use crate::formatos::ooxml::texto::escape_start;
use crate::formatos::{Replacement, order_for_writing};
use crate::{Error, Result};

/// Una línea sigue en el mismo párrafo si el hueco hasta la anterior es
/// menor que esta fracción de su alto (con cajas holgadas, unas 1,5 veces el
/// interlineado normal).
const PARAGRAPH_GAP: f32 = 0.5;

/// Texto de la página con su mapa de cajas. Los saltos que añade PDFium
/// (`\r\n`) quedan en un `\n`; el guion de fin de línea se quita y su caja
/// pasa a la letra anterior, para que se tape con ella.
pub(super) fn extract(page: &PdfPage) -> Result<PageText> {
    let text = page.text().map_err(pdf_error)?;
    let mut out = String::new();
    let mut boxes: Vec<Vec<Rect>> = Vec::new();
    let mut lines: Vec<Option<Rect>> = Vec::new();
    let mut line: Option<Rect> = None;
    for ch in text.chars().iter() {
        let generated = ch.is_generated().unwrap_or(false);
        let bounds = if generated {
            None
        } else {
            ch.loose_bounds()
                .ok()
                .map(|b| Rect::of(&b))
                .filter(|b| !b.is_empty())
        };
        let c = ch.unicode_char().unwrap_or('\u{FFFD}');
        let mut grow_line = |b: Rect| line = Some(line.map_or(b, |l| l.union(&b)));
        if ch.is_hyphen().unwrap_or(false) {
            if let (Some(b), Some(last)) = (bounds, boxes.last_mut()) {
                last.push(b);
                grow_line(b);
            }
            continue;
        }
        match c {
            '\r' => continue,
            '\n' => {
                out.push('\n');
                boxes.push(Vec::new());
                lines.push(line.take());
                continue;
            }
            c if c.is_control() && c != '\t' => continue,
            _ => {}
        }
        out.push(c);
        if let Some(b) = bounds {
            grow_line(b);
        }
        boxes.push(bounds.into_iter().collect());
    }
    lines.push(line);
    let paragraph_breaks = lines
        .windows(2)
        .map(|pair| match (pair[0], pair[1]) {
            (Some(above), Some(below)) => {
                let height = above.height().max(below.height());
                // Una línea que empieza más arriba es otra columna.
                below.top > above.top || above.bottom - below.top > PARAGRAPH_GAP * height
            }
            _ => true,
        })
        .collect();
    Ok(PageText {
        text: out,
        boxes,
        paragraph_breaks,
    })
}

/// Párrafos de cada página con las sustituciones hechas.
pub(super) fn paragraphs(
    document: &PdfDocument,
    replacements: &[Replacement],
) -> Result<Vec<Vec<String>>> {
    let lengths: Vec<usize> = document
        .pages
        .iter()
        .map(|p| p.text.chars().count())
        .collect();
    order_for_writing(replacements, &lengths)?;
    let mut pages = Vec::with_capacity(document.pages.len());
    for (index, page) in document.pages.iter().enumerate() {
        let mut own: Vec<&Replacement> =
            replacements.iter().filter(|r| r.segment == index).collect();
        own.sort_by_key(|r| r.start);
        let mut own = own.into_iter().peekable();
        let chars: Vec<char> = page.text.chars().collect();
        let mut paragraphs = Vec::new();
        let mut current = String::new();
        let mut flush = |current: &mut String| {
            let paragraph = current.split_whitespace().collect::<Vec<_>>().join(" ");
            if !paragraph.is_empty() {
                paragraphs.push(paragraph);
            }
            current.clear();
        };
        let mut newline = 0;
        let mut i = 0;
        while i < chars.len() {
            if let Some(r) = own.next_if(|r| r.start == i) {
                current.push_str(&r.text);
                newline += chars[r.start..r.end].iter().filter(|c| **c == '\n').count();
                i = r.end;
                continue;
            }
            if chars[i] == '\n' {
                if page.paragraph_breaks[newline] {
                    flush(&mut current);
                } else {
                    current.push(' ');
                }
                newline += 1;
            } else {
                current.push(chars[i]);
            }
            i += 1;
        }
        flush(&mut current);
        pages.push(paragraphs);
    }
    Ok(pages)
}

pub(super) fn markdown(document: &PdfDocument, replacements: &[Replacement]) -> Result<String> {
    let sections: Vec<String> = paragraphs(document, replacements)?
        .iter()
        .enumerate()
        .map(|(index, paragraphs)| {
            let mut blocks = vec![format!("## Página {}", index + 1)];
            blocks.extend(paragraphs.iter().map(|p| escape_start(p)));
            blocks.join("\n\n")
        })
        .collect();
    Ok(join(&sections))
}

pub(super) fn plain_text(document: &PdfDocument, replacements: &[Replacement]) -> Result<String> {
    let sections: Vec<String> = paragraphs(document, replacements)?
        .iter()
        .map(|paragraphs| paragraphs.join("\n"))
        .collect();
    Ok(join(&sections))
}

pub(super) fn docx(document: &PdfDocument, replacements: &[Replacement]) -> Result<Vec<u8>> {
    use docx_rs::{BreakType, Docx, Paragraph, Run};

    let mut docx = Docx::new();
    for (index, paragraphs) in paragraphs(document, replacements)?.iter().enumerate() {
        if index > 0 {
            docx =
                docx.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)));
        }
        for paragraph in paragraphs {
            docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(paragraph)));
        }
    }
    let mut bytes = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut bytes)
        .map_err(|e| Error::Pdf(format!("no se ha podido crear el DOCX: {e}")))?;
    Ok(bytes.into_inner())
}

fn join(sections: &[String]) -> String {
    if sections.is_empty() {
        String::new()
    } else {
        sections.join("\n\n") + "\n"
    }
}
