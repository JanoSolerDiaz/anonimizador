//! Los registros del motor y de las operaciones solo llevan recuentos, tipos,
//! identificadores y tiempos. Nunca texto de los documentos, valores
//! detectados ni valores del almacén (PRD, sección de seguridad).
//!
//! Un suscriptor propio guarda cada evento y cada campo como texto, y las
//! pruebas buscan en ellos el texto de entrada.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use jiff::Zoned;
use nucleo::almacen::Store;
use nucleo::formatos::docx::DocxDocument;
use nucleo::motor::troceado::{
    DOCUMENT_OVERLAP_TOKENS, DOCUMENT_WINDOW_TOKENS, MAX_WINDOW_TOKENS, OVERLAP_TOKENS,
    plan_windows,
};
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize, deanonymize_text};
use nucleo::operaciones::{Detector, OutputFormat, model_id};
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
    assert_nothing_logged(&records, &forbidden);
}

fn assert_nothing_logged(records: &[Captured], forbidden: &[String]) {
    for record in records {
        for logged in record.values() {
            for value in forbidden {
                assert!(
                    !logged.contains(value.as_str()),
                    "un registro contiene «{value}»: {record:?}"
                );
            }
        }
    }
}

/// Detector que encuentra unos valores fijos, sin el modelo. Como el modelo,
/// no devuelve dos entidades que se pisen: gana la más larga.
struct Fake(Vec<(String, String)>);

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            let mut taken = vec![false; text.chars().count()];
            for (value, kind) in &self.0 {
                for (byte, _) in text.match_indices(value.as_str()) {
                    let start = text[..byte].chars().count();
                    let span = start..start + value.chars().count();
                    if taken[span.clone()].iter().any(|t| *t) {
                        continue;
                    }
                    taken[span].iter_mut().for_each(|t| *t = true);
                    entities.push(Entity {
                        segment,
                        start,
                        end: start + value.chars().count(),
                        kind: kind.clone(),
                        label: format!("ETIQUETA_{kind}"),
                        text: value.clone(),
                        score: 0.99,
                    });
                }
            }
        }
        Ok(entities)
    }
}

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(file)
}

/// Valores esperados de un documento del corpus, los largos primero para que
/// no se pisen.
fn corpus_values(name: &str) -> Vec<(String, String)> {
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus(&format!("{name}.json"))).unwrap()).unwrap();
    let mut values: Vec<(String, String)> = json["entidades"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["texto"].as_str().unwrap().to_owned(),
                e["tipo"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    values.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
    values.dedup();
    values
}

#[test]
fn anonymizing_and_deanonymizing_never_log_document_text_or_store_values() {
    const NAME: &str = "es-01-contrato-arrendamiento";
    let docx = corpus(&format!("{NAME}.docx"));
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("nota.txt");
    std::fs::write(&text, SEGMENTS.join("\n\n")).unwrap();
    let mut values = corpus_values(NAME);
    values.extend(
        [
            ("Marta Lozano Ibáñez", "NOMBRE_PERSONA"),
            ("48291736Q", "DNI"),
            ("marta.lozano@correo-ejemplo.es", "EMAIL"),
            ("4532 0151 1283 0366", "TARJETA"),
            ("Zaragoza", "LUGAR"),
        ]
        .map(|(v, k)| (v.to_owned(), k.to_owned())),
    );
    values.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
    let detector = Fake(values);

    let capture = Capture::default();
    let store = tracing::subscriber::with_default(capture.clone(), || {
        let mut store = Store::new();
        let options = |format| AnonymizeOptions {
            format,
            model: model_id("modelo"),
            now: Zoned::now(),
        };
        let out = dir.path().join("salida");
        std::fs::create_dir(&out).unwrap();
        let docx_out = anonymize(
            &docx,
            &out,
            &detector,
            &mut store,
            &options(OutputFormat::Docx),
        )
        .unwrap()
        .output;
        let text_out = anonymize(
            &text,
            &out,
            &detector,
            &mut store,
            &options(OutputFormat::Text),
        )
        .unwrap()
        .output;
        let back = dir.path().join("vuelta");
        std::fs::create_dir(&back).unwrap();
        assert!(matches!(
            deanonymize(&docx_out, &back, &store, None).unwrap(),
            Outcome::Done(_)
        ));
        let pasted = std::fs::read_to_string(&text_out).unwrap();
        assert!(matches!(
            deanonymize_text(&pasted, &store, None).unwrap(),
            Outcome::Done(_)
        ));
        store
    });
    let records = capture.records.lock().unwrap().clone();

    // Que la prueba no pase en vacío: están los cuatro eventos.
    let messages: Vec<&str> = records
        .iter()
        .filter_map(|r| r.fields.get("message").map(String::as_str))
        .collect();
    assert_eq!(
        messages
            .iter()
            .filter(|m| **m == "documento anonimizado")
            .count(),
        2,
        "{records:?}"
    );
    assert_eq!(
        messages
            .iter()
            .filter(|m| **m == "documento desanonimizado")
            .count(),
        2,
        "{records:?}"
    );
    assert!(store.entries().count() > 10);

    // Ni los valores del almacén, ni sus palabras, ni los párrafos de entrada.
    let mut forbidden: Vec<String> = Vec::new();
    for entry in store.entries() {
        forbidden.push(entry.value().to_owned());
        forbidden.extend(entry.variants().iter().cloned());
        forbidden.extend(
            entry
                .value()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|w| w.chars().count() >= 4 && w.chars().any(char::is_alphabetic))
                .map(str::to_owned),
        );
    }
    let document = DocxDocument::from_bytes(&std::fs::read(&docx).unwrap()).unwrap();
    forbidden.extend(
        document
            .segments()
            .iter()
            .map(|s| s.text.clone())
            .filter(|t| t.chars().count() >= 20),
    );
    forbidden.extend(SEGMENTS.map(str::to_owned));
    assert_nothing_logged(&records, &forbidden);
}
