//! Errores del núcleo.
//!
//! Los mensajes se muestran al usuario: en español, en una frase y sin jerga.
//! Nunca incluyen texto de documentos ni valores del almacén.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No se encuentra la carpeta de recursos de la aplicación.")]
    ResourcesDirNotFound,

    #[error("Falta un archivo necesario de la aplicación: {}.", .0.display())]
    ResourceMissing(PathBuf),

    #[error("No se ha podido cargar el modelo de anonimización: {0}")]
    ModelLoad(String),

    #[error("No se ha podido cargar el lector de PDF: {0}")]
    PdfiumLoad(String),

    #[error("Error de lectura o escritura: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
