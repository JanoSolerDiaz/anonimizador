//! Formato PDF (PRD, «Formato PDF»): lectura con cajas, páginas escaneadas,
//! contraseña, escritura con las páginas con datos rasterizadas, forma corta,
//! limpieza, verificación y salidas de texto.
//!
//! Los PDF se crean en cada prueba con PDFium y datos inventados. Necesitan
//! PDFium en `recursos/`, por eso están ignoradas por defecto:
//!
//!   cargo test -p nucleo --test formato_pdf -- --include-ignored

use image::{DynamicImage, GrayImage};
use pdfium_render::prelude::*;

use nucleo::Error;
use nucleo::formatos::Replacement;
use nucleo::formatos::pdf::{self, PdfDocument, WriteOptions, find_value, verify};

const NEEDS_PDFIUM: &str = "necesita PDFium: node scripts/descargar-binarios.mjs";

/// Una línea de texto: posición en puntos, texto y cuerpo.
type Line<'a> = (f32, f32, &'a str, f32);

/// PDF A4 con una página por lista de líneas.
fn build(
    pages: &[&[Line]],
    edit: impl FnOnce(&mut pdfium_render::prelude::PdfDocument),
) -> Vec<u8> {
    let pdfium = pdf::pdfium().unwrap();
    let mut document = pdfium.create_new_pdf().unwrap();
    let font = document.fonts_mut().helvetica();
    for lines in pages {
        let mut page = document
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .unwrap();
        for &(x, y, text, size) in *lines {
            page.objects_mut()
                .create_text_object(
                    PdfPoints::new(x),
                    PdfPoints::new(y),
                    text,
                    font,
                    PdfPoints::new(size),
                )
                .unwrap();
        }
    }
    edit(&mut document);
    document.save_to_bytes().unwrap()
}

fn simple(pages: &[&[Line]]) -> Vec<u8> {
    build(pages, |_| {})
}

/// Sustitución de la primera aparición de `value` en la página `segment`.
fn replace(document: &PdfDocument, segment: usize, value: &str, marker: &str) -> Replacement {
    let (start, end) = find_value(document.segments()[segment], value)[0];
    Replacement {
        segment,
        start,
        end,
        text: marker.to_owned(),
    }
}

fn no_short_form(_: &str) -> Option<String> {
    None
}

fn page_texts(bytes: &[u8]) -> Vec<String> {
    let pdfium = pdf::pdfium().unwrap();
    let document = pdfium.load_pdf_from_byte_slice(bytes, None).unwrap();
    document
        .pages()
        .iter()
        .map(|p| p.text().unwrap().all())
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

const CONTRACT: &[Line] = &[
    (72.0, 760.0, "Contrato con Marta Gonz-", 12.0),
    (72.0, 745.0, "\u{e1}lez Ruiz, con DNI 12345678Z.", 12.0),
    (72.0, 700.0, "Segundo p\u{e1}rrafo aqu\u{ed}.", 12.0),
];
const NO_DATA: &[Line] = &[(72.0, 760.0, "P\u{e1}gina sin datos.", 12.0)];

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn each_page_is_a_segment_with_line_end_hyphens_joined() {
    let _ = NEEDS_PDFIUM;
    let document = PdfDocument::from_bytes(&simple(&[CONTRACT, NO_DATA])).unwrap();
    let segments = document.segments();
    assert_eq!(segments.len(), 2);
    assert!(
        segments[0].starts_with("Contrato con Marta González Ruiz, con DNI 12345678Z.\n"),
        "{:?}",
        segments[0]
    );
    assert_eq!(segments[1], "Página sin datos.");
    // La «z» antes del guion lleva también la caja del guion.
    let z = segments[0].chars().position(|c| c == 'á').unwrap() - 1;
    assert_eq!(document.pages()[0].boxes(z).len(), 2);
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn text_outputs_have_one_paragraph_per_block() {
    let document = PdfDocument::from_bytes(&simple(&[CONTRACT, NO_DATA])).unwrap();
    let marta = replace(&document, 0, "Marta González Ruiz", "[NOMBRE_PERSONA_0001]");
    let pages = document.paragraphs(std::slice::from_ref(&marta)).unwrap();
    assert_eq!(
        pages,
        vec![
            vec![
                "Contrato con [NOMBRE_PERSONA_0001], con DNI 12345678Z.".to_owned(),
                "Segundo párrafo aquí.".to_owned(),
            ],
            vec!["Página sin datos.".to_owned()],
        ]
    );
    assert_eq!(
        document.markdown(std::slice::from_ref(&marta)).unwrap(),
        "## Página 1\n\nContrato con [NOMBRE_PERSONA_0001], con DNI 12345678Z.\n\n\
         Segundo párrafo aquí.\n\n## Página 2\n\nPágina sin datos.\n"
    );
    assert_eq!(
        document.plain_text(std::slice::from_ref(&marta)).unwrap(),
        "Contrato con [NOMBRE_PERSONA_0001], con DNI 12345678Z.\nSegundo párrafo aquí.\n\n\
         Página sin datos.\n"
    );
    let docx = document.docx(&[marta]).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("word/document.xml").unwrap(), &mut xml)
        .unwrap();
    assert!(xml.contains("Contrato con [NOMBRE_PERSONA_0001], con DNI 12345678Z."));
    assert!(xml.contains("w:type=\"page\""));
    assert!(!xml.contains("Marta"));
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn pages_with_data_become_images_and_the_rest_is_copied() {
    let original = simple(&[CONTRACT, NO_DATA]);
    let document = PdfDocument::from_bytes(&original).unwrap();
    let replacements = [
        replace(&document, 0, "Marta González Ruiz", "[NOMBRE_PERSONA_0001]"),
        replace(&document, 0, "12345678Z", "[ID_DOCUMENTO_PERSONA_0001]"),
    ];
    let output = document
        .write(&replacements, &WriteOptions::default(), &mut no_short_form)
        .unwrap();
    assert_eq!(output.rasterized_pages, 1);
    let texts = page_texts(&output.bytes);
    assert_eq!(texts.len(), 2);
    assert!(texts[0].contains("[NOMBRE_PERSONA_0001]"), "{:?}", texts[0]);
    assert!(texts[0].contains("[ID_DOCUMENTO_PERSONA_0001]"));
    assert!(
        !texts[0].contains("Contrato"),
        "la página es una imagen: {:?}",
        texts[0]
    );
    assert_eq!(texts[1], "Página sin datos.");
    verify(&output.bytes, &["Marta González Ruiz", "12345678Z"]).unwrap();
    for value in ["Marta", "Gonz", "12345678Z"] {
        assert!(
            !contains(&output.bytes, value.as_bytes()),
            "{value} en los bytes"
        );
    }
}

/// Render en gris de la página `index`, `width`×`height` píxeles.
fn render(bytes: &[u8], index: u16, width: i32, height: i32) -> GrayImage {
    let pdfium = pdf::pdfium().unwrap();
    let document = pdfium.load_pdf_from_byte_slice(bytes, None).unwrap();
    let page = document.pages().get(index as PdfPageIndex).unwrap();
    page.render_with_config(&PdfRenderConfig::new().set_target_size(width, height))
        .unwrap()
        .as_image()
        .unwrap()
        .to_luma8()
}

/// Todo lo oscuro del original queda tapado en la salida, salvo donde está
/// el marcador. Comprueba que los rectángulos caen sobre el texto.
fn assert_everything_covered(rotation: PdfPageRenderRotation) {
    let original = build(&[&[(72.0, 760.0, "Marta Lozano Pons", 14.0)]], |document| {
        document.pages().get(0).unwrap().set_rotation(rotation);
    });
    let document = PdfDocument::from_bytes(&original).unwrap();
    let all = replace(&document, 0, "Marta Lozano Pons", "[NOMBRE_PERSONA_0001]");
    let output = document
        .write(&[all], &WriteOptions::default(), &mut no_short_form)
        .unwrap();

    // La página de salida sin el marcador: solo la imagen pintada. El
    // permiso de PDFium se suelta antes de renderizar.
    let (width_pt, height_pt, marker, image_only) = {
        let pdfium = pdf::pdfium().unwrap();
        let written = pdfium
            .load_pdf_from_byte_slice(&output.bytes, None)
            .unwrap();
        let mut page = written.pages().get(0).unwrap();
        assert_eq!(page.rotation().unwrap(), PdfPageRenderRotation::None);
        let index = page
            .objects()
            .iter()
            .position(|o| o.object_type() == PdfPageObjectType::Text)
            .unwrap();
        let marker = page
            .objects()
            .get(index)
            .unwrap()
            .bounds()
            .unwrap()
            .to_rect();
        page.objects_mut().remove_object_at_index(index).unwrap();
        let size = (page.width().value, page.height().value);
        drop(page);
        (size.0, size.1, marker, written.save_to_bytes().unwrap())
    };
    // Un A4 girado se ve apaisado, y la página nueva tiene ese tamaño.
    let turned = matches!(
        rotation,
        PdfPageRenderRotation::Degrees90 | PdfPageRenderRotation::Degrees270
    );
    assert_eq!(
        width_pt > height_pt,
        turned,
        "{width_pt}×{height_pt} con {rotation:?}"
    );
    let scale = 2.0;
    let (width, height) = ((width_pt * scale) as i32, (height_pt * scale) as i32);
    let before = render(&original, 0, width, height);
    let after = render(&image_only, 0, width, height);
    let mut dark = 0;
    for (x, y, pixel) in before.enumerate_pixels() {
        if pixel.0[0] > 128 {
            continue;
        }
        dark += 1;
        assert!(
            after.get_pixel(x, y).0[0] > 128,
            "queda texto sin tapar en ({x}, {y}) con giro {rotation:?}"
        );
    }
    assert!(dark > 50, "la prueba debe mirar algún píxel del texto");

    // El marcador cae dentro de la zona del texto original, no encima ni al
    // lado. La holgura cubre el margen de 1 pt y lo que las cajas de PDFium
    // pasan de la tinta de las letras.
    let (mut left, mut right, mut top, mut bottom) = (f32::MAX, f32::MIN, f32::MIN, f32::MAX);
    for (x, y, pixel) in before.enumerate_pixels() {
        if pixel.0[0] <= 128 {
            let (px, py) = (x as f32 / scale, height_pt - y as f32 / scale);
            left = left.min(px);
            right = right.max(px);
            top = top.max(py);
            bottom = bottom.min(py);
        }
    }
    let slack = 6.0;
    assert!(
        marker.left().value >= left - slack
            && marker.right().value <= right + slack
            && marker.bottom().value >= bottom - slack
            && marker.top().value <= top + slack,
        "el marcador {marker:?} se sale del texto [{left}, {bottom}, {right}, {top}] con giro {rotation:?}"
    );
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn rectangles_cover_the_text() {
    assert_everything_covered(PdfPageRenderRotation::None);
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn rectangles_cover_the_text_on_rotated_pages() {
    assert_everything_covered(PdfPageRenderRotation::Degrees90);
    assert_everything_covered(PdfPageRenderRotation::Degrees270);
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_marker_that_does_not_fit_uses_the_short_form() {
    let original = simple(&[&[(72.0, 760.0, "Firma: Ana.", 7.0)]]);
    let document = PdfDocument::from_bytes(&original).unwrap();
    let ana = replace(&document, 0, "Ana", "[NOMBRE_PERSONA_0001]");
    let mut asked = Vec::new();
    let output = document
        .write(&[ana], &WriteOptions::default(), &mut |marker: &str| {
            asked.push(marker.to_owned());
            Some("[N1]".to_owned())
        })
        .unwrap();
    assert_eq!(asked, ["[NOMBRE_PERSONA_0001]"]);
    assert_eq!(output.short_forms, 1);
    let texts = page_texts(&output.bytes);
    assert_eq!(texts[0], "[N1]");
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn metadata_outline_and_attachments_do_not_reach_the_output() {
    let original = build(&[CONTRACT, NO_DATA], |document| {
        document
            .attachments_mut()
            .create_attachment_from_bytes("Marta.txt", b"Marta Gonzalez")
            .unwrap();
    });
    let document = PdfDocument::from_bytes(&original).unwrap();
    assert_eq!(document.attachments(), 1);
    let marta = replace(&document, 0, "Marta González Ruiz", "[NOMBRE_PERSONA_0001]");
    let output = document
        .write(&[marta], &WriteOptions::default(), &mut no_short_form)
        .unwrap();
    for key in [
        &b"EmbeddedFiles"[..],
        b"/Metadata",
        b"/Outlines",
        b"/Title",
        b"/Author",
    ] {
        assert!(
            !contains(&output.bytes, key),
            "{}",
            String::from_utf8_lossy(key)
        );
    }
    let pdfium = pdf::pdfium().unwrap();
    let written = pdfium
        .load_pdf_from_byte_slice(&output.bytes, None)
        .unwrap();
    assert_eq!(written.attachments().len(), 0);
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn other_appearances_of_a_value_are_replaced_too() {
    let original = simple(&[
        CONTRACT,
        &[(72.0, 760.0, "Firmado: Marta Gonz\u{e1}lez Ruiz", 12.0)],
    ]);
    let document = PdfDocument::from_bytes(&original).unwrap();
    let marta = replace(&document, 0, "Marta González Ruiz", "[NOMBRE_PERSONA_0001]");
    let literals = [(
        "Marta González Ruiz".to_owned(),
        "[NOMBRE_PERSONA_0001]".to_owned(),
    )];
    let all = document.with_literals(&[marta], &literals);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].segment, 1);
    let output = document
        .write(&all, &WriteOptions::default(), &mut no_short_form)
        .unwrap();
    assert_eq!(output.rasterized_pages, 2);
    verify(&output.bytes, &["Marta González Ruiz"]).unwrap();
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn verification_fails_when_a_value_is_still_there() {
    let original = simple(&[CONTRACT, NO_DATA]);
    assert!(matches!(
        verify(&original, &["Marta González Ruiz"]),
        Err(Error::PdfVerificationFailed)
    ));
    assert!(matches!(
        verify(&original, &["MARTA GONZALEZ RUIZ"]),
        Err(Error::PdfVerificationFailed)
    ));
    verify(&original, &["Pablo"]).unwrap();
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn the_resolution_must_be_between_150_and_300() {
    let document = PdfDocument::from_bytes(&simple(&[CONTRACT])).unwrap();
    let marta = replace(&document, 0, "Marta González Ruiz", "[NOMBRE_PERSONA_0001]");
    for resolution in [100, 301] {
        assert!(matches!(
            document.write(
                std::slice::from_ref(&marta),
                &WriteOptions { resolution },
                &mut no_short_form
            ),
            Err(Error::InvalidResolution(r)) if r == resolution
        ));
    }
    for resolution in [150, 300] {
        document
            .write(
                std::slice::from_ref(&marta),
                &WriteOptions { resolution },
                &mut no_short_form,
            )
            .unwrap();
    }
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn a_page_with_only_an_image_is_scanned_but_a_blank_page_is_not() {
    let scanned = build(&[CONTRACT, &[]], |document| {
        let mut page = document.pages().get(1).unwrap();
        let image = DynamicImage::new_rgb8(20, 20);
        page.objects_mut()
            .create_image_object(
                PdfPoints::new(0.0),
                PdfPoints::new(0.0),
                &image,
                Some(PdfPoints::new(595.0)),
                Some(PdfPoints::new(842.0)),
            )
            .unwrap();
    });
    let error = PdfDocument::from_bytes(&scanned).unwrap_err();
    assert!(matches!(error, Error::ScannedPdf));
    assert_eq!(
        error.to_string(),
        "Este PDF contiene páginas escaneadas. La v1 no puede anonimizarlas."
    );

    let blank = simple(&[CONTRACT, &[]]);
    let document = PdfDocument::from_bytes(&blank).unwrap();
    assert_eq!(document.segments()[1], "");
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn damaged_and_protected_pdfs_are_rejected() {
    assert!(matches!(
        PdfDocument::from_bytes(b"%PDF-1.4 basura"),
        Err(Error::InvalidPdf(_))
    ));
    assert!(matches!(
        PdfDocument::from_bytes(&protected_pdf()),
        Err(Error::PdfProtected)
    ));
}

#[test]
#[ignore = "necesita PDFium: node scripts/descargar-binarios.mjs"]
fn form_fields_are_flattened_and_read() {
    let document = PdfDocument::from_bytes(&form_pdf()).unwrap();
    assert!(
        document.segments()[0].contains("Marta Lozano"),
        "{:?}",
        document.segments()[0]
    );
    let marta = replace(&document, 0, "Marta Lozano", "[NOMBRE_PERSONA_0001]");
    let output = document
        .write(&[marta], &WriteOptions::default(), &mut no_short_form)
        .unwrap();
    verify(&output.bytes, &["Marta Lozano"]).unwrap();
    assert!(!contains(&output.bytes, b"AcroForm"));
}

#[test]
fn pdfs_are_recognized_by_their_signature() {
    assert!(pdf::is_pdf(b"%PDF-1.7\n..."));
    assert!(pdf::is_pdf(b"\xef\xbb\xbf%PDF-1.4"));
    assert!(!pdf::is_pdf(b"PK\x03\x04"));
}

/// PDF mínimo escrito a mano, con las posiciones de la tabla `xref` bien
/// calculadas.
fn handmade(objects: &[&str], trailer: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Cifrado estándar con una clave de usuario que no es la vacía: PDFium
/// pide contraseña.
fn protected_pdf() -> Vec<u8> {
    let hex32 = "A1".repeat(32);
    handmade(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>",
            &format!("<< /Filter /Standard /V 1 /R 2 /O <{hex32}> /U <{hex32}> /P -4 >>"),
        ],
        &format!("/Encrypt 4 0 R /ID [<{0}> <{0}>]", "B2".repeat(16)),
    )
}

/// Formulario con un campo de texto relleno y su apariencia.
fn form_pdf() -> Vec<u8> {
    let appearance = "BT /Helv 12 Tf 2 6 Td (Marta Lozano) Tj ET";
    handmade(
        &[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /DR << /Font << /Helv 6 0 R >> >> /DA (/Helv 12 Tf 0 g) >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Annots [4 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (nombre) /V (Marta Lozano) /Rect [72 700 272 720] /P 3 0 R /F 4 /DA (/Helv 12 Tf 0 g) /AP << /N 5 0 R >> >>",
            &format!(
                "<< /Type /XObject /Subtype /Form /BBox [0 0 200 20] /Resources << /Font << /Helv 6 0 R >> >> /Length {} >>\nstream\n{appearance}\nendstream",
                appearance.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        ],
        "",
    )
}
