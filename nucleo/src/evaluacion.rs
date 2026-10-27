//! Evaluación de la detección con un corpus sintético.
//!
//! Cada texto `nombre.txt` del corpus lleva al lado `nombre.json` con las
//! entidades que contiene: `{"entidades": [{"tipo": …, "texto": …}]}`. Las
//! posiciones no se escriben a mano: se buscan en el texto. Cada aparición
//! cuenta, salvo las que quedan dentro de otra entidad más larga.
//!
//! La comparación es estricta: una detección es correcta solo si coinciden
//! los dos bordes y el tipo. Aparte se cuenta cuántas entidades esperadas
//! quedan tapadas por detecciones de cualquier tipo, que es lo que importa
//! para que un dato no se escape.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::motor::tipos::TypeTable;
use crate::{Error, Result};

/// Entidad tal como se escribe en el JSON del corpus.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEntity {
    #[serde(rename = "tipo")]
    pub kind: String,
    #[serde(rename = "texto")]
    pub text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedFile {
    entidades: Vec<ExpectedEntity>,
}

/// Tramo de texto con su tipo. `start` y `end` son posiciones en caracteres
/// dentro del texto completo, con `end` excluido.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: String,
}

impl Span {
    fn contains(&self, other: &Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Busca en `text` cada aparición de las entidades esperadas, como palabra
/// completa. Las apariciones contenidas en otra más larga se descartan.
/// Devuelve los tramos ordenados por posición.
pub fn locate(text: &str, expected: &[ExpectedEntity]) -> Result<Vec<Span>> {
    let table = TypeTable::builtin();
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    for entity in expected {
        if table.is_optional(&entity.kind).is_none() {
            return Err(Error::UnknownType(entity.kind.clone()));
        }
        let needle: Vec<char> = entity.text.chars().collect();
        if needle.is_empty() {
            return Err(Error::Corpus(format!(
                "hay una entidad de tipo {} sin texto",
                entity.kind
            )));
        }
        let before = found.len();
        for start in 0..chars.len().saturating_sub(needle.len() - 1) {
            let end = start + needle.len();
            let whole_word = !(start > 0 && chars[start - 1].is_alphanumeric()
                || end < chars.len() && chars[end].is_alphanumeric());
            if chars[start..end] == needle[..] && whole_word {
                found.push(Span {
                    start,
                    end,
                    kind: entity.kind.clone(),
                });
            }
        }
        if found.len() == before {
            return Err(Error::Corpus(format!(
                "no aparece «{}» ({})",
                entity.text, entity.kind
            )));
        }
    }

    found.sort();
    found.dedup();
    let mut spans: Vec<Span> = Vec::new();
    for (i, span) in found.iter().enumerate() {
        let mut inside_another = false;
        for (j, other) in found.iter().enumerate() {
            if i == j || !span.overlaps(other) {
                continue;
            }
            let (a, b) = (span.start, span.end);
            if (a, b) == (other.start, other.end) {
                return Err(Error::Corpus(format!(
                    "«{}» tiene dos tipos: {} y {}",
                    chars[a..b].iter().collect::<String>(),
                    span.kind,
                    other.kind
                )));
            }
            if other.contains(span) {
                inside_another = true;
            } else if !span.contains(other) {
                return Err(Error::Corpus(format!(
                    "«{}» y «{}» se solapan sin que una contenga a la otra",
                    chars[a..b].iter().collect::<String>(),
                    chars[other.start..other.end].iter().collect::<String>()
                )));
            }
        }
        if !inside_another {
            spans.push(span.clone());
        }
    }
    Ok(spans)
}

/// Texto del corpus con sus entidades esperadas ya localizadas.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusText {
    /// Nombre del archivo sin extensión.
    pub name: String,
    pub text: String,
    pub expected: Vec<Span>,
}

/// Lee todos los pares `.txt` + `.json` de una carpeta, ordenados por nombre.
pub fn load_corpus(dir: &Path) -> Result<Vec<CorpusText>> {
    let mut texts = Vec::new();
    let mut jsons = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("txt") => texts.push(stem.to_owned()),
            Some("json") => jsons.push(stem.to_owned()),
            _ => {}
        }
    }
    texts.sort();
    if let Some(orphan) = jsons.iter().find(|j| !texts.contains(j)) {
        return Err(Error::Corpus(format!("falta {orphan}.txt")));
    }
    if texts.is_empty() {
        return Err(Error::Corpus(format!("no hay textos en {}", dir.display())));
    }

    let mut corpus = Vec::with_capacity(texts.len());
    for name in texts {
        let json_path = dir.join(format!("{name}.json"));
        if !json_path.is_file() {
            return Err(Error::Corpus(format!("falta {name}.json")));
        }
        let text = std::fs::read_to_string(dir.join(format!("{name}.txt")))?;
        let file: ExpectedFile = serde_json::from_str(&std::fs::read_to_string(&json_path)?)
            .map_err(|e| Error::Corpus(format!("{name}.json: {e}")))?;
        let expected = locate(&text, &file.entidades).map_err(|e| match e {
            Error::Corpus(detail) => Error::Corpus(format!("{name}: {detail}")),
            Error::UnknownType(kind) => {
                Error::Corpus(format!("{name}: no existe el tipo de dato «{kind}»"))
            }
            other => other,
        })?;
        corpus.push(CorpusText {
            name,
            text,
            expected,
        });
    }
    Ok(corpus)
}

/// Recuentos de un tipo de dato.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Entidades esperadas de este tipo.
    pub expected: usize,
    /// Detecciones de este tipo.
    pub detected: usize,
    /// Detecciones con los mismos bordes y tipo que una esperada.
    pub correct: usize,
    /// Esperadas cuyas letras y cifras quedan todas dentro de alguna
    /// detección, sea del tipo que sea.
    pub covered: usize,
}

fn ratio(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

impl Counts {
    /// De lo detectado, cuánto era correcto.
    pub fn precision(&self) -> Option<f64> {
        ratio(self.correct, self.detected)
    }

    /// De lo esperado, cuánto se detectó con bordes y tipo exactos.
    pub fn recall(&self) -> Option<f64> {
        ratio(self.correct, self.expected)
    }

    /// De lo esperado, cuánto quedó tapado.
    pub fn coverage(&self) -> Option<f64> {
        ratio(self.covered, self.expected)
    }

    fn add(&mut self, other: &Counts) {
        self.expected += other.expected;
        self.detected += other.detected;
        self.correct += other.correct;
        self.covered += other.covered;
    }
}

/// Diferencias entre lo esperado y lo detectado en un texto.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    /// Esperadas sin una detección exacta.
    pub missed: Vec<Span>,
    /// Detecciones que no coinciden con ninguna esperada.
    pub spurious: Vec<Span>,
}

/// Recuentos acumulados por tipo de dato.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Evaluation {
    by_kind: BTreeMap<String, Counts>,
}

impl Evaluation {
    /// Compara las entidades de un texto y acumula los recuentos.
    pub fn add(&mut self, text: &str, expected: &[Span], detected: &[Span]) -> Comparison {
        let chars: Vec<char> = text.chars().collect();
        let mut hidden = vec![false; chars.len()];
        for d in detected {
            hidden[d.start..d.end].fill(true);
        }

        let mut matched = vec![false; expected.len()];
        let mut comparison = Comparison::default();
        for d in detected {
            self.counts(&d.kind).detected += 1;
            let hit = (0..expected.len()).find(|&i| !matched[i] && expected[i] == *d);
            match hit {
                Some(i) => {
                    matched[i] = true;
                    self.counts(&d.kind).correct += 1;
                }
                None => comparison.spurious.push(d.clone()),
            }
        }
        for (e, matched) in expected.iter().zip(matched) {
            let counts = self.counts(&e.kind);
            counts.expected += 1;
            if (e.start..e.end).all(|i| hidden[i] || chars[i].is_whitespace()) {
                counts.covered += 1;
            }
            if !matched {
                comparison.missed.push(e.clone());
            }
        }
        comparison
    }

    fn counts(&mut self, kind: &str) -> &mut Counts {
        self.by_kind.entry(kind.to_owned()).or_default()
    }

    /// Recuentos de cada tipo que ha aparecido, esperado o detectado.
    pub fn by_kind(&self) -> &BTreeMap<String, Counts> {
        &self.by_kind
    }

    /// Suma de todos los tipos.
    pub fn total(&self) -> Counts {
        let mut total = Counts::default();
        for counts in self.by_kind.values() {
            total.add(counts);
        }
        total
    }
}
