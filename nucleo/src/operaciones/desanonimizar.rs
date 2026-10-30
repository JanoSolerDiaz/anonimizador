//! Desanonimizar (PRD, «Desanonimización»): buscar los marcadores, tomar su
//! valor del almacén o de los diccionarios importados y escribir el mismo
//! formato que la entrada.
//!
//! En DOCX los marcadores se buscan en el texto de cada párrafo, así que se
//! encuentran aunque alguien los haya partido en varios trozos de formato.
//! Si un marcador tiene valores distintos en dos fuentes, no se escribe nada
//! y se devuelven los conflictos para que el usuario elija una fuente para
//! todo el documento. El almacén no cambia.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::InputFormat;
use super::marcadores::{FoundMarker, canonical, find_markers};
use crate::Result;
use crate::almacen::Store;
use crate::formatos::Replacement;
use crate::formatos::docx::DocxDocument;
use crate::formatos::salida::{Suffix, description, save_new};
use crate::formatos::texto::TextDocument;
use crate::motor::tipos::TypeTable;

/// De dónde sale el valor de un marcador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerSource {
    /// Las entradas propias.
    Store,
    /// Un diccionario importado, por su nombre.
    Dictionary(String),
}

/// Marcador con valores distintos en varias fuentes. No lleva los valores:
/// quien quiera mostrarlos los busca en cada fuente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub marker: String,
    pub sources: Vec<MarkerSource>,
}

/// Marcadores restaurados y marcadores de un tipo conocido que no están en
/// ninguna fuente, sin repetir y en orden de aparición.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub restored: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    Done(T),
    /// Hace falta elegir una fuente; no se ha escrito nada.
    Conflicts(Vec<Conflict>),
}

/// Qué cambiar en el documento.
struct Plan {
    replacements: Vec<Replacement>,
    /// Marcador tal como está escrito → valor, para los destinos de enlaces
    /// y los códigos de campo de un DOCX.
    literals: Vec<(String, String)>,
    report: Report,
}

enum Lookup {
    Value(String),
    Missing,
    Conflict(Vec<MarkerSource>),
}

/// Texto pegado, con los marcadores restaurados.
pub fn deanonymize_text(
    text: &str,
    store: &Store,
    source: Option<&MarkerSource>,
) -> Result<Outcome<(String, Report)>> {
    let document = TextDocument::from_bytes(text.as_bytes())?;
    let plan = match plan(&document.segments(), &[], store, source) {
        Outcome::Done(plan) => plan,
        Outcome::Conflicts(conflicts) => return Ok(Outcome::Conflicts(conflicts)),
    };
    let restored =
        String::from_utf8(document.write(&plan.replacements)?).expect("el texto sigue en UTF-8");
    log(&plan.report);
    Ok(Outcome::Done((restored, plan.report)))
}

/// Desanonimiza `input` y deja la salida en `dir` con el nombre
/// `descripción_desanonimizado.ext`, en el mismo formato y sin sobrescribir
/// nada.
pub fn deanonymize(
    input: &Path,
    dir: &Path,
    store: &Store,
    source: Option<&MarkerSource>,
) -> Result<Outcome<(PathBuf, Report)>> {
    let bytes = std::fs::read(input)?;
    let format = InputFormat::detect(input, &bytes)?;
    let (data, report) = match format {
        InputFormat::Docx => {
            let document = DocxDocument::from_bytes(&bytes)?;
            let segments: Vec<&str> = document
                .segments()
                .iter()
                .map(|s| s.text.as_str())
                .collect();
            let extra: Vec<&str> = document
                .external_links()
                .iter()
                .map(|l| l.target.as_str())
                .chain(document.field_codes().iter().map(|f| f.text.as_str()))
                .collect();
            let plan = match plan(&segments, &extra, store, source) {
                Outcome::Done(plan) => plan,
                Outcome::Conflicts(conflicts) => return Ok(Outcome::Conflicts(conflicts)),
            };
            (
                document.write(&plan.replacements, &plan.literals)?,
                plan.report,
            )
        }
        InputFormat::Markdown | InputFormat::Text => {
            let document = TextDocument::from_bytes(&bytes)?;
            let plan = match plan(&document.segments(), &[], store, source) {
                Outcome::Done(plan) => plan,
                Outcome::Conflicts(conflicts) => return Ok(Outcome::Conflicts(conflicts)),
            };
            (document.write(&plan.replacements)?, plan.report)
        }
    };
    let extension = match format {
        InputFormat::Docx => "docx",
        InputFormat::Markdown => "md",
        InputFormat::Text => "txt",
    };
    let output = save_new(
        dir,
        &description(input),
        Suffix::Deanonymized,
        extension,
        |out: &mut dyn Write| Ok(out.write_all(&data)?),
    )?;
    log(&report);
    Ok(Outcome::Done((output, report)))
}

/// Busca los marcadores de los segmentos y de los textos de `extra`
/// (enlaces y códigos de campo, que solo admiten sustitución literal).
fn plan(
    segments: &[&str],
    extra: &[&str],
    store: &Store,
    source: Option<&MarkerSource>,
) -> Outcome<Plan> {
    let mut plan = Plan {
        replacements: Vec::new(),
        literals: Vec::new(),
        report: Report::default(),
    };
    let mut conflicts: Vec<Conflict> = Vec::new();
    let texts = segments
        .iter()
        .enumerate()
        .map(|(index, text)| (Some(index), *text))
        .chain(extra.iter().map(|text| (None, *text)));
    for (segment, text) in texts {
        for found in find_markers(text) {
            match lookup(&found, store, source) {
                Lookup::Value(value) => {
                    push_unique(&mut plan.report.restored, &found.key);
                    match segment {
                        Some(segment) => plan.replacements.push(Replacement {
                            segment,
                            start: found.start,
                            end: found.end,
                            text: value,
                        }),
                        None => {
                            let written: String = text
                                .chars()
                                .skip(found.start)
                                .take(found.end - found.start)
                                .collect();
                            if !plan.literals.iter().any(|(w, _)| *w == written) {
                                plan.literals.push((written, value));
                            }
                        }
                    }
                }
                Lookup::Missing => {
                    if !found.short && is_known_kind(&found.key) {
                        push_unique(&mut plan.report.missing, &found.key);
                    }
                }
                Lookup::Conflict(sources) => {
                    if !conflicts.iter().any(|c| c.marker == found.key) {
                        conflicts.push(Conflict {
                            marker: found.key.clone(),
                            sources,
                        });
                    }
                }
            }
        }
    }
    if conflicts.is_empty() {
        Outcome::Done(plan)
    } else {
        tracing::info!(
            conflictos = conflicts.len(),
            "marcadores con valores distintos"
        );
        Outcome::Conflicts(conflicts)
    }
}

/// Valor del marcador en las entradas propias y en los diccionarios.
fn lookup(found: &FoundMarker, store: &Store, source: Option<&MarkerSource>) -> Lookup {
    let same_short = |short: Option<&str>| short.is_some_and(|s| canonical(s) == found.key);
    let mut values: Vec<(MarkerSource, &str)> = Vec::new();
    let own = if found.short {
        store.entries().find(|e| same_short(e.short_form()))
    } else {
        store.entry_for_marker(&found.key)
    };
    if let Some(entry) = own {
        values.push((MarkerSource::Store, entry.value()));
    }
    for dictionary in store.imported() {
        let entry = if found.short {
            dictionary
                .entries()
                .iter()
                .find(|e| same_short(e.short_form()))
        } else {
            dictionary.entry_for_marker(&found.key)
        };
        if let Some(entry) = entry {
            values.push((
                MarkerSource::Dictionary(dictionary.name().to_owned()),
                entry.value(),
            ));
        }
    }
    let Some((_, first)) = values.first() else {
        return Lookup::Missing;
    };
    if values.iter().all(|(_, value)| value == first) {
        return Lookup::Value((*first).to_owned());
    }
    match source.and_then(|chosen| values.iter().find(|(s, _)| s == chosen)) {
        Some((_, value)) => Lookup::Value((*value).to_owned()),
        None => Lookup::Conflict(values.into_iter().map(|(s, _)| s).collect()),
    }
}

/// Si `[TIPO_0042]` es de un tipo de la tabla. Un marcador de otro tipo
/// puede ser texto normal entre corchetes y no se da por perdido.
fn is_known_kind(key: &str) -> bool {
    key.strip_prefix('[')
        .and_then(|rest| rest.rsplit_once('_'))
        .is_some_and(|(kind, _)| TypeTable::builtin().contains_marker(kind))
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_owned());
    }
}

fn log(report: &Report) {
    tracing::info!(
        restaurados = report.restored.len(),
        sin_fuente = report.missing.len(),
        "documento desanonimizado"
    );
}
