//! Tabla de tipos: etiqueta del modelo → marcador en español.
//!
//! Los nombres los aprobó Jano el 2026-10-26. Esta prueba los fija para que
//! no cambien por descuido: un cambio en la tabla debe ser deliberado.

use std::collections::BTreeSet;

use nucleo::Error;
use nucleo::motor::tipos::{TypeTable, labels_from_config};
use nucleo::recursos::Resources;

const APPROVED: [(&str, &str); 35] = [
    ("PERSON_NAME", "NOMBRE_PERSONA"),
    ("PERSON_ALIAS", "ALIAS_PERSONA"),
    ("PERSON_IDENTIFIER", "ID_DOCUMENTO_PERSONA"),
    ("PERSON_ATTRIBUTE", "CARACTERISTICA_PERSONA"),
    ("PERSON_ROLE_OR_TITLE", "CARGO"),
    ("DATE_OF_BIRTH", "FECHA_NACIMIENTO"),
    ("PROPER_NAME", "NOMBRE_PROPIO"),
    ("EMAIL_ADDRESS", "EMAIL"),
    ("PHONE_NUMBER", "TELEFONO"),
    ("CONTACT_HANDLE", "CONTACTO"),
    ("POSTAL_ADDRESS", "DIRECCION_POSTAL"),
    ("LOCATION", "LUGAR"),
    ("GEO_LOCATION", "COORDENADAS"),
    ("DOCUMENT_IDENTIFIER", "IDENTIFICADOR_DOCUMENTO"),
    ("DOCUMENT_REFERENCE", "REFERENCIA"),
    ("ORGANIZATION_NAME", "ORGANIZACION"),
    ("ORGANIZATION_IDENTIFIER", "CIF"),
    ("CRIMINAL_OFFENCE_DATA", "ANTECEDENTES"),
    ("PAYMENT_CARD", "TARJETA_BANCARIA"),
    ("PAYMENT_CARD_SECURITY", "CVV_TARJETA"),
    ("BANK_ACCOUNT_IDENTIFIER", "CUENTA_BANCARIA"),
    ("FINANCIAL_AMOUNT", "IMPORTE"),
    ("ACCOUNT_IDENTIFIER", "USUARIO"),
    ("AUTH_SECRET", "CLAVE_SECRETO"),
    ("DEVICE_IDENTIFIER", "DISPOSITIVO"),
    ("IP_ADDRESS", "IP"),
    ("IDENTIFYING_LINK", "ENLACE"),
    ("VEHICLE_IDENTIFIER", "VEHICULO"),
    ("HEALTH_DATA", "DATOS_SALUD"),
    ("BIOMETRIC_DATA", "DATOS_BIOMETRIA"),
    ("ETHNIC_ORIGIN", "ORIGEN_ETNICO"),
    ("POLITICAL_OPINION", "IDEOLOGIA"),
    ("RELIGION_OR_BELIEF", "RELIGION"),
    ("SEXUAL_ORIENTATION", "ORIENTACION_SEXUAL"),
    ("TRADE_UNION_MEMBERSHIP", "SINDICATO"),
];

#[test]
fn every_type_comes_from_the_model() {
    // No hay reglas deterministas (decisión de Jano del 2026-10-26), así que
    // cada marcador corresponde a una etiqueta del modelo y nada más.
    let table = TypeTable::builtin();
    let markers: BTreeSet<&str> = table.markers().collect();
    let approved: BTreeSet<&str> = APPROVED.iter().map(|(_, marker)| *marker).collect();
    assert_eq!(markers, approved);
    for marker in ["DNI", "NIE", "IBAN"] {
        assert!(!table.contains_marker(marker), "{marker} sobra");
    }
}

#[test]
fn only_cargo_importe_lugar_and_coordenadas_are_optional() {
    let table = TypeTable::builtin();
    let optional: BTreeSet<&str> = table.optional_markers().collect();
    assert_eq!(
        optional,
        BTreeSet::from(["CARGO", "IMPORTE", "LUGAR", "COORDENADAS"])
    );
    assert_eq!(table.is_optional("LUGAR"), Some(true));
    assert_eq!(table.is_optional("NOMBRE_PERSONA"), Some(false));
    assert_eq!(table.is_optional("NO_EXISTE"), None);
}

#[test]
fn every_marker_is_unique_and_safe_inside_a_bracketed_marker() {
    let table = TypeTable::builtin();
    let markers: Vec<&str> = table.markers().collect();
    let unique: BTreeSet<&str> = markers.iter().copied().collect();
    assert_eq!(markers.len(), unique.len(), "marcadores repetidos");
    for marker in markers {
        assert!(
            marker
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "{marker}"
        );
        assert!(
            !marker.starts_with('_') && !marker.ends_with('_'),
            "{marker}"
        );
    }
}

#[test]
fn labels_from_config_reads_id2label_in_id_order() {
    let config = r#"{"id2label": {"2": "I-PERSON_NAME", "0": "O", "1": "B-PERSON_NAME"}}"#;
    assert_eq!(
        labels_from_config(config).unwrap(),
        ["O", "B-PERSON_NAME", "I-PERSON_NAME"]
    );
}

#[test]
fn labels_from_config_rejects_gaps_and_bad_ids() {
    for config in [
        r#"{"id2label": {"0": "O", "2": "B-PERSON_NAME"}}"#,
        r#"{"id2label": {"0": "O", "uno": "B-PERSON_NAME"}}"#,
        r#"{"id2label": {}}"#,
        r#"{"otra": 1}"#,
        "no es json",
    ] {
        assert!(
            matches!(
                labels_from_config(config),
                Err(Error::InvalidModelConfig(_))
            ),
            "{config}"
        );
    }
}

#[test]
fn check_model_labels_accepts_bio_labels_of_known_types() {
    let table = TypeTable::builtin();
    let labels = ["O", "B-PERSON_NAME", "I-PERSON_NAME", "B-LOCATION"].map(String::from);
    assert!(table.check_model_labels(&labels).is_ok());
}

#[test]
fn check_model_labels_names_every_unknown_type() {
    let table = TypeTable::builtin();
    let labels = ["O", "B-PERSON_NAME", "B-PET_NAME", "I-SHOE_SIZE"].map(String::from);
    match table.check_model_labels(&labels) {
        Err(Error::InvalidTypeTable(message)) => {
            assert!(message.contains("PET_NAME"), "{message}");
            assert!(message.contains("SHOE_SIZE"), "{message}");
        }
        other => panic!("se esperaba InvalidTypeTable, llegó {other:?}"),
    }
}

#[test]
fn check_model_labels_rejects_labels_without_bio_prefix() {
    let table = TypeTable::builtin();
    let labels = ["O", "PERSON_NAME"].map(String::from);
    assert!(matches!(
        table.check_model_labels(&labels),
        Err(Error::InvalidTypeTable(_))
    ));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn the_table_covers_every_label_of_the_model() {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    let table = TypeTable::builtin();
    let name = "modelo";
    let config = std::fs::read_to_string(resources.model(name).config_path()).unwrap();
    let labels = labels_from_config(&config).unwrap();
    table
        .check_model_labels(&labels)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
}
