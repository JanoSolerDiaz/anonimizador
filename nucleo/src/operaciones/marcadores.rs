//! Búsqueda tolerante de marcadores, escrita a mano sin `regex` (PRD,
//! «Desanonimización»). Un modelo de lenguaje puede haber cambiado un poco
//! el marcador, así que se aceptan mayúsculas o minúsculas, espacios dentro
//! de los corchetes y menos ceros a la izquierda: `[ nombre_persona_42 ]` es
//! `[NOMBRE_PERSONA_0042]`. También se reconocen las formas cortas de los
//! PDF, como `[P42]`.

use crate::motor::marcadores::format_marker;

/// Caracteres que se miran dentro de los corchetes antes de rendirse.
const MAX_NAME_CHARS: usize = 64;

/// Marcador encontrado en un texto. `start` y `end` son caracteres, con
/// `end` excluido; `key` es la forma canónica, la que guarda el almacén.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMarker {
    pub start: usize,
    pub end: usize,
    pub key: String,
    /// Forma corta (`[P42]`) en lugar del marcador completo.
    pub short: bool,
}

/// Marcadores de `text`, en orden y sin solaparse.
pub fn find_markers(text: &str) -> Vec<FoundMarker> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(marker) = marker_at(&chars, i)
        {
            i = marker.end;
            found.push(marker);
            continue;
        }
        i += 1;
    }
    found
}

/// Forma canónica de un marcador escrito de cualquier manera, o el texto tal
/// cual si no es un marcador.
pub fn canonical(marker: &str) -> String {
    match find_markers(marker).as_slice() {
        [only] if only.start == 0 && only.end == marker.chars().count() => only.key.clone(),
        _ => marker.to_owned(),
    }
}

/// El marcador que empieza en el corchete `chars[start]`, si lo hay.
fn marker_at(chars: &[char], start: usize) -> Option<FoundMarker> {
    let mut j = start + 1;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    let name_start = j;
    while j < chars.len()
        && j - name_start < MAX_NAME_CHARS
        && (chars[j].is_ascii_alphanumeric() || chars[j] == '_')
    {
        j += 1;
    }
    let name: String = chars[name_start..j].iter().collect();
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if chars.get(j) != Some(&']') {
        return None;
    }

    let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let (prefix, number) = name.split_at(name.len() - digits);
    let number: u32 = number.parse().ok()?;
    let prefix = prefix.to_ascii_uppercase();
    let (key, short) = match prefix.strip_suffix('_') {
        Some(kind) if is_kind(kind) => (format_marker(kind, number), false),
        Some(_) => return None,
        None if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphabetic()) => {
            (format!("[{prefix}{number}]"), true)
        }
        None => return None,
    };
    Some(FoundMarker {
        start,
        end: j + 1,
        key,
        short,
    })
}

/// Letras y guiones bajos, empezando y acabando en letra: `NOMBRE_PERSONA`.
fn is_kind(kind: &str) -> bool {
    kind.starts_with(|c: char| c.is_ascii_alphabetic())
        && kind.ends_with(|c: char| c.is_ascii_alphabetic())
        && kind.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
}
