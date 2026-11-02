//! Medidas frente a los objetivos del PRD: un DOCX de unas diez páginas de
//! principio a fin con el modelo ya cargado, y un almacén con 1.000
//! documentos. No son pruebas de tiempo con límite, porque el tiempo depende
//! de la máquina y de si está caliente: imprimen las medidas para el informe
//! de detección y comprueban que el resultado es correcto.
//!
//! ```bash
//! cargo test --release -p nucleo --test rendimiento -- --include-ignored --nocapture --test-threads=1
//! ```

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jiff::Zoned;
use nucleo::almacen::Store;
use nucleo::almacen::archivo::{self, MemoryKey, StoreKey};
use nucleo::formatos::docx::DocxDocument;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize};
use nucleo::operaciones::{Detector, OutputFormat, model_id};
use nucleo::recursos::Resources;

/// Ejecuciones de cada medida; se da la mediana.
const RUNS: usize = 3;

fn ten_pages() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus/rendimiento/diez-paginas.txt");
    // En Windows, git puede dejar el archivo con saltos `\r\n`.
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// DOCX con un párrafo por cada párrafo del texto. Cada frase va en su
/// propio fragmento (`w:r`), y una de cada tres en negrita, como queda un
/// documento de Word en el que se ha editado el formato.
fn docx_from_text(text: &str) -> Vec<u8> {
    let mut body = String::new();
    for paragraph in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        body.push_str("<w:p>");
        for (i, sentence) in paragraph.split_inclusive(". ").enumerate() {
            let props = if i % 3 == 1 {
                "<w:rPr><w:b/></w:rPr>"
            } else {
                ""
            };
            body.push_str(&format!(
                "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                escape(sentence)
            ));
        }
        body.push_str("</w:p>");
    }
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in [
        ("[Content_Types].xml", "<Types/>".to_owned()),
        (
            "word/document.xml",
            format!("<w:document {w}><w:body>{body}</w:body></w:document>"),
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn segments_text(path: &Path) -> Vec<String> {
    DocxDocument::from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .segments()
        .iter()
        .map(|s| s.text.clone())
        .collect()
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn seconds(time: Duration) -> String {
    format!("{:.2} s", time.as_secs_f64())
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn a_ten_page_docx_from_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("diez-paginas.docx");
    std::fs::write(&input, docx_from_text(&ten_pages())).unwrap();
    let original = segments_text(&input);
    assert!(original.len() > 300, "{} párrafos", original.len());
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");

    for threads in [2, 4] {
        let start = Instant::now();
        let engine = Engine::load(
            &resources,
            &resources.model("modelo"),
            EngineOptions {
                threads,
                ..EngineOptions::default()
            },
        )
        .unwrap();
        let load = start.elapsed();

        let refs: Vec<&str> = original.iter().map(String::as_str).collect();
        let (mut detecting, mut anonymizing, mut restoring) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..RUNS {
            // Solo el modelo sobre los mismos párrafos, justo antes, para
            // separar lo que añade el resto con la máquina en el mismo estado.
            let start = Instant::now();
            engine.detect(&refs).unwrap();
            detecting.push(start.elapsed());

            let out = tempfile::tempdir().unwrap();
            let mut store = Store::new();
            let start = Instant::now();
            let summary = anonymize(
                &input,
                out.path(),
                &engine,
                &mut store,
                &AnonymizeOptions {
                    format: OutputFormat::Docx,
                    model: model_id("modelo"),
                    now: Zoned::now(),
                    spreadsheet_amounts: false,
                },
            )
            .unwrap();
            anonymizing.push(start.elapsed());
            assert!(!summary.entities.is_empty());

            let back = tempfile::tempdir().unwrap();
            let start = Instant::now();
            let Outcome::Done((restored, report)) =
                deanonymize(&summary.output, back.path(), &store, None).unwrap()
            else {
                panic!("conflictos con un solo almacén");
            };
            restoring.push(start.elapsed());
            assert!(report.missing.is_empty(), "{:?}", report.missing);
            assert_eq!(segments_text(&restored), original, "ida y vuelta");
        }
        println!(
            "DOCX de diez páginas, {threads} hilos: carga {}, solo el modelo {}, anonimizar {}, desanonimizar {} (mediana de {RUNS})",
            seconds(load),
            seconds(median(detecting)),
            seconds(median(anonymizing)),
            seconds(median(restoring)),
        );
    }
}

/// Detector que toma cada párrafo entero como un nombre.
struct EveryParagraph;

impl Detector for EveryParagraph {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        Ok(segments
            .iter()
            .enumerate()
            .map(|(segment, text)| Entity {
                segment,
                start: 0,
                end: text.chars().count(),
                kind: "NOMBRE_PERSONA".to_owned(),
                label: "PERSON_NAME".to_owned(),
                text: (*text).to_owned(),
                score: 0.99,
            })
            .collect())
    }
}

/// Lo que cuesta todo menos el modelo: leer el DOCX, el almacén, sustituir y
/// escribir. Cada párrafo entero es un dato, el peor caso para el escritor.
#[test]
fn a_ten_page_docx_without_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("diez-paginas.docx");
    std::fs::write(&input, docx_from_text(&ten_pages())).unwrap();
    let original = segments_text(&input);
    let (mut anonymizing, mut restoring) = (Vec::new(), Vec::new());
    for _ in 0..RUNS {
        let out = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let start = Instant::now();
        let summary = anonymize(
            &input,
            out.path(),
            &EveryParagraph,
            &mut store,
            &AnonymizeOptions {
                format: OutputFormat::Docx,
                model: model_id("modelo"),
                now: Zoned::now(),
                spreadsheet_amounts: false,
            },
        )
        .unwrap();
        anonymizing.push(start.elapsed());
        let back = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let Outcome::Done((restored, _)) =
            deanonymize(&summary.output, back.path(), &store, None).unwrap()
        else {
            panic!("conflictos con un solo almacén");
        };
        restoring.push(start.elapsed());
        assert_eq!(segments_text(&restored), original, "ida y vuelta");
    }
    println!(
        "DOCX de diez páginas sin el modelo, {} párrafos sustituidos: anonimizar {}, desanonimizar {} (mediana de {RUNS})",
        original.len(),
        seconds(median(anonymizing)),
        seconds(median(restoring)),
    );
}

/// Valores distintos de cada documento del almacén.
const VALUES_PER_DOCUMENT: usize = 30;
const DOCUMENTS: usize = 1_000;

fn anonymize_text(dir: &Path, store: &mut Store, n: usize) -> PathBuf {
    let text: Vec<String> = (0..VALUES_PER_DOCUMENT)
        .map(|v| format!("Persona {n:04} número {v:02}"))
        .collect();
    let input = dir.join(format!("documento-{n:04}.txt"));
    std::fs::write(&input, text.join("\n\n")).unwrap();
    anonymize(
        &input,
        dir,
        &EveryParagraph,
        store,
        &AnonymizeOptions {
            format: OutputFormat::Text,
            model: model_id("modelo"),
            now: Zoned::now(),
            spreadsheet_amounts: false,
        },
    )
    .unwrap()
    .output
}

#[test]
fn a_store_with_a_thousand_documents() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::new();
    for n in 0..DOCUMENTS {
        anonymize_text(dir.path(), &mut store, n);
    }
    assert_eq!(store.documents().len(), DOCUMENTS);
    assert_eq!(store.entries().count(), DOCUMENTS * VALUES_PER_DOCUMENT);

    let keys = MemoryKey::with_key(StoreKey::generate().unwrap());
    let path = dir.path().join("almacen.anon");
    let (mut saving, mut loading, mut one_more) = (Vec::new(), Vec::new(), Vec::new());
    for run in 0..RUNS {
        let start = Instant::now();
        archivo::save(&path, &store, &keys).unwrap();
        saving.push(start.elapsed());

        let start = Instant::now();
        let mut loaded = archivo::load(&path, &keys).unwrap();
        loading.push(start.elapsed());
        assert_eq!(loaded.documents().len(), DOCUMENTS);

        // Un documento más sobre el almacén lleno: asignar sus marcadores no
        // debe crecer con el tamaño del almacén.
        let start = Instant::now();
        anonymize_text(dir.path(), &mut loaded, DOCUMENTS + run);
        one_more.push(start.elapsed());
    }
    let size = std::fs::metadata(&path).unwrap().len();
    println!(
        "Almacén con {DOCUMENTS} documentos y {} entradas: {:.1} MB, guardar {}, abrir {}, un documento más {} (mediana de {RUNS})",
        DOCUMENTS * VALUES_PER_DOCUMENT,
        size as f64 / 1_000_000.0,
        seconds(median(saving)),
        seconds(median(loading)),
        seconds(median(one_more)),
    );
}
