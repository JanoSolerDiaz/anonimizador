//! Marcadores: qué texto sustituye a cada dato y cómo se reconoce que dos
//! apariciones son el mismo dato.
//!
//! La clave de un dato es su valor normalizado, sin el tipo. Así, si el modelo
//! clasifica «Valencia» como lugar en un sitio y como organización en otro,
//! las dos apariciones comparten marcador y la restauración no es ambigua.
//!
//! En el Hito 2 el registro vive en memoria. En el Hito 3 se guardará en el
//! almacén cifrado, que tiene los mismos campos.

use std::collections::{BTreeMap, HashMap};

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Forma de comparar valores: minúsculas, sin acentos y con los espacios
/// colapsados y recortados. «JUAN PÉREZ» y «Juan Perez» dan «juan perez».
pub fn normalize(value: &str) -> String {
    let without_accents: String = value
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase();
    without_accents
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Texto del marcador, con cuatro dígitos como mínimo: `[EMAIL_0007]`.
pub fn format_marker(kind: &str, number: u32) -> String {
    format!("[{kind}_{number:04}]")
}

/// Un dato con su marcador.
#[derive(Debug, Clone)]
pub struct MarkerEntry {
    marker: String,
    kind: String,
    value: String,
    normalized_key: String,
    variants: Vec<String>,
}

impl MarkerEntry {
    pub fn marker(&self) -> &str {
        &self.marker
    }

    /// Tipo con el que se creó el marcador.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Forma de la primera aparición; es la que se restaura.
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn normalized_key(&self) -> &str {
        &self.normalized_key
    }

    /// Formas literales distintas encontradas, en orden de aparición.
    pub fn variants(&self) -> &[String] {
        &self.variants
    }
}

/// Registro de marcadores con un contador por tipo.
#[derive(Debug, Default)]
pub struct MarkerRegistry {
    counters: BTreeMap<String, u32>,
    entries: Vec<MarkerEntry>,
    by_key: HashMap<String, usize>,
    by_marker: HashMap<String, usize>,
}

impl MarkerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Devuelve el marcador del valor. Si el valor normalizado ya tiene uno,
    /// lo reutiliza aunque `kind` sea otro; si no, crea el siguiente de `kind`.
    /// `kind` debe ser un marcador de la tabla de tipos.
    pub fn assign(&mut self, kind: &str, literal: &str) -> &str {
        let key = normalize(literal);
        let index = match self.by_key.get(&key) {
            Some(&index) => {
                let entry = &mut self.entries[index];
                if !entry.variants.iter().any(|v| v == literal) {
                    entry.variants.push(literal.to_owned());
                }
                index
            }
            None => {
                let counter = self.counters.entry(kind.to_owned()).or_insert(0);
                *counter += 1;
                let marker = format_marker(kind, *counter);
                let index = self.entries.len();
                self.by_key.insert(key.clone(), index);
                self.by_marker.insert(marker.clone(), index);
                self.entries.push(MarkerEntry {
                    marker,
                    kind: kind.to_owned(),
                    value: literal.to_owned(),
                    normalized_key: key,
                    variants: vec![literal.to_owned()],
                });
                index
            }
        };
        &self.entries[index].marker
    }

    /// Último número usado para el tipo, o 0 si aún no hay ninguno.
    pub fn counter(&self, kind: &str) -> u32 {
        self.counters.get(kind).copied().unwrap_or(0)
    }

    /// Entradas en orden de creación.
    pub fn entries(&self) -> impl Iterator<Item = &MarkerEntry> {
        self.entries.iter()
    }

    pub fn entry_for_marker(&self, marker: &str) -> Option<&MarkerEntry> {
        self.by_marker
            .get(marker)
            .map(|&index| &self.entries[index])
    }
}
