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

    #[error("La tabla de tipos de datos no es válida: {0}")]
    InvalidTypeTable(String),

    #[error("La configuración del modelo no es válida: {0}")]
    InvalidModelConfig(String),

    #[error("Error de lectura o escritura: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_is_a_spanish_sentence() {
        let errors = [
            Error::ResourcesDirNotFound,
            Error::ResourceMissing(PathBuf::from("modelo.onnx")),
            Error::ModelLoad("detalle".into()),
            Error::PdfiumLoad("detalle".into()),
            Error::InvalidTypeTable("detalle".into()),
            Error::InvalidModelConfig("detalle".into()),
            Error::Tokenizer("detalle".into()),
            Error::Io(std::io::Error::other("detalle")),
        ];
        for error in errors {
            let message = error.to_string();
            let first = message.chars().next().unwrap();
            assert!(first.is_uppercase(), "debe empezar en mayúscula: {message}");
            assert!(
                !message.contains('\n'),
                "debe ser una sola línea: {message}"
            );
        }
    }

    #[test]
    fn resources_dir_not_found_message() {
        assert_eq!(
            Error::ResourcesDirNotFound.to_string(),
            "No se encuentra la carpeta de recursos de la aplicación."
        );
    }

    #[test]
    fn resource_missing_names_the_file() {
        let path = PathBuf::from("recursos").join("modelo").join("config.json");
        let message = Error::ResourceMissing(path.clone()).to_string();
        assert_eq!(
            message,
            format!(
                "Falta un archivo necesario de la aplicación: {}.",
                path.display()
            )
        );
    }

    #[test]
    fn load_errors_keep_the_cause() {
        assert_eq!(
            Error::ModelLoad("sin memoria".into()).to_string(),
            "No se ha podido cargar el modelo de anonimización: sin memoria"
        );
        assert_eq!(
            Error::PdfiumLoad("biblioteca no encontrada".into()).to_string(),
            "No se ha podido cargar el lector de PDF: biblioteca no encontrada"
        );
    }

    #[test]
    fn io_errors_convert_with_question_mark() {
        fn read_missing() -> Result<Vec<u8>> {
            Ok(std::fs::read("archivo-que-no-existe-en-ningun-sitio")?)
        }
        let error = read_missing().unwrap_err();
        assert!(matches!(error, Error::Io(_)));
        assert!(
            error
                .to_string()
                .starts_with("Error de lectura o escritura: ")
        );
    }
}
