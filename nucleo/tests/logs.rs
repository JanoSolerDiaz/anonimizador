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

use nucleo::motor::troceado::{
    DOCUMENT_OVERLAP_TOKENS, DOCUMENT_WINDOW_TOKENS, MAX_WINDOW_TOKENS, OVERLAP_TOKENS,
    plan_windows,
};
use nucleo::motor::{Engine, EngineOptions};
use nucleo::recursos::Resources;

/// Evento o tramo capturado, con sus campos como texto.
#[derive(Debug, Clone, Default)]
struct Captured {
    fields: BTreeMap<String, String>,
}

impl Captured {
    /// Valores de todos los campos, incluido el mensaje. Los nombres de los
    /// campos los escribe el código, no vienen del documento.
    fn values(&self) -> Vec<&str> {
        self.fields.values().map(String::as_str).collect()
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
    fn push(&self, visit: impl FnOnce(&mut FieldsVisitor)) {
        let mut fields = BTreeMap::new();
        visit(&mut FieldsVisitor(&mut fields));
        self.records.lock().unwrap().push(Captured { fields });
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.push(|v| span.record(v));
        Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        self.push(|v| values.record(v));
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        self.push(|v| event.record(v));
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
    "Pagó con la tarjeta 4532 0151 1283 0366 en Zaragoza por una diabetes.",
];

/// Campo numérico del evento de detección.
fn detection_field(records: &[Captured], field: &str) -> usize {
    records
        .iter()
        .find(|r| r.fields.contains_key("entidades"))
        .unwrap_or_else(|| panic!("falta el evento de detección: {records:?}"))
        .fields[field]
        .parse()
        .unwrap()
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn the_document_reading_counts_its_own_windows() {
    // Treinta párrafos cortos: la lectura párrafo a párrafo lee un trozo por
    // párrafo y la seguida trocea todos juntos con sus propios tamaños.
    let segments: Vec<String> = (0..30)
        .map(|i| {
            format!("El expediente {i} sigue abierto y pendiente de revisión por la comisión.")
        })
        .collect();
    let refs: Vec<&str> = segments.iter().map(String::as_str).collect();
    let (records, _) = capture_detection(&refs);
    let tokens = detection_field(&records, "tokens");
    let document = plan_windows(tokens, DOCUMENT_WINDOW_TOKENS, DOCUMENT_OVERLAP_TOKENS).len();
    assert_ne!(
        document,
        plan_windows(tokens, MAX_WINDOW_TOKENS, OVERLAP_TOKENS).len(),
        "la prueba no distingue los tamaños: {tokens} tokens"
    );
    assert_eq!(
        detection_field(&records, "ventanas"),
        segments.len() + document
    );
}

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

    // Ni los valores detectados ni ninguna palabra del texto de entrada.
    let mut forbidden: Vec<String> = entities.iter().map(|e| e.text.clone()).collect();
    for segment in SEGMENTS {
        forbidden.push(segment.to_owned());
        forbidden.extend(
            segment
                .split(|c: char| c.is_whitespace() || c == ',')
                .map(|w| w.trim_matches('.'))
                // Las cifras sueltas podrían coincidir por azar con un tiempo
                // en milisegundos; la tarjeta entera ya está entre las entidades.
                .filter(|w| w.chars().count() >= 4 && w.chars().any(char::is_alphabetic))
                .map(str::to_owned),
        );
    }
    for record in &records {
        for logged in record.values() {
            for value in &forbidden {
                assert!(
                    !logged.contains(value.as_str()),
                    "un registro contiene «{value}»: {record:?}"
                );
            }
        }
    }
}
