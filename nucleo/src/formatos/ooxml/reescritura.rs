//! Escritura sobre el XML original. Cada parte se recorre con el mismo
//! criterio que su extractor y se empalman los bytes nuevos en su sitio, sin
//! volver a serializar el resto. Las partes sin cambios se copian del ZIP tal
//! cual.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Write};

use quick_xml::Reader;
use quick_xml::escape::partial_escape;
use quick_xml::events::{BytesStart, Event};

use super::{Office, append_text, attribute, invalid, read_part, xml_error};
use crate::Result;

/// Qué se cambia en una parte.
#[derive(Default, Clone, Copy)]
pub(crate) struct Rules<'a> {
    /// Elementos cuyo texto es un fragmento, numerados en orden.
    pub fragments: &'a [&'a str],
    /// Dentro de estos elementos no hay fragmentos ni literales (las
    /// lecturas fonéticas de Excel, por ejemplo).
    pub skip_inside: &'a [&'a str],
    /// Texto nuevo de los fragmentos que cambian.
    pub new_fragments: Option<&'a HashMap<usize, String>>,
    /// Tabuladores y saltos que se borran, numerados como en el extractor.
    pub removed_breaks: Option<&'a HashSet<usize>>,
    /// Elementos de tabulador o salto que se cuentan.
    pub breaks: &'a [&'a str],
    /// Si los saltos solo cuentan dentro de este elemento (`w:r` en Word).
    pub break_run: Option<&'a str>,
    /// Si los fragmentos con un espacio en un extremo necesitan
    /// `xml:space="preserve"`.
    pub preserve_space: bool,
    /// Elementos cuyo texto se vacía.
    pub blank: &'a [&'a str],
    /// Elementos cuyo texto recibe las sustituciones literales.
    pub literal: &'a [&'a str],
    /// Atributos que se vacían.
    pub blank_attributes: &'a [&'a str],
    /// Atributos (elemento, atributo) que reciben las sustituciones
    /// literales.
    pub literal_attributes: &'a [(&'a str, &'a str)],
    /// Si el destino de los enlaces externos recibe las sustituciones
    /// literales.
    pub link_targets: bool,
    pub literals: &'a [(String, String)],
    /// Elementos cuyo texto pasa por `transform` (fórmulas, por ejemplo).
    pub transformed: &'a [&'a str],
    /// Atributos (elemento, atributo) cuyo valor pasa por `transform`.
    pub transformed_attributes: &'a [(&'a str, &'a str)],
    /// Recibe el nombre del elemento, el del atributo (o `None` si es el
    /// texto del elemento) y el texto, y devuelve el texto nuevo.
    pub transform: Option<Transform<'a>>,
}

/// Función que transforma un texto: recibe el nombre del elemento, el del
/// atributo (o `None` si es el texto del elemento) y el texto.
pub(crate) type Transform<'a> = &'a dyn Fn(&str, Option<&str>, &str) -> String;

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
    Transformed,
}

/// Literales en el orden en que se aplican: las largas primero, para que una
/// que contenga a otra no se quede a medias. Sin las vacías.
pub(crate) fn sorted_literals(literals: &[(String, String)]) -> Vec<(String, String)> {
    let mut literals: Vec<(String, String)> = literals
        .iter()
        .filter(|(from, _)| !from.is_empty())
        .cloned()
        .collect();
    literals.sort_by_key(|(from, _)| std::cmp::Reverse(from.chars().count()));
    literals.dedup_by(|a, b| a.0 == b.0);
    literals
}

/// Escribe el paquete `original` de nuevo. Para cada parte, `rewrite` recibe
/// su nombre y su XML si `wants` la pide, y devuelve el XML nuevo o `None` si
/// no cambia.
pub(crate) fn write_package(
    original: &[u8],
    office: Office,
    wants: impl Fn(&str) -> bool,
    mut rewrite: impl FnMut(&str, &str) -> Result<Option<String>>,
) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(original)).map_err(|_| {
        invalid(
            office,
            format!("no es un archivo {}", office.extension().to_uppercase()),
        )
    })?;
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.map(|n| n.into_owned()))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| invalid(office, "no se pueden leer sus partes".into()))?;
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (index, name) in names.iter().enumerate() {
        let rewritten = if wants(name) {
            match read_part(&mut archive, office, name)? {
                Some(xml) => rewrite(name, &xml)?,
                None => None,
            }
        } else {
            None
        };
        let file = archive
            .by_index_raw(index)
            .map_err(|_| invalid(office, format!("no se puede leer {name}")))?;
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
        written.map_err(|_| invalid(office, format!("no se puede escribir {name}")))?;
    }
    output
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|_| {
            invalid(
                office,
                format!(
                    "no se puede escribir el {}",
                    office.extension().to_uppercase()
                ),
            )
        })
}

/// La parte con los cambios de `rules`, o `None` si no cambia nada.
pub(crate) fn rewrite(
    office: Office,
    part: &str,
    xml: &str,
    rules: &Rules,
) -> Result<Option<String>> {
    let mut reader = Reader::from_str(xml);
    // Bytes que se cambian: inicio, fin y texto nuevo, sin solaparse.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut fragment = 0usize;
    let mut open: Option<Open> = None;
    let mut run_depth = 0usize;
    let mut skip_depth = 0usize;
    let mut break_count = 0usize;
    // Inicio de un salto escrito con apertura y cierre que se borra.
    let mut open_break: Option<usize> = None;
    loop {
        let before = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|_| xml_error(office, part, &reader))?;
        let after = reader.buffer_position() as usize;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name();
                let name = name.as_ref();
                if rules.skip_inside.contains(&name) && !empty {
                    skip_depth += 1;
                }
                if rules.break_run == Some(name) && !empty {
                    run_depth += 1;
                }
                if rules.breaks.contains(&name) && (rules.break_run.is_none() || run_depth > 0) {
                    if rules
                        .removed_breaks
                        .is_some_and(|removed| removed.contains(&break_count))
                    {
                        if empty {
                            edits.push((before, after, String::new()));
                        } else {
                            open_break = Some(before);
                        }
                    }
                    break_count += 1;
                }
                let kind = if skip_depth > 0 {
                    None
                } else if rules.fragments.contains(&name) {
                    fragment += 1;
                    Some(Kind::Fragment(fragment - 1))
                } else if rules.blank.contains(&name) {
                    Some(Kind::Blank)
                } else if rules.literal.contains(&name) {
                    Some(Kind::Literal)
                } else if rules.transformed.contains(&name) && rules.transform.is_some() {
                    Some(Kind::Transformed)
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
                    append_text(&mut o.text, &event)
                        .map_err(|_| xml_error(office, part, &reader))?;
                }
            }
            Event::End(e) => {
                let name = e.name();
                if rules.skip_inside.contains(&name.as_ref()) {
                    skip_depth = skip_depth.saturating_sub(1);
                }
                if rules.break_run == Some(name.as_ref()) {
                    run_depth = run_depth.saturating_sub(1);
                }
                if rules.breaks.contains(&name.as_ref())
                    && let Some(start) = open_break.take()
                {
                    edits.push((start, after, String::new()));
                }
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
                        Kind::Transformed => {
                            let element = o.tag.name();
                            let transform = rules.transform.expect("solo con transform");
                            let new = transform(element.as_ref(), None, &o.text);
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
    Ok(splice(xml, edits))
}

/// El XML con los cambios de bytes `edits` (inicio, fin, texto nuevo), o
/// `None` si no hay ninguno. No pueden solaparse.
pub(crate) fn splice(xml: &str, mut edits: Vec<(usize, usize, String)>) -> Option<String> {
    if edits.is_empty() {
        return None;
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
    Some(result)
}

/// La etiqueta reescrita si alguno de sus atributos cambia.
fn changed_attributes(e: &BytesStart, rules: &Rules, empty: bool) -> Result<Option<String>> {
    let mut set: Vec<(&str, String)> = rules
        .blank_attributes
        .iter()
        .filter(|key| attribute(e, key).is_some_and(|v| !v.is_empty()))
        .map(|key| (*key, String::new()))
        .collect();
    let name = e.name();
    for (element, key) in rules.literal_attributes {
        if name.as_ref() == *element
            && let Some(value) = attribute(e, key)
        {
            let new = apply_literals(&value, rules.literals);
            if new != value {
                set.push((key, new));
            }
        }
    }
    if let Some(transform) = rules.transform {
        for (element, key) in rules.transformed_attributes {
            if name.as_ref() == *element
                && let Some(value) = attribute(e, key)
            {
                let new = transform(element, Some(key), &value);
                if new != value {
                    set.push((key, new));
                }
            }
        }
    }
    if rules.link_targets
        && name.as_ref() == "Relationship"
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

/// La etiqueta con los atributos de `set` cambiados o añadidos. Los que no
/// cambian se copian tal como estaban escritos; los nuevos se escapan lo
/// justo (`&`, `<` y las comillas dobles), como hace Office.
pub(crate) fn rebuild(e: &BytesStart, set: &[(&str, &str)], empty: bool) -> Result<String> {
    let failed = || crate::Error::InvalidReplacement("no se puede reescribir una etiqueta".into());
    let escape = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
    };
    let mut tag = format!("<{}", e.name().as_ref());
    // Los atributos quedan en el mismo orden; los nuevos, al final.
    for a in e.attributes() {
        let a = a.map_err(|_| failed())?;
        let key = a.key.as_ref();
        let value = match set.iter().find(|(k, _)| *k == key) {
            Some((_, changed)) => escape(changed),
            // Escrito entre comillas simples podría llevar comillas dobles.
            None => a.value.replace('"', "&quot;"),
        };
        tag.push_str(&format!(" {key}=\"{value}\""));
    }
    for (key, value) in set.iter().filter(|(k, _)| attribute(e, k).is_none()) {
        tag.push_str(&format!(" {key}=\"{}\"", escape(value)));
    }
    tag.push_str(if empty { "/>" } else { ">" });
    Ok(tag)
}

/// `text` con cada literal cambiado por su sustituta. `literals` va ya
/// ordenada por [`sorted_literals`].
pub(crate) fn apply_literals(text: &str, literals: &[(String, String)]) -> String {
    let mut result = text.to_owned();
    for (from, to) in literals {
        if result.contains(from.as_str()) {
            result = result.replace(from.as_str(), to);
        }
    }
    result
}
