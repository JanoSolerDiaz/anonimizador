//! Estructura del almacén una vez descifrado (PRD, «Estructura, una vez
//! descifrado»): los nombres de los campos del JSON, la ida y vuelta, y los
//! errores de un archivo que no es un almacén o es de una versión más nueva.

use jiff::civil::date;
use serde_json::{Value, json};

use nucleo::Error;
use nucleo::almacen::{DocumentRecord, OutputFile, SourceFile, Store};

fn sample_document(id: &str, markers: &[&str]) -> DocumentRecord {
    DocumentRecord {
        id: id.to_owned(),
        created: "2026-10-22T10:14:03+02:00".to_owned(),
        expires: date(2027, 1, 20),
        app_version: "1.0.0".to_owned(),
        model: "bardsai/eu-pii-anonimization-multilang@62ca571".to_owned(),
        origin: SourceFile {
            name: "Contrato cliente.docx".to_owned(),
            sha256: "ab".repeat(32),
        },
        output: OutputFile {
            name: "contrato-cliente_anonimizado.docx".to_owned(),
            format: "docx".to_owned(),
        },
        markers: markers.iter().map(|m| (*m).to_owned()).collect(),
    }
}

fn to_value(store: &Store) -> Value {
    serde_json::from_slice(&store.to_json()).unwrap()
}

/// El ejemplo del PRD, con los puntos suspensivos rellenados.
fn prd_example() -> Value {
    json!({
        "formato": "anonimizador-almacen",
        "version_formato": 1,
        "contadores": { "PERSONA": 42, "EMAIL": 7 },
        "entradas": [{
            "marcador": "[PERSONA_0042]",
            "forma_corta": "[P42]",
            "tipo": "PERSONA",
            "etiqueta_modelo": "B-NOMBRE",
            "valor": "Juan Pérez",
            "clave_normalizada": "juan perez",
            "variantes": ["Juan Pérez", "JUAN PÉREZ"],
            "documentos": ["b7c1e2a4-0000-4000-8000-000000000001"]
        }],
        "documentos": [{
            "id": "b7c1e2a4-0000-4000-8000-000000000001",
            "creado": "2026-10-22T10:14:03+02:00",
            "caduca": "2027-01-20",
            "version_app": "1.0.0",
            "modelo": "bardsai/eu-pii-anonimization-multilang@62ca571",
            "origen": { "nombre": "Contrato cliente.pdf", "sha256": "ab".repeat(32) },
            "salida": { "nombre": "contrato-cliente_anonimizado.pdf", "formato": "pdf" },
            "marcadores": ["[PERSONA_0042]"]
        }],
        "importados": []
    })
}

fn load(value: &Value) -> nucleo::Result<Store> {
    Store::from_json(&serde_json::to_vec(value).unwrap())
}

#[test]
fn an_empty_store_has_the_prd_skeleton() {
    assert_eq!(
        to_value(&Store::new()),
        json!({
            "formato": "anonimizador-almacen",
            "version_formato": 1,
            "contadores": {},
            "entradas": [],
            "documentos": [],
            "importados": []
        })
    );
}

#[test]
fn entries_and_documents_use_the_prd_field_names() {
    let mut store = Store::new();
    let marker = store
        .assign("NOMBRE_PERSONA", "B-NOMBRE", "Juan Pérez", "doc-1")
        .to_owned();
    store.record_document(sample_document("doc-1", &[&marker]));

    let value = to_value(&store);
    assert_eq!(value["contadores"], json!({ "NOMBRE_PERSONA": 1 }));
    assert_eq!(
        value["entradas"][0],
        json!({
            "marcador": "[NOMBRE_PERSONA_0001]",
            "tipo": "NOMBRE_PERSONA",
            "etiqueta_modelo": "B-NOMBRE",
            "valor": "Juan Pérez",
            "clave_normalizada": "juan perez",
            "variantes": ["Juan Pérez"],
            "documentos": ["doc-1"]
        }),
        "sin forma_corta si no se usó en un PDF"
    );
    assert_eq!(
        value["documentos"][0],
        json!({
            "id": "doc-1",
            "creado": "2026-10-22T10:14:03+02:00",
            "caduca": "2027-01-20",
            "version_app": "1.0.0",
            "modelo": "bardsai/eu-pii-anonimization-multilang@62ca571",
            "origen": { "nombre": "Contrato cliente.docx", "sha256": "ab".repeat(32) },
            "salida": { "nombre": "contrato-cliente_anonimizado.docx", "formato": "docx" },
            "marcadores": ["[NOMBRE_PERSONA_0001]"]
        })
    );
}

#[test]
fn the_prd_example_loads_and_numbering_continues() {
    let mut store = load(&prd_example()).unwrap();
    assert_eq!(store.counter("PERSONA"), 42);
    let entry = store.entry_for_marker("[PERSONA_0042]").unwrap();
    assert_eq!(entry.short_form(), Some("[P42]"));
    assert_eq!(entry.value(), "Juan Pérez");
    assert_eq!(store.documents().len(), 1);
    assert_eq!(store.documents()[0].expires, date(2027, 1, 20));

    // Un valor ya conocido conserva su marcador; uno nuevo sigue la cuenta.
    assert_eq!(
        store.assign("PERSONA", "B-NOMBRE", "juan perez", "otro"),
        "[PERSONA_0042]"
    );
    assert_eq!(
        store.assign("PERSONA", "B-NOMBRE", "Ana Gil", "otro"),
        "[PERSONA_0043]"
    );
}

#[test]
fn json_round_trip_keeps_everything() {
    let mut store = Store::new();
    for (kind, literal, doc) in [
        ("NOMBRE_PERSONA", "Ana Gil", "doc-1"),
        ("EMAIL", "ana@ejemplo.es", "doc-1"),
        ("NOMBRE_PERSONA", "ANA GIL", "doc-2"),
    ] {
        store.assign(kind, "B-X", literal, doc);
    }
    store.record_document(sample_document(
        "doc-1",
        &["[NOMBRE_PERSONA_0001]", "[EMAIL_0001]"],
    ));
    store.record_document(sample_document("doc-2", &["[NOMBRE_PERSONA_0001]"]));

    let json = store.to_json();
    let mut again = Store::from_json(&json).unwrap();
    assert_eq!(again.to_json(), json);
    assert_eq!(again.documents(), store.documents());
    // Los índices se reconstruyen al cargar.
    assert_eq!(
        again.assign("NOMBRE_PERSONA", "B-X", "ana gil", "doc-3"),
        "[NOMBRE_PERSONA_0001]"
    );
    assert_eq!(
        again.assign("EMAIL", "B-X", "luis@ejemplo.es", "doc-3"),
        "[EMAIL_0002]"
    );
}

#[test]
fn recording_a_document_twice_replaces_it() {
    let mut store = Store::new();
    store.record_document(sample_document("doc-1", &[]));
    store.record_document(sample_document("doc-1", &["[EMAIL_0001]"]));
    assert_eq!(store.documents().len(), 1);
    assert_eq!(store.documents()[0].markers, ["[EMAIL_0001]"]);
}

#[test]
fn another_format_is_not_a_store() {
    let mut value = prd_example();
    value["formato"] = json!("anonimizador-diccionario");
    assert!(matches!(load(&value), Err(Error::StoreInvalid(_))));
}

#[test]
fn a_newer_format_version_is_reported_as_such() {
    let mut value = prd_example();
    value["version_formato"] = json!(2);
    let error = load(&value).unwrap_err();
    assert!(matches!(error, Error::StoreVersion(2)), "{error:?}");
    assert!(error.to_string().contains("más nueva"), "{error}");
}

#[test]
fn malformed_json_is_invalid() {
    for bytes in [
        &b""[..],
        b"{",
        b"[]",
        b"{\"formato\": \"anonimizador-almacen\"}",
    ] {
        assert!(
            matches!(Store::from_json(bytes), Err(Error::StoreInvalid(_))),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn error_messages_never_contain_store_values() {
    // Un valor con el tipo equivocado: serde_json citaría el texto en su mensaje.
    let mut value = prd_example();
    value["contadores"]["PERSONA"] = json!("Juan Pérez");
    let wrong_type = load(&value).unwrap_err();

    let mut value = prd_example();
    value["entradas"][0]["valor"] = json!(["Juan Pérez"]);
    let wrong_shape = load(&value).unwrap_err();

    for error in [wrong_type, wrong_shape] {
        assert!(matches!(error, Error::StoreInvalid(_)), "{error:?}");
        let text = format!("{error} {error:?}");
        assert!(!text.contains("Juan"), "el error filtra un valor: {text}");
    }
}

#[test]
fn inconsistent_entries_are_invalid() {
    // Dos entradas con la misma clave o el mismo marcador harían ambigua la
    // restauración.
    let mut same_key = prd_example();
    let mut twin = same_key["entradas"][0].clone();
    twin["marcador"] = json!("[PERSONA_0041]");
    same_key["entradas"].as_array_mut().unwrap().push(twin);

    let mut same_marker = prd_example();
    let mut twin = same_marker["entradas"][0].clone();
    twin["clave_normalizada"] = json!("otra persona");
    same_marker["entradas"].as_array_mut().unwrap().push(twin);

    for value in [same_key, same_marker] {
        let error = load(&value).unwrap_err();
        assert!(matches!(error, Error::StoreInvalid(_)), "{error:?}");
        assert!(!format!("{error} {error:?}").contains("juan"), "{error:?}");
    }
}
