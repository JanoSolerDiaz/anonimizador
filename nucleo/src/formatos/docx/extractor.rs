//! Recorrido del XML de cada parte para sacar segmentos, fragmentos y lo
//! demás que necesitan el escritor y los avisos.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::estilos::Styles;
use super::{ExternalLink, FieldCode, FragmentPos, ParagraphStyle, Segment, TableCell, Warnings};
use crate::{Error, Result};

/// Elementos de `docProps/core.xml` que se analizan como segmentos.
pub(super) const CORE_TEXT_FIELDS: [&str; 4] =
    ["dc:title", "dc:subject", "cp:keywords", "dc:description"];
/// Tabuladores y saltos dentro de una carrera: un espacio en el texto.
pub(super) const BREAK_ELEMENTS: [&str; 3] = ["w:tab", "w:br", "w:cr"];
/// Metadatos de autor que se vacían.
pub(super) const CORE_AUTHOR_FIELDS: [&str; 2] = ["dc:creator", "cp:lastModifiedBy"];
pub(super) const APP_AUTHOR_FIELDS: [&str; 1] = ["Company"];

/// Lo que se va reuniendo al recorrer las partes.
#[derive(Debug, Default)]
pub(super) struct Collected {
    pub segments: Vec<Segment>,
    /// Fragmentos de cada parte, en orden, y si cada uno es texto borrado
    /// con control de cambios.
    pub parts: Vec<(String, Vec<String>, Vec<bool>)>,
    pub links: Vec<ExternalLink>,
    pub field_codes: Vec<FieldCode>,
    pub author_metadata: Vec<String>,
    pub revision_authors: Vec<String>,
    pub warnings: Warnings,
}

/// Párrafo abierto mientras se recorre.
struct OpenParagraph {
    slot: usize,
    text: String,
    map: Vec<Option<FragmentPos>>,
    breaks: Vec<(usize, usize)>,
    style_id: Option<String>,
    num_id: Option<String>,
    level: u8,
    cell: Option<TableCell>,
}

/// Tabla abierta: su número y la fila y la columna en curso.
struct OpenTable {
    index: usize,
    row: Option<usize>,
    column: Option<usize>,
}

/// Qué texto se está leyendo.
enum Reading {
    Nothing,
    Fragment(String),
    FieldCode(String),
}

/// Segmentos, fragmentos, códigos de campo, autores y avisos de una parte
/// con párrafos de Word.
pub(super) fn extract_part(
    part: &str,
    xml: &str,
    styles: &Styles,
    out: &mut Collected,
) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    let mut fragments: Vec<String> = Vec::new();
    let mut deleted: Vec<bool> = Vec::new();
    // Un hueco por párrafo, en orden de apertura: un cuadro de texto se
    // cierra antes que el párrafo que lo contiene, pero va después.
    let mut slots: Vec<Option<Segment>> = Vec::new();
    let mut paragraphs: Vec<OpenParagraph> = Vec::new();
    let mut tables: Vec<OpenTable> = Vec::new();
    let mut table_count = 0;
    let mut run_depth = 0usize;
    // Tabuladores y saltos de las carreras, numerados como los fragmentos.
    let mut break_count = 0usize;
    // Dentro de un cambio de formato de párrafo, el estilo es el antiguo.
    let mut old_properties_depth = 0usize;
    let mut object_depth = 0usize;
    let mut fallback_depth = 0usize;
    let mut reading = Reading::Nothing;

    loop {
        let event = reader.read_event().map_err(|_| xml_error(part, &reader))?;
        let (element, empty) = match &event {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = element {
            for key in ["w:author", "w:initials"] {
                if let Some(value) = attribute(e, key) {
                    push_unique(&mut out.revision_authors, value);
                }
            }
            match e.name().as_ref() {
                "w:p" if !empty => {
                    let cell = tables.last().and_then(|t| {
                        Some(TableCell {
                            table: t.index,
                            row: t.row?,
                            column: t.column?,
                        })
                    });
                    slots.push(None);
                    paragraphs.push(OpenParagraph {
                        slot: slots.len() - 1,
                        text: String::new(),
                        map: Vec::new(),
                        breaks: Vec::new(),
                        style_id: None,
                        num_id: None,
                        level: 0,
                        cell,
                    });
                }
                "w:tbl" if !empty => {
                    tables.push(OpenTable {
                        index: table_count,
                        row: None,
                        column: None,
                    });
                    table_count += 1;
                }
                "w:tr" => {
                    if let Some(t) = tables.last_mut() {
                        t.row = Some(t.row.map_or(0, |r| r + 1));
                        t.column = None;
                    }
                }
                "w:tc" => {
                    if let Some(t) = tables.last_mut() {
                        t.column = Some(t.column.map_or(0, |c| c + 1));
                    }
                }
                "w:r" if !empty => run_depth += 1,
                "w:t" | "w:delText" => {
                    deleted.push(e.name().as_ref() == "w:delText");
                    if empty {
                        add_fragment(String::new(), &mut fragments, &mut paragraphs);
                    } else {
                        reading = Reading::Fragment(String::new());
                    }
                }
                "w:instrText" | "w:delInstrText" => {
                    if empty {
                        add_field_code(part, String::new(), out);
                    } else {
                        reading = Reading::FieldCode(String::new());
                    }
                }
                "w:tab" | "w:br" | "w:cr" if run_depth > 0 => {
                    if let Some(p) = paragraphs.last_mut() {
                        p.breaks.push((p.map.len(), break_count));
                        p.text.push(' ');
                        p.map.push(None);
                    }
                    break_count += 1;
                }
                "w:pPrChange" if !empty => old_properties_depth += 1,
                "w:pStyle" if old_properties_depth == 0 => {
                    if let Some(p) = paragraphs.last_mut() {
                        p.style_id = attribute(e, "w:val");
                    }
                }
                "w:numId" if old_properties_depth == 0 => {
                    if let Some(p) = paragraphs.last_mut() {
                        p.num_id = attribute(e, "w:val");
                    }
                }
                "w:ilvl" if old_properties_depth == 0 => {
                    if let Some(p) = paragraphs.last_mut() {
                        p.level = attribute(e, "w:val")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                    }
                }
                // Las copias de compatibilidad no cuentan otra vez.
                "a:blip" if fallback_depth == 0 => out.warnings.images += 1,
                "v:imagedata" if fallback_depth == 0 && object_depth == 0 => {
                    out.warnings.images += 1
                }
                "o:OLEObject" if fallback_depth == 0 => out.warnings.objects += 1,
                "w:object" if !empty => object_depth += 1,
                "mc:Fallback" if !empty => fallback_depth += 1,
                _ => {}
            }
        }
        match event {
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Reading::Fragment(text) | Reading::FieldCode(text) = &mut reading {
                    append_text(text, &event).map_err(|_| xml_error(part, &reader))?;
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "w:t" | "w:delText" => {
                    if let Reading::Fragment(text) =
                        std::mem::replace(&mut reading, Reading::Nothing)
                    {
                        add_fragment(text, &mut fragments, &mut paragraphs);
                    }
                }
                "w:instrText" | "w:delInstrText" => {
                    if let Reading::FieldCode(text) =
                        std::mem::replace(&mut reading, Reading::Nothing)
                    {
                        add_field_code(part, text, out);
                    }
                }
                "w:p" => {
                    if let Some(p) = paragraphs.pop() {
                        let slot = p.slot;
                        slots[slot] = finish_paragraph(part, p, styles);
                    }
                }
                "w:r" => run_depth = run_depth.saturating_sub(1),
                "w:tbl" => {
                    tables.pop();
                }
                "w:pPrChange" => old_properties_depth = old_properties_depth.saturating_sub(1),
                "w:object" => object_depth = object_depth.saturating_sub(1),
                "mc:Fallback" => fallback_depth = fallback_depth.saturating_sub(1),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    out.segments.extend(slots.into_iter().flatten());
    out.parts.push((part.to_owned(), fragments, deleted));
    Ok(())
}

fn add_fragment(text: String, fragments: &mut Vec<String>, paragraphs: &mut [OpenParagraph]) {
    let index = fragments.len();
    if let Some(p) = paragraphs.last_mut() {
        for (offset, c) in text.chars().enumerate() {
            p.text.push(c);
            p.map.push(Some(FragmentPos {
                fragment: index,
                offset,
            }));
        }
    }
    fragments.push(text);
}

fn add_field_code(part: &str, text: String, out: &mut Collected) {
    let index = out.field_codes.iter().filter(|f| f.part == part).count();
    out.field_codes.push(FieldCode {
        part: part.to_owned(),
        index,
        text,
    });
}

/// El segmento del párrafo, o `None` si no tiene texto que analizar.
fn finish_paragraph(part: &str, p: OpenParagraph, styles: &Styles) -> Option<Segment> {
    if p.text.trim().is_empty() {
        return None;
    }
    let heading = p.style_id.as_deref().and_then(|id| styles.heading(id));
    let style = match (heading, p.num_id.as_deref()) {
        (Some(level), _) => ParagraphStyle::Heading(level),
        // `numId` 0 quita la numeración que heredaría del estilo.
        (None, Some(num)) if num != "0" => ParagraphStyle::ListItem {
            ordered: styles.ordered(num, p.level),
            level: p.level,
        },
        _ => ParagraphStyle::Normal,
    };
    Some(Segment {
        text: p.text,
        part: part.to_owned(),
        style,
        cell: p.cell,
        map: p.map,
        breaks: p.breaks,
    })
}

/// Título, asunto, palabras clave y descripción como segmentos, y los
/// metadatos de autor.
pub(super) fn extract_core(part: &str, xml: &str, out: &mut Collected) -> Result<()> {
    let mut fragments = Vec::new();
    for (name, text) in element_texts(
        part,
        xml,
        &[&CORE_TEXT_FIELDS[..], &CORE_AUTHOR_FIELDS[..]].concat(),
    )? {
        if CORE_AUTHOR_FIELDS.contains(&name.as_str()) {
            push_unique(&mut out.author_metadata, text);
            continue;
        }
        let index = fragments.len();
        if !text.trim().is_empty() {
            out.segments.push(Segment {
                map: (0..text.chars().count())
                    .map(|offset| {
                        Some(FragmentPos {
                            fragment: index,
                            offset,
                        })
                    })
                    .collect(),
                text: text.clone(),
                part: part.to_owned(),
                style: ParagraphStyle::Metadata,
                cell: None,
                breaks: Vec::new(),
            });
        }
        fragments.push(text);
    }
    let deleted = vec![false; fragments.len()];
    out.parts.push((part.to_owned(), fragments, deleted));
    Ok(())
}

pub(super) fn extract_app(part: &str, xml: &str, out: &mut Collected) -> Result<()> {
    for (_, text) in element_texts(part, xml, &APP_AUTHOR_FIELDS)? {
        push_unique(&mut out.author_metadata, text);
    }
    Ok(())
}

/// Destinos de los enlaces externos de un archivo de relaciones.
pub(super) fn extract_links(part: &str, xml: &str, out: &mut Collected) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().map_err(|_| xml_error(part, &reader))? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "Relationship" => {
                if attribute(&e, "TargetMode").as_deref() == Some("External")
                    && let (Some(id), Some(target)) = (attribute(&e, "Id"), attribute(&e, "Target"))
                {
                    out.links.push(ExternalLink {
                        rels_part: part.to_owned(),
                        id,
                        target,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

/// Texto de cada elemento llamado como alguno de `names`, en orden.
fn element_texts(part: &str, xml: &str, names: &[&str]) -> Result<Vec<(String, String)>> {
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    let mut current: Option<(String, String)> = None;
    loop {
        let event = reader.read_event().map_err(|_| xml_error(part, &reader))?;
        match &event {
            Event::Start(e) if names.contains(&e.name().as_ref()) => {
                current = Some((e.name().as_ref().to_owned(), String::new()));
            }
            Event::Empty(e) if names.contains(&e.name().as_ref()) => {
                found.push((e.name().as_ref().to_owned(), String::new()));
            }
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some((_, text)) = &mut current {
                    append_text(text, &event).map_err(|_| xml_error(part, &reader))?;
                }
            }
            Event::End(e)
                if current
                    .as_ref()
                    .is_some_and(|(n, _)| n == e.name().as_ref()) =>
            {
                found.extend(current.take());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}

/// Añade a `text` el texto de un evento, con las entidades resueltas.
pub(super) fn append_text(text: &mut String, event: &Event<'_>) -> std::result::Result<(), ()> {
    match event {
        Event::Text(e) => text.push_str(&e.xml10_content()),
        Event::CData(e) => text.push_str(&e.xml10_content()),
        Event::GeneralRef(e) => {
            if e.is_char_ref() {
                text.push(e.resolve_char_ref().ok().flatten().ok_or(())?);
            } else {
                text.push_str(resolve_predefined_entity(&e.xml10_content()).ok_or(())?);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Valor de un atributo con las entidades resueltas.
pub(super) fn attribute(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.contains(&value) {
        list.push(value);
    }
}

/// Error de XML con la parte y la posición, sin el texto del documento.
pub(super) fn xml_error<R>(part: &str, reader: &Reader<R>) -> Error {
    Error::DocxInvalid(format!(
        "{part} no es XML válido (posición {})",
        reader.error_position()
    ))
}
