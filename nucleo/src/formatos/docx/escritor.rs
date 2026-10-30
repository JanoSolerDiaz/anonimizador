//! Escritura sobre el XML original. Las sustituciones cambian el texto de
//! los fragmentos; luego cada parte se recorre con el mismo criterio que el
//! extractor y se empalman los bytes nuevos en su sitio, sin volver a
//! serializar el resto. Las partes sin cambios se copian del ZIP tal cual.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use quick_xml::escape::partial_escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use super::extractor::{
    APP_AUTHOR_FIELDS, CORE_AUTHOR_FIELDS, CORE_TEXT_FIELDS, append_text, attribute, xml_error,
};
use super::{APP_PART, CORE_PART, DocxDocument, FragmentPos, is_word_rels, read_part, text_parts};
use crate::formatos::{Replacement, order_for_writing};
use crate::{Error, Result};

/// Parte con personas que han comentado o revisado, a veces con su correo.
const PEOPLE_PART: &str = "word/people.xml";

/// Qué se cambia en una parte.
#[derive(Default)]
struct Rules<'a> {
    /// Elementos cuyo texto es un fragmento, numerados en orden.
    fragments: &'a [&'a str],
    /// Texto nuevo de los fragmentos que cambian.
    new_fragments: Option<&'a HashMap<usize, String>>,
    /// Si los fragmentos con un espacio en un extremo necesitan
    /// `xml:space="preserve"`.
    preserve_space: bool,
    /// Elementos cuyo texto se vacía.
    blank: &'a [&'a str],
    /// Elementos cuyo texto recibe las sustituciones literales.
    literal: &'a [&'a str],
    /// Atributos que se vacían.
    blank_attributes: &'a [&'a str],
    /// Si el destino de los enlaces externos recibe las sustituciones
    /// literales.
    link_targets: bool,
    literals: &'a [&'a (String, String)],
}

/// Elemento con texto abierto mientras se recorre.
struct Open<'x> {
    kind: Kind,
    tag: BytesStart<'x>,
    /// Bytes de la etiqueta de apertura.
    tag_range: (usize, usize),
    text: String,
}

enum Kind {
    Fragment(usize),
    Blank,
    Literal,
}

impl DocxDocument {
    /// El DOCX con las sustituciones aplicadas sobre los segmentos y las
    /// literales (`valor`, `nuevo`) en destinos de enlaces y códigos de campo.
    /// Además vacía los metadatos de autor y los autores de comentarios y
    /// control de cambios.
    pub fn write(
        &self,
        replacements: &[Replacement],
        literals: &[(String, String)],
    ) -> Result<Vec<u8>> {
        let new_fragments = self.apply(replacements)?;
        // Las largas primero, para que una que contenga a otra no se quede a
        // medias.
        let mut literals: Vec<&(String, String)> = literals
            .iter()
            .filter(|(from, _)| !from.is_empty())
            .collect();
        literals.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));

        let mut archive = zip::ZipArchive::new(Cursor::new(&self.original[..]))
            .map_err(|_| Error::DocxInvalid("no es un archivo DOCX".into()))?;
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.map(|n| n.into_owned()))
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| Error::DocxInvalid("no es un archivo DOCX".into()))?;
        let texts = text_parts(&names);
        let none = HashMap::new();

        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (index, name) in names.iter().enumerate() {
            let rules = if texts.contains(name) {
                Some(Rules {
                    fragments: &["w:t", "w:delText"],
                    new_fragments: Some(new_fragments.get(name.as_str()).unwrap_or(&none)),
                    preserve_space: true,
                    literal: &["w:instrText", "w:delInstrText"],
                    blank_attributes: &["w:author", "w:initials"],
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == CORE_PART {
                Some(Rules {
                    fragments: &CORE_TEXT_FIELDS,
                    new_fragments: Some(new_fragments.get(name.as_str()).unwrap_or(&none)),
                    blank: &CORE_AUTHOR_FIELDS,
                    ..Rules::default()
                })
            } else if name == APP_PART {
                Some(Rules {
                    blank: &APP_AUTHOR_FIELDS,
                    ..Rules::default()
                })
            } else if is_word_rels(name) {
                Some(Rules {
                    link_targets: true,
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == PEOPLE_PART {
                Some(Rules {
                    blank_attributes: &["w15:author", "w15:userId"],
                    ..Rules::default()
                })
            } else {
                None
            };
            let rewritten = match rules {
                Some(rules) => match read_part(&mut archive, name)? {
                    Some(xml) => rewrite(name, &xml, &rules)?,
                    None => None,
                },
                None => None,
            };
            let file = archive
                .by_index_raw(index)
                .map_err(|_| Error::DocxInvalid(format!("no se puede leer {name}")))?;
            let written = match rewritten {
                Some(xml) => {
                    let options = file.options();
                    drop(file);
                    output
                        .start_file(name.as_str(), options)
                        .and_then(|()| Ok(output.write_all(xml.as_bytes())?))
                }
                None => output.raw_copy_file(file),
            };
            written.map_err(|_| Error::DocxInvalid(format!("no se puede escribir {name}")))?;
        }
        output
            .finish()
            .map(|cursor| cursor.into_inner())
            .map_err(|_| Error::DocxInvalid("no se puede escribir el DOCX".into()))
    }

    /// Texto nuevo de cada fragmento que cambia, por parte. El marcador va
    /// en el primer fragmento que cubre la sustitución; de los siguientes se
    /// borra la parte cubierta.
    fn apply(&self, replacements: &[Replacement]) -> Result<HashMap<&str, HashMap<usize, String>>> {
        let lengths: Vec<usize> = self.segments.iter().map(|s| s.map.len()).collect();
        let mut changed: HashMap<&str, HashMap<usize, Vec<char>>> = HashMap::new();
        // De la última a la primera: en un mismo fragmento, las posiciones de
        // las que faltan no se mueven.
        for r in order_for_writing(replacements, &lengths)? {
            let segment = &self.segments[r.segment];
            let positions: Vec<FragmentPos> = segment.map[r.start..r.end]
                .iter()
                .flatten()
                .copied()
                .collect();
            let Some(first) = positions.first().copied() else {
                return Err(Error::InvalidReplacement(format!(
                    "los caracteres {}..{} del segmento {} no tienen texto que sustituir",
                    r.start, r.end, r.segment
                )));
            };
            let fragments = changed.entry(segment.part.as_str()).or_default();
            // Los caracteres de un fragmento van seguidos en el segmento.
            for group in positions.chunk_by(|a, b| a.fragment == b.fragment) {
                let fragment = group[0].fragment;
                let chars = fragments
                    .entry(fragment)
                    .or_insert_with(|| self.fragment(&segment.part, fragment).chars().collect());
                let text = if fragment == first.fragment {
                    r.text.as_str()
                } else {
                    ""
                };
                chars.splice(
                    group[0].offset..=group[group.len() - 1].offset,
                    text.chars(),
                );
            }
        }
        Ok(changed
            .into_iter()
            .map(|(part, fragments)| {
                let fragments = fragments
                    .into_iter()
                    .map(|(index, chars)| (index, chars.into_iter().collect::<String>()))
                    .filter(|(index, text)| text != self.fragment(part, *index))
                    .collect();
                (part, fragments)
            })
            .collect())
    }
}

/// La parte con los cambios de `rules`, o `None` si no cambia nada.
fn rewrite(part: &str, xml: &str, rules: &Rules) -> Result<Option<String>> {
    let mut reader = Reader::from_str(xml);
    // Bytes que se cambian: inicio, fin y texto nuevo, sin solaparse.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut fragment = 0usize;
    let mut open: Option<Open> = None;
    loop {
        let before = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|_| xml_error(part, &reader))?;
        let after = reader.buffer_position() as usize;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name();
                let name = name.as_ref();
                let kind = if rules.fragments.contains(&name) {
                    fragment += 1;
                    Some(Kind::Fragment(fragment - 1))
                } else if rules.blank.contains(&name) {
                    Some(Kind::Blank)
                } else if rules.literal.contains(&name) {
                    Some(Kind::Literal)
                } else {
                    None
                };
                if let Some(tag) = changed_attributes(&e, rules, empty)? {
                    edits.push((before, after, tag));
                }
                // Un elemento vacío no tiene fragmento con caracteres: no
                // puede cambiar.
                if let Some(kind) = kind
                    && !empty
                {
                    open = Some(Open {
                        kind,
                        tag: e,
                        tag_range: (before, after),
                        text: String::new(),
                    });
                }
            }
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some(o) = &mut open {
                    append_text(&mut o.text, &event).map_err(|_| xml_error(part, &reader))?;
                }
            }
            Event::End(e) => {
                if let Some(o) = open.take_if(|o| o.tag.name() == e.name()) {
                    let new = match o.kind {
                        Kind::Fragment(index) => rules
                            .new_fragments
                            .and_then(|fragments| fragments.get(&index))
                            .cloned(),
                        Kind::Blank => (!o.text.is_empty()).then(String::new),
                        Kind::Literal => {
                            let new = apply_literals(&o.text, rules.literals);
                            (new != o.text).then_some(new)
                        }
                    };
                    if let Some(new) = new {
                        let at_edge = new.starts_with(char::is_whitespace)
                            || new.ends_with(char::is_whitespace);
                        if rules.preserve_space
                            && matches!(o.kind, Kind::Fragment(_))
                            && at_edge
                            && attribute(&o.tag, "xml:space").as_deref() != Some("preserve")
                        {
                            let tag = rebuild(&o.tag, &[("xml:space", "preserve")], false)?;
                            edits.push((o.tag_range.0, o.tag_range.1, tag));
                        }
                        edits.push((o.tag_range.1, before, partial_escape(&new).into_owned()));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|edit| edit.0);
    let mut result = String::with_capacity(xml.len());
    let mut at = 0;
    for (start, end, text) in edits {
        result.push_str(&xml[at..start]);
        result.push_str(&text);
        at = end;
    }
    result.push_str(&xml[at..]);
    Ok(Some(result))
}

/// La etiqueta reescrita si alguno de sus atributos cambia.
fn changed_attributes(e: &BytesStart, rules: &Rules, empty: bool) -> Result<Option<String>> {
    let mut set: Vec<(&str, String)> = rules
        .blank_attributes
        .iter()
        .filter(|key| attribute(e, key).is_some_and(|v| !v.is_empty()))
        .map(|key| (*key, String::new()))
        .collect();
    if rules.link_targets
        && e.name().as_ref() == "Relationship"
        && attribute(e, "TargetMode").as_deref() == Some("External")
        && let Some(target) = attribute(e, "Target")
    {
        let new = apply_literals(&target, rules.literals);
        if new != target {
            set.push(("Target", new));
        }
    }
    if set.is_empty() {
        return Ok(None);
    }
    let set: Vec<(&str, &str)> = set.iter().map(|(k, v)| (*k, v.as_str())).collect();
    rebuild(e, &set, empty).map(Some)
}

/// La etiqueta con los atributos de `set` cambiados o añadidos.
fn rebuild(e: &BytesStart, set: &[(&str, &str)], empty: bool) -> Result<String> {
    let name = e.name();
    let mut tag = BytesStart::new(name.as_ref());
    // Los atributos quedan en el mismo orden; los nuevos, al final.
    for a in e.attributes().flatten() {
        let key = a.key.as_ref();
        match set.iter().find(|(k, _)| *k == key) {
            Some(changed) => tag.push_attribute(*changed),
            None => {
                let value = attribute(e, key).unwrap_or_default();
                tag.push_attribute((key, value.as_str()));
            }
        }
    }
    for added in set.iter().filter(|(k, _)| attribute(e, k).is_none()) {
        tag.push_attribute(*added);
    }
    let mut writer = Writer::new(Vec::new());
    let event = if empty {
        Event::Empty(tag)
    } else {
        Event::Start(tag)
    };
    writer
        .write_event(event)
        .map_err(|_| Error::DocxInvalid("no se puede reescribir una etiqueta".into()))?;
    String::from_utf8(writer.into_inner())
        .map_err(|_| Error::DocxInvalid("no se puede reescribir una etiqueta".into()))
}

fn apply_literals(text: &str, literals: &[&(String, String)]) -> String {
    let mut result = text.to_owned();
    for (from, to) in literals {
        if result.contains(from.as_str()) {
            result = result.replace(from.as_str(), to);
        }
    }
    result
}
