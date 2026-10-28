//! Regla de tarjetas de pago.
//!
//! Es la única regla determinista del motor: Jano decidió el 2026-10-28
//! recuperarla porque el modelo dejaba cifras de tarjetas a la vista (ver
//! `DECISIONES.md`). Una secuencia de 13 a 19 cifras, juntas o en grupos
//! separados por un espacio o un guion, sin letras ni cifras pegadas, que
//! cumple el algoritmo de Luhn, es una `TARJETA_BANCARIA`. La regla gana a
//! lo que haya detectado el modelo en ese tramo.

use super::Entity;
use super::decodificacion::is_edge_noise;
use super::tipos::TypeTable;

const MIN_DIGITS: usize = 13;
const MAX_DIGITS: usize = 19;
const CARD_LABEL: &str = "PAYMENT_CARD";

/// Comprueba el dígito de control de Luhn. Solo acepta cifras ASCII.
pub fn luhn_valid(digits: &str) -> bool {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0');
            match (i % 2 == 1, d * 2) {
                (false, _) => d,
                (true, doubled) if doubled > 9 => doubled - 9,
                (true, doubled) => doubled,
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

fn is_group_separator(c: char) -> bool {
    matches!(c, ' ' | '-' | '\u{a0}')
}

/// Tramos `(inicio, fin)` de las tarjetas del texto, en caracteres y con el
/// fin excluido.
pub fn find_payment_cards(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && chars[i - 1].is_alphanumeric()) {
            i += 1;
            continue;
        }
        // Cifras, con un solo separador entre dos grupos.
        let start = i;
        let mut digits = String::new();
        let mut j = i;
        let end = loop {
            while j < chars.len() && chars[j].is_ascii_digit() {
                digits.push(chars[j]);
                j += 1;
            }
            if j + 1 < chars.len() && is_group_separator(chars[j]) && chars[j + 1].is_ascii_digit()
            {
                j += 1;
            } else {
                break j;
            }
        };
        let glued = end < chars.len() && chars[end].is_alphanumeric();
        if !glued && (MIN_DIGITS..=MAX_DIGITS).contains(&digits.len()) && luhn_valid(&digits) {
            found.push((start, end));
        }
        i = end;
    }
    found
}

/// Aplica la regla a las entidades de un segmento. Las entidades del modelo
/// contenidas en una tarjeta se quitan y las que la pisan en parte se
/// recortan; si una ya contiene la tarjeta entera, se deja como está. Al
/// final las entidades quedan ordenadas por segmento y posición.
pub fn apply_card_rule(text: &str, segment: usize, entities: &mut Vec<Entity>) {
    let cards = find_payment_cards(text);
    if cards.is_empty() {
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let kind = TypeTable::builtin()
        .marker_for_label(CARD_LABEL)
        .expect("la tabla de tipos tiene las tarjetas")
        .to_owned();

    for (start, end) in cards {
        let ours = |e: &Entity| e.segment == segment && e.start < end && start < e.end;
        let already_covered = entities.iter().any(|e| {
            ours(e) && e.start <= start && end <= e.end && (e.start, e.end) != (start, end)
        });
        if already_covered {
            continue;
        }
        entities.retain(|e| !(ours(e) && start <= e.start && e.end <= end));
        for e in entities.iter_mut().filter(|e| ours(e)) {
            // Solo pisa la tarjeta por un lado: se queda la parte de fuera.
            if e.start < start {
                e.end = start;
            } else {
                e.start = end;
            }
            while e.start < e.end && is_edge_noise(chars[e.start]) {
                e.start += 1;
            }
            while e.end > e.start && is_edge_noise(chars[e.end - 1]) {
                e.end -= 1;
            }
            e.text = chars[e.start..e.end].iter().collect();
        }
        entities.retain(|e| e.start < e.end);
        entities.push(Entity {
            segment,
            start,
            end,
            kind: kind.clone(),
            label: CARD_LABEL.to_owned(),
            text: chars[start..end].iter().collect(),
            score: 1.0,
        });
    }
    entities.sort_by_key(|e| (e.segment, e.start));
}
