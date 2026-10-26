//! Normalización de valores y registro de marcadores.
//!
//! Reglas del PRD: el mismo valor recibe siempre el mismo marcador, en todos
//! los documentos; la numeración es por tipo y no se reinicia; dos valores son
//! el mismo si coinciden en minúsculas, sin acentos y con espacios colapsados.

use nucleo::motor::marcadores::{MarkerRegistry, format_marker, normalize};

#[test]
fn normalize_lowercases_strips_accents_and_collapses_spaces() {
    assert_eq!(normalize("JUAN PÉREZ"), "juan perez");
    assert_eq!(normalize("  Juan   Perez \t"), "juan perez");
    assert_eq!(normalize("Àngels Comí"), "angels comi");
    assert_eq!(normalize("François Müller"), "francois muller");
    assert_eq!(normalize("Calle\nMayor"), "calle mayor");
}

#[test]
fn normalize_handles_precomposed_and_decomposed_accents_alike() {
    // «é» como un solo carácter (U+00E9) y como «e» + acento combinante (U+0301).
    assert_eq!(normalize("P\u{e9}rez"), normalize("Pe\u{301}rez"));
}

#[test]
fn normalize_keeps_digits_and_punctuation() {
    assert_eq!(normalize("12.345.678-Z"), "12.345.678-z");
    assert_eq!(normalize("Ana@Ejemplo.ES"), "ana@ejemplo.es");
}

#[test]
fn format_marker_pads_to_four_digits_but_never_truncates() {
    assert_eq!(format_marker("NOMBRE_PERSONA", 42), "[NOMBRE_PERSONA_0042]");
    assert_eq!(format_marker("EMAIL", 7), "[EMAIL_0007]");
    assert_eq!(format_marker("EMAIL", 9999), "[EMAIL_9999]");
    assert_eq!(format_marker("EMAIL", 10000), "[EMAIL_10000]");
}

#[test]
fn spelling_variants_share_one_marker() {
    let mut registry = MarkerRegistry::new();
    let first = registry.assign("NOMBRE_PERSONA", "JUAN PÉREZ").to_owned();
    let second = registry.assign("NOMBRE_PERSONA", "Juan Perez").to_owned();
    assert_eq!(first, "[NOMBRE_PERSONA_0001]");
    assert_eq!(second, first);
}

#[test]
fn each_type_has_its_own_counter() {
    let mut registry = MarkerRegistry::new();
    assert_eq!(registry.assign("NOMBRE_PERSONA", "Ana Gil"), "[NOMBRE_PERSONA_0001]");
    assert_eq!(registry.assign("EMAIL", "ana@ejemplo.es"), "[EMAIL_0001]");
    assert_eq!(registry.assign("NOMBRE_PERSONA", "Luis Mas"), "[NOMBRE_PERSONA_0002]");
    assert_eq!(registry.counter("NOMBRE_PERSONA"), 2);
    assert_eq!(registry.counter("EMAIL"), 1);
    assert_eq!(registry.counter("IBAN"), 0);
}

#[test]
fn the_same_value_in_two_documents_gets_the_same_marker_and_counters_continue() {
    let mut registry = MarkerRegistry::new();
    // Primer documento.
    let ana = registry.assign("NOMBRE_PERSONA", "Ana Gil").to_owned();
    registry.assign("NOMBRE_PERSONA", "Luis Mas");
    // Segundo documento, con el mismo registro.
    assert_eq!(registry.assign("NOMBRE_PERSONA", "ANA GIL"), ana);
    assert_eq!(registry.assign("NOMBRE_PERSONA", "Marta Sanz"), "[NOMBRE_PERSONA_0003]");
}

#[test]
fn the_key_does_not_depend_on_the_type() {
    // El modelo puede clasificar el mismo texto con tipos distintos en dos
    // sitios. Para restaurar sin ambigüedad, el valor conserva su primer marcador.
    let mut registry = MarkerRegistry::new();
    let first = registry.assign("ORGANIZACION", "Valencia").to_owned();
    assert_eq!(registry.assign("LUGAR", "valencia"), first);
    assert_eq!(registry.counter("LUGAR"), 0);
}

#[test]
fn every_literal_variant_is_recorded_once_and_the_first_one_is_the_value() {
    let mut registry = MarkerRegistry::new();
    registry.assign("NOMBRE_PERSONA", "Juan Pérez");
    registry.assign("NOMBRE_PERSONA", "JUAN PÉREZ");
    registry.assign("NOMBRE_PERSONA", "Juan Pérez");
    let entry = registry.entry_for_marker("[NOMBRE_PERSONA_0001]").unwrap();
    assert_eq!(entry.value(), "Juan Pérez");
    assert_eq!(entry.kind(), "NOMBRE_PERSONA");
    assert_eq!(entry.normalized_key(), "juan perez");
    assert_eq!(entry.variants(), ["Juan Pérez", "JUAN PÉREZ"]);
}

#[test]
fn entries_are_listed_in_creation_order() {
    let mut registry = MarkerRegistry::new();
    registry.assign("EMAIL", "a@ejemplo.es");
    registry.assign("NOMBRE_PERSONA", "Ana Gil");
    registry.assign("EMAIL", "A@EJEMPLO.ES");
    let markers: Vec<&str> = registry.entries().map(|e| e.marker()).collect();
    assert_eq!(markers, ["[EMAIL_0001]", "[NOMBRE_PERSONA_0001]"]);
}

#[test]
fn unknown_markers_are_not_found() {
    let mut registry = MarkerRegistry::new();
    registry.assign("EMAIL", "a@ejemplo.es");
    assert!(registry.entry_for_marker("[EMAIL_0002]").is_none());
    assert!(registry.entry_for_marker("[DNI_0001]").is_none());
}

#[test]
fn counters_go_past_9999_without_losing_digits() {
    let mut registry = MarkerRegistry::new();
    for n in 0..10_000 {
        registry.assign("EMAIL", &format!("usuario{n}@ejemplo.es"));
    }
    assert_eq!(registry.counter("EMAIL"), 10_000);
    assert_eq!(registry.assign("EMAIL", "otro@ejemplo.es"), "[EMAIL_10001]");
}
