//! Regla de tarjetas de pago: única excepción a «sin reglas deterministas»
//! (decisión de Jano del 2026-10-28). Una secuencia de 13 a 19 cifras, juntas
//! o en grupos separados por un espacio o un guion, que cumple el algoritmo
//! de Luhn es una `TARJETA_BANCARIA`, y gana a lo que diga el modelo.

use nucleo::motor::Entity;
use nucleo::motor::reglas::{apply_card_rule, find_payment_cards, luhn_valid};

// ---------------------------------------------------------------- Luhn

#[test]
fn luhn_accepts_valid_numbers() {
    for digits in [
        "4532015112830366",
        "5425233430109903",
        "79927398713",
        "4222222222222",
    ] {
        assert!(luhn_valid(digits), "{digits}");
    }
}

#[test]
fn luhn_rejects_wrong_check_digits_and_non_digits() {
    for digits in [
        "4532015112830367",
        "79927398710",
        "",
        "4532 0151",
        "45320151128303a6",
    ] {
        assert!(!luhn_valid(digits), "{digits:?}");
    }
}

// ---------------------------------------------------------------- búsqueda

fn cards(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    find_payment_cards(text)
        .into_iter()
        .map(|(start, end)| chars[start..end].iter().collect())
        .collect()
}

#[test]
fn finds_cards_in_groups_with_spaces_or_hyphens_or_together() {
    assert_eq!(
        cards("Tarjeta 4532 0151 1283 0366, CVV 731."),
        ["4532 0151 1283 0366"]
    );
    assert_eq!(
        cards("pago con 4532-0151-1283-0366."),
        ["4532-0151-1283-0366"]
    );
    assert_eq!(cards("4532015112830366"), ["4532015112830366"]);
    assert_eq!(cards("Visa de 13 cifras: 4222222222222"), ["4222222222222"]);
}

#[test]
fn positions_are_in_characters() {
    assert_eq!(
        find_payment_cards("Pagó Núria: 4532015112830366"),
        [(12, 28)]
    );
}

#[test]
fn finds_several_cards() {
    assert_eq!(
        cards("Primera 4532 0151 1283 0366 y segunda 5425 2334 3010 9903."),
        ["4532 0151 1283 0366", "5425 2334 3010 9903"]
    );
}

#[test]
fn ignores_numbers_that_fail_luhn() {
    assert!(cards("Tarjeta 4532 0151 1283 0367.").is_empty());
}

#[test]
fn ignores_numbers_glued_to_letters_or_digits() {
    assert!(cards("ref X4532015112830366").is_empty());
    assert!(cards("ref 4532015112830366X").is_empty());
}

#[test]
fn a_double_separator_breaks_the_number() {
    assert!(cards("4532  0151 1283 0366").is_empty());
    assert!(cards("4532 -0151 1283 0366").is_empty());
}

#[test]
fn ignores_too_short_and_too_long_numbers() {
    // 12 cifras con Luhn válido y 20 cifras (como el final de un IBAN).
    assert!(luhn_valid("123456789015"));
    assert!(cards("número 1234 5678 9015").is_empty());
    assert!(cards("ES91 2100 0418 4502 0005 1332").is_empty());
    assert!(cards("FR76 3000 4008 2000 0123 4567 864").is_empty());
}

#[test]
fn phones_and_ids_are_not_cards() {
    assert!(cards("+34 612 345 678, DNI 48291736Q, 06 12 45 78 90").is_empty());
}

#[test]
fn a_separator_at_the_end_is_not_part_of_the_card() {
    assert_eq!(
        cards("4532 0151 1283 0366 - gracias"),
        ["4532 0151 1283 0366"]
    );
}

// ---------------------------------------------------------------- fusión

const TEXT: &str = "Card number 5369 2714 6382 9011, code 905, for Emily Carter.";
// «5369 2714 6382 9011» va de 12 a 31.

fn entity(segment: usize, start: usize, end: usize, kind: &str, text: &str) -> Entity {
    let chars: Vec<char> = text.chars().collect();
    Entity {
        segment,
        start,
        end,
        kind: kind.to_owned(),
        label: "X".to_owned(),
        text: chars[start..end].iter().collect(),
        score: 0.8,
    }
}

fn summary(entities: &[Entity]) -> Vec<(usize, String, String)> {
    entities
        .iter()
        .map(|e| (e.segment, e.text.clone(), e.kind.clone()))
        .collect()
}

fn s(segment: usize, text: &str, kind: &str) -> (usize, String, String) {
    (segment, text.to_owned(), kind.to_owned())
}

#[test]
fn the_rule_adds_a_card_the_model_missed() {
    let mut entities = vec![entity(0, 47, 59, "NOMBRE_PERSONA", TEXT)];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [
            s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA"),
            s(0, "Emily Carter", "NOMBRE_PERSONA"),
        ]
    );
    let card = &entities[0];
    assert_eq!((card.start, card.end), (12, 31));
    assert_eq!(card.label, "PAYMENT_CARD");
    assert_eq!(card.score, 1.0);
}

#[test]
fn model_pieces_inside_the_card_are_replaced() {
    let mut entities = vec![
        entity(0, 12, 16, "TARJETA_BANCARIA", TEXT),
        entity(0, 17, 31, "REFERENCIA", TEXT),
    ];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA")]
    );
}

#[test]
fn a_model_entity_with_the_same_span_and_another_type_is_replaced() {
    let mut entities = vec![entity(0, 12, 31, "TELEFONO", TEXT)];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA")]
    );
    assert_eq!(entities[0].score, 1.0);
}

#[test]
fn a_model_entity_that_sticks_out_is_trimmed() {
    // El modelo une el final de la tarjeta con lo que sigue: se queda solo
    // la parte de fuera, sin la coma ni el espacio del borde.
    let mut entities = vec![entity(0, 27, 41, "REFERENCIA", TEXT)];
    assert_eq!(entities[0].text, "9011, code 905");
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [
            s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA"),
            s(0, "code 905", "REFERENCIA"),
        ]
    );
    assert_eq!((entities[1].start, entities[1].end), (33, 41));

    let mut entities = vec![entity(0, 5, 21, "REFERENCIA", TEXT)];
    assert_eq!(entities[0].text, "number 5369 2714");
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [
            s(0, "number", "REFERENCIA"),
            s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA"),
        ]
    );
}

#[test]
fn a_model_entity_that_contains_the_whole_card_is_kept() {
    let mut entities = vec![entity(0, 5, 31, "CUENTA_BANCARIA", TEXT)];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [s(0, "number 5369 2714 6382 9011", "CUENTA_BANCARIA")]
    );
}

#[test]
fn entities_of_other_segments_are_left_alone() {
    let other = "Otra cosa";
    let mut entities = vec![
        entity(1, 0, 4, "LUGAR", other),
        entity(0, 12, 16, "TARJETA_BANCARIA", TEXT),
    ];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [
            s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA"),
            s(1, "Otra", "LUGAR"),
        ]
    );
}

#[test]
fn without_cards_nothing_changes() {
    let text = "Llamad al 612 345 678.";
    let mut entities = vec![entity(0, 10, 21, "TELEFONO", text)];
    let before = entities.clone();
    apply_card_rule(text, 0, &mut entities);
    assert_eq!(entities, before);
}

#[test]
fn the_length_limits_are_13_and_19_digits() {
    assert!(luhn_valid("4532015112830366120"));
    assert!(luhn_valid("45320151128303661230"));
    assert_eq!(cards("de 19: 4532015112830366120"), ["4532015112830366120"]);
    assert!(cards("de 20: 45320151128303661230").is_empty());
}

#[test]
fn an_entity_of_another_segment_at_the_same_positions_is_not_touched() {
    let other = "Texto de otro párrafo bastante más largo";
    let mut entities = vec![entity(1, 9, 13, "LUGAR", other)];
    apply_card_rule(TEXT, 0, &mut entities);
    assert_eq!(
        summary(&entities),
        [
            s(0, "5369 2714 6382 9011", "TARJETA_BANCARIA"),
            s(1, "otro", "LUGAR"),
        ]
    );
}
