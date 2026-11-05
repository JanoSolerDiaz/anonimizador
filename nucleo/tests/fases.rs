//! Lo que la app necesita de `nucleo` además de la CLI: anonimizar en dos
//! fases con su progreso, e inspeccionar un archivo antes de procesarlo.

use std::cell::RefCell;
use std::path::Path;

use jiff::Zoned;

use nucleo::Error;
use nucleo::almacen::Store;
use nucleo::formatos::pdf::DEFAULT_RESOLUTION;
use nucleo::motor::Entity;
use nucleo::operaciones::anonimizar::{AnonymizeOptions, Stage, finish, prepare};
use nucleo::operaciones::{Detector, InputFormat, Inspection, OutputFormat, inspect};

/// Toma «Marta Lozano» como nombre e informa del progreso en dos pasos.
struct Fake;

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            if let Some(byte) = text.find("Marta Lozano") {
                let start = text[..byte].chars().count();
                entities.push(Entity {
                    segment,
                    start,
                    end: start + 12,
                    kind: "NOMBRE_PERSONA".to_owned(),
                    label: "PERSON_NAME".to_owned(),
                    text: "Marta Lozano".to_owned(),
                    score: 0.99,
                });
            }
        }
        Ok(entities)
    }

    fn detect_with_progress(
        &self,
        segments: &[&str],
        progress: &dyn Fn(f32),
    ) -> nucleo::Result<Vec<Entity>> {
        progress(0.5);
        let entities = self.detect(segments)?;
        progress(1.0);
        Ok(entities)
    }
}

fn options(format: OutputFormat) -> AnonymizeOptions {
    AnonymizeOptions {
        format,
        model: "modelo".to_owned(),
        now: Zoned::now(),
        spreadsheet_amounts: false,
        pdf_resolution: DEFAULT_RESOLUTION,
    }
}

fn write(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_two_phases_report_their_stages_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let input = write(dir.path(), "nota.txt", "Firma: Marta Lozano.\n");
    let stages = RefCell::new(Vec::new());
    let record = |stage: Stage| stages.borrow_mut().push(stage);

    let prepared = prepare(&input, &Fake, &options(OutputFormat::Text), &record).unwrap();
    assert_eq!(
        *stages.borrow(),
        [Stage::Reading, Stage::Analyzing(0.5), Stage::Analyzing(1.0)]
    );

    // Preparar no ha tocado ningún almacén; terminar sí.
    let mut store = Store::new();
    let summary = finish(
        prepared,
        dir.path(),
        &mut store,
        &options(OutputFormat::Text),
        &record,
    )
    .unwrap();
    assert_eq!(stages.borrow().last(), Some(&Stage::Writing));
    assert_eq!(summary.entities["NOMBRE_PERSONA"], 1);
    assert_eq!(
        std::fs::read_to_string(&summary.output).unwrap(),
        "Firma: [NOMBRE_PERSONA_0001].\n"
    );
    assert_eq!(store.documents().len(), 1);
}

#[test]
fn errors_while_preparing_leave_the_store_alone() {
    let dir = tempfile::tempdir().unwrap();
    let input = write(dir.path(), "nota.txt", "Hola.\n");
    assert!(matches!(
        prepare(&input, &Fake, &options(OutputFormat::Docx), &|_| {}),
        Err(Error::UnsupportedConversion { .. })
    ));
}

#[test]
fn inspecting_tells_the_format_and_its_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let text = write(dir.path(), "nota.txt", "Hola.\n");
    let docx = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx/es-01-contrato-arrendamiento.docx");
    for (path, format) in [(&text, InputFormat::Text), (&docx, InputFormat::Docx)] {
        match inspect(path) {
            Inspection::Supported(found) => assert_eq!(found, format),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        InputFormat::Docx.outputs(),
        [
            OutputFormat::Docx,
            OutputFormat::Markdown,
            OutputFormat::Text
        ]
    );
    assert_eq!(
        InputFormat::Pdf.outputs(),
        [
            OutputFormat::Pdf,
            OutputFormat::Docx,
            OutputFormat::Markdown,
            OutputFormat::Text
        ]
    );
    assert_eq!(
        InputFormat::Markdown.outputs(),
        [OutputFormat::Markdown, OutputFormat::Text]
    );
    assert!(!InputFormat::Pdf.deanonymizable());
    assert!(InputFormat::Xlsx.deanonymizable());
}

#[test]
fn inspecting_rejects_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let rtf = write(dir.path(), "carta.rtf", "{\\rtf1}");
    let old = write(dir.path(), "carta.doc", "\u{d0}");
    assert!(matches!(
        inspect(&rtf),
        Inspection::Rejected(Error::UnsupportedFormat(_))
    ));
    assert!(matches!(
        inspect(&old),
        Inspection::Rejected(Error::OldOfficeFormat(_))
    ));
    assert!(matches!(
        inspect(&dir.path().join("no-existe.txt")),
        Inspection::Rejected(Error::Io(_))
    ));
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn inspecting_a_pdf_tells_digital_from_scanned() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/pdf/es-01-contrato-servicios.pdf");
    assert!(matches!(
        inspect(&corpus),
        Inspection::Supported(InputFormat::Pdf)
    ));
    let dir = tempfile::tempdir().unwrap();
    let broken = write(dir.path(), "roto.pdf", "%PDF-1.4 basura");
    assert!(matches!(
        inspect(&broken),
        Inspection::Rejected(Error::InvalidPdf(_))
    ));
}
