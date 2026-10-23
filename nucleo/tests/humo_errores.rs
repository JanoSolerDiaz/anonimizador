//! Camino de error de la carga nativa: una biblioteca de ONNX Runtime que no
//! es válida da un error claro en vez de abortar el proceso. Vive en su propio
//! binario porque ONNX Runtime solo se inicializa una vez por proceso.

use std::fs;

use nucleo::Error;
use nucleo::recursos::{MODEL_FILES, Resources};

#[test]
fn an_invalid_onnxruntime_library_gives_a_model_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let resources = Resources::new(dir.path());
    let mut files: Vec<_> = MODEL_FILES
        .iter()
        .map(|name| resources.model_dir().join(name))
        .collect();
    files.push(resources.onnxruntime_path());
    files.push(resources.pdfium_path());
    for file in &files {
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, b"esto no es una biblioteca").unwrap();
    }

    let error = resources.smoke_test().unwrap_err();

    assert!(matches!(error, Error::ModelLoad(_)), "{error:?}");
    assert!(
        error
            .to_string()
            .starts_with("No se ha podido cargar el modelo de anonimización: ")
    );
}
