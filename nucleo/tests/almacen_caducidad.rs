//! Caducidad, olvidar y vaciar (PRD, «Caducidad» y panel «Almacén»). Las
//! fechas son fijas: nada depende del reloj.

use jiff::civil::{Date, date};

use nucleo::almacen::{DEFAULT_RETENTION_DAYS, DocumentRecord, OutputFile, SourceFile, Store};

fn document(id: &str, expires: Date) -> DocumentRecord {
    DocumentRecord {
        id: id.to_owned(),
        created: "2026-10-22T10:14:03+02:00".to_owned(),
        expires,
        app_version: "1.0.0".to_owned(),
        model: "modelo@1".to_owned(),
        origin: SourceFile {
            name: format!("{id}.docx"),
            sha256: "ab".repeat(32),
        },
        output: OutputFile {
            name: format!("{id}_anonimizado.docx"),
            format: "docx".to_owned(),
        },
        markers: Vec::new(),
    }
}

/// Dos documentos que comparten a Ana; Luis solo está en el A y Marta solo
/// en el B. El A caduca el 1 de enero y el B el 1 de febrero.
fn two_documents() -> Store {
    let mut store = Store::new();
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", "doc-a");
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Luis Mas", "doc-a");
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", "doc-b");
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Marta Sanz", "doc-b");
    store.record_document(document("doc-a", date(2027, 1, 1)));
    store.record_document(document("doc-b", date(2027, 2, 1)));
    store
}

fn values(store: &Store) -> Vec<&str> {
    store.entries().map(|e| e.value()).collect()
}

fn document_ids(store: &Store) -> Vec<&str> {
    store.documents().iter().map(|d| d.id.as_str()).collect()
}

#[test]
fn documents_expire_after_90_days_by_default() {
    assert_eq!(DEFAULT_RETENTION_DAYS, 90);
    let store = Store::new();
    assert_eq!(store.retention_days(), 90);
    // El ejemplo del PRD: creado el 22 de octubre, caduca el 20 de enero.
    assert_eq!(store.expiry_from(date(2026, 10, 22)), date(2027, 1, 20));
}

#[test]
fn the_retention_period_can_be_set_when_creating_the_store() {
    let store = Store::new().with_retention_days(30);
    assert_eq!(store.retention_days(), 30);
    assert_eq!(store.expiry_from(date(2026, 12, 15)), date(2027, 1, 14));
    // También al cargarlo, porque el plazo no se guarda en el archivo.
    let loaded = Store::from_json(&store.to_json())
        .unwrap()
        .with_retention_days(7);
    assert_eq!(loaded.retention_days(), 7);
}

#[test]
fn before_the_expiry_date_nothing_is_purged() {
    let mut store = two_documents();
    store.purge(date(2026, 12, 31));
    assert_eq!(document_ids(&store), ["doc-a", "doc-b"]);
    assert_eq!(values(&store), ["Ana Gil", "Luis Mas", "Marta Sanz"]);
}

#[test]
fn on_the_expiry_date_the_document_and_its_unshared_entries_go() {
    let mut store = two_documents();
    store.purge(date(2027, 1, 1));
    assert_eq!(document_ids(&store), ["doc-b"]);
    // Luis solo estaba en el A; Ana sigue porque la usa el B.
    assert_eq!(values(&store), ["Ana Gil", "Marta Sanz"]);
    let ana = store.entry_for_marker("[NOMBRE_PERSONA_0001]").unwrap();
    assert_eq!(ana.documents(), ["doc-b"]);
    assert!(store.entry_for_marker("[NOMBRE_PERSONA_0002]").is_none());
}

#[test]
fn when_every_document_expires_the_store_has_no_entries() {
    let mut store = two_documents();
    store.purge(date(2027, 3, 1));
    assert!(store.documents().is_empty());
    assert_eq!(store.entries().count(), 0);
}

#[test]
fn forgetting_a_document_keeps_the_entries_other_documents_use() {
    let mut store = two_documents();
    store.forget("doc-b");
    assert_eq!(document_ids(&store), ["doc-a"]);
    assert_eq!(values(&store), ["Ana Gil", "Luis Mas"]);
    assert_eq!(
        store
            .entry_for_marker("[NOMBRE_PERSONA_0001]")
            .unwrap()
            .documents(),
        ["doc-a"]
    );
    // Marta ya no está: ni por marcador ni por valor.
    assert!(store.entry_for_marker("[NOMBRE_PERSONA_0003]").is_none());
}

#[test]
fn forgetting_an_unknown_document_changes_nothing() {
    let mut store = two_documents();
    let before = store.to_json();
    store.forget("doc-z");
    assert_eq!(store.to_json(), before);
}

#[test]
fn removed_markers_are_never_given_again() {
    // Un documento anonimizado antes puede seguir llevando [NOMBRE_PERSONA_0003]:
    // si se diera a otra persona, al desanonimizarlo saldría el nombre equivocado.
    let mut store = two_documents();
    store.forget("doc-b");
    store.purge(date(2027, 3, 1));
    assert_eq!(store.counter("NOMBRE_PERSONA"), 3);
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Marta Sanz", "doc-c"),
        "[NOMBRE_PERSONA_0004]"
    );
}

#[test]
fn after_removing_entries_the_lookups_still_work() {
    let mut store = two_documents();
    store.forget("doc-a");
    // Ana sigue con su marcador; Luis vuelve como dato nuevo.
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "ana gil", "doc-c"),
        "[NOMBRE_PERSONA_0001]"
    );
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Luis Mas", "doc-c"),
        "[NOMBRE_PERSONA_0004]"
    );
    assert_eq!(
        store
            .entry_for_marker("[NOMBRE_PERSONA_0003]")
            .unwrap()
            .value(),
        "Marta Sanz"
    );
    // Y lo que queda se guarda y se vuelve a cargar sin incoherencias.
    let again = Store::from_json(&store.to_json()).unwrap();
    assert_eq!(again.to_json(), store.to_json());
}

#[test]
fn clearing_empties_everything_but_keeps_the_numbering() {
    // Con un diccionario importado; su estructura llega en la tarea 6.
    let mut value: serde_json::Value = serde_json::from_slice(&two_documents().to_json()).unwrap();
    value["importados"] = serde_json::json!([{ "nombre": "de otra persona" }]);
    let mut store = Store::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    store.clear();
    assert!(store.documents().is_empty());
    assert_eq!(store.entries().count(), 0);
    assert!(store.entry_for_marker("[NOMBRE_PERSONA_0001]").is_none());
    let value: serde_json::Value = serde_json::from_slice(&store.to_json()).unwrap();
    assert_eq!(value["entradas"], serde_json::json!([]));
    assert_eq!(value["documentos"], serde_json::json!([]));
    assert_eq!(value["importados"], serde_json::json!([]));
    assert_eq!(
        store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Ana Gil", "doc-c"),
        "[NOMBRE_PERSONA_0004]"
    );
}
