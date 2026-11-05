//! Operaciones completas sobre archivos: anonimizar y desanonimizar. Juntan
//! los extractores y escritores de [`crate::formatos`], el motor y el
//! almacén.

pub mod anonimizar;
pub mod desanonimizar;
pub mod marcadores;

use std::path::Path;

use crate::formatos::ooxml::Office;
use crate::formatos::pdf::is_pdf;
use crate::motor::{Engine, Entity};
use crate::{Error, Result};

/// Lock de los recursos, leído al compilar: de ahí sale la revisión exacta
/// del modelo que se anota en el almacén.
const RESOURCES_LOCK: &str = include_str!("../../../scripts/recursos.lock.json");

/// Cabecera de un archivo ZIP, que es lo que es un DOCX, un PPTX o un XLSX.
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// Lo que encuentra los datos personales de unos segmentos. Lo hace el
/// motor; las pruebas usan uno falso para no cargar el modelo.
pub trait Detector {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>>;

    /// Como `detect`, llamando a `progress` con la parte hecha (de 0 a 1).
    /// Sin más, solo avisa al terminar.
    fn detect_with_progress(
        &self,
        segments: &[&str],
        progress: &dyn Fn(f32),
    ) -> Result<Vec<Entity>> {
        let entities = self.detect(segments)?;
        progress(1.0);
        Ok(entities)
    }
}

impl Detector for Engine {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        Engine::detect(self, segments)
    }

    fn detect_with_progress(
        &self,
        segments: &[&str],
        progress: &dyn Fn(f32),
    ) -> Result<Vec<Entity>> {
        Engine::detect_with_progress(self, segments, progress)
    }
}

/// Formato del archivo de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Markdown,
    Text,
}

impl InputFormat {
    /// PDF, DOCX, PPTX y XLSX se reconocen por su contenido; MD y TXT, por la
    /// extensión. Un archivo de Office 97-2003 (`.doc`, `.ppt`, `.xls`) se
    /// rechaza con un error que explica cómo convertirlo.
    pub fn detect(path: &Path, bytes: &[u8]) -> Result<InputFormat> {
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if is_pdf(bytes) {
            return Ok(InputFormat::Pdf);
        }
        if bytes.starts_with(ZIP_SIGNATURE)
            && let Some(office) = Office::of_package(bytes)
        {
            return Ok(InputFormat::of_office(office));
        }
        match extension.as_str() {
            "pdf" => Err(Error::InvalidPdf("no empieza como un PDF".into())),
            // Uno con contraseña no es un ZIP: lo distingue el extractor.
            "docx" => Ok(InputFormat::Docx),
            "pptx" => Ok(InputFormat::Pptx),
            "xlsx" => Ok(InputFormat::Xlsx),
            "doc" => Err(Error::OldOfficeFormat(Office::Word)),
            "ppt" => Err(Error::OldOfficeFormat(Office::PowerPoint)),
            "xls" => Err(Error::OldOfficeFormat(Office::Excel)),
            "md" | "markdown" => Ok(InputFormat::Markdown),
            "txt" => Ok(InputFormat::Text),
            "" => Err(Error::UnsupportedFormat("sin extensión".into())),
            _ => Err(Error::UnsupportedFormat(extension)),
        }
    }

    fn of_office(office: Office) -> InputFormat {
        match office {
            Office::Word => InputFormat::Docx,
            Office::PowerPoint => InputFormat::Pptx,
            Office::Excel => InputFormat::Xlsx,
        }
    }

    pub fn label(self) -> &'static str {
        self.same_output().label()
    }

    /// Extensión del archivo de este formato.
    pub fn extension(self) -> &'static str {
        self.same_output().extension()
    }

    /// El formato de salida que conserva el formato de entrada.
    pub fn same_output(self) -> OutputFormat {
        match self {
            InputFormat::Pdf => OutputFormat::Pdf,
            InputFormat::Docx => OutputFormat::Docx,
            InputFormat::Pptx => OutputFormat::Pptx,
            InputFormat::Xlsx => OutputFormat::Xlsx,
            InputFormat::Markdown => OutputFormat::Markdown,
            InputFormat::Text => OutputFormat::Text,
        }
    }

    /// Si se puede entregar en `output`: cada formato en el suyo, todos en MD
    /// y TXT, y el PDF también en DOCX.
    pub fn can_become(self, output: OutputFormat) -> bool {
        matches!(output, OutputFormat::Markdown | OutputFormat::Text)
            || self.same_output() == output
            || (self == InputFormat::Pdf && output == OutputFormat::Docx)
    }
}

/// Lo que es un archivo antes de procesarlo, para la lista de la interfaz.
#[derive(Debug)]
pub enum Inspection {
    /// Se puede anonimizar.
    Supported(InputFormat),
    /// Un PDF con páginas escaneadas: no se puede anonimizar en la v1.
    ScannedPdf,
    /// No se admite, con el motivo para el usuario.
    Rejected(Error),
}

/// Mira qué es `path` sin procesarlo. Un PDF se abre entero para saber si
/// tiene páginas escaneadas o contraseña.
pub fn inspect(path: &Path) -> Inspection {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return Inspection::Rejected(error.into()),
    };
    match InputFormat::detect(path, &bytes) {
        Ok(InputFormat::Pdf) => match crate::formatos::pdf::PdfDocument::from_bytes(&bytes) {
            Ok(_) => Inspection::Supported(InputFormat::Pdf),
            Err(Error::ScannedPdf) => Inspection::ScannedPdf,
            Err(error) => Inspection::Rejected(error),
        },
        Ok(format) => Inspection::Supported(format),
        Err(error) => Inspection::Rejected(error),
    }
}

impl InputFormat {
    /// Formatos en que se puede entregar, el propio primero.
    pub fn outputs(self) -> Vec<OutputFormat> {
        let mut outputs = vec![self.same_output()];
        for output in [
            OutputFormat::Docx,
            OutputFormat::Markdown,
            OutputFormat::Text,
        ] {
            if output != self.same_output() && self.can_become(output) {
                outputs.push(output);
            }
        }
        outputs
    }

    /// Si un archivo de este formato se puede desanonimizar. Un PDF no:
    /// se desanonimiza el DOCX, MD o TXT que se sacó de él.
    pub fn deanonymizable(self) -> bool {
        self != InputFormat::Pdf
    }
}

/// Formato del archivo de salida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Markdown,
    Text,
}

impl OutputFormat {
    /// Extensión del archivo, que es también el formato en el almacén.
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Pdf => "pdf",
            OutputFormat::Docx => "docx",
            OutputFormat::Pptx => "pptx",
            OutputFormat::Xlsx => "xlsx",
            OutputFormat::Markdown => "md",
            OutputFormat::Text => "txt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OutputFormat::Pdf => "PDF",
            OutputFormat::Docx => "DOCX",
            OutputFormat::Pptx => "PPTX",
            OutputFormat::Xlsx => "XLSX",
            OutputFormat::Markdown => "MD",
            OutputFormat::Text => "TXT",
        }
    }
}

/// Modelo tal como se guarda en el almacén: `repositorio@revisión` del lock
/// para la carpeta `recursos/<name>`, o el nombre de la carpeta si el lock no
/// la conoce.
pub fn model_id(name: &str) -> String {
    let lock: serde_json::Value =
        serde_json::from_str(RESOURCES_LOCK).expect("el lock de recursos es JSON válido");
    let destination = format!("recursos/{name}");
    lock["modelos"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["destino"] == destination.as_str())
        .and_then(|m| {
            Some(format!(
                "{}@{}",
                m["repositorio"].as_str()?,
                m["revision"].as_str()?
            ))
        })
        .unwrap_or_else(|| name.to_owned())
}
