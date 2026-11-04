//! Corpus sintético de PDF (`tests/corpus/pdf`), generado por
//! `nucleo/examples/generar_corpus_pdf.rs`. Las pruebas necesitan PDFium (y
//! la última, el modelo), por eso están ignoradas por defecto:
//!
//!   cargo test -p nucleo --test corpus_pdf -- --include-ignored

#[path = "../examples/generar_corpus_pdf.rs"]
#[allow(dead_code)]
mod generador;

use std::path::{Path, PathBuf};

use jiff::Zoned;

use nucleo::almacen::Store;
use nucleo::formatos::pdf::{DEFAULT_RESOLUTION, PdfDocument, find_value};
use nucleo::motor::{Engine, EngineOptions};
use nucleo::operaciones::OutputFormat;
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::recursos::Resources;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/pdf")
        .join(format!("{}.pdf", generador::NAME))
}

fn values() -> Vec<(String, String)> {
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus().with_extension("json")).unwrap()).unwrap();
    json["entidades"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["texto"].as_str().unwrap().to_owned(),
                e["tipo"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn page_texts(bytes: &[u8]) -> Vec<String> {
    PdfDocument::from_bytes(bytes)
        .unwrap()
        .segments()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn the_saved_corpus_has_the_text_the_generator_writes() {
    let saved = std::fs::read(corpus()).unwrap();
    assert_eq!(page_texts(&saved), page_texts(&generador::contract()));
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(corpus().with_extension("json")).unwrap()).unwrap();
    assert_eq!(expected, generador::expected());
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn every_expected_value_is_in_the_extracted_text() {
    let document = PdfDocument::from_bytes(&std::fs::read(corpus()).unwrap()).unwrap();
    assert_eq!(document.attachments(), 1);
    let text = document.segments().join("\n");
    for (value, kind) in values() {
        assert!(
            !find_value(&text, &value).is_empty(),
            "{kind} «{value}» no está en {text:?}"
        );
    }
    // El nombre partido con guion vuelve entero.
    assert!(
        text.contains("Marta Lozano Ibáñez, mayor de edad"),
        "{text:?}"
    );
}

#[test]
#[ignore = "necesita PDFium y los modelos descargados en recursos/"]
fn with_the_real_model_no_expected_value_is_left_in_the_pdf() {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    let engine = Engine::load(
        &resources,
        &resources.model("modelo"),
        EngineOptions::default(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let options = AnonymizeOptions {
        format: OutputFormat::Pdf,
        model: "modelo".to_owned(),
        now: Zoned::now(),
        spreadsheet_amounts: false,
        pdf_resolution: DEFAULT_RESOLUTION,
    };
    let summary = anonymize(&corpus(), dir.path(), &engine, &mut Store::new(), &options)
        .expect("la anonimización y su verificación deben pasar");
    assert_eq!(summary.warnings.attachments, 1);

    let original = page_texts(&std::fs::read(corpus()).unwrap());
    let output = std::fs::read(&summary.output).unwrap();
    let texts = page_texts(&output);
    // La página de cláusulas no tiene datos: se copia con su texto.
    assert_eq!(texts[1], original[1]);
    let all = texts.join("\n");
    let left: Vec<String> = values()
        .into_iter()
        .filter(|(value, _)| !find_value(&all, value).is_empty())
        .map(|(value, kind)| format!("{kind} «{value}»"))
        .collect();
    assert!(left.is_empty(), "quedan {left:?}");
}
