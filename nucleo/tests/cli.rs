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
#[ignore = "necesita los recursos descargados en recursos/"]
fn check_resources_with_real_resources_succeeds() {
    let output = cli(&["comprobar-recursos"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Modelo cargado."));
    assert!(text.contains("PDFium cargado."));
}
