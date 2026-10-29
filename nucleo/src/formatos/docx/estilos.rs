//! Estilos y numeración: qué párrafos son títulos y qué listas van
//! numeradas. Los títulos se reconocen por el nombre del estilo
//! («heading 1»), que es el mismo en todos los idiomas de Word; el
//! identificador cambia (`Heading1`, `Ttulo1`…).

use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::extractor::{attribute, xml_error};
use crate::Result;

#[derive(Debug, Default)]
pub(super) struct Styles {
    /// Identificador de estilo de párrafo → nivel de título.
    headings: HashMap<String, u8>,
    /// `numId` → (`ilvl` → numerada).
    lists: HashMap<String, HashMap<u8, bool>>,
}

impl Styles {
    pub(super) fn read(styles: Option<&str>, numbering: Option<&str>) -> Result<Styles> {
        let mut result = Styles::default();
        if let Some(xml) = styles {
            result.headings = read_headings(xml)?;
        }
        if let Some(xml) = numbering {
            result.lists = read_numbering(xml)?;
        }
        Ok(result)
    }

    pub(super) fn heading(&self, style_id: &str) -> Option<u8> {
        self.headings.get(style_id).copied()
    }

    /// Si la lista `num_id` va numerada en el nivel `level`. Sin datos de
    /// numeración, se toma por lista de viñetas.
    pub(super) fn ordered(&self, num_id: &str, level: u8) -> bool {
        self.lists
            .get(num_id)
            .and_then(|levels| levels.get(&level))
            .copied()
            .unwrap_or(false)
    }
}

fn read_headings(xml: &str) -> Result<HashMap<String, u8>> {
    let part = "word/styles.xml";
    let mut reader = Reader::from_str(xml);
    let mut headings = HashMap::new();
    let mut current: Option<String> = None;
    loop {
        match reader.read_event().map_err(|_| xml_error(part, &reader))? {
            Event::Start(e) if e.name().as_ref() == "w:style" => {
                current = (attribute(&e, "w:type").as_deref() == Some("paragraph"))
                    .then(|| attribute(&e, "w:styleId"))
                    .flatten();
            }
            Event::End(e) if e.name().as_ref() == "w:style" => current = None,
            Event::Empty(e) if e.name().as_ref() == "w:name" => {
                if let (Some(id), Some(name)) = (&current, attribute(&e, "w:val"))
                    && let Some(level) = heading_level(&name)
                {
                    headings.insert(id.clone(), level);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(headings)
}

/// «heading 1» … «heading 9».
fn heading_level(name: &str) -> Option<u8> {
    let level: u8 = name.to_lowercase().strip_prefix("heading ")?.parse().ok()?;
    (1..=9).contains(&level).then_some(level)
}

fn read_numbering(xml: &str) -> Result<HashMap<String, HashMap<u8, bool>>> {
    let part = "word/numbering.xml";
    let mut reader = Reader::from_str(xml);
    let mut abstracts: HashMap<String, HashMap<u8, bool>> = HashMap::new();
    let mut nums: Vec<(String, String)> = Vec::new();
    let mut abstract_id: Option<String> = None;
    let mut level: Option<u8> = None;
    let mut num_id: Option<String> = None;
    loop {
        let event = reader.read_event().map_err(|_| xml_error(part, &reader))?;
        let (start, empty) = match &event {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            handle_numbering_element(
                e,
                empty,
                &mut abstracts,
                &mut nums,
                &mut abstract_id,
                &mut level,
                &mut num_id,
            );
        }
        match event {
            Event::End(e) if e.name().as_ref() == "w:abstractNum" => abstract_id = None,
            Event::End(e) if e.name().as_ref() == "w:lvl" => level = None,
            Event::End(e) if e.name().as_ref() == "w:num" => num_id = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(nums
        .into_iter()
        .filter_map(|(num, abstract_id)| Some((num, abstracts.get(&abstract_id)?.clone())))
        .collect())
}

fn handle_numbering_element(
    e: &BytesStart<'_>,
    empty: bool,
    abstracts: &mut HashMap<String, HashMap<u8, bool>>,
    nums: &mut Vec<(String, String)>,
    abstract_id: &mut Option<String>,
    level: &mut Option<u8>,
    num_id: &mut Option<String>,
) {
    match e.name().as_ref() {
        "w:abstractNum" if !empty => *abstract_id = attribute(e, "w:abstractNumId"),
        "w:lvl" if !empty => *level = attribute(e, "w:ilvl").and_then(|v| v.parse().ok()),
        "w:numFmt" => {
            if let (Some(id), Some(lvl), Some(format)) =
                (abstract_id.as_ref(), *level, attribute(e, "w:val"))
            {
                let ordered = format != "bullet" && format != "none";
                abstracts
                    .entry(id.clone())
                    .or_default()
                    .insert(lvl, ordered);
            }
        }
        "w:num" if !empty => *num_id = attribute(e, "w:numId"),
        "w:abstractNumId" => {
            if let (Some(num), Some(target)) = (num_id.as_ref(), attribute(e, "w:val")) {
                nums.push((num.clone(), target));
            }
        }
        _ => {}
    }
}
