//! Anonimizar un archivo de principio a fin: extraer, detectar, asignar
//! marcadores en el almacén, sustituir, escribir la salida sin dejarla a
//! medias y registrar el documento.
//!
//! Casi todo se prueba con un detector falso que encuentra los valores
//! esperados del corpus; la prueba con el modelo real va aparte (`#[ignore]`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jiff::Zoned;
use nucleo::almacen::Store;
use nucleo::formatos::docx::DocxDocument;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::{Detector, OutputFormat, model_id};
use nucleo::recursos::Resources;
use nucleo::{Error, Result, VERSION};
use sha2::{Digest, Sha256};

const MODEL: &str =
    "bardsai/eu-pii-anonimization-multilang@62ca5712368705bac11b8fea19e42e65cce088dd";

const CORPUS: [&str; 4] = [
    "es-01-contrato-arrendamiento",
    "ca-01-informe-auditoria",
    "en-01-employment-review",
    "fr-01-fiche-client",
];

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(name)
}

fn expected(name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(corpus(&format!("{name}.json"))).unwrap()).unwrap()
}

/// Detector que encuentra unos valores fijos, los largos primero y sin
/// solapes, como haría el modelo.
struct Fake {
    values: Vec<(String, String)>,
}

impl Fake {
    fn new(values: &[(&str, &str)]) -> Fake {
        let mut values: Vec<(String, String)> = values
            .iter()
            .map(|(text, kind)| (text.to_string(), kind.to_string()))
            .collect();
        values.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
        Fake { values }
    }

    fn for_corpus(name: &str) -> Fake {
        let json = expected(name);
        let values: Vec<(String, String)> = json["entidades"]
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
        let refs: Vec<(&str, &str)> = values
            .iter()
            .map(|(t, k)| (t.as_str(), k.as_str()))
            .collect();
        Fake::new(&refs)
    }
}

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            let chars: Vec<char> = text.chars().collect();
            let mut taken = vec![false; chars.len()];
            for (value, kind) in &self.values {
                let needle: Vec<char> = value.chars().collect();
                let mut start = 0;
                while start + needle.len() <= chars.len() {
                    let end = start + needle.len();
                    if chars[start..end] == needle[..] && !taken[start..end].iter().any(|t| *t) {
                        taken[start..end].iter_mut().for_each(|t| *t = true);
                        entities.push(Entity {
                            segment,
                            start,
                            end,
                            kind: kind.clone(),
                            label: format!("ETIQUETA_{kind}"),
                            text: value.clone(),
                            score: 0.99,
                        });
                        start = end;
                    } else {
                        start += 1;
                    }
                }
            }
        }
        Ok(entities)
    }
}

fn now() -> Zoned {
    "2026-10-30T10:14:03+01:00[Europe/Madrid]".parse().unwrap()
}

fn options(format: OutputFormat) -> AnonymizeOptions {
    AnonymizeOptions {
        format,
        model: MODEL.to_owned(),
        now: now(),
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Todo el texto que puede contener datos: segmentos, enlaces y códigos de
/// campo.
fn docx_text(bytes: &[u8]) -> String {
    let document = DocxDocument::from_bytes(bytes).unwrap();
    let mut all: Vec<String> = document.segments().iter().map(|s| s.text.clone()).collect();
    all.extend(document.external_links().iter().map(|l| l.target.clone()));
    all.extend(document.field_codes().iter().map(|f| f.text.clone()));
    all.join("\n")
}

fn write_text(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn every_expected_value_disappears_from_the_docx() {
    for name in CORPUS {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = anonymize(
            &corpus(&format!("{name}.docx")),
            dir.path(),
            &Fake::for_corpus(name),
            &mut store,
            &options(OutputFormat::Docx),
        )
        .unwrap();
        assert_eq!(
            summary.output,
            dir.path().join(format!("{name}_anonimizado.docx"))
        );
        let text = docx_text(&std::fs::read(&summary.output).unwrap());
        for entity in expected(name)["entidades"].as_array().unwrap() {
            let value = entity["texto"].as_str().unwrap();
            assert!(!text.contains(value), "{name}: queda {value:?}");
        }
        assert!(text.contains('['), "{name}: no hay marcadores");
    }
}

#[test]
fn the_original_does_not_change() {
    let input = corpus("es-01-contrato-arrendamiento.docx");
    let before = std::fs::read(&input).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    anonymize(
        &input,
        dir.path(),
        &Fake::for_corpus("es-01-contrato-arrendamiento"),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap();
    assert_eq!(sha256(&std::fs::read(&input).unwrap()), sha256(&before));
}

#[test]
fn the_store_records_the_document_and_its_entries() {
    let name = "es-01-contrato-arrendamiento";
    let input = corpus(&format!("{name}.docx"));
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let summary = anonymize(
        &input,
        dir.path(),
        &Fake::for_corpus(name),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap();

    assert_eq!(store.documents().len(), 1);
    let record = &store.documents()[0];
    assert_eq!(record.id, summary.document_id);
    assert!(uuid::Uuid::parse_str(&record.id).is_ok(), "{}", record.id);
    assert_eq!(record.created, "2026-10-30T10:14:03+01:00");
    assert_eq!(record.expires, "2027-01-28".parse().unwrap());
    assert_eq!(record.app_version, VERSION);
    assert_eq!(record.model, MODEL);
    assert_eq!(record.origin.name, format!("{name}.docx"));
    assert_eq!(
        record.origin.sha256,
        sha256(&std::fs::read(&input).unwrap())
    );
    assert_eq!(record.output.name, format!("{name}_anonimizado.docx"));
    assert_eq!(record.output.format, "docx");

    // Un marcador por valor distinto, sin repetir, en orden de aparición.
    let entities = expected(name)["entidades"].as_array().unwrap().len();
    assert_eq!(record.markers.len(), entities, "{:?}", record.markers);
    assert_eq!(record.markers[0], "[LUGAR_0001]");
    assert_eq!(record.markers[1], "[NOMBRE_PERSONA_0001]");
    let lucia = store.entry_for_marker("[NOMBRE_PERSONA_0001]").unwrap();
    assert_eq!(lucia.value(), "Lucía Ferrer Ibáñez");
    assert_eq!(lucia.model_label(), "ETIQUETA_NOMBRE_PERSONA");
    assert_eq!(lucia.documents(), std::slice::from_ref(&record.id));
    for marker in &record.markers {
        assert!(store.entry_for_marker(marker).is_some(), "{marker}");
    }
}

#[test]
fn the_same_value_twice_gets_one_marker() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(
        dir.path(),
        "carta.txt",
        "Ana López firma.\n\nOtra vez Ana López.\n",
    );
    let out = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let summary = anonymize(
        &input,
        out.path(),
        &Fake::new(&[("Ana López", "NOMBRE_PERSONA")]),
        &mut store,
        &options(OutputFormat::Text),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&summary.output).unwrap(),
        "[NOMBRE_PERSONA_0001] firma.\n\nOtra vez [NOMBRE_PERSONA_0001].\n"
    );
    assert_eq!(store.documents()[0].markers, ["[NOMBRE_PERSONA_0001]"]);
    assert_eq!(
        summary.entities,
        BTreeMap::from([("NOMBRE_PERSONA".to_owned(), 2)])
    );
}

#[test]
fn the_summary_counts_entities_by_type_and_gives_the_warnings() {
    let name = "fr-01-fiche-client";
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let summary = anonymize(
        &corpus(&format!("{name}.docx")),
        dir.path(),
        &Fake::for_corpus(name),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap();
    let json = expected(name);
    assert_eq!(
        summary.warnings.images,
        json["imagenes"].as_u64().unwrap() as usize
    );
    assert_eq!(
        summary.warnings.objects,
        json["objetos"].as_u64().unwrap() as usize
    );
    let document =
        DocxDocument::from_bytes(&std::fs::read(corpus(&format!("{name}.docx"))).unwrap()).unwrap();
    let texts: Vec<&str> = document
        .segments()
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entity in Fake::for_corpus(name).detect(&texts).unwrap() {
        *counts.entry(entity.kind).or_default() += 1;
    }
    assert_eq!(summary.entities, counts);
}

#[test]
fn text_and_markdown_keep_their_text_with_either_extension() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(
        dir.path(),
        "Nota Rápida.md",
        "# Para Ana López\r\n\r\nGracias.\r\n",
    );
    for (format, file, extension) in [
        (OutputFormat::Markdown, "nota-rapida_anonimizado.md", "md"),
        (OutputFormat::Text, "nota-rapida_anonimizado.txt", "txt"),
    ] {
        let out = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = anonymize(
            &input,
            out.path(),
            &Fake::new(&[("Ana López", "NOMBRE_PERSONA")]),
            &mut store,
            &options(format),
        )
        .unwrap();
        assert_eq!(summary.output, out.path().join(file));
        assert_eq!(
            std::fs::read_to_string(&summary.output).unwrap(),
            "# Para [NOMBRE_PERSONA_0001]\r\n\r\nGracias.\r\n"
        );
        assert_eq!(store.documents()[0].output.format, extension);
    }
}

#[test]
fn a_docx_can_become_markdown_or_text() {
    let name = "es-01-contrato-arrendamiento";
    for (format, extension) in [(OutputFormat::Markdown, "md"), (OutputFormat::Text, "txt")] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = anonymize(
            &corpus(&format!("{name}.docx")),
            dir.path(),
            &Fake::for_corpus(name),
            &mut store,
            &options(format),
        )
        .unwrap();
        assert_eq!(
            summary.output,
            dir.path().join(format!("{name}_anonimizado.{extension}"))
        );
        let text = std::fs::read_to_string(&summary.output).unwrap();
        assert!(
            text.contains("doña [NOMBRE_PERSONA_0001], con DNI"),
            "{text}"
        );
        assert!(!text.contains("Lucía"), "{text}");
        assert_eq!(store.documents()[0].output.format, extension);
    }
    let dir = tempfile::tempdir().unwrap();
    let summary = anonymize(
        &corpus(&format!("{name}.docx")),
        dir.path(),
        &Fake::for_corpus(name),
        &mut Store::new(),
        &options(OutputFormat::Markdown),
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(&summary.output)
            .unwrap()
            .starts_with("# Contrato")
    );
}

#[test]
fn text_cannot_become_docx_and_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "nota.txt", "Hola Ana López.\n");
    let out = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let error = anonymize(
        &input,
        out.path(),
        &Fake::new(&[("Ana López", "NOMBRE_PERSONA")]),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::UnsupportedConversion { .. }),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Un archivo TXT no se puede convertir a DOCX."
    );
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(store.documents().is_empty());
    assert_eq!(store.counter("NOMBRE_PERSONA"), 0);
}

#[test]
fn an_unknown_format_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "informe.rtf", "{\\rtf1 Hola}");
    let error = anonymize(
        &input,
        dir.path(),
        &Fake::new(&[]),
        &mut Store::new(),
        &options(OutputFormat::Text),
    )
    .unwrap_err();
    assert!(matches!(error, Error::UnsupportedFormat(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "El formato «rtf» no se admite; use DOCX, MD o TXT."
    );
}

#[test]
fn a_docx_is_recognised_by_its_content_whatever_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("contrato.bin");
    std::fs::copy(corpus("es-01-contrato-arrendamiento.docx"), &input).unwrap();
    let out = tempfile::tempdir().unwrap();
    let summary = anonymize(
        &input,
        out.path(),
        &Fake::for_corpus("es-01-contrato-arrendamiento"),
        &mut Store::new(),
        &options(OutputFormat::Docx),
    )
    .unwrap();
    assert_eq!(summary.output, out.path().join("contrato_anonimizado.docx"));
}

#[test]
fn a_protected_docx_fails_without_touching_anything() {
    let out = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let error = anonymize(
        &corpus("es-02-protegido.docx"),
        out.path(),
        &Fake::new(&[]),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap_err();
    assert!(matches!(error, Error::DocxProtected), "{error:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(store.documents().is_empty());
}

#[test]
fn an_existing_output_is_never_overwritten() {
    let name = "es-01-contrato-arrendamiento";
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let run = |store: &mut Store| {
        anonymize(
            &corpus(&format!("{name}.docx")),
            dir.path(),
            &Fake::for_corpus(name),
            store,
            &options(OutputFormat::Docx),
        )
        .unwrap()
    };
    let first = run(&mut store);
    let second = run(&mut store);
    assert_eq!(
        second.output,
        dir.path().join(format!("{name}_anonimizado_2.docx"))
    );
    assert_ne!(first.document_id, second.document_id);
    assert_eq!(store.documents().len(), 2);
    // El segundo documento reutiliza los marcadores del primero.
    assert_eq!(store.documents()[0].markers, store.documents()[1].markers);
}

#[test]
fn if_the_output_cannot_be_written_the_store_keeps_no_trace() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "nota.txt", "Hola Ana López.\n");
    let mut store = Store::new();
    let error = anonymize(
        &input,
        &dir.path().join("no-existe"),
        &Fake::new(&[("Ana López", "NOMBRE_PERSONA")]),
        &mut store,
        &options(OutputFormat::Text),
    )
    .unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error:?}");
    assert!(store.documents().is_empty());
    assert_eq!(store.entries().count(), 0);
}

#[test]
fn a_failed_run_does_not_take_values_from_earlier_documents() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "nota.txt", "Hola Ana López.\n");
    let mut store = Store::new();
    let fake = Fake::new(&[("Ana López", "NOMBRE_PERSONA")]);
    let first = anonymize(
        &input,
        dir.path(),
        &fake,
        &mut store,
        &options(OutputFormat::Text),
    )
    .unwrap();
    anonymize(
        &input,
        &dir.path().join("no-existe"),
        &fake,
        &mut store,
        &options(OutputFormat::Text),
    )
    .unwrap_err();
    let entry = store.entry_for_marker("[NOMBRE_PERSONA_0001]").unwrap();
    assert_eq!(entry.documents(), [first.document_id]);
}

#[test]
fn the_model_id_comes_from_the_lock() {
    assert_eq!(model_id("modelo"), MODEL);
    assert_eq!(model_id("otro-modelo"), "otro-modelo");
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn with_the_real_model_no_expected_value_is_left_in_the_corpus() {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    let engine = Engine::load(
        &resources,
        &resources.model("modelo"),
        EngineOptions::default(),
    )
    .unwrap();
    let mut store = Store::new();
    for name in CORPUS {
        let dir = tempfile::tempdir().unwrap();
        let input = corpus(&format!("{name}.docx"));
        let before = sha256(&std::fs::read(&input).unwrap());
        let summary = anonymize(
            &input,
            dir.path(),
            &engine,
            &mut store,
            &AnonymizeOptions {
                format: OutputFormat::Docx,
                model: model_id("modelo"),
                now: Zoned::now(),
            },
        )
        .unwrap();
        let text = docx_text(&std::fs::read(&summary.output).unwrap());
        for entity in expected(name)["entidades"].as_array().unwrap() {
            let value = entity["texto"].as_str().unwrap();
            assert!(!text.contains(value), "{name}: queda {value:?}");
        }
        assert_eq!(sha256(&std::fs::read(&input).unwrap()), before, "{name}");
        let record = store.documents().last().unwrap();
        assert_eq!(record.id, summary.document_id);
        assert!(!record.markers.is_empty(), "{name}");
    }
}

#[test]
fn markdown_cannot_become_docx_either() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "nota.md", "# Hola\n");
    let error = anonymize(
        &input,
        dir.path(),
        &Fake::new(&[]),
        &mut Store::new(),
        &options(OutputFormat::Docx),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Un archivo MD no se puede convertir a DOCX."
    );
}

#[test]
fn the_extension_is_read_without_case() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_text(dir.path(), "NOTA.TXT", "Hola Ana López.\n");
    let out = tempfile::tempdir().unwrap();
    let summary = anonymize(
        &input,
        out.path(),
        &Fake::new(&[("Ana López", "NOMBRE_PERSONA")]),
        &mut Store::new(),
        &options(OutputFormat::Text),
    )
    .unwrap();
    assert_eq!(summary.output, out.path().join("nota_anonimizado.txt"));
}
