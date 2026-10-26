//! Segmentación del texto en palabras, con posiciones en caracteres.
//!
//! Es la misma definición de palabra que usa la tarjeta del modelo v2 y no
//! depende del tokenizador: se parte por espacios y por
//! `, ; | " < > ( ) [ ] { } * ^`. La puntuación de los bordes que no está en
//! esa lista (como el punto final) sigue dentro de la palabra; se recorta al
//! decodificar las entidades.

/// Palabra del texto. `start` y `end` son posiciones en caracteres, con `end`
/// excluido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word {
    pub start: usize,
    pub end: usize,
}

const DELIMITERS: &str = ",;|\"<>()[]{}*^";

fn is_separator(c: char) -> bool {
    c.is_whitespace() || DELIMITERS.contains(c)
}

pub fn split_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start = None;
    let mut position = 0;
    for c in text.chars() {
        match (is_separator(c), start) {
            (true, Some(s)) => {
                words.push(Word {
                    start: s,
                    end: position,
                });
                start = None;
            }
            (false, None) => start = Some(position),
            _ => {}
        }
        position += 1;
    }
    if let Some(s) = start {
        words.push(Word {
            start: s,
            end: position,
        });
    }
    words
}
