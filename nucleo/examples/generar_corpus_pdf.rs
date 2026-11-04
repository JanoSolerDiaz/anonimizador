//! Genera el corpus sintético de PDF de `tests/corpus/pdf`.
//!
//! Un contrato de tres páginas que reúne los casos del formato: un nombre
//! partido con guion al final de la línea, datos en varias líneas, una
//! página sin datos (debe copiarse tal cual), el mismo nombre repetido en
//! otra página, un testigo en letra pequeña (su marcador no cabe y va en
//! forma corta) y un archivo adjunto con un nombre. Junto al PDF va un JSON
//! con lo que debe detectarse. Todos los datos son inventados.
//!
//! PDFium no da los mismos bytes en cada ejecución, pero sí el mismo texto.
//! Necesita PDFium en `recursos/`. Uso:
//! `cargo run -p nucleo --example generar_corpus_pdf [carpeta]`; sin carpeta
//! escribe en `tests/corpus`.

use std::fs;
use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;
use serde_json::{Value, json};

/// Nombre del documento del corpus, sin extensión.
pub const NAME: &str = "es-01-contrato-servicios";

/// Una línea: posición vertical en puntos, texto y cuerpo.
type Line = (f32, &'static str, f32);

const PAGES: [&[Line]; 3] = [
    &[
        (780.0, "CONTRATO DE PRESTACI\u{d3}N DE SERVICIOS", 14.0),
        (740.0, "En Valencia, a 4 de marzo de 2026.", 11.0),
        (715.0, "De una parte, D.\u{aa} Marta Lozano Ib\u{e1}-", 11.0),
        (
            701.0,
            "\u{f1}ez, mayor de edad, con DNI 48291736Q y domicilio en",
            11.0,
        ),
        (687.0, "Calle del Coso 87, 50001 Zaragoza.", 11.0),
        (
            662.0,
            "De otra parte, Iv\u{e1}n Prieto Salas, en nombre de Servicios",
            11.0,
        ),
        (648.0, "Ejemplo SL, con CIF B12345674.", 11.0),
        (
            623.0,
            "Contacto: 654 218 903 y marta.lozano@correo-ejemplo.es.",
            11.0,
        ),
    ],
    &[
        (780.0, "CL\u{c1}USULAS", 14.0),
        (
            740.0,
            "Primera. El servicio se prestar\u{e1} durante doce meses.",
            11.0,
        ),
        (
            715.0,
            "Segunda. El precio se pagar\u{e1} por meses vencidos.",
            11.0,
        ),
        (
            690.0,
            "Tercera. Cualquiera de las partes puede resolverlo con",
            11.0,
        ),
        (676.0, "un preaviso de treinta d\u{ed}as.", 11.0),
    ],
    &[
        (780.0, "FIRMAS", 14.0),
        (740.0, "Firmado: Marta Lozano Ib\u{e1}\u{f1}ez", 11.0),
        (715.0, "Firmado: Iv\u{e1}n Prieto Salas", 11.0),
        (690.0, "Testigo: Luc\u{ed}a Ferrer Gil", 5.0),
    ],
];

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus"));
    generate(&dir)?;
    println!("Corpus de PDF generado en {}", dir.display());
    Ok(())
}

/// Escribe el PDF y su JSON en `dir/pdf`.
pub fn generate(dir: &Path) -> std::io::Result<()> {
    let folder = dir.join("pdf");
    fs::create_dir_all(&folder)?;
    fs::write(folder.join(format!("{NAME}.pdf")), contract())?;
    let json = serde_json::to_string_pretty(&expected()).expect("JSON");
    fs::write(folder.join(format!("{NAME}.json")), json + "\n")?;
    Ok(())
}

/// El contrato en PDF.
pub fn contract() -> Vec<u8> {
    let pdfium = nucleo::formatos::pdf::pdfium().expect("PDFium en recursos/");
    let mut document = pdfium.create_new_pdf().expect("documento nuevo");
    let font = document.fonts_mut().helvetica();
    for lines in PAGES {
        let mut page = document
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .expect("página");
        for &(y, text, size) in lines {
            page.objects_mut()
                .create_text_object(
                    PdfPoints::new(72.0),
                    PdfPoints::new(y),
                    text,
                    font,
                    PdfPoints::new(size),
                )
                .expect("texto");
        }
    }
    document
        .attachments_mut()
        .create_attachment_from_bytes("notas-marta-lozano.txt", b"Llamar a Marta Lozano.")
        .expect("adjunto");
    document.save_to_bytes().expect("guardar")
}

/// Lo que debe detectarse, y la página que no tiene datos.
pub fn expected() -> Value {
    let entity = |texto: &str, tipo: &str| json!({ "texto": texto, "tipo": tipo });
    json!({
        "entidades": [
            entity("Marta Lozano Ibáñez", "NOMBRE_PERSONA"),
            entity("48291736Q", "ID_DOCUMENTO_PERSONA"),
            entity("Calle del Coso 87, 50001 Zaragoza", "DIRECCION_POSTAL"),
            entity("Iván Prieto Salas", "NOMBRE_PERSONA"),
            entity("B12345674", "CIF"),
            entity("654 218 903", "TELEFONO"),
            entity("marta.lozano@correo-ejemplo.es", "EMAIL"),
            entity("Lucía Ferrer Gil", "NOMBRE_PERSONA"),
        ],
        "paginas_sin_datos": [2],
        "adjuntos": 1
    })
}
