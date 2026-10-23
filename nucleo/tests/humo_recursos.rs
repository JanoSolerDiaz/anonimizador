//! Prueba de humo: carga ONNX Runtime con el modelo y PDFium desde
//! `recursos/`. Necesita los recursos descargados, por eso está ignorada por
//! defecto. Ejecutar con:
//!
//!   cargo test -p nucleo -- --include-ignored

use nucleo::recursos::Resources;

#[test]
#[ignore = "necesita `node scripts/descargar-modelo.mjs` y `node scripts/descargar-binarios.mjs`"]
fn native_libraries_and_model_load() {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    let report = resources
        .smoke_test()
        .expect("fallo al cargar los recursos");

    for input in ["input_ids", "attention_mask"] {
        assert!(
            report.model_inputs.iter().any(|name| name == input),
            "el modelo no tiene la entrada {input}: {:?}",
            report.model_inputs
        );
    }
    assert!(
        report.model_outputs.iter().any(|name| name == "logits"),
        "el modelo no tiene la salida logits: {:?}",
        report.model_outputs
    );
    assert_eq!(report.pdf_pages, 1);
}

#[test]
fn missing_resources_dir_reports_the_file() {
    let resources = Resources::new("carpeta-que-no-existe");
    let error = resources.check_files().unwrap_err();
    assert!(matches!(error, nucleo::Error::ResourceMissing(_)));
}
