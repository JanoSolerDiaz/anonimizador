//! Anonimizar un PDF de principio a fin con un detector falso: salida PDF
//! verificada, formas cortas en el almacén, sustitución literal en otras
//! páginas, salidas DOCX, MD y TXT, y errores que no dejan nada a medias.
//!
//! Necesitan PDFium en `recursos/`, por eso están ignoradas por defecto:
//!
//!   cargo test -p nucleo --test anonimizar_pdf -- --include-ignored

use std::path::Path;

use jiff::Zoned;
use pdfium_render::prelude::*;

use nucleo::almacen::Store;
use nucleo::formatos::pdf::{self, DEFAULT_RESOLUTION};
use nucleo::motor::Entity;
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize, deanonymize_text};
use nucleo::operaciones::{Detector, OutputFormat};
use nucleo::{Error, Result};

/// Encuentra unos valores fijos, en todas las páginas o solo en la primera.
struct Fake {
    values: Vec<(&'static str, &'static str)>,
    first_page_only: bool,
}

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            if self.first_page_only && segment > 0 {
                continue;
            }
            for (value, kind) in &self.values {
                for (start, end) in pdf::find_value(text, value) {
                    entities.push(Entity {
                        segment,
                        start,
                        end,
                        kind: (*kind).to_owned(),
                        label: format!("ETIQUETA_{kind}"),
                        text: text.chars().skip(start).take(end - start).collect(),
                        score: 0.99,
                    });
                }
            }
        }
        Ok(entities)
    }
}

fn fake(first_page_only: bool) -> Fake {
    Fake {
        values: vec![
            ("Marta González Ruiz", "NOMBRE_PERSONA"),
            ("12345678Z", "ID_DOCUMENTO_PERSONA"),
            ("Ana", "NOMBRE_PERSONA"),
        ],
        first_page_only,
    }
}

fn options(format: OutputFormat) -> AnonymizeOptions {
    AnonymizeOptions {
        format,
        model: "modelo-de-prueba".to_owned(),
        now: "2026-11-03T10:14:03+01:00[Europe/Madrid]"
            .parse::<Zoned>()
            .unwrap(),
        spreadsheet_amounts: false,
        pdf_resolution: DEFAULT_RESOLUTION,
    }
}

/// PDF con un contrato en la página 1 y una firma en la 2, con un adjunto.
fn write_contract(dir: &Path) -> std::path::PathBuf {
    let bytes = {
        let pdfium = pdf::pdfium().unwrap();
        let mut document = pdfium.create_new_pdf().unwrap();
        let font = document.fonts_mut().helvetica();
        let pages: [&[(f32, &str, f32)]; 2] = [
            &[
                (
                    760.0,
                    "Contrato con Marta Gonz\u{e1}lez Ruiz, con DNI 12345678Z.",
                    12.0,
                ),
                (700.0, "Testigo: Ana.", 6.0),
            ],
            &[(760.0, "Firmado: Marta Gonz\u{e1}lez Ruiz", 12.0)],
        ];
        for lines in pages {
            let mut page = document
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .unwrap();
            for &(y, text, size) in lines {
                page.objects_mut()
                    .create_text_object(
                        PdfPoints::new(72.0),
                        PdfPoints::new(y),
                        text,
                        font,
                        PdfPoints::new(size),
                    )
                    .unwrap();
            }
        }
        document
            .attachments_mut()
            .create_attachment_from_bytes("notas.txt", b"Marta")
            .unwrap();
        document.save_to_bytes().unwrap()
    };
    let path = dir.join("Contrato cliente.pdf");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn page_texts(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap();
    let pdfium = pdf::pdfium().unwrap();
    let document = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    document
        .pages()
        .iter()
        .map(|p| p.text().unwrap().all())
        .collect()
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_pdf_comes_out_as_a_verified_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_contract(dir.path());
    let mut store = Store::new();
    let summary = anonymize(
        &input,
        dir.path(),
        &fake(false),
        &mut store,
        &options(OutputFormat::Pdf),
    )
    .unwrap();

    assert_eq!(
        summary.output.file_name().unwrap(),
        "contrato-cliente_anonimizado.pdf"
    );
    assert_eq!(summary.entities["NOMBRE_PERSONA"], 3);
    assert_eq!(summary.entities["ID_DOCUMENTO_PERSONA"], 1);
    assert_eq!(summary.warnings.attachments, 1);

    let texts = page_texts(&summary.output);
    assert!(texts[0].contains("[NOMBRE_PERSONA_0001]"), "{texts:?}");
    // El DNI ocupa unos 62 pt: el marcador completo no llega a 5 pt.
    assert!(texts[0].contains("[I1]"), "{texts:?}");
    assert_eq!(texts[1], "[NOMBRE_PERSONA_0001]");
    let all = texts.join("\n");
    for value in ["Marta", "12345678Z", "Ana", "Contrato"] {
        assert!(!all.contains(value), "{value} sigue en la salida");
    }

    // «Ana», en letra pequeña, no cabe con el marcador completo.
    let ana = store
        .entries()
        .find(|e| e.value() == "Ana")
        .expect("Ana está en el almacén");
    assert_eq!(ana.short_form(), Some("[N1]"));
    assert!(texts[0].contains("[N1]"), "{texts:?}");

    let record = &store.documents()[0];
    assert_eq!(record.output.format, "pdf");
    assert_eq!(record.origin.name, "Contrato cliente.pdf");
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_value_found_on_one_page_is_covered_on_every_page() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_contract(dir.path());
    let mut store = Store::new();
    let summary = anonymize(
        &input,
        dir.path(),
        &fake(true),
        &mut store,
        &options(OutputFormat::Pdf),
    )
    .unwrap();
    let texts = page_texts(&summary.output);
    assert_eq!(texts[1], "[NOMBRE_PERSONA_0001]");
    // El recuento es de lo que detectó el modelo.
    assert_eq!(summary.entities["NOMBRE_PERSONA"], 2);
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_pdf_can_come_out_as_docx_md_or_txt_and_be_restored() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_contract(dir.path());
    let mut store = Store::new();
    let md = anonymize(
        &input,
        dir.path(),
        &fake(false),
        &mut store,
        &options(OutputFormat::Markdown),
    )
    .unwrap();
    let text = std::fs::read_to_string(&md.output).unwrap();
    assert_eq!(
        text,
        "## Página 1\n\nContrato con [NOMBRE_PERSONA_0001], con DNI [ID_DOCUMENTO_PERSONA_0001].\n\n\
         Testigo: [NOMBRE_PERSONA_0002].\n\n## Página 2\n\nFirmado: [NOMBRE_PERSONA_0001]\n"
    );
    match deanonymize_text(&text, &store, None).unwrap() {
        Outcome::Done((restored, report)) => {
            assert!(restored.contains("Contrato con Marta González Ruiz, con DNI 12345678Z."));
            assert!(report.missing.is_empty());
        }
        Outcome::Conflicts(c) => panic!("conflicto inesperado: {c:?}"),
    }

    let txt = anonymize(
        &input,
        dir.path(),
        &fake(false),
        &mut store,
        &options(OutputFormat::Text),
    )
    .unwrap();
    assert_eq!(txt.output.extension().unwrap(), "txt");

    let docx = anonymize(
        &input,
        dir.path(),
        &fake(false),
        &mut store,
        &options(OutputFormat::Docx),
    )
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    match deanonymize(&docx.output, out.path(), &store, None).unwrap() {
        Outcome::Done((path, report)) => {
            assert_eq!(path.extension().unwrap(), "docx");
            assert_eq!(report.restored.len(), 3);
        }
        Outcome::Conflicts(c) => panic!("conflicto inesperado: {c:?}"),
    }
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn an_anonymized_pdf_cannot_be_deanonymized() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_contract(dir.path());
    let mut store = Store::new();
    let summary = anonymize(
        &input,
        dir.path(),
        &fake(false),
        &mut store,
        &options(OutputFormat::Pdf),
    )
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(
        deanonymize(&summary.output, out.path(), &store, None),
        Err(Error::PdfNotDeanonymizable)
    ));
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_failed_pdf_leaves_no_output_and_no_store_entries() {
    let dir = tempfile::tempdir().unwrap();
    let input = write_contract(dir.path());
    let mut store = Store::new();
    let mut bad = options(OutputFormat::Pdf);
    bad.pdf_resolution = 400;
    assert!(matches!(
        anonymize(&input, dir.path(), &fake(false), &mut store, &bad),
        Err(Error::InvalidResolution(400))
    ));
    assert_eq!(store.entries().count(), 0);
    assert!(store.documents().is_empty());
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["Contrato cliente.pdf"]);
}

#[test]
fn a_pdf_does_not_become_pptx_or_xlsx() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.pdf");
    std::fs::write(&input, b"%PDF-1.4\n").unwrap();
    let mut store = Store::new();
    for format in [OutputFormat::Pptx, OutputFormat::Xlsx] {
        assert!(matches!(
            anonymize(
                &input,
                dir.path(),
                &fake(false),
                &mut store,
                &options(format)
            ),
            Err(Error::UnsupportedConversion { from: "PDF", .. })
        ));
    }
}

#[test]
fn a_pdf_extension_without_a_pdf_inside_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.pdf");
    std::fs::write(&input, b"no soy un pdf").unwrap();
    let mut store = Store::new();
    assert!(matches!(
        anonymize(
            &input,
            dir.path(),
            &fake(false),
            &mut store,
            &options(OutputFormat::Pdf)
        ),
        Err(Error::InvalidPdf(_))
    ));
}
