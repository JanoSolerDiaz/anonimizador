//! Formas cortas de los marcadores (`[N1]`), que se usan en un PDF cuando el
//! marcador completo no cabe en el hueco del dato (PRD, «Escritura» del PDF).
//!
//! Las iniciales de los tipos se repiten (`NOMBRE_PERSONA` y `NOMBRE_PROPIO`
//! empiezan por N), así que la forma corta lleva un contador propio de esa
//! letra y no el número del marcador. Ver `DECISIONES.md`.

use serde_json::{Value, json};

use nucleo::almacen::Store;
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize_text};

const DOC: &str = "b7c1e2a4-0000-4000-8000-000000000001";

fn to_value(store: &Store) -> Value {
    serde_json::from_slice(&store.to_json()).unwrap()
}

#[test]
fn types_with_the_same_initial_get_different_short_forms() {
    let mut store = Store::new();
    let person = store
        .assign("NOMBRE_PERSONA", "PERSON_NAME", "Marta Lozano", DOC)
        .to_owned();
    let proper = store
        .assign("NOMBRE_PROPIO", "PROPER_NAME", "Proyecto Faro", DOC)
        .to_owned();
    assert_eq!(person, "[NOMBRE_PERSONA_0001]");
    assert_eq!(proper, "[NOMBRE_PROPIO_0001]");

    assert_eq!(store.short_form(&person).as_deref(), Some("[N1]"));
    assert_eq!(store.short_form(&proper).as_deref(), Some("[N2]"));
}

#[test]
fn asking_again_returns_the_same_short_form() {
    let mut store = Store::new();
    let marker = store
        .assign("EMAIL", "EMAIL", "marta@ejemplo.test", DOC)
        .to_owned();
    assert_eq!(store.short_form(&marker).as_deref(), Some("[E1]"));
    assert_eq!(store.short_form(&marker).as_deref(), Some("[E1]"));
    let entry = store.entry_for_marker(&marker).unwrap();
    assert_eq!(entry.short_form(), Some("[E1]"));
}

#[test]
fn an_unknown_marker_has_no_short_form() {
    let mut store = Store::new();
    assert_eq!(store.short_form("[EMAIL_0001]"), None);
}

#[test]
fn short_forms_are_saved_and_never_reused() {
    let mut store = Store::new();
    let first = store
        .assign("TELEFONO", "PHONE", "654218903", DOC)
        .to_owned();
    store.short_form(&first);
    let value = to_value(&store);
    assert_eq!(value["contadores_cortos"], json!({ "T": 1 }));
    assert_eq!(value["entradas"][0]["forma_corta"], "[T1]");

    // Al olvidar el documento la entrada se va, pero el contador se queda.
    let mut store = Store::from_json(&store.to_json()).unwrap();
    store.forget(DOC);
    let second = store
        .assign("TELEFONO", "PHONE", "699001122", "otro")
        .to_owned();
    assert_eq!(store.short_form(&second).as_deref(), Some("[T2]"));
}

#[test]
fn a_store_without_short_counters_continues_after_its_short_forms() {
    // El ejemplo del PRD lleva `[P42]` y no tiene `contadores_cortos`.
    let store = json!({
        "formato": "anonimizador-almacen",
        "version_formato": 1,
        "contadores": { "PERSONA": 42 },
        "entradas": [{
            "marcador": "[PERSONA_0042]",
            "forma_corta": "[P42]",
            "tipo": "PERSONA",
            "etiqueta_modelo": "PERSON_NAME",
            "valor": "Juan Pérez",
            "clave_normalizada": "juan perez",
            "variantes": ["Juan Pérez"],
            "documentos": [DOC]
        }],
        "documentos": [],
        "importados": []
    });
    let mut store = Store::from_json(&serde_json::to_vec(&store).unwrap()).unwrap();
    let marker = store
        .assign("PERSONA", "PERSON_NAME", "Ana Ruiz", DOC)
        .to_owned();
    assert_eq!(store.short_form(&marker).as_deref(), Some("[P43]"));
}

#[test]
fn deanonymizing_finds_the_value_of_a_short_form() {
    let mut store = Store::new();
    let person = store
        .assign("NOMBRE_PERSONA", "PERSON_NAME", "Marta Lozano", DOC)
        .to_owned();
    let proper = store
        .assign("NOMBRE_PROPIO", "PROPER_NAME", "Proyecto Faro", DOC)
        .to_owned();
    store.short_form(&person);
    store.short_form(&proper);
    match deanonymize_text("Firmado en [ n2 ].", &store, None).unwrap() {
        Outcome::Done((text, _)) => assert_eq!(text, "Firmado en Proyecto Faro."),
        Outcome::Conflicts(c) => panic!("conflicto inesperado: {c:?}"),
    }
}
