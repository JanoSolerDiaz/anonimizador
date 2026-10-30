//! Búsqueda tolerante de marcadores.

/// Marcador encontrado en un texto. `start` y `end` son caracteres, con
/// `end` excluido; `key` es la forma canónica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMarker {
    pub start: usize,
    pub end: usize,
    pub key: String,
    pub short: bool,
}

pub fn find_markers(_text: &str) -> Vec<FoundMarker> {
    Vec::new()
}
