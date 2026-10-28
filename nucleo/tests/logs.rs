//! Los registros del motor solo llevan recuentos, tipos y tiempos. Nunca
//! texto de los documentos ni valores detectados (PRD, sección de seguridad).
//!
//! Un suscriptor propio guarda cada evento y cada campo como texto, y las
//! pruebas buscan en ellos el texto de entrada.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use nucleo::motor::{Engine, EngineOptions};
use nucleo::recursos::Resources;

/// Evento o tramo capturado: nombre del objetivo y campos como texto.
#[derive(Debug, Clone, Default)]
struct Captured {
    target: String,
    fields: BTreeMap<String, String>,
}

impl Captured {
    fn all_text(&self) -> String {
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("{} {}", self.target, fields.join(" "))
    }
}

struct FieldsVisitor<'a>(&'a mut BTreeMap<String, String>);

impl Visit for FieldsVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

#[derive(Clone, Default)]
struct Capture {
    records: Arc<Mutex<Vec<Captured>>>,
    next_id: Arc<AtomicU64>,
}

impl Capture {
    fn push(&self, metadata: &Metadata<'_>, visit: impl FnOnce(&mut FieldsVisitor)) {
        let mut fields = BTreeMap::new();
        visit(&mut FieldsVisitor(&mut fields));
        self.records.lock().unwrap().push(Captured {
            target: metadata.target().to_owned(),
            fields,
        });
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.push(span.metadata(), |v| span.record(v));
        Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        let mut fields = BTreeMap::new();
        values.record(&mut FieldsVisitor(&mut fields));
        self.records.lock().unwrap().push(Captured {
            target: "registro de tramo".to_owned(),
            fields,
        });
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        self.push(event.metadata(), |v| event.record(v));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// Carga el motor y detecta con un suscriptor que lo captura todo.
fn capture_detection(segments: &[&str]) -> (Vec<Captured>, Vec<nucleo::motor::Entity>) {
    let capture = Capture::default();
    let entities = tracing::subscriber::with_default(capture.clone(), || {
        let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
        let engine = Engine::load(
            &resources,
            &resources.model("modelo"),
            EngineOptions::default(),
        )
        .unwrap();
        engine.detect(segments).unwrap()
    });
    let records = capture.records.lock().unwrap().clone();
    (records, entities)
}

const SEGMENTS: [&str; 2] = [
    "Contrato firmado por Marta Lozano Ibáñez, con DNI 48291736Q y correo marta.lozano@correo-ejemplo.es.",
    "Pagó con la tarjeta 4532 0151 1283 0366 en Zaragoza por diabetes tipo 2.",
];

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn logs_report_counts_and_times() {
    let (records, entities) = capture_detection(&SEGMENTS);
    assert!(!entities.is_empty());

    let load = records
        .iter()
        .find(|r| r.fields.contains_key("carga_ms"))
        .unwrap_or_else(|| panic!("falta el evento de carga: {records:?}"));
    assert_eq!(
        load.fields.get("modelo").map(String::as_str),
        Some("modelo")
    );

    let detection = records
        .iter()
        .find(|r| r.fields.contains_key("entidades"))
        .unwrap_or_else(|| panic!("falta el evento de detección: {records:?}"));
    assert_eq!(detection.fields["segmentos"], "2");
    assert_eq!(detection.fields["entidades"], entities.len().to_string());
    for field in [
        "tokens",
        "ventanas",
        "tokenizacion_ms",
        "inferencia_ms",
        "decodificacion_ms",
    ] {
        assert!(
            detection.fields.contains_key(field),
            "falta {field}: {detection:?}"
        );
    }

    // Un evento por tipo, con su recuento.
    let mut expected: BTreeMap<String, usize> = BTreeMap::new();
    for e in &entities {
        *expected.entry(e.kind.clone()).or_default() += 1;
    }
    let by_kind: BTreeMap<String, usize> = records
        .iter()
        .filter_map(|r| {
            Some((
                r.fields.get("tipo")?.clone(),
                r.fields.get("n")?.parse().ok()?,
            ))
        })
        .collect();
    assert_eq!(by_kind, expected);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn logs_never_contain_document_text_or_detected_values() {
    let (records, entities) = capture_detection(&SEGMENTS);
    assert!(!records.is_empty());
    let logged: Vec<String> = records.iter().map(Captured::all_text).collect();

    // Ni los valores detectados ni ninguna palabra del texto de entrada.
    let mut forbidden: Vec<String> = entities.iter().map(|e| e.text.clone()).collect();
    for segment in SEGMENTS {
        forbidden.push(segment.to_owned());
        forbidden.extend(
            segment
                .split(|c: char| c.is_whitespace() || c == ',')
                .map(|w| w.trim_matches('.'))
                .filter(|w| w.chars().count() >= 4)
                .map(str::to_owned),
        );
    }
    for line in &logged {
        for value in &forbidden {
            assert!(
                !line.contains(value.as_str()),
                "un registro contiene «{value}»: {line}"
            );
        }
    }
}
