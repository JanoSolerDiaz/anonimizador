//! Prueba de humo: carga ONNX Runtime con cada modelo y PDFium desde
//! `recursos/`. Necesita los recursos descargados, por eso está ignorada por
//! defecto. Ejecutar con:
//!
//!   cargo test -p nucleo -- --include-ignored

use nucleo::recursos::Resources;

/// Carpetas de los modelos que se comparan en el hito 2 (v1 y v2).
const MODELS: [&str; 2] = ["modelo", "modelo-v2"];

#[test]
#[ignore = "necesita `node scripts/descargar-modelo.mjs` y `node scripts/descargar-binarios.mjs`"]
fn native_libraries_and_every_model_load() {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    let models: Vec<_> = MODELS.iter().map(|name| resources.model(name)).collect();

    let report = resources
        .smoke_test_models(&models)
        .expect("fallo al cargar los recursos");

    assert_eq!(report.models.len(), MODELS.len());
    for (model, name) in report.models.iter().zip(MODELS) {
        assert_eq!(model.name, name);
        for input in ["input_ids", "attention_mask"] {
            assert!(
                model.inputs.iter().any(|i| i == input),
                "{name} no tiene la entrada {input}: {:?}",
                model.inputs
            );
        }
        assert!(
            model.outputs.iter().any(|o| o == "logits"),
            "{name} no tiene la salida logits: {:?}",
            model.outputs
        );
    }
    assert_eq!(report.pdf_pages, 1);
}

#[test]
fn missing_resources_dir_reports_the_file() {
    let resources = Resources::new("carpeta-que-no-existe");
    let error = resources.check_files().unwrap_err();
    assert!(matches!(error, nucleo::Error::ResourceMissing(_)));
}
