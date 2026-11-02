//! Escritura sobre el XML original. Las sustituciones cambian el texto de
//! los fragmentos; luego cada parte se recorre con el mismo criterio que el
//! extractor y se empalman los bytes nuevos en su sitio, sin volver a
//! serializar el resto. Las partes sin cambios se copian del ZIP tal cual.

use std::collections::{HashMap, HashSet};

use super::extractor::BREAK_ELEMENTS;
use super::{DocxDocument, FragmentPos, is_word_rels, text_parts};
use crate::formatos::ooxml::reescritura::{Rules, rewrite, sorted_literals, write_package};
use crate::formatos::ooxml::{
    APP_AUTHOR_FIELDS, APP_PART, CORE_AUTHOR_FIELDS, CORE_PART, CORE_TEXT_FIELDS, CUSTOM_PART,
    Office, PROPERTY_TEXTS,
};
use crate::formatos::{Replacement, order_for_writing};
use crate::{Error, Result};

/// Parte con personas que han comentado o revisado, a veces con su correo.
const PEOPLE_PART: &str = "word/people.xml";

/// Lo que cambian las sustituciones en cada parte.
#[derive(Default)]
struct Changes<'a> {
    /// Texto nuevo de los fragmentos que cambian.
    fragments: HashMap<&'a str, HashMap<usize, String>>,
    /// Tabuladores y saltos cubiertos por una sustitución.
    breaks: HashMap<&'a str, HashSet<usize>>,
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
        let changes = self.apply(replacements)?;
        let literals = sorted_literals(literals);
        let names: Vec<String> = self.parts_in_package()?;
        let texts = text_parts(&names);
        let none = HashMap::new();
        let rules_for = |name: &str| -> Option<Rules> {
            if texts.iter().any(|t| t == name) {
                Some(Rules {
                    fragments: &["w:t", "w:delText"],
                    new_fragments: Some(changes.fragments.get(name).unwrap_or(&none)),
                    removed_breaks: changes.breaks.get(name),
                    breaks: &BREAK_ELEMENTS,
                    break_run: Some("w:r"),
                    preserve_space: true,
                    literal: &["w:instrText", "w:delInstrText"],
                    blank_attributes: &["w:author", "w:initials"],
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == CORE_PART {
                Some(Rules {
                    fragments: &CORE_TEXT_FIELDS,
                    new_fragments: Some(changes.fragments.get(name).unwrap_or(&none)),
                    blank: &CORE_AUTHOR_FIELDS,
                    ..Rules::default()
                })
            } else if name == APP_PART || name == CUSTOM_PART {
                Some(Rules {
                    blank: &APP_AUTHOR_FIELDS,
                    literal: &PROPERTY_TEXTS,
                    literals: &literals,
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
            }
        };
        write_package(
            &self.original,
            Office::Word,
            |name| rules_for(name).is_some(),
            |name, xml| match rules_for(name) {
                Some(rules) => rewrite(Office::Word, name, xml, &rules),
                None => Ok(None),
            },
        )
    }

    fn parts_in_package(&self) -> Result<Vec<String>> {
        let package = crate::formatos::ooxml::Package::open(&self.original, Office::Word)?;
        Ok(package.names().to_vec())
    }

    /// Texto nuevo de cada fragmento que cambia, por parte. El marcador va
    /// en el primer fragmento que cubre la sustitución; de los siguientes se
    /// borra la parte cubierta, y también los tabuladores y saltos que haya
    /// en medio: así, al restaurar el valor, que lleva un espacio en su
    /// lugar, el texto vuelve a ser el mismo.
    fn apply(&self, replacements: &[Replacement]) -> Result<Changes<'_>> {
        let lengths: Vec<usize> = self.segments.iter().map(|s| s.map.len()).collect();
        let mut changed: HashMap<&str, HashMap<usize, Vec<char>>> = HashMap::new();
        let mut breaks: HashMap<&str, HashSet<usize>> = HashMap::new();
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
            let removed = breaks.entry(segment.part.as_str()).or_default();
            for &(at, index) in &segment.breaks {
                if (r.start..r.end).contains(&at) {
                    removed.insert(index);
                }
            }
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
        let fragments = changed
            .into_iter()
            .map(|(part, fragments)| {
                let fragments = fragments
                    .into_iter()
                    .map(|(index, chars)| (index, chars.into_iter().collect::<String>()))
                    .filter(|(index, text)| text != self.fragment(part, *index))
                    .collect();
                (part, fragments)
            })
            .collect();
        Ok(Changes { fragments, breaks })
    }
}
