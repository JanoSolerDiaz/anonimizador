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

// ---------------------------------------------------------------- entidades

use nucleo::motor::texto::split_paragraphs;

const SAMPLE: &str =
    "Informe interno sin datos.\n\nCarlos García, DNI 12345678Z, Madrid, +34 612 345 678.\n";

fn sample_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("muestra.txt");
    std::fs::write(&path, SAMPLE).unwrap();
    (dir, path)
}

#[test]
fn usage_mentions_entidades() {
    assert!(stderr(&cli(&[])).contains("cli entidades <archivo>"));
}

#[test]
fn entidades_without_a_file_prints_usage() {
    let output = cli(&["entidades"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Uso:"));
}

#[test]
fn entidades_with_a_missing_file_fails_with_a_clear_message() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-existe.txt");
    let output = cli(&["entidades", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(
        message.starts_with("No se puede leer el archivo"),
        "{message}"
    );
    assert!(message.contains("no-existe.txt"), "{message}");
    assert!(stdout(&output).is_empty());
}

#[test]
fn entidades_rejects_bad_options_before_loading_anything() {
    let (_dir, path) = sample_file();
    let file = path.to_str().unwrap();
    for args in [
        vec!["entidades", file, "--rapido"],
        vec!["entidades", file, "otro.txt"],
        vec!["entidades", file, "--modelo"],
        vec!["entidades", file, "--decodificacion", "otra"],
        vec!["entidades", file, "--umbral", "mucho"],
        vec!["entidades", file, "--umbral", "1.5"],
        vec!["entidades", file, "--umbral", "-0.1"],
        vec!["entidades", file, "--no-sustituir"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("Uso:"), "{args:?}");
    }
}

#[test]
fn entidades_explains_which_types_cannot_be_skipped() {
    let (_dir, path) = sample_file();
    let file = path.to_str().unwrap();
    let output = cli(&["entidades", file, "--no-sustituir", "LUGAR,NOMBRE_PERSONA"]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(
        message.contains("no se puede dejar sin sustituir: NOMBRE_PERSONA"),
        "{message}"
    );

    let output = cli(&["entidades", file, "--no-sustituir", "SITIO"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("No existe el tipo de dato «SITIO»"));
}

/// Entidades del JSON como (segmento, texto, tipo), comprobando que cada una
/// tiene todos los campos y que sus posiciones coinciden con el párrafo.
fn parse_entities(json: &str, source: &str) -> Vec<(u64, String, String)> {
    let value: serde_json::Value = serde_json::from_str(json).expect("la salida no es JSON");
    let paragraphs = split_paragraphs(source);
    value
        .as_array()
        .expect("la salida debe ser una lista")
        .iter()
        .map(|e| {
            let segment = e["segmento"].as_u64().unwrap();
            let start = e["inicio"].as_u64().unwrap() as usize;
            let end = e["fin"].as_u64().unwrap() as usize;
            let text = e["texto"].as_str().unwrap();
            let chars: Vec<char> = paragraphs[segment as usize].text.chars().collect();
            assert_eq!(chars[start..end].iter().collect::<String>(), text);
            assert!(e["etiqueta"].as_str().is_some_and(|l| !l.is_empty()));
            let score = e["puntuacion"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&score));
            (
                segment,
                text.to_owned(),
                e["tipo"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn triples(expected: &[(u64, &str, &str)]) -> Vec<(u64, String, String)> {
    expected
        .iter()
        .map(|(s, t, k)| (*s, t.to_string(), k.to_string()))
        .collect()
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_prints_json_with_offsets_that_match_the_text() {
    let (_dir, path) = sample_file();
    let output = cli(&["entidades", path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        parse_entities(&stdout(&output), SAMPLE),
        triples(&[
            (1, "Carlos García", "NOMBRE_PERSONA"),
            (1, "12345678Z", "ID_DOCUMENTO_PERSONA"),
            (1, "Madrid", "LUGAR"),
            (1, "+34 612 345 678", "TELEFONO"),
        ])
    );
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_can_skip_optional_types() {
    let (_dir, path) = sample_file();
    let file = path.to_str().unwrap();
    let output = cli(&["entidades", file, "--no-sustituir", "LUGAR,CARGO"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let kinds: Vec<String> = parse_entities(&stdout(&output), SAMPLE)
        .into_iter()
        .map(|(_, _, kind)| kind)
        .collect();
    assert_eq!(
        kinds,
        ["NOMBRE_PERSONA", "ID_DOCUMENTO_PERSONA", "TELEFONO"]
    );
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_accepts_model_strategy_and_threshold() {
    let (_dir, path) = sample_file();
    let output = cli(&[
        "entidades",
        path.to_str().unwrap(),
        "--modelo",
        "modelo-v2",
        "--decodificacion",
        "media",
        "--umbral",
        "0",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(parse_entities(&stdout(&output), SAMPLE).len(), 4);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_prints_timings_on_stderr_only() {
    let (_dir, path) = sample_file();
    let output = cli(&["entidades", path.to_str().unwrap(), "--tiempos"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let timings = stderr(&output);
    for phase in [
        "Carga del modelo",
        "Tokenización",
        "Inferencia",
        "Decodificación",
        "Total",
    ] {
        assert!(timings.contains(phase), "falta «{phase}»: {timings}");
    }
    parse_entities(&stdout(&output), SAMPLE);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_on_a_file_without_data_prints_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vacio.txt");
    std::fs::write(&path, "\n\n").unwrap();
    let output = cli(&["entidades", path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "[]");
}
