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

    #[error("No se ha podido preparar el texto para el modelo: {0}")]
    Tokenizer(String),

    #[error("No se ha podido analizar el texto con el modelo: {0}")]
    Inference(String),

    #[error("No existe el tipo de dato «{0}».")]
    UnknownType(String),

    #[error("Este tipo de dato no se puede dejar sin sustituir: {0}.")]
    TypeNotOptional(String),

    #[error("El corpus de prueba no es válido: {0}.")]
    Corpus(String),

    #[error("El almacén no tiene un formato válido: {0}.")]
    StoreInvalid(String),

    #[error(
        "El almacén es de una versión más nueva de la aplicación (formato {0}); actualice la aplicación para abrirlo."
    )]
    StoreVersion(u32),

    #[error(
        "No se encuentra la clave del almacén en el almacén de credenciales del sistema; sin ella no se puede abrir."
    )]
    StoreKeyMissing,

    #[error("El almacén no se puede descifrar: la clave no es la suya o el archivo está dañado.")]
    StoreUndecryptable,

    #[error("No se ha podido usar el almacén de credenciales del sistema: {0}.")]
    Keyring(String),

    #[error("Ese documento no está en el almacén.")]
    DocumentNotFound,

    #[error("El diccionario no tiene un formato válido: {0}.")]
    DictionaryInvalid(String),

    #[error(
        "El diccionario es de una versión más nueva de la aplicación (formato {0}); actualice la aplicación para importarlo."
    )]
    DictionaryVersion(u32),

    #[error(
        "El archivo de texto no está en UTF-8; guárdelo en UTF-8 desde su editor y vuelva a intentarlo."
    )]
    TextNotUtf8,

    #[error("Las sustituciones no son válidas: {0}.")]
    InvalidReplacement(String),

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
            Error::Inference("detalle".into()),
            Error::UnknownType("SITIO".into()),
            Error::TypeNotOptional("NOMBRE_PERSONA".into()),
            Error::Corpus("detalle".into()),
            Error::StoreInvalid("detalle".into()),
            Error::StoreVersion(2),
            Error::StoreKeyMissing,
            Error::StoreUndecryptable,
            Error::Keyring("detalle".into()),
            Error::DocumentNotFound,
            Error::DictionaryInvalid("detalle".into()),
            Error::DictionaryVersion(2),
            Error::TextNotUtf8,
            Error::InvalidReplacement("detalle".into()),
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
