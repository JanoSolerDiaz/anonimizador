//! Normalización de valores y asignación de marcadores.
//!
//! Reglas del PRD: el mismo valor recibe siempre el mismo marcador, en todos
//! los documentos; la numeración es por tipo y no se reinicia; dos valores son
//! el mismo si coinciden en minúsculas, sin acentos y con espacios colapsados.

use nucleo::almacen::Store;
use nucleo::motor::marcadores::{format_marker, normalize};

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

// Desde el hito 3, los marcadores los asigna el almacén.

const DOC_A: &str = "documento-a";
const DOC_B: &str = "documento-b";

/// Asigna en el documento A con una etiqueta de modelo cualquiera.
fn assign(store: &mut Store, kind: &str, literal: &str) -> String {
    store.assign(kind, "B-ETIQUETA", literal, DOC_A).to_owned()
}

#[test]
fn spelling_variants_share_one_marker() {
    let mut store = Store::new();
    let first = assign(&mut store, "NOMBRE_PERSONA", "JUAN PÉREZ");
    let second = assign(&mut store, "NOMBRE_PERSONA", "Juan Perez");
    assert_eq!(first, "[NOMBRE_PERSONA_0001]");
    assert_eq!(second, first);
}

#[test]
fn each_type_has_its_own_counter() {
    let mut store = Store::new();
    assert_eq!(
        assign(&mut store, "NOMBRE_PERSONA", "Ana Gil"),
        "[NOMBRE_PERSONA_0001]"
    );
    assert_eq!(
        assign(&mut store, "EMAIL", "ana@ejemplo.es"),
        "[EMAIL_0001]"
    );
    assert_eq!(
        assign(&mut store, "NOMBRE_PERSONA", "Luis Mas"),
        "[NOMBRE_PERSONA_0002]"
    );
    assert_eq!(store.counter("NOMBRE_PERSONA"), 2);
    assert_eq!(store.counter("EMAIL"), 1);
    assert_eq!(store.counter("IBAN"), 0);
}

#[test]
fn the_same_value_in_two_documents_gets_the_same_marker_and_counters_continue() {
    let mut store = Store::new();
    let ana = store
        .assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", DOC_A)
        .to_owned();
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Luis Mas", DOC_A);
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "ANA GIL", DOC_B),
        ana
    );
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Marta Sanz", DOC_B),
        "[NOMBRE_PERSONA_0003]"
    );
    // Cada entrada sabe en qué documentos aparece, una vez por documento.
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", DOC_B);
    let docs = |marker: &str| store.entry_for_marker(marker).unwrap().documents().to_vec();
    assert_eq!(docs("[NOMBRE_PERSONA_0001]"), [DOC_A, DOC_B]);
    assert_eq!(docs("[NOMBRE_PERSONA_0002]"), [DOC_A]);
    assert_eq!(docs("[NOMBRE_PERSONA_0003]"), [DOC_B]);
}

#[test]
fn the_key_does_not_depend_on_the_type() {
    // El modelo puede clasificar el mismo texto con tipos distintos en dos
    // sitios. Para restaurar sin ambigüedad, el valor conserva su primer marcador.
    let mut store = Store::new();
    let first = assign(&mut store, "ORGANIZACION", "Valencia");
    assert_eq!(assign(&mut store, "LUGAR", "valencia"), first);
    assert_eq!(store.counter("LUGAR"), 0);
}

#[test]
fn every_literal_variant_is_recorded_once_and_the_first_one_is_the_value() {
    let mut store = Store::new();
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Juan Pérez", DOC_A);
    store.assign("NOMBRE_PERSONA", "I-OTRA", "JUAN PÉREZ", DOC_A);
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Juan Pérez", DOC_A);
    let entry = store.entry_for_marker("[NOMBRE_PERSONA_0001]").unwrap();
    assert_eq!(entry.value(), "Juan Pérez");
    assert_eq!(entry.kind(), "NOMBRE_PERSONA");
    // La etiqueta del modelo también es la de la primera aparición.
    assert_eq!(entry.model_label(), "B-NOMBRE");
    assert_eq!(entry.normalized_key(), "juan perez");
    assert_eq!(entry.variants(), ["Juan Pérez", "JUAN PÉREZ"]);
    assert_eq!(entry.short_form(), None);
}

#[test]
fn entries_are_listed_in_creation_order() {
    let mut store = Store::new();
    assign(&mut store, "EMAIL", "a@ejemplo.es");
    assign(&mut store, "NOMBRE_PERSONA", "Ana Gil");
    assign(&mut store, "EMAIL", "A@EJEMPLO.ES");
    let markers: Vec<&str> = store.entries().map(|e| e.marker()).collect();
    assert_eq!(markers, ["[EMAIL_0001]", "[NOMBRE_PERSONA_0001]"]);
}

#[test]
fn unknown_markers_are_not_found() {
    let mut store = Store::new();
    assign(&mut store, "EMAIL", "a@ejemplo.es");
    assert!(store.entry_for_marker("[EMAIL_0002]").is_none());
    assert!(store.entry_for_marker("[DNI_0001]").is_none());
}

#[test]
fn counters_go_past_9999_without_losing_digits() {
    let mut store = Store::new();
    for n in 0..10_000 {
        assign(&mut store, "EMAIL", &format!("usuario{n}@ejemplo.es"));
    }
    assert_eq!(store.counter("EMAIL"), 10_000);
    assert_eq!(
        assign(&mut store, "EMAIL", "otro@ejemplo.es"),
        "[EMAIL_10001]"
    );
}
