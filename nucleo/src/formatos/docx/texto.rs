//! MD y TXT desde el texto extraído y ya sustituido (PRD, «Salida MD y TXT
//! desde DOCX»). Entran el cuerpo y, al final, las notas al pie y al final;
//! los encabezados, los pies, los comentarios y las propiedades se dejan
//! fuera. Los cambios se ven como aceptados: el texto borrado no sale
//! (decisión de Jano del 2026-10-30).

use std::collections::BTreeMap;

use super::{DOCUMENT_PART, DocxDocument, ParagraphStyle, TableCell};
use crate::Result;
use crate::formatos::{Replacement, order_for_writing};

const NOTES_TITLE: &str = "Notas";
const NOTE_PARTS: [&str; 2] = ["word/footnotes.xml", "word/endnotes.xml"];

/// Párrafo tal como se ve, ya sustituido y sin espacios en los extremos.
struct Visible {
    text: String,
    style: ParagraphStyle,
    cell: Option<TableCell>,
}

impl DocxDocument {
    /// Markdown con títulos, listas y tablas básicas.
    pub fn markdown(&self, replacements: &[Replacement]) -> Result<String> {
        let (body, notes) = self.visible(replacements)?;
        let mut out = markdown_blocks(&body);
        if !notes.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("## {NOTES_TITLE}\n\n"));
            out.push_str(&markdown_blocks(&notes));
        }
        Ok(out)
    }

    /// Solo el texto, un párrafo por línea.
    pub fn plain_text(&self, replacements: &[Replacement]) -> Result<String> {
        let (body, notes) = self.visible(replacements)?;
        let mut out = lines(&body);
        if !notes.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("{NOTES_TITLE}\n"));
            out.push_str(&lines(&notes));
        }
        Ok(out)
    }

    /// Párrafos del cuerpo y de las notas con las sustituciones aplicadas y
    /// sin el texto borrado. Los que se quedan sin texto no salen.
    fn visible(&self, replacements: &[Replacement]) -> Result<(Vec<Visible>, Vec<Visible>)> {
        let lengths: Vec<usize> = self.segments.iter().map(|s| s.map.len()).collect();
        // Cada carácter con si se ve. Un tabulador o un salto se ve siempre.
        let mut chars: Vec<Vec<(char, bool)>> = self
            .segments
            .iter()
            .map(|s| {
                s.text
                    .chars()
                    .zip(&s.map)
                    .map(|(c, pos)| (c, pos.is_none_or(|p| !self.is_deleted(&s.part, p.fragment))))
                    .collect()
            })
            .collect();
        for r in order_for_writing(replacements, &lengths)? {
            let segment = &mut chars[r.segment];
            // Un marcador sobre texto borrado se va con él.
            let seen = segment[r.start..r.end].iter().any(|(_, seen)| *seen);
            segment.splice(r.start..r.end, r.text.chars().map(|c| (c, seen)));
        }

        let (mut body, mut notes) = (Vec::new(), Vec::new());
        for (segment, chars) in self.segments.iter().zip(chars) {
            let text: String = chars
                .into_iter()
                .filter(|(_, seen)| *seen)
                .map(|(c, _)| c)
                .collect();
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let paragraph = Visible {
                text: text.to_owned(),
                style: segment.style,
                cell: segment.cell,
            };
            if segment.part == DOCUMENT_PART {
                body.push(paragraph);
            } else if NOTE_PARTS.contains(&segment.part.as_str()) {
                notes.push(paragraph);
            }
        }
        Ok((body, notes))
    }

    fn is_deleted(&self, part: &str, fragment: usize) -> bool {
        self.parts
            .iter()
            .find(|p| p.name == part)
            .is_some_and(|p| p.deleted[fragment])
    }
}

fn lines(paragraphs: &[Visible]) -> String {
    paragraphs.iter().map(|p| format!("{}\n", p.text)).collect()
}

/// Bloques separados por una línea en blanco; los elementos seguidos de una
/// lista van en el mismo bloque.
fn markdown_blocks(paragraphs: &[Visible]) -> String {
    let mut blocks: Vec<String> = Vec::new();
    // Número del elemento en curso de cada nivel de la lista abierta.
    let mut counters: Vec<usize> = Vec::new();
    let mut in_list = false;
    let mut i = 0;
    while i < paragraphs.len() {
        let paragraph = &paragraphs[i];
        if let Some(cell) = paragraph.cell {
            let rows = paragraphs[i..]
                .iter()
                .take_while(|p| p.cell.is_some_and(|c| c.table == cell.table))
                .count();
            blocks.push(table(&paragraphs[i..i + rows]));
            (counters, in_list) = (Vec::new(), false);
            i += rows;
            continue;
        }
        match paragraph.style {
            ParagraphStyle::ListItem { ordered, level } => {
                let level = usize::from(level);
                counters.truncate(level + 1);
                counters.resize(level + 1, 0);
                counters[level] += 1;
                let sign = if ordered {
                    format!("{}.", counters[level])
                } else {
                    "-".to_owned()
                };
                let line = format!(
                    "{}{sign} {}",
                    "    ".repeat(level),
                    escape_start(&paragraph.text)
                );
                match blocks.last_mut() {
                    Some(block) if in_list => {
                        block.push('\n');
                        block.push_str(&line);
                    }
                    _ => blocks.push(line),
                }
                in_list = true;
            }
            ParagraphStyle::Heading(level) => {
                blocks.push(format!(
                    "{} {}",
                    "#".repeat(usize::from(level)),
                    paragraph.text
                ));
                (counters, in_list) = (Vec::new(), false);
            }
            ParagraphStyle::Normal | ParagraphStyle::Metadata => {
                blocks.push(escape_start(&paragraph.text));
                (counters, in_list) = (Vec::new(), false);
            }
        }
        i += 1;
    }
    if blocks.is_empty() {
        String::new()
    } else {
        blocks.join("\n\n") + "\n"
    }
}

/// Tabla con la primera fila como cabecera. Los párrafos de una celda se
/// juntan con un espacio.
fn table(paragraphs: &[Visible]) -> String {
    let mut grid: BTreeMap<usize, BTreeMap<usize, Vec<&str>>> = BTreeMap::new();
    for paragraph in paragraphs {
        let cell = paragraph.cell.expect("solo párrafos de tabla");
        grid.entry(cell.row)
            .or_default()
            .entry(cell.column)
            .or_default()
            .push(&paragraph.text);
    }
    let columns = grid
        .values()
        .filter_map(|row| row.keys().max())
        .max()
        .map_or(0, |c| c + 1);
    let line = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let mut lines = Vec::new();
    for (index, row) in grid.values().enumerate() {
        let cells = (0..columns)
            .map(|c| {
                row.get(&c)
                    .map(|texts| texts.join(" ").replace('|', "\\|"))
                    .unwrap_or_default()
            })
            .collect();
        lines.push(line(cells));
        if index == 0 {
            lines.push(line(vec!["---".to_owned(); columns]));
        }
    }
    lines.join("\n")
}

/// Escapa lo que al principio de un párrafo Markdown leería como título,
/// cita, lista o tabla.
fn escape_start(text: &str) -> String {
    if text.starts_with(['#', '>', '-', '+', '*', '=', '|', '`', '~']) {
        return format!("\\{text}");
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && text[digits..].starts_with(['.', ')']) {
        return format!("{}\\{}", &text[..digits], &text[digits..]);
    }
    text.to_owned()
}
