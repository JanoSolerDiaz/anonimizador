//! Un extractor y un escritor por formato. El extractor devuelve el texto y un
//! mapa de dónde viene cada carácter; el escritor aplica los cambios sobre el
//! archivo original usando ese mapa.
//!
//! Anonimizar y desanonimizar producen lo mismo: una lista de
//! [`Replacement`]. Los escritores solo saben aplicar sustituciones.

pub mod docx;
pub mod ooxml;
pub mod pdf;
pub mod salida;
pub mod texto;

use crate::{Error, Result};

/// Cambiar los caracteres `start..end` del segmento `segment` por `text`.
/// Las posiciones son caracteres dentro del segmento, con `end` excluido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub segment: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Comprueba las sustituciones y las ordena de la última a la primera: así,
/// al aplicarlas una a una, las posiciones de las que faltan siguen valiendo.
/// `segment_lengths` es la longitud en caracteres de cada segmento. Da error
/// si una sustitución está vacía, se sale de su segmento o se solapa con otra.
pub fn order_for_writing<'a>(
    replacements: &'a [Replacement],
    segment_lengths: &[usize],
) -> Result<Vec<&'a Replacement>> {
    for r in replacements {
        let Some(&length) = segment_lengths.get(r.segment) else {
            return Err(Error::InvalidReplacement(format!(
                "el segmento {} no existe",
                r.segment
            )));
        };
        if r.start >= r.end || r.end > length {
            return Err(Error::InvalidReplacement(format!(
                "la posición {}..{} no vale en el segmento {} de {} caracteres",
                r.start, r.end, r.segment, length
            )));
        }
    }
    let mut ordered: Vec<&Replacement> = replacements.iter().collect();
    ordered.sort_by_key(|r| std::cmp::Reverse((r.segment, r.start)));
    // Ordenadas al revés: la siguiente de la lista va antes en el texto.
    for pair in ordered.windows(2) {
        let (later, earlier) = (pair[0], pair[1]);
        if later.segment == earlier.segment && earlier.end > later.start {
            return Err(Error::InvalidReplacement(format!(
                "dos sustituciones se solapan en el segmento {}",
                later.segment
            )));
        }
    }
    Ok(ordered)
}
