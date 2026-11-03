//! Anonimizar y desanonimizar PowerPoint y Excel de principio a fin con el
//! corpus de `tests/corpus/pptx` y `tests/corpus/xlsx`.
//!
//! Casi todo se prueba con un detector falso que encuentra los valores
//! esperados del corpus; la prueba con el modelo real va aparte (`#[ignore]`).

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use jiff::Zoned;
use nucleo::almacen::Store;
use nucleo::error::Office;
use nucleo::formatos::pdf::DEFAULT_RESOLUTION;
use nucleo::formatos::pptx::PptxDocument;
use nucleo::formatos::xlsx::XlsxDocument;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::operaciones::anonimizar::{AnonymizeOptions, Summary, anonymize};
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize};
use nucleo::operaciones::{Detector, OutputFormat, model_id};
use nucleo::recursos::Resources;
use nucleo::{Error, Result};

const PPTX: (&str, &str) = ("pptx", "es-01-presentacion-ventas");
const XLSX: (&str, &str) = ("xlsx", "es-01-nominas");

fn corpus((folder, name): (&str, &str), extension: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus")
        .join(folder)
        .join(format!("{name}.{extension}"))
}

fn file(which: (&str, &str)) -> PathBuf {
    corpus(which, which.0)
}

fn expected(which: (&str, &str)) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(corpus(which, "json")).unwrap()).unwrap()
}

/// Valores esperados: (texto, tipo).
fn values(which: (&str, &str)) -> Vec<(String, String)> {
    expected(which)["entidades"]
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

/// Detector que encuentra unos valores fijos, los largos primero y sin
/// solapes, como haría el modelo.
struct Fake(Vec<(String, String)>);

impl Fake {
    fn for_corpus(which: (&str, &str)) -> Fake {
        let mut values = values(which);
        values.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
        Fake(values)
    }
}

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
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
                    taken[span.clone()].iter_mut().for_each(|t| *t = true);
                    entities.push(Entity {
                        segment,
                        start: span.start,
                        end: span.end,
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

fn options(format: OutputFormat, spreadsheet_amounts: bool) -> AnonymizeOptions {
    AnonymizeOptions {
        format,
        model: model_id("modelo"),
        now: Zoned::now(),
        spreadsheet_amounts,
        pdf_resolution: DEFAULT_RESOLUTION,
    }
}

/// Todo el XML del paquete, parte a parte.
fn parts(bytes: &[u8]) -> Vec<(String, String)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .filter_map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let name = file.name().unwrap().into_owned();
            let mut text = String::new();
            file.read_to_string(&mut text).ok()?;
            Some((name, text))
        })
        .collect()
}

fn run(
    which: (&str, &str),
    format: OutputFormat,
    amounts: bool,
    store: &mut Store,
    dir: &Path,
) -> Summary {
    anonymize(
        &file(which),
        dir,
        &Fake::for_corpus(which),
        store,
        &options(format, amounts),
    )
    .unwrap()
}

fn assert_gone(which: (&str, &str), label: &str, text: &str, keep_amounts: bool) {
    for (value, kind) in values(which) {
        if keep_amounts && kind == "IMPORTE" {
            continue;
        }
        assert!(!text.contains(&value), "{label}: queda «{value}»");
    }
}

#[test]
fn no_expected_value_is_left_in_any_part() {
    for (which, format) in [(PPTX, OutputFormat::Pptx), (XLSX, OutputFormat::Xlsx)] {
        let dir = tempfile::tempdir().unwrap();
        let summary = run(which, format, true, &mut Store::new(), dir.path());
        assert_eq!(
            summary.output,
            dir.path()
                .join(format!("{}_anonimizado.{}", which.1, which.0))
        );
        for (name, xml) in parts(&std::fs::read(&summary.output).unwrap()) {
            assert_gone(which, &format!("{}/{name}", which.1), &xml, false);
            for author in expected(which)["autores"].as_array().unwrap() {
                let author = author.as_str().unwrap();
                assert!(
                    !xml.contains(&format!("\"{author}\""))
                        && !xml.contains(&format!(">{author}<")),
                    "{}/{name}: queda el autor «{author}»",
                    which.1
                );
            }
        }
    }
}

#[test]
fn the_summary_counts_types_and_gives_the_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let summary = run(
        PPTX,
        OutputFormat::Pptx,
        false,
        &mut Store::new(),
        dir.path(),
    );
    assert!(summary.entities["NOMBRE_PERSONA"] > 0);
    let json = expected(PPTX);
    assert_eq!(
        summary.warnings.images,
        json["imagenes"].as_u64().unwrap() as usize
    );
    assert_eq!(
        summary.warnings.objects,
        json["objetos"].as_u64().unwrap() as usize
    );
}

#[test]
fn amounts_stay_in_excel_unless_asked() {
    let dir = tempfile::tempdir().unwrap();
    let default = run(
        XLSX,
        OutputFormat::Xlsx,
        false,
        &mut Store::new(),
        dir.path(),
    );
    let sheet = parts(&std::fs::read(&default.output).unwrap())
        .into_iter()
        .find(|(name, _)| name == "xl/worksheets/sheet1.xml")
        .unwrap()
        .1;
    assert!(sheet.contains("<v>2150.75</v>"), "{sheet}");
    assert!(!default.entities.contains_key("IMPORTE"));
    assert_gone(XLSX, "por defecto", &sheet, true);

    let asked = run(
        XLSX,
        OutputFormat::Xlsx,
        true,
        &mut Store::new(),
        dir.path(),
    );
    assert_eq!(asked.entities["IMPORTE"], 2);
}

#[test]
fn a_sheet_named_after_a_person_is_renamed_and_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    let summary = run(XLSX, OutputFormat::Xlsx, false, &mut store, dir.path());
    let book = XlsxDocument::from_bytes(&std::fs::read(&summary.output).unwrap()).unwrap();
    let renamed = expected(XLSX)["hoja_renombrada"][1]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        book.segments()[0].text.starts_with(&renamed),
        "{}",
        book.segments()[0].text
    );

    let back = tempfile::tempdir().unwrap();
    let Outcome::Done((restored, _)) =
        deanonymize(&summary.output, back.path(), &store, None).unwrap()
    else {
        panic!("conflictos con un solo almacén");
    };
    let book = XlsxDocument::from_bytes(&std::fs::read(restored).unwrap()).unwrap();
    assert_eq!(
        book.segments()[0].text,
        expected(XLSX)["hoja_renombrada"][0]
    );
}

#[test]
fn the_corpus_comes_back_exactly_after_a_round_trip() {
    for (which, format) in [(PPTX, OutputFormat::Pptx), (XLSX, OutputFormat::Xlsx)] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = run(which, format, true, &mut store, dir.path());
        let back = tempfile::tempdir().unwrap();
        let Outcome::Done((restored, report)) =
            deanonymize(&summary.output, back.path(), &store, None).unwrap()
        else {
            panic!("conflictos con un solo almacén");
        };
        assert!(report.missing.is_empty(), "{:?}", report.missing);
        assert_eq!(
            restored,
            back.path().join(format!(
                "{}-anonimizado_desanonimizado.{}",
                which.1, which.0
            ))
        );
        let original = std::fs::read(file(which)).unwrap();
        let restored = std::fs::read(restored).unwrap();
        match format {
            OutputFormat::Pptx => {
                let (a, b) = (
                    PptxDocument::from_bytes(&original).unwrap(),
                    PptxDocument::from_bytes(&restored).unwrap(),
                );
                let texts = |d: &PptxDocument| {
                    d.segments()
                        .iter()
                        .map(|s| s.text.clone())
                        .collect::<Vec<_>>()
                };
                assert_eq!(texts(&b), texts(&a), "{}", which.1);
                assert_eq!(b.literal_texts(), a.literal_texts(), "{}", which.1);
            }
            _ => {
                let (a, b) = (
                    XlsxDocument::from_bytes(&original).unwrap(),
                    XlsxDocument::from_bytes(&restored).unwrap(),
                );
                let texts = |d: &XlsxDocument| {
                    d.segments()
                        .iter()
                        .map(|s| s.text.clone())
                        .collect::<Vec<_>>()
                };
                assert_eq!(texts(&b), texts(&a), "{}", which.1);
                assert_eq!(b.literal_texts(), a.literal_texts(), "{}", which.1);
            }
        }
    }
}

#[test]
fn markdown_and_text_carry_no_expected_value() {
    for which in [PPTX, XLSX] {
        for format in [OutputFormat::Markdown, OutputFormat::Text] {
            let dir = tempfile::tempdir().unwrap();
            let summary = run(which, format, false, &mut Store::new(), dir.path());
            let text = std::fs::read_to_string(&summary.output).unwrap();
            assert!(text.contains("[NOMBRE_PERSONA_"), "{text}");
            assert_gone(
                which,
                &format!("{} {}", which.1, format.label()),
                &text,
                which == XLSX,
            );
        }
    }
}

#[test]
fn office_files_are_recognised_by_their_content_and_old_ones_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // Un libro con extensión de Word sigue siendo un libro.
    let disguised = dir.path().join("informe.docx");
    std::fs::copy(file(XLSX), &disguised).unwrap();
    let out = tempfile::tempdir().unwrap();
    let summary = anonymize(
        &disguised,
        out.path(),
        &Fake::for_corpus(XLSX),
        &mut Store::new(),
        &options(OutputFormat::Xlsx, false),
    )
    .unwrap();
    assert_eq!(summary.output, out.path().join("informe_anonimizado.xlsx"));

    let ole = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    for (name, office) in [
        ("viejo.xls", Office::Excel),
        ("viejo.ppt", Office::PowerPoint),
        ("viejo.doc", Office::Word),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, ole).unwrap();
        let error = anonymize(
            &path,
            out.path(),
            &Fake(Vec::new()),
            &mut Store::new(),
            &options(OutputFormat::Text, false),
        )
        .unwrap_err();
        assert!(
            matches!(error, Error::OldOfficeFormat(o) if o == office),
            "{error:?}"
        );
    }
    let protected = dir.path().join("protegido.xlsx");
    std::fs::write(&protected, ole).unwrap();
    let error = anonymize(
        &protected,
        out.path(),
        &Fake(Vec::new()),
        &mut Store::new(),
        &options(OutputFormat::Xlsx, false),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::Protected(Office::Excel)),
        "{error:?}"
    );

    // Cada formato de Office sale en el suyo, en MD o en TXT.
    let error = anonymize(
        &file(PPTX),
        out.path(),
        &Fake(Vec::new()),
        &mut Store::new(),
        &options(OutputFormat::Docx, false),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Un archivo PPTX no se puede convertir a DOCX."
    );
    assert!(
        std::fs::read_dir(out.path()).unwrap().count() == 1,
        "no debe escribir nada más"
    );
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
    for (which, format) in [(PPTX, OutputFormat::Pptx), (XLSX, OutputFormat::Xlsx)] {
        let dir = tempfile::tempdir().unwrap();
        let summary = anonymize(
            &file(which),
            dir.path(),
            &engine,
            &mut Store::new(),
            &options(format, false),
        )
        .unwrap();
        let all: String = parts(&std::fs::read(&summary.output).unwrap())
            .into_iter()
            .map(|(_, xml)| xml)
            .collect();
        let left: Vec<String> = values(which)
            .into_iter()
            .filter(|(value, kind)| kind != "IMPORTE" && all.contains(value))
            .map(|(value, kind)| format!("{kind} «{value}»"))
            .collect();
        assert!(left.is_empty(), "{}: quedan {left:?}", which.1);
    }
}
