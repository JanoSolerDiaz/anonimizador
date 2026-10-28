//! Marcadores: qué texto sustituye a cada dato y cómo se reconoce que dos
//! apariciones son el mismo dato.
//!
//! La clave de un dato es su valor normalizado, sin el tipo. Así, si el modelo
//! clasifica «Valencia» como lugar en un sitio y como organización en otro,
//! las dos apariciones comparten marcador y la restauración no es ambigua.
//!
//! Desde el hito 3, los marcadores los asigna el almacén (`crate::almacen`).

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
