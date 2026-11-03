//! PDF digital (PRD, «Formato PDF»): extracción de cada página con la caja
//! de cada carácter, rasterización de las páginas con datos, limpieza y
//! verificación obligatoria.
//!
//! La salida se construye siempre en un documento nuevo: las páginas sin
//! datos se copian y las demás se rehacen como imagen. Así no pasan el
//! diccionario `Info`, el XMP, el outline, los adjuntos ni los formularios
//! del original (ver `DECISIONES.md`).

mod busqueda;
mod escritura;
mod texto;

use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

pub use busqueda::find_value;
pub use escritura::{DEFAULT_RESOLUTION, MAX_RESOLUTION, MIN_RESOLUTION, PdfOutput, WriteOptions};

use super::Replacement;
use crate::recursos::Resources;
use crate::{Error, Result};

/// Lo que precede a todo PDF. Puede ir tras algo de basura, dentro del
/// primer kilobyte.
const PDF_SIGNATURE: &[u8] = b"%PDF-";

/// Si los bytes son de un PDF.
pub fn is_pdf(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(PDF_SIGNATURE.len())
        .any(|w| w == PDF_SIGNATURE)
}

/// Acceso exclusivo a PDFium. Solo un hilo puede tenerlo a la vez.
///
/// PDFium no admite varios hilos. La función `thread_safe` de
/// `pdfium-render` protege cada llamada por separado, pero no una serie de
/// ellas, y su caché de páginas por documento es global: dos documentos
/// abiertos a la vez en hilos distintos llegaron a dar el texto de una
/// página vacío. Todo lo que use PDFium se hace con este permiso.
pub struct PdfiumGuard {
    _lock: MutexGuard<'static, ()>,
    pdfium: &'static Pdfium,
}

impl std::ops::Deref for PdfiumGuard {
    type Target = Pdfium;

    fn deref(&self) -> &Pdfium {
        self.pdfium
    }
}

/// PDFium, cargado la primera vez desde la carpeta de recursos. Espera a que
/// lo suelte otro hilo; no se debe pedir dos veces en el mismo hilo.
pub fn pdfium() -> Result<PdfiumGuard> {
    static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    // Si otro hilo falló con el permiso, PDFium sigue sirviendo.
    let lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let pdfium = PDFIUM
        .get_or_init(|| {
            let resources = Resources::locate().map_err(|e| e.to_string())?;
            let bindings =
                Pdfium::bind_to_library(resources.pdfium_path()).map_err(|e| e.to_string())?;
            Ok(Pdfium::new(bindings))
        })
        .as_ref()
        .map_err(|e| Error::PdfiumLoad(e.clone()))?;
    Ok(PdfiumGuard {
        _lock: lock,
        pdfium,
    })
}

/// Rectángulo en puntos, con el origen abajo a la izquierda como en PDF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    fn of(rect: &PdfRect) -> Rect {
        Rect {
            left: rect.left().value,
            bottom: rect.bottom().value,
            right: rect.right().value,
            top: rect.top().value,
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }

    fn union(&self, other: &Rect) -> Rect {
        Rect {
            left: self.left.min(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.max(other.right),
            top: self.top.max(other.top),
        }
    }

    fn grown(&self, margin: f32) -> Rect {
        Rect {
            left: self.left - margin,
            bottom: self.bottom - margin,
            right: self.right + margin,
            top: self.top + margin,
        }
    }

    fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
}

/// Texto de una página con el mapa carácter → cajas.
#[derive(Debug, Clone)]
pub struct PageText {
    /// Texto que se envía al motor: los saltos de línea son `\n` y los
    /// guiones de fin de línea se han quitado.
    pub text: String,
    /// Cajas de cada carácter de `text`. Un salto o un espacio que añade
    /// PDFium no tiene caja; la última letra antes de un guion de fin de
    /// línea lleva también la del guion.
    boxes: Vec<Vec<Rect>>,
    /// Para cada `\n` de `text` (en orden), si empieza un párrafo nuevo.
    paragraph_breaks: Vec<bool>,
}

impl PageText {
    pub fn boxes(&self, index: usize) -> &[Rect] {
        &self.boxes[index]
    }
}

/// PDF de entrada ya leído.
#[derive(Debug)]
pub struct PdfDocument {
    bytes: Vec<u8>,
    pages: Vec<PageText>,
    attachments: usize,
}

impl PdfDocument {
    /// Lee el PDF: aplana formularios y anotaciones y extrae el texto de cada
    /// página. Un PDF con contraseña, dañado o con páginas escaneadas da
    /// error.
    pub fn from_bytes(bytes: &[u8]) -> Result<PdfDocument> {
        let pdfium = pdfium()?;
        let document = open(&pdfium, bytes)?;
        let mut pages = Vec::new();
        for mut page in document.pages().iter() {
            page.flatten().map_err(pdf_error)?;
            let text = texto::extract(&page)?;
            if is_scanned(&page, &text) {
                return Err(Error::ScannedPdf);
            }
            pages.push(text);
        }
        let attachments = document.attachments().len() as usize;
        Ok(PdfDocument {
            bytes: bytes.to_vec(),
            pages,
            attachments,
        })
    }

    /// Un segmento por página.
    pub fn segments(&self) -> Vec<&str> {
        self.pages.iter().map(|p| p.text.as_str()).collect()
    }

    pub fn pages(&self) -> &[PageText] {
        &self.pages
    }

    /// Archivos adjuntos, que no pasan a la salida (con aviso).
    pub fn attachments(&self) -> usize {
        self.attachments
    }

    /// Las sustituciones más las de cada otra aparición de los valores de
    /// `literals` (valor → marcador) en cualquier página. Así un valor que
    /// el modelo vio en una página no se queda en otra.
    pub fn with_literals(
        &self,
        replacements: &[Replacement],
        literals: &[(String, String)],
    ) -> Vec<Replacement> {
        let mut all = replacements.to_vec();
        let mut ordered: Vec<&(String, String)> = literals.iter().collect();
        ordered.sort_by_key(|(value, _)| std::cmp::Reverse(value.chars().count()));
        for (index, page) in self.pages.iter().enumerate() {
            for (value, marker) in &ordered {
                for (start, end) in find_value(&page.text, value) {
                    let overlaps = all
                        .iter()
                        .any(|r| r.segment == index && r.start < end && start < r.end);
                    if !overlaps {
                        all.push(Replacement {
                            segment: index,
                            start,
                            end,
                            text: marker.clone(),
                        });
                    }
                }
            }
        }
        all
    }

    /// PDF anonimizado. `short_form` da la forma corta de un marcador que no
    /// cabe en su hueco.
    pub fn write(
        &self,
        replacements: &[Replacement],
        options: &WriteOptions,
        short_form: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Result<PdfOutput> {
        escritura::write(self, replacements, options, short_form)
    }

    /// Párrafos de cada página, con las sustituciones hechas.
    pub fn paragraphs(&self, replacements: &[Replacement]) -> Result<Vec<Vec<String>>> {
        texto::paragraphs(self, replacements)
    }

    /// Markdown: una sección por página, un párrafo por bloque de texto.
    pub fn markdown(&self, replacements: &[Replacement]) -> Result<String> {
        texto::markdown(self, replacements)
    }

    /// Texto: una línea por párrafo y una línea en blanco entre páginas.
    pub fn plain_text(&self, replacements: &[Replacement]) -> Result<String> {
        texto::plain_text(self, replacements)
    }

    /// DOCX sin maquetación: un párrafo por bloque y un salto de página
    /// entre páginas.
    pub fn docx(&self, replacements: &[Replacement]) -> Result<Vec<u8>> {
        texto::docx(self, replacements)
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Verificación obligatoria (PRD): vuelve a extraer el texto de `bytes` y
/// busca cada valor. Si aparece alguno, o la salida trae adjuntos, falla.
pub fn verify(bytes: &[u8], values: &[&str]) -> Result<()> {
    let pdfium = pdfium()?;
    let document = open(&pdfium, bytes)?;
    if !document.attachments().is_empty() {
        return Err(Error::PdfVerificationFailed);
    }
    for page in document.pages().iter() {
        let text = texto::extract(&page)?;
        if values
            .iter()
            .any(|value| !find_value(&text.text, value).is_empty())
        {
            return Err(Error::PdfVerificationFailed);
        }
    }
    Ok(())
}

fn open<'a>(pdfium: &'a Pdfium, bytes: &'a [u8]) -> Result<PdfiumDocument<'a>> {
    pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                Error::PdfProtected
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
                Error::PdfProtected
            }
            _ => Error::InvalidPdf("está dañado o no es un PDF".into()),
        })
}

/// Una página es escaneada si no tiene ningún carácter visible pero sí una
/// imagen. Una página en blanco no lo es.
fn is_scanned(page: &PdfPage, text: &PageText) -> bool {
    let has_text = text.text.chars().any(|c| !c.is_whitespace());
    !has_text
        && page
            .objects()
            .iter()
            .any(|o| o.object_type() == PdfPageObjectType::Image)
}

pub(crate) fn pdf_error(error: PdfiumError) -> Error {
    Error::Pdf(format!("{error:?}"))
}

type PdfiumDocument<'a> = pdfium_render::prelude::PdfDocument<'a>;
