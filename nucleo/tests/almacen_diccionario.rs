//! Exportar e importar el diccionario de un documento (PRD, «Exportar e
//! importar»). Lo importado se guarda aparte de las entradas propias y no
//! toca los contadores.

use jiff::civil::date;
use serde_json::{Value, json};

use nucleo::Error;
use nucleo::almacen::{DocumentRecord, OutputFile, SourceFile, Store};

fn document(id: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_owned(),
        created: "2026-10-22T10:14:03+02:00".to_owned(),
        expires: date(2027, 1, 20),
        app_version: "1.0.0".to_owned(),
        model: "modelo@1".to_owned(),
        origin: SourceFile {
            name: "Contrato Ana Gil.docx".to_owned(),
            sha256: "ab".repeat(32),
        },
        output: OutputFile {
            name: "contrato-ana-gil_anonimizado.docx".to_owned(),
            format: "docx".to_owned(),
        },
        markers: Vec::new(),
    }
}

/// Almacén con dos documentos: el A tiene a Ana y su correo; el B, a Ana y
/// a Luis.
fn sender() -> Store {
    let mut store = Store::new();
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", "doc-a");
    store.assign("EMAIL", "B-EMAIL", "ana@ejemplo.es", "doc-a");
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "ANA GIL", "doc-b");
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Luis Mas", "doc-b");
    store.record_document(document("doc-a"));
    store.record_document(document("doc-b"));
    store
}

fn exported(store: &Store, id: &str) -> Value {
    serde_json::from_slice(&store.export_dictionary(id).unwrap()).unwrap()
}

#[test]
fn the_dictionary_has_only_what_is_needed_to_restore_that_document() {
    assert_eq!(
        exported(&sender(), "doc-b"),
        json!({
            "formato": "anonimizador-diccionario",
            "version_formato": 1,
            "documento": {
                "id": "doc-b",
                "creado": "2026-10-22T10:14:03+02:00",
                "salida": {
                    "nombre": "contrato-ana-gil_anonimizado.docx",
                    "formato": "docx"
                }
            },
            "entradas": [
                { "marcador": "[NOMBRE_PERSONA_0001]", "tipo": "NOMBRE_PERSONA", "valor": "Ana Gil" },
                { "marcador": "[NOMBRE_PERSONA_0002]", "tipo": "NOMBRE_PERSONA", "valor": "Luis Mas" }
            ]
        }),
        "sin el correo del documento A, sin variantes, sin el nombre original del archivo"
    );
}

#[test]
fn exporting_an_unknown_document_is_an_error() {
    let error = sender().export_dictionary("doc-z").unwrap_err();
    assert!(matches!(error, Error::DocumentNotFound), "{error:?}");
}

#[test]
fn importing_in_another_store_restores_the_markers_of_that_document() {
    let dictionary = sender().export_dictionary("doc-a").unwrap();
    let mut receiver = Store::new();
    receiver
        .import_dictionary(&dictionary, "contrato-ana-gil_diccionario.json")
        .unwrap();

    let imported = &receiver.imported()[0];
    assert_eq!(imported.name(), "contrato-ana-gil_diccionario.json");
    assert_eq!(imported.document().id, "doc-a");
    assert_eq!(
        imported
            .entry_for_marker("[NOMBRE_PERSONA_0001]")
            .unwrap()
            .value(),
        "Ana Gil"
    );
    assert_eq!(
        imported.entry_for_marker("[EMAIL_0001]").unwrap().value(),
        "ana@ejemplo.es"
    );
    assert!(imported.entry_for_marker("[NOMBRE_PERSONA_0002]").is_none());
}

#[test]
fn imported_entries_stay_apart_and_do_not_touch_the_counters() {
    let dictionary = sender().export_dictionary("doc-b").unwrap();
    let mut receiver = Store::new();
    receiver.assign("NOMBRE_PERSONA", "B-NOMBRE", "Pedro Ruiz", "propio");
    receiver
        .import_dictionary(&dictionary, "de-otra.json")
        .unwrap();

    // El [NOMBRE_PERSONA_0001] propio sigue siendo Pedro.
    assert_eq!(receiver.counter("NOMBRE_PERSONA"), 1);
    assert_eq!(receiver.entries().count(), 1);
    assert_eq!(
        receiver
            .entry_for_marker("[NOMBRE_PERSONA_0001]")
            .unwrap()
            .value(),
        "Pedro Ruiz"
    );
    // Un dato que solo está en lo importado recibe un marcador propio nuevo.
    assert_eq!(
        receiver.assign("NOMBRE_PERSONA", "B-NOMBRE", "Luis Mas", "propio"),
        "[NOMBRE_PERSONA_0002]"
    );
}

#[test]
fn importing_the_same_document_again_replaces_it() {
    let mut receiver = Store::new();
    let first = sender().export_dictionary("doc-a").unwrap();
    receiver.import_dictionary(&first, "primera.json").unwrap();
    let mut changed = sender();
    changed.assign("DNI", "B-DNI", "12345678Z", "doc-a");
    let second = changed.export_dictionary("doc-a").unwrap();
    receiver.import_dictionary(&second, "segunda.json").unwrap();

    assert_eq!(receiver.imported().len(), 1);
    assert_eq!(receiver.imported()[0].name(), "segunda.json");
    assert_eq!(receiver.imported()[0].entries().len(), 3);
}

#[test]
fn imported_dictionaries_survive_saving_and_loading_the_store() {
    let mut receiver = Store::new();
    receiver
        .import_dictionary(&sender().export_dictionary("doc-a").unwrap(), "d.json")
        .unwrap();
    let json = receiver.to_json();
    let again = Store::from_json(&json).unwrap();
    assert_eq!(again.to_json(), json);
    assert_eq!(again.imported(), receiver.imported());
}

#[test]
fn a_malformed_dictionary_is_rejected_and_nothing_is_imported() {
    let good = exported(&sender(), "doc-a");
    let mut repeated = good.clone();
    let twin = repeated["entradas"][0].clone();
    repeated["entradas"].as_array_mut().unwrap().push(twin);
    let mut other_format = good.clone();
    other_format["formato"] = json!("anonimizador-almacen");
    let mut no_value = good.clone();
    no_value["entradas"][0]
        .as_object_mut()
        .unwrap()
        .remove("valor");

    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"no es json".to_vec(),
        sender().to_json(),
        serde_json::to_vec(&other_format).unwrap(),
        serde_json::to_vec(&no_value).unwrap(),
        serde_json::to_vec(&repeated).unwrap(),
    ];
    let mut receiver = Store::new();
    for bytes in cases {
        let error = receiver.import_dictionary(&bytes, "malo.json").unwrap_err();
        assert!(matches!(error, Error::DictionaryInvalid(_)), "{error:?}");
    }
    assert!(receiver.imported().is_empty());
}

#[test]
fn a_dictionary_from_a_newer_version_is_reported_as_such() {
    let mut newer = exported(&sender(), "doc-a");
    newer["version_formato"] = json!(2);
    let error = Store::new()
        .import_dictionary(&serde_json::to_vec(&newer).unwrap(), "nuevo.json")
        .unwrap_err();
    assert!(matches!(error, Error::DictionaryVersion(2)), "{error:?}");
    assert!(error.to_string().contains("más nueva"), "{error}");
}

#[test]
fn import_errors_never_contain_values() {
    let mut wrong_type = exported(&sender(), "doc-a");
    wrong_type["entradas"][0]["marcador"] = json!(["Ana Gil"]);
    let error = Store::new()
        .import_dictionary(&serde_json::to_vec(&wrong_type).unwrap(), "malo.json")
        .unwrap_err();
    let text = format!("{error} {error:?}");
    assert!(!text.contains("Ana"), "{text}");
}
