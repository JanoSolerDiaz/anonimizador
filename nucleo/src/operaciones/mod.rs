//! Operaciones completas sobre archivos: anonimizar y desanonimizar. Juntan
//! los extractores y escritores de [`crate::formatos`], el motor y el
//! almacén.

pub mod anonimizar;
pub mod desanonimizar;
pub mod marcadores;

use std::path::Path;

use crate::motor::{Engine, Entity};
use crate::{Error, Result};

/// Lock de los recursos, leído al compilar: de ahí sale la revisión exacta
/// del modelo que se anota en el almacén.
const RESOURCES_LOCK: &str = include_str!("../../../scripts/recursos.lock.json");

/// Cabecera de un archivo ZIP, que es lo que es un DOCX.
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// Lo que encuentra los datos personales de unos segmentos. Lo hace el
/// motor; las pruebas usan uno falso para no cargar el modelo.
pub trait Detector {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>>;
}

impl Detector for Engine {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        Engine::detect(self, segments)
    }
}

/// Formato del archivo de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Docx,
    Markdown,
    Text,
}

impl InputFormat {
    /// Un DOCX se reconoce por su contenido; MD y TXT, por la extensión.
    pub fn detect(path: &Path, bytes: &[u8]) -> Result<InputFormat> {
        if bytes.starts_with(ZIP_SIGNATURE) {
            return Ok(InputFormat::Docx);
        }
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            // Un DOCX con contraseña no es un ZIP: lo distingue el extractor.
            "docx" => Ok(InputFormat::Docx),
            "md" | "markdown" => Ok(InputFormat::Markdown),
            "txt" => Ok(InputFormat::Text),
            "" => Err(Error::UnsupportedFormat("sin extensión".into())),
            _ => Err(Error::UnsupportedFormat(extension)),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InputFormat::Docx => "DOCX",
            InputFormat::Markdown => "MD",
            InputFormat::Text => "TXT",
        }
    }
}

/// Formato del archivo de salida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Docx,
    Markdown,
    Text,
}

impl OutputFormat {
    /// Extensión del archivo, que es también el formato en el almacén.
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Docx => "docx",
            OutputFormat::Markdown => "md",
            OutputFormat::Text => "txt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OutputFormat::Docx => "DOCX",
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
