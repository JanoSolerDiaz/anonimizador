//! MD y TXT. Los segmentos son los párrafos (separados por líneas en
//! blanco) y el escritor cambia solo los caracteres sustituidos: los finales
//! de línea, los espacios y la marca de orden de bytes salen igual.
//!
//! Solo se admite UTF-8: con otra codificación, reescribir el archivo
//! estropearía los caracteres que no se entienden.

use super::{Replacement, order_for_writing};
use crate::motor::texto::split_paragraphs;
use crate::{Error, Result};

/// Marca de orden de bytes que algunos editores de Windows ponen al principio.
const BOM: char = '\u{FEFF}';

/// Archivo de texto ya leído.
#[derive(Debug, Clone)]
pub struct TextDocument {
    text: String,
    /// Párrafos, como bytes de inicio y fin dentro de `text`.
    paragraphs: Vec<(usize, usize)>,
}

impl TextDocument {
    pub fn from_bytes(bytes: &[u8]) -> Result<TextDocument> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::TextNotUtf8)?;
        // La marca no es un espacio: sin quitarla, el modelo la vería pegada
        // a la primera palabra.
        let skip = if text.starts_with(BOM) {
            BOM.len_utf8()
        } else {
            0
        };
        let body = &text[skip..];
        let paragraphs = split_paragraphs(body)
            .into_iter()
            .map(|p| {
                // Cada párrafo es un trozo de `body`: su inicio es la distancia
                // entre los dos punteros.
                let from = skip + (p.text.as_ptr() as usize - body.as_ptr() as usize);
                (from, from + p.text.len())
            })
            .collect();
        Ok(TextDocument {
            text: text.to_owned(),
            paragraphs,
        })
    }

    /// Texto de cada párrafo, en orden, para el motor.
    pub fn segments(&self) -> Vec<&str> {
        self.paragraphs
            .iter()
            .map(|&(from, to)| &self.text[from..to])
            .collect()
    }

    /// El archivo con las sustituciones aplicadas.
    pub fn write(&self, replacements: &[Replacement]) -> Result<Vec<u8>> {
        let segments = self.segments();
        let lengths: Vec<usize> = segments.iter().map(|s| s.chars().count()).collect();
        let mut out = self.text.clone();
        for r in order_for_writing(replacements, &lengths)? {
            let (from, _) = self.paragraphs[r.segment];
            let segment = segments[r.segment];
            let start = from + byte_offset(segment, r.start);
            let end = from + byte_offset(segment, r.end);
            out.replace_range(start..end, &r.text);
        }
        Ok(out.into_bytes())
    }
}

/// Byte en el que empieza el carácter `position` de `text`, o su longitud.
fn byte_offset(text: &str, position: usize) -> usize {
    text.char_indices()
        .nth(position)
        .map_or(text.len(), |(byte, _)| byte)
}
