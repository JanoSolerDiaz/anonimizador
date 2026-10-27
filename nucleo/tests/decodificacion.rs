//! Decodificación: de las probabilidades de cada token a entidades con su
//! posición en caracteres.
//!
//! Reglas del PRD: softmax; esquema BIO, donde un `I-` sin `B-` previo del
//! mismo tipo abre una entidad nueva; la palabra toma la etiqueta de su primer
//! subtoken; se descartan las entidades con puntuación media por debajo del
//! umbral (0,5 por defecto); se quitan espacios y puntuación de los bordes.
//! Además, la estrategia de la tarjeta de v2: media de los subtokens de cada
//! palabra, sumando B- e I- de cada tipo.

use nucleo::Error;
use nucleo::motor::decodificacion::{
    DecodeOptions, Detection, Labels, WordStrategy, decode, softmax_rows,
};
use nucleo::motor::texto::split_words;
use nucleo::motor::tokenizacion::Token;

const O: usize = 0;
const B_PER: usize = 1;
const I_PER: usize = 2;
const B_LOC: usize = 3;
const I_LOC: usize = 4;

fn labels() -> Labels {
    let names = [
        "O",
        "B-PERSON_NAME",
        "I-PERSON_NAME",
        "B-LOCATION",
        "I-LOCATION",
    ];
    Labels::parse(&names.map(String::from)).unwrap()
}

/// Fila de probabilidades: `p` para la etiqueta indicada y el resto repartido.
fn row(label: usize, p: f32) -> Vec<f32> {
    let rest = (1.0 - p) / 4.0;
    (0..5).map(|i| if i == label { p } else { rest }).collect()
}

/// Un token por palabra.
fn word_tokens(text: &str) -> Vec<Token> {
    split_words(text)
        .iter()
        .map(|w| Token {
            id: 0,
            start: w.start,
            end: w.end,
        })
        .collect()
}

fn options(strategy: WordStrategy) -> DecodeOptions {
    DecodeOptions {
        strategy,
        ..DecodeOptions::default()
    }
}

fn run(text: &str, tokens: &[Token], rows: &[Vec<f32>], strategy: WordStrategy) -> Vec<Detection> {
    let probs: Vec<f32> = rows.concat();
    decode(text, tokens, &probs, &labels(), &options(strategy))
}

/// (texto, etiqueta) de cada entidad.
fn found(text: &str, detections: &[Detection]) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    detections
        .iter()
        .map(|d| (chars[d.start..d.end].iter().collect(), d.label.clone()))
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(t, l)| (t.to_string(), l.to_string()))
        .collect()
}

const BOTH: [WordStrategy; 2] = [WordStrategy::FirstSubtoken, WordStrategy::WordMean];

// ---------------------------------------------------------------- softmax

#[test]
fn softmax_rows_sum_to_one_and_keep_the_order() {
    let mut logits = vec![1.0, 2.0, 3.0, 0.0, 0.0, 0.0];
    softmax_rows(&mut logits, 3);
    let (a, b) = logits.split_at(3);
    assert!((a.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    assert!(a[2] > a[1] && a[1] > a[0]);
    assert!(b.iter().all(|&p| (p - 1.0 / 3.0).abs() < 1e-6));
}

#[test]
fn softmax_is_stable_with_large_logits() {
    let mut logits = vec![1000.0, 1001.0];
    softmax_rows(&mut logits, 2);
    assert!(logits.iter().all(|p| p.is_finite()));
    assert!((logits[1] - 0.7310586).abs() < 1e-5);
}

// ---------------------------------------------------------------- etiquetas

#[test]
fn labels_without_bio_prefix_are_rejected() {
    let names = ["O", "PERSON_NAME"].map(String::from);
    assert!(matches!(
        Labels::parse(&names),
        Err(Error::InvalidModelConfig(_))
    ));
}

// ---------------------------------------------------------------- BIO

#[test]
fn begin_and_inside_build_entities() {
    let text = "Juan Pérez vive en Madrid";
    let rows = [
        row(B_PER, 0.9),
        row(I_PER, 0.8),
        row(O, 0.9),
        row(O, 0.9),
        row(B_LOC, 0.7),
    ];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Juan Pérez", "PERSON_NAME"), ("Madrid", "LOCATION")]),
            "{strategy:?}"
        );
        assert_eq!((d[0].start, d[0].end), (0, 10));
        assert_eq!((d[1].start, d[1].end), (19, 25));
    }
}

#[test]
fn inside_without_begin_opens_an_entity() {
    let text = "vive Ana";
    let rows = [row(O, 0.9), row(I_PER, 0.9)];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Ana", "PERSON_NAME")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn inside_of_another_type_opens_a_new_entity() {
    let text = "Ana Madrid";
    let rows = [row(B_PER, 0.9), row(I_LOC, 0.9)];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Ana", "PERSON_NAME"), ("Madrid", "LOCATION")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn two_begins_in_a_row_are_two_entities() {
    let text = "Ana Luis";
    let rows = [row(B_PER, 0.9), row(B_PER, 0.9)];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Ana", "PERSON_NAME"), ("Luis", "PERSON_NAME")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn an_entity_keeps_the_text_between_its_words() {
    let text = "en Calle Mayor, 5 vive";
    let rows = [
        row(O, 0.9),
        row(B_LOC, 0.9),
        row(I_LOC, 0.9),
        row(I_LOC, 0.9),
        row(O, 0.9),
    ];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Calle Mayor, 5", "LOCATION")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn tokens_outside_words_are_ignored() {
    // «Ana  Gil»: el token del doble espacio dice B-LOCATION, pero no es de ninguna palabra.
    let text = "Ana  Gil";
    let tokens = [
        Token {
            id: 0,
            start: 0,
            end: 3,
        },
        Token {
            id: 0,
            start: 3,
            end: 5,
        },
        Token {
            id: 0,
            start: 5,
            end: 8,
        },
    ];
    let rows = [row(B_PER, 0.9), row(B_LOC, 0.9), row(I_PER, 0.9)];
    for strategy in BOTH {
        let d = run(text, &tokens, &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Ana  Gil", "PERSON_NAME")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn offsets_are_in_characters() {
    let text = "😀 ñú Ana";
    let rows = [row(O, 0.9), row(O, 0.9), row(B_PER, 0.9)];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!((d[0].start, d[0].end), (5, 8), "{strategy:?}");
    }
}

#[test]
fn empty_input_gives_no_entities() {
    for strategy in BOTH {
        assert!(run("", &[], &[], strategy).is_empty());
        assert!(run("   ", &[], &[], strategy).is_empty());
    }
}

// ---------------------------------------------------------------- bordes

#[test]
fn spaces_and_punctuation_are_trimmed_from_the_edges() {
    let text = "¿«Ana» vive en Madrid.";
    let rows = [row(B_PER, 0.9), row(O, 0.9), row(O, 0.9), row(B_LOC, 0.9)];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[("Ana", "PERSON_NAME"), ("Madrid", "LOCATION")]),
            "{strategy:?}"
        );
    }
}

#[test]
fn symbols_that_belong_to_the_data_are_kept() {
    let text = "+34 612 llama a @ana_gil 100€";
    let rows = [
        row(B_PER, 0.9),
        row(I_PER, 0.9),
        row(O, 0.9),
        row(O, 0.9),
        row(B_LOC, 0.9),
        row(B_PER, 0.9),
    ];
    for strategy in BOTH {
        let d = run(text, &word_tokens(text), &rows, strategy);
        assert_eq!(
            found(text, &d),
            pairs(&[
                ("+34 612", "PERSON_NAME"),
                ("@ana_gil", "LOCATION"),
                ("100€", "PERSON_NAME")
            ]),
            "{strategy:?}"
        );
    }
}

#[test]
fn an_entity_of_only_punctuation_is_dropped() {
    let text = "Ana ... Gil";
    let rows = [row(O, 0.9), row(B_PER, 0.9), row(O, 0.9)];
    for strategy in BOTH {
        assert!(
            run(text, &word_tokens(text), &rows, strategy).is_empty(),
            "{strategy:?}"
        );
    }
}

// ---------------------------------------------------------------- umbral

#[test]
fn entities_below_the_threshold_are_dropped() {
    let text = "Ana Gil Luis";
    // Media de «Ana Gil»: (0,6 + 0,3) / 2 = 0,45. «Luis»: 0,6.
    let rows = [row(B_PER, 0.6), row(I_PER, 0.3), row(B_PER, 0.6)];
    let d = run(text, &word_tokens(text), &rows, WordStrategy::FirstSubtoken);
    assert_eq!(found(text, &d), pairs(&[("Luis", "PERSON_NAME")]));
    assert!((d[0].score - 0.6).abs() < 1e-6);
}

#[test]
fn the_threshold_is_configurable() {
    let text = "Ana";
    let probs = row(B_PER, 0.4);
    let low = DecodeOptions {
        threshold: 0.3,
        ..options(WordStrategy::FirstSubtoken)
    };
    let high = DecodeOptions {
        threshold: 0.5,
        ..low
    };
    assert_eq!(
        decode(text, &word_tokens(text), &probs, &labels(), &low).len(),
        1
    );
    assert!(decode(text, &word_tokens(text), &probs, &labels(), &high).is_empty());
}

#[test]
fn default_options() {
    let defaults = DecodeOptions::default();
    assert_eq!(defaults.threshold, 0.5);
    assert_eq!(defaults.strategy, WordStrategy::FirstSubtoken);
}

// ---------------------------------------------------------------- subtokens

fn two_subtokens() -> (&'static str, Vec<Token>) {
    // «Pérez» partido en «Pé» y «rez».
    (
        "Pérez",
        vec![
            Token {
                id: 0,
                start: 0,
                end: 2,
            },
            Token {
                id: 0,
                start: 2,
                end: 5,
            },
        ],
    )
}

#[test]
fn first_subtoken_decides_the_whole_word() {
    let (text, tokens) = two_subtokens();
    let d = run(
        text,
        &tokens,
        &[row(B_PER, 0.9), row(O, 0.9)],
        WordStrategy::FirstSubtoken,
    );
    assert_eq!(found(text, &d), pairs(&[("Pérez", "PERSON_NAME")]));

    let d = run(
        text,
        &tokens,
        &[row(O, 0.9), row(B_PER, 0.9)],
        WordStrategy::FirstSubtoken,
    );
    assert!(d.is_empty());
}

#[test]
fn word_mean_pools_begin_and_inside_across_subtokens() {
    let (text, tokens) = two_subtokens();
    // Primer subtoken: gana O (0,55 frente a 0,45 de PERSONA). Media de la
    // palabra: O (0,55 + 0,25) / 2 = 0,4; PERSONA (0,45 + 0,75) / 2 = 0,6.
    let rows = [
        vec![0.55, 0.25, 0.2, 0.0, 0.0],
        vec![0.25, 0.05, 0.7, 0.0, 0.0],
    ];
    assert!(run(text, &tokens, &rows, WordStrategy::FirstSubtoken).is_empty());
    let d = run(text, &tokens, &rows, WordStrategy::WordMean);
    assert_eq!(found(text, &d), pairs(&[("Pérez", "PERSON_NAME")]));
    assert!((d[0].score - 0.6).abs() < 1e-6);
}

#[test]
fn word_mean_splits_neighbours_when_the_first_subtoken_looks_like_a_begin() {
    let text = "Ana Luis";
    // En las dos, la palabra «Luis» es PERSONA con 0,9. Lo que cambia es la
    // probabilidad de B- en su primer subtoken: con 0,5 o más empieza otra
    // persona; con menos, sigue la anterior.
    let begins = [vec![0.1, 0.8, 0.1, 0.0, 0.0], vec![0.1, 0.6, 0.3, 0.0, 0.0]];
    let d = run(text, &word_tokens(text), &begins, WordStrategy::WordMean);
    assert_eq!(
        found(text, &d),
        pairs(&[("Ana", "PERSON_NAME"), ("Luis", "PERSON_NAME")])
    );

    let continues = [vec![0.1, 0.8, 0.1, 0.0, 0.0], vec![0.1, 0.3, 0.6, 0.0, 0.0]];
    let d = run(text, &word_tokens(text), &continues, WordStrategy::WordMean);
    assert_eq!(found(text, &d), pairs(&[("Ana Luis", "PERSON_NAME")]));
}

#[test]
#[should_panic(expected = "probabilidades")]
fn probabilities_must_match_tokens_and_labels() {
    let text = "Ana";
    decode(
        text,
        &word_tokens(text),
        &[0.5, 0.5],
        &labels(),
        &DecodeOptions::default(),
    );
}

#[test]
fn an_entity_exactly_at_the_threshold_is_kept() {
    // El PRD descarta las que quedan «por debajo» del umbral.
    let text = "Ana";
    let d = run(
        text,
        &word_tokens(text),
        &[row(B_PER, 0.5)],
        WordStrategy::FirstSubtoken,
    );
    assert_eq!(found(text, &d), pairs(&[("Ana", "PERSON_NAME")]));
}

// ------------------------------------------------- media de palabra (v2)
//
// Estas pruebas reproducen el decodificador de referencia de la tarjeta de
// v2 (pii_detector.py, revisión 5e61416).

#[test]
fn word_mean_looks_at_begin_mass_of_the_first_subtoken_only() {
    // «Pérez» en dos subtokens. En la media pesa más B- (0,55 frente a 0,35),
    // pero el primer subtoken solo tiene 0,3 de B-: la palabra sigue a «Ana».
    let text = "Ana Pérez";
    let tokens = [
        Token {
            id: 0,
            start: 0,
            end: 3,
        },
        Token {
            id: 0,
            start: 3,
            end: 6,
        },
        Token {
            id: 0,
            start: 6,
            end: 9,
        },
    ];
    let rows = [
        vec![0.1, 0.9, 0.0, 0.0, 0.0],
        vec![0.1, 0.3, 0.6, 0.0, 0.0],
        vec![0.1, 0.8, 0.1, 0.0, 0.0],
    ];
    let d = run(text, &tokens, &rows, WordStrategy::WordMean);
    assert_eq!(found(text, &d), pairs(&[("Ana Pérez", "PERSON_NAME")]));
}

#[test]
fn word_mean_does_not_merge_across_a_line_break_or_other_separators() {
    let continues = [row(B_PER, 0.9), row(I_PER, 0.9)];
    for text in [
        "Ana
Gil",
        "Ana; Gil",
        "Ana | Gil",
        "Ana: Gil",
    ] {
        let d = run(text, &word_tokens(text), &continues, WordStrategy::WordMean);
        assert_eq!(found(text, &d).len(), 2, "{text:?}: {:?}", found(text, &d));
    }
}

#[test]
fn word_mean_merges_across_spaces_commas_hyphens_slashes_dots_and_parentheses() {
    for text in [
        "Calle Mayor",
        "Mayor, 5",
        "Mayor - 5",
        "Mayor / 5",
        "Mayor (5)",
    ] {
        // El guion y la barra sueltos son palabras: el modelo dice que no son
        // datos, y no impiden unir «Mayor» y «5».
        let words = split_words(text);
        let rows: Vec<Vec<f32>> = (0..words.len())
            .map(|i| match i {
                0 => row(B_LOC, 0.9),
                _ if i == words.len() - 1 => row(I_LOC, 0.9),
                _ => row(O, 0.9),
            })
            .collect();
        let d = run(text, &word_tokens(text), &rows, WordStrategy::WordMean);
        assert_eq!(found(text, &d).len(), 1, "{text:?}: {:?}", found(text, &d));
    }
}

#[test]
fn word_mean_ignores_tokens_that_start_in_trimmed_edge_punctuation() {
    // «Ana.» con un token para «Ana» y otro para «.». El punto se recorta de
    // la palabra, así que su token no cuenta en la media.
    let text = "Ana.";
    let tokens = [
        Token {
            id: 0,
            start: 0,
            end: 3,
        },
        Token {
            id: 0,
            start: 3,
            end: 4,
        },
    ];
    let rows = [row(B_PER, 0.9), row(O, 0.9)];
    let d = run(text, &tokens, &rows, WordStrategy::WordMean);
    assert_eq!(found(text, &d), pairs(&[("Ana", "PERSON_NAME")]));
}

#[test]
fn word_mean_begin_mass_of_exactly_one_half_starts_a_new_entity() {
    // La referencia une solo si la B- del primer subtoken es menor que 0,5.
    let text = "Ana Luis";
    let rows = [vec![0.1, 0.8, 0.1, 0.0, 0.0], vec![0.1, 0.5, 0.4, 0.0, 0.0]];
    let d = run(text, &word_tokens(text), &rows, WordStrategy::WordMean);
    assert_eq!(found(text, &d).len(), 2);
}
