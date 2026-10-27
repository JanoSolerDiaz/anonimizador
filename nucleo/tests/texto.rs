//! Segmentación en palabras y asignación de tokens a palabras.
//!
//! Las palabras se definen igual que en la tarjeta del modelo v2: se parte por
//! espacios y por `, ; | " < > ( ) [ ] { } * ^`. No dependen del modelo.
//! Todas las posiciones están en caracteres, con el final excluido.

use nucleo::motor::texto::{Word, split_words};
use nucleo::motor::tokenizacion::{Token, assign_words};

fn spans(text: &str) -> Vec<(usize, usize)> {
    split_words(text).iter().map(|w| (w.start, w.end)).collect()
}

fn words_of(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    split_words(text)
        .iter()
        .map(|w| chars[w.start..w.end].iter().collect())
        .collect()
}

#[test]
fn splits_on_whitespace() {
    assert_eq!(spans("Juan Pérez"), [(0, 4), (5, 10)]);
    assert_eq!(
        words_of("  Juan \t Pérez\n\nMadrid  "),
        ["Juan", "Pérez", "Madrid"]
    );
}

#[test]
fn non_breaking_space_also_separates_words() {
    assert_eq!(words_of("Juan\u{a0}Pérez"), ["Juan", "Pérez"]);
}

#[test]
fn splits_on_every_delimiter_of_the_model_card() {
    assert_eq!(
        words_of(r#"a,b;c|d"e<f>g(h)i[j]k{l}m*n^o"#),
        [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o"
        ]
    );
}

#[test]
fn keeps_dots_hyphens_at_signs_slashes_and_apostrophes_inside_words() {
    assert_eq!(
        words_of("ana@ejemplo.es 12.345.678-Z C/Mayor l'Hospitalet l·l"),
        [
            "ana@ejemplo.es",
            "12.345.678-Z",
            "C/Mayor",
            "l'Hospitalet",
            "l·l"
        ]
    );
}

#[test]
fn edge_punctuation_that_is_not_a_delimiter_stays_in_the_word() {
    // El recorte de puntuación en los bordes se hace al decodificar entidades.
    assert_eq!(words_of("Vive en Madrid."), ["Vive", "en", "Madrid."]);
}

#[test]
fn offsets_are_in_characters_not_bytes() {
    assert_eq!(spans("ñ 😀 é"), [(0, 1), (2, 3), (4, 5)]);
    assert_eq!(spans("(Muñoz)"), [(1, 6)]);
}

#[test]
fn empty_or_blank_text_has_no_words() {
    assert!(split_words("").is_empty());
    assert!(split_words(" \t\n ").is_empty());
    assert!(split_words(" ( ) , ").is_empty());
}

fn token(start: usize, end: usize) -> Token {
    Token { id: 0, start, end }
}

fn word(start: usize, end: usize) -> Word {
    Word { start, end }
}

#[test]
fn a_token_belongs_to_the_word_it_overlaps() {
    // «Juan Pérez»: v1 da «Juan» [0,4) y v2 da « Pérez» [4,10), con el espacio.
    let words = [word(0, 4), word(5, 10)];
    let tokens = [token(0, 2), token(2, 4), token(4, 10)];
    assert_eq!(assign_words(&tokens, &words), [Some(0), Some(0), Some(1)]);
}

#[test]
fn tokens_with_only_spaces_or_delimiters_belong_to_no_word() {
    // «a  (b)»: palabras «a» y «b»; tokens «a», « », « (», «b» y «)».
    let words = [word(0, 1), word(4, 5)];
    let tokens = [
        token(0, 1),
        token(1, 2),
        token(2, 4),
        token(4, 5),
        token(5, 6),
    ];
    assert_eq!(
        assign_words(&tokens, &words),
        [Some(0), None, None, Some(1), None]
    );
}

#[test]
fn a_token_spanning_two_words_belongs_to_the_first_one() {
    let words = [word(0, 1), word(2, 3)];
    let tokens = [token(0, 3)];
    assert_eq!(assign_words(&tokens, &words), [Some(0)]);
}

#[test]
fn no_words_means_no_assignments() {
    assert_eq!(assign_words(&[token(0, 1)], &[]), [None]);
    assert!(assign_words(&[], &[word(0, 1)]).is_empty());
}

// ---------------------------------------------------------------- párrafos

use nucleo::motor::texto::{Paragraph, split_paragraphs};

fn paragraphs(text: &str) -> Vec<(usize, &str)> {
    split_paragraphs(text)
        .into_iter()
        .map(|Paragraph { start, text }| (start, text))
        .collect()
}

#[test]
fn paragraphs_are_separated_by_blank_lines() {
    assert_eq!(paragraphs("A\nB\n\nC"), [(0, "A\nB"), (5, "C")]);
    assert_eq!(paragraphs("A\n \t \nC"), [(0, "A"), (6, "C")]);
}

#[test]
fn windows_line_endings_are_not_part_of_the_paragraph() {
    assert_eq!(paragraphs("A\r\nB\r\n\r\nC\r\n"), [(0, "A\r\nB"), (8, "C")]);
}

#[test]
fn leading_and_trailing_blank_lines_are_ignored() {
    assert_eq!(paragraphs("\n\n  \nA\n\n\n"), [(5, "A")]);
    assert!(paragraphs("").is_empty());
    assert!(paragraphs("\n \n\t\n").is_empty());
}

#[test]
fn paragraph_starts_are_in_characters() {
    assert_eq!(paragraphs("ñú\n\né😀"), [(0, "ñú"), (4, "é😀")]);
}

#[test]
fn a_comma_between_digits_does_not_split_the_word() {
    // Coma decimal y separador de miles: «189,90 €», «£1,100».
    assert_eq!(
        words_of("Total: 189,90 € y £1,100, más 3, 4 y 5,a"),
        [
            "Total:", "189,90", "€", "y", "£1,100", "más", "3", "4", "y", "5", "a"
        ]
    );
    assert_eq!(words_of(",5 y 5,"), ["5", "y", "5"]);
}
