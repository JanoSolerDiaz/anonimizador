//! Tipos opcionales: `CARGO`, `IMPORTE`, `LUGAR` y `COORDENADAS` se pueden
//! dejar sin sustituir. Por defecto se sustituyen todos. El resto de tipos no
//! se puede desactivar.

use std::sync::OnceLock;

use nucleo::Error;
use nucleo::motor::opcionales::SkippedTypes;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::recursos::Resources;

fn entity(kind: &str, text: &str) -> Entity {
    Entity {
        segment: 0,
        start: 0,
        end: text.chars().count(),
        kind: kind.to_owned(),
        label: String::new(),
        text: text.to_owned(),
        score: 0.9,
    }
}

fn sample() -> Vec<Entity> {
    vec![
        entity("NOMBRE_PERSONA", "Ana Gil"),
        entity("CARGO", "directora"),
        entity("LUGAR", "Valencia"),
        entity("DIRECCION_POSTAL", "Calle Mayor 5, Valencia"),
        entity("IMPORTE", "1.200 €"),
        entity("COORDENADAS", "39.47, -0.37"),
    ]
}

fn kinds(entities: &[Entity]) -> Vec<&str> {
    entities.iter().map(|e| e.kind.as_str()).collect()
}

#[test]
fn by_default_every_type_is_substituted() {
    let mut entities = sample();
    let skipped = SkippedTypes::default();
    assert!(skipped.is_empty());
    skipped.retain(&mut entities);
    assert_eq!(entities, sample());
    assert_eq!(EngineOptions::default().skipped, SkippedTypes::default());
}

#[test]
fn skipping_lugar_keeps_postal_addresses() {
    let mut entities = sample();
    SkippedTypes::new(&["LUGAR"]).unwrap().retain(&mut entities);
    assert_eq!(
        kinds(&entities),
        ["NOMBRE_PERSONA", "CARGO", "DIRECCION_POSTAL", "IMPORTE", "COORDENADAS"]
    );
}

#[test]
fn the_four_optional_types_can_be_skipped_together() {
    let mut entities = sample();
    let skipped = SkippedTypes::new(&["CARGO", "IMPORTE", "LUGAR", "COORDENADAS"]).unwrap();
    assert!(skipped.contains("IMPORTE"));
    assert!(!skipped.contains("NOMBRE_PERSONA"));
    skipped.retain(&mut entities);
    assert_eq!(kinds(&entities), ["NOMBRE_PERSONA", "DIRECCION_POSTAL"]);
}

#[test]
fn repeated_types_are_accepted() {
    let skipped = SkippedTypes::new(&["LUGAR", "LUGAR"]).unwrap();
    assert_eq!(skipped, SkippedTypes::new(&["LUGAR"]).unwrap());
}

#[test]
fn a_mandatory_type_cannot_be_skipped() {
    match SkippedTypes::new(&["LUGAR", "NOMBRE_PERSONA"]) {
        Err(error @ Error::TypeNotOptional(_)) => {
            let message = error.to_string();
            assert!(message.contains("NOMBRE_PERSONA"), "{message}");
            assert!(message.contains("CARGO") && message.contains("COORDENADAS"), "{message}");
        }
        other => panic!("se esperaba TypeNotOptional, llegó {other:?}"),
    }
}

#[test]
fn an_unknown_type_is_an_error() {
    for kind in ["SITIO", "lugar", ""] {
        assert!(
            matches!(SkippedTypes::new(&[kind]), Err(Error::UnknownType(_))),
            "{kind:?}"
        );
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn the_engine_drops_skipped_types() {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
        Engine::load(&resources, &resources.model("modelo"), EngineOptions::default()).unwrap()
    });
    let segments = ["Carlos García, DNI 12345678Z, Madrid, +34 612 345 678."];
    let options = EngineOptions {
        skipped: SkippedTypes::new(&["LUGAR"]).unwrap(),
        ..EngineOptions::default()
    };
    let entities = engine.detect_with(&segments, &options).unwrap();
    assert_eq!(
        kinds(&entities),
        ["NOMBRE_PERSONA", "ID_DOCUMENTO_PERSONA", "TELEFONO"]
    );
}
