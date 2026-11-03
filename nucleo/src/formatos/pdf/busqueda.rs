//! Búsqueda de un valor en el texto de una página. La usan la sustitución
//! literal y la verificación obligatoria, con la misma regla para que una
//! salida que pasó la primera pase la segunda.
//!
//! Se comparan sin mayúsculas, acentos ni ligaduras («ﬁ» es «fi»), y cualquier serie de espacios o
//! saltos vale lo mismo: «Marta\nLozano» es «Marta Lozano». Un valor corto
//! (menos de [`WHOLE_WORD_BELOW`] caracteres) solo cuenta como palabra
//! entera: «Ana» no está en «Mariana». Uno más largo cuenta también dentro de
//! otra palabra, como un DNI pegado a su etiqueta («DNI12345678Z»).

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Por debajo de esta longitud, el valor solo se busca como palabra entera.
pub const WHOLE_WORD_BELOW: usize = 6;

/// Posiciones (en caracteres de `text`, con el final excluido) donde aparece
/// `value`, sin solaparse.
pub fn find_value(text: &str, value: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = fold(value).into_iter().map(|(c, _)| c).collect();
    let needle = trim(&needle);
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = fold(text);
    let whole_word = needle.len() < WHOLE_WORD_BELOW;
    let mut found = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let matches = hay[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|((c, _), n)| c == n);
        let end = i + needle.len();
        let bounded = !whole_word
            || ((i == 0 || !hay[i - 1].0.is_alphanumeric())
                && (end == hay.len() || !hay[end].0.is_alphanumeric()));
        if matches && bounded {
            let start = hay[i].1;
            let last = hay[end - 1];
            // El último carácter plegado viene de `last.1`; el final es el
            // siguiente carácter del original.
            found.push((start, last.1 + 1));
            i = end;
        } else {
            i += 1;
        }
    }
    found
}

/// Texto plegado: cada carácter en minúscula y sin acentos, con su
/// posición en el original. Las series de espacios quedan en uno.
fn fold(text: &str) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for (index, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            if out.last().is_none_or(|(last, _)| *last != ' ') {
                out.push((' ', index));
            }
            continue;
        }
        for folded in c.nfkd().filter(|c| !is_combining_mark(*c)) {
            for lower in folded.to_lowercase() {
                out.push((lower, index));
            }
        }
    }
    out
}

fn trim(chars: &[char]) -> &[char] {
    let start = chars.iter().position(|c| *c != ' ').unwrap_or(chars.len());
    let end = chars
        .iter()
        .rposition(|c| *c != ' ')
        .map_or(start, |e| e + 1);
    &chars[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_case_accents_and_line_breaks() {
        assert_eq!(
            find_value("Firma: MARTA\nLOZANO.", "Marta Lozano"),
            vec![(7, 19)]
        );
        assert_eq!(find_value("de Gonzalez", "González"), vec![(3, 11)]);
    }

    #[test]
    fn short_values_only_match_whole_words() {
        assert!(find_value("Mariana firmó", "Ana").is_empty());
        assert_eq!(find_value("Firmó Ana.", "Ana"), vec![(6, 9)]);
    }

    #[test]
    fn long_values_match_inside_a_word() {
        assert_eq!(find_value("DNI12345678Z", "12345678Z"), vec![(3, 12)]);
    }

    #[test]
    fn every_occurrence_is_found() {
        assert_eq!(find_value("Ana y Ana", "Ana"), vec![(0, 3), (6, 9)]);
        assert!(find_value("texto", "  ").is_empty());
    }
}
