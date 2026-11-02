//! Texto de DrawingML: el de las formas y tablas de PowerPoint, los cuadros
//! de texto y gráficos de Excel y los diagramas (SmartArt).
//!
//! Un párrafo es un `a:p`; un fragmento, el contenido de un `a:t`; un salto
//! de línea es un `a:br`, que va entre las carreras y en el texto cuenta como
//! un espacio. Los comentarios antiguos de PowerPoint guardan su texto en un
//! `p:text` sin párrafos: cada uno es un párrafo de un solo fragmento.

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{Office, append_text, attribute, xml_error};
use crate::Result;

/// Elementos cuyo texto es un fragmento.
pub(crate) const FRAGMENTS: [&str; 2] = ["a:t", "p:text"];
/// Saltos de línea.
pub(crate) const BREAKS: [&str; 1] = ["a:br"];
/// Tipos de marcador de posición que son el título de la diapositiva.
const TITLE_PLACEHOLDERS: [&str; 2] = ["title", "ctrTitle"];

/// Posición de un carácter en el original: fragmento de la parte y carácter
/// dentro del fragmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentPos {
    pub fragment: usize,
    pub offset: usize,
}

/// Celda de tabla. Las tablas se numeran por parte; la fila y la columna
/// empiezan en 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCell {
    pub table: usize,
    pub row: usize,
    pub column: usize,
}

/// Párrafo con texto. `map` tiene una posición por carácter de `text`;
/// `None` es un salto, que en el texto es un espacio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub text: String,
    pub map: Vec<Option<FragmentPos>>,
    /// Saltos: posición en `text` y número del salto en la parte.
    pub breaks: Vec<(usize, usize)>,
    pub cell: Option<TableCell>,
    /// Si está en el título de la diapositiva.
    pub title: bool,
}

/// Párrafos con texto y todos los fragmentos de una parte, en orden.
pub(crate) struct DrawingText {
    pub paragraphs: Vec<Paragraph>,
    pub fragments: Vec<String>,
    /// Textos que solo admiten sustitución literal: valores en caché de los
    /// gráficos y texto alternativo de imágenes y formas.
    pub literal_texts: Vec<String>,
    pub images: usize,
}

/// Elementos con valores en caché de un gráfico.
pub(crate) const CACHED_VALUES: [&str; 1] = ["c:v"];
/// Atributos con el texto alternativo de una forma o imagen.
pub(crate) const ALT_TEXTS: [(&str, &str); 10] = [
    ("p:cNvPr", "descr"),
    ("p:cNvPr", "title"),
    ("xdr:cNvPr", "descr"),
    ("xdr:cNvPr", "title"),
    ("pic:cNvPr", "descr"),
    ("pic:cNvPr", "title"),
    ("dsp:cNvPr", "descr"),
    ("dsp:cNvPr", "title"),
    ("wp:docPr", "descr"),
    ("wp:docPr", "title"),
];

struct OpenTable {
    index: usize,
    row: Option<usize>,
    column: Option<usize>,
}

pub(crate) fn extract(office: Office, part: &str, xml: &str) -> Result<DrawingText> {
    let mut reader = Reader::from_str(xml);
    let mut out = DrawingText {
        paragraphs: Vec::new(),
        fragments: Vec::new(),
        literal_texts: Vec::new(),
        images: 0,
    };
    let mut cached: Option<String> = None;
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut tables: Vec<OpenTable> = Vec::new();
    let mut table_count = 0;
    // Si cada forma abierta es un título.
    let mut shapes: Vec<bool> = Vec::new();
    let mut breaks = 0usize;
    let mut fallback_depth = 0usize;
    let mut reading: Option<String> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(office, part, &reader))?;
        let (element, empty) = match &event {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = element {
            for (element, key) in ALT_TEXTS {
                if e.name().as_ref() == element
                    && let Some(text) = attribute(e, key)
                    && !text.is_empty()
                {
                    out.literal_texts.push(text);
                }
            }
            match e.name().as_ref() {
                "c:v" if !empty => cached = Some(String::new()),
                "p:sp" | "dsp:sp" if !empty => shapes.push(false),
                "p:ph" => {
                    let kind = attribute(e, "type");
                    if let Some(title) = shapes.last_mut()
                        && kind
                            .as_deref()
                            .is_some_and(|k| TITLE_PLACEHOLDERS.contains(&k))
                    {
                        *title = true;
                    }
                }
                "a:p" if !empty => paragraphs.push(Paragraph {
                    text: String::new(),
                    map: Vec::new(),
                    breaks: Vec::new(),
                    cell: tables.last().and_then(|t| {
                        Some(TableCell {
                            table: t.index,
                            row: t.row?,
                            column: t.column?,
                        })
                    }),
                    title: shapes.last().copied().unwrap_or(false),
                }),
                "a:tbl" if !empty => {
                    tables.push(OpenTable {
                        index: table_count,
                        row: None,
                        column: None,
                    });
                    table_count += 1;
                }
                "a:tr" => {
                    if let Some(t) = tables.last_mut() {
                        t.row = Some(t.row.map_or(0, |r| r + 1));
                        t.column = None;
                    }
                }
                "a:tc" => {
                    if let Some(t) = tables.last_mut() {
                        t.column = Some(t.column.map_or(0, |c| c + 1));
                    }
                }
                "a:br" => {
                    if let Some(p) = paragraphs.last_mut() {
                        p.breaks.push((p.map.len(), breaks));
                        p.text.push(' ');
                        p.map.push(None);
                    }
                    breaks += 1;
                }
                "a:t" | "p:text" => {
                    if empty {
                        add_fragment(String::new(), &mut out, &mut paragraphs, e.name().as_ref());
                    } else {
                        reading = Some(String::new());
                    }
                }
                "a:blip" if fallback_depth == 0 => out.images += 1,
                "mc:Fallback" if !empty => fallback_depth += 1,
                _ => {}
            }
        }
        match event {
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some(text) = reading.as_mut().or(cached.as_mut()) {
                    append_text(text, &event).map_err(|_| xml_error(office, part, &reader))?;
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "c:v" => {
                    if let Some(text) = cached.take()
                        && !text.trim().is_empty()
                    {
                        out.literal_texts.push(text);
                    }
                }
                name @ ("a:t" | "p:text") => {
                    if let Some(text) = reading.take() {
                        add_fragment(text, &mut out, &mut paragraphs, name);
                    }
                }
                "a:p" => {
                    if let Some(p) = paragraphs.pop()
                        && !p.text.trim().is_empty()
                    {
                        out.paragraphs.push(p);
                    }
                }
                "a:tbl" => {
                    tables.pop();
                }
                "p:sp" | "dsp:sp" => {
                    shapes.pop();
                }
                "mc:Fallback" => fallback_depth = fallback_depth.saturating_sub(1),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Un `a:t` va dentro del párrafo abierto. Un `p:text` es un párrafo él
/// solo, ya terminado.
fn add_fragment(text: String, out: &mut DrawingText, open: &mut [Paragraph], element: &str) {
    let index = out.fragments.len();
    let map = (0..text.chars().count()).map(|offset| {
        Some(FragmentPos {
            fragment: index,
            offset,
        })
    });
    if element == "p:text" {
        if !text.trim().is_empty() {
            out.paragraphs.push(Paragraph {
                text: text.clone(),
                map: map.collect(),
                breaks: Vec::new(),
                cell: None,
                title: false,
            });
        }
    } else if let Some(p) = open.last_mut() {
        p.text.push_str(&text);
        p.map.extend(map);
    }
    out.fragments.push(text);
}
