//! Segmentación del texto en palabras, con posiciones en caracteres.
//!
//! Parte de la definición de palabra de la tarjeta del modelo v2 y no
//! depende del tokenizador: se parte por espacios y por
//! `, ; | " < > ( ) [ ] { } * ^`, salvo la coma entre dos cifras, que es
//! decimal o de miles. La puntuación de los bordes que no está en
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
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut start = None;
    for (position, &c) in chars.iter().enumerate() {
        // Una coma entre dos cifras es decimal o de miles: «189,90», «1,100».
        let digit_comma = c == ','
            && position > 0
            && chars[position - 1].is_ascii_digit()
            && chars.get(position + 1).is_some_and(char::is_ascii_digit);
        match (is_separator(c) && !digit_comma, start) {
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
    }
    if let Some(s) = start {
        words.push(Word {
            start: s,
            end: chars.len(),
        });
    }
    words
}

/// Párrafo de un archivo de texto. `start` es la posición en caracteres de su
/// primer carácter dentro del archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paragraph<'a> {
    pub start: usize,
    pub text: &'a str,
}

/// Divide un texto en párrafos separados por líneas en blanco (vacías o solo
/// con espacios). El salto de línea final de cada párrafo, `\n` o `\r\n`, no
/// forma parte de él.
pub fn split_paragraphs(text: &str) -> Vec<Paragraph<'_>> {
    let mut paragraphs = Vec::new();
    // (carácter de inicio, byte de inicio, byte de fin) del párrafo abierto.
    let mut open: Option<(usize, usize, usize)> = None;
    let (mut byte, mut position) = (0, 0);
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if content.trim().is_empty() {
            if let Some((start, from, to)) = open.take() {
                paragraphs.push(Paragraph {
                    start,
                    text: &text[from..to],
                });
            }
        } else {
            let end = byte + content.len();
            match open.as_mut() {
                Some(current) => current.2 = end,
                None => open = Some((position, byte, end)),
            }
        }
        byte += line.len();
        position += line.chars().count();
    }
    if let Some((start, from, to)) = open {
        paragraphs.push(Paragraph {
            start,
            text: &text[from..to],
        });
    }
    paragraphs
}
