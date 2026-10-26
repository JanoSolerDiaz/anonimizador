//! Pruebas de la CLI ejecutando el binario real.

use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(args)
        // Que una variable del entorno del desarrollador no altere el resultado.
        .env_remove(nucleo::recursos::RESOURCES_ENV)
        .output()
        .expect("no se ha podido ejecutar la CLI")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn version_prints_the_crate_version() {
    let output = cli(&["version"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output).trim(),
        format!("anonimizador {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn without_arguments_prints_usage_and_exits_with_2() {
    let output = cli(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Uso:"));
    assert!(stdout(&output).is_empty());
}

#[test]
fn unknown_command_prints_usage_and_exits_with_2() {
    let output = cli(&["borrar-todo"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("comprobar-recursos"));
}

#[test]
fn check_resources_with_missing_folder_fails_with_a_clear_message() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-existe");

    let output = cli(&["comprobar-recursos", missing.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(
        message.starts_with("Falta un archivo necesario de la aplicación:"),
        "{message}"
    );
    assert!(message.contains("model_quantized.onnx"), "{message}");
}

#[test]
fn check_resources_looks_in_the_folder_of_the_requested_model() {
    let dir = tempfile::tempdir().unwrap();

    let output = cli(&[
        "comprobar-recursos",
        dir.path().to_str().unwrap(),
        "--modelo",
        "modelo-v2",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    let expected = dir.path().join("modelo-v2").join("model_quantized.onnx");
    assert!(
        message.contains(&expected.display().to_string()),
        "{message}"
    );
}

#[test]
fn check_resources_rejects_modelo_without_a_name() {
    let output = cli(&["comprobar-recursos", "--modelo"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Uso:"));
}

#[test]
fn check_resources_rejects_unknown_options_and_extra_folders() {
    assert_eq!(
        cli(&["comprobar-recursos", "--rapido"]).status.code(),
        Some(2)
    );
    assert_eq!(
        cli(&["comprobar-recursos", "una", "otra"]).status.code(),
        Some(2)
    );
}

#[test]
#[ignore = "necesita los recursos descargados en recursos/"]
fn check_resources_with_real_resources_succeeds() {
    let output = cli(&["comprobar-recursos"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Modelo «modelo» cargado."), "{text}");
    assert!(text.contains("PDFium cargado."));
}

#[test]
#[ignore = "necesita los dos modelos descargados en recursos/"]
fn check_resources_loads_several_models() {
    let output = cli(&[
        "comprobar-recursos",
        "--modelo",
        "modelo",
        "--modelo",
        "modelo-v2",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Modelo «modelo» cargado."), "{text}");
    assert!(text.contains("Modelo «modelo-v2» cargado."), "{text}");
}
