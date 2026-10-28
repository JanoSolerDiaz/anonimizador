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
        "otro-modelo",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    let expected = dir.path().join("otro-modelo").join("model_quantized.onnx");
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
        "modelo",
        "--decodificacion",
        "media",
        "--umbral",
        "0",
    ]);
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

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_passes_the_strategy_to_the_engine() {
    // Con la media de palabra, los trozos de «ejemplo-empresa» llevan el
    // enlace hacia ORGANIZACION; con el primer subtoken sale como ENLACE.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("enlace.txt");
    let text = "Ha intentado restablecer la contraseña desde https://intranet.ejemplo-empresa.es/perfil/pmartin sin éxito. Escribid a pablo.martin.c@correo-ejemplo.com hoy.\n";
    std::fs::write(&path, text).unwrap();
    let kinds = |strategy: &str| -> Vec<String> {
        let output = cli(&[
            "entidades",
            path.to_str().unwrap(),
            "--decodificacion",
            strategy,
            "--umbral",
            "0",
        ]);
        assert!(output.status.success(), "{}", stderr(&output));
        parse_entities(&stdout(&output), text)
            .into_iter()
            .map(|(_, _, kind)| kind)
            .collect()
    };
    assert!(kinds("primer").contains(&"ENLACE".to_owned()));
    assert!(!kinds("media").contains(&"ENLACE".to_owned()));
}

// ---------------------------------------------------------------- evaluar

/// Corpus de un solo texto con las entidades esperadas que se indiquen.
fn sample_corpus(entities_json: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("muestra.txt"), SAMPLE).unwrap();
    std::fs::write(
        dir.path().join("muestra.json"),
        format!(r#"{{"entidades": {entities_json}}}"#),
    )
    .unwrap();
    dir
}

const SAMPLE_ENTITIES: &str = r#"[
    {"tipo": "NOMBRE_PERSONA", "texto": "Carlos García"},
    {"tipo": "ID_DOCUMENTO_PERSONA", "texto": "12345678Z"},
    {"tipo": "LUGAR", "texto": "Madrid"},
    {"tipo": "TELEFONO", "texto": "+34 612 345 678"}
]"#;

/// Columnas de la fila de la tabla que empieza por `first`.
fn row(report: &str, first: &str) -> Vec<String> {
    report
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find(|cells| cells.first().is_some_and(|c| c == first))
        .unwrap_or_else(|| panic!("falta la fila «{first}»: {report}"))
}

#[test]
fn usage_mentions_evaluar() {
    assert!(stderr(&cli(&[])).contains("cli evaluar <carpeta>"));
}

#[test]
fn evaluar_without_a_folder_prints_usage() {
    let output = cli(&["evaluar"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Uso:"));
}

#[test]
fn evaluar_rejects_bad_options_before_loading_anything() {
    let dir = sample_corpus(SAMPLE_ENTITIES);
    let folder = dir.path().to_str().unwrap();
    for args in [
        vec!["evaluar", folder, "--rapido"],
        vec!["evaluar", folder, "otra"],
        vec!["evaluar", folder, "--modelo"],
        vec!["evaluar", folder, "--decodificacion", "otra"],
        vec!["evaluar", folder, "--umbral", "2"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("Uso:"), "{args:?}");
    }
}

#[test]
fn evaluar_with_an_invalid_corpus_fails_before_loading_the_model() {
    let dir = sample_corpus(r#"[{"tipo": "NOMBRE_PERSONA", "texto": "Lucía Ferrer"}]"#);
    let output = cli(&["evaluar", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("«Lucía Ferrer»"), "{message}");
    assert!(message.contains("muestra"), "{message}");
    assert!(stdout(&output).is_empty());
}

#[test]
fn evaluar_with_a_missing_folder_fails() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(&["evaluar", dir.path().join("no-existe").to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr(&output).is_empty());
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_prints_a_table_by_type_with_a_total() {
    let dir = sample_corpus(SAMPLE_ENTITIES);
    let output = cli(&["evaluar", dir.path().to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("Textos: 1"), "{report}");
    assert!(report.contains("Entidades esperadas: 4"), "{report}");
    for header in [
        "Tipo",
        "Esperadas",
        "Detectadas",
        "Correctas",
        "Precisión",
        "Exhaustividad",
        "Tapadas",
    ] {
        assert!(report.contains(header), "falta «{header}»: {report}");
    }
    assert_eq!(
        row(&report, "LUGAR"),
        ["LUGAR", "1", "1", "1", "1,00", "1,00", "1,00"]
    );
    assert_eq!(
        row(&report, "TOTAL"),
        ["TOTAL", "4", "4", "4", "1,00", "1,00", "1,00"]
    );
    for phase in [
        "Carga del modelo",
        "Tokenización",
        "Inferencia",
        "Decodificación",
        "Total",
    ] {
        assert!(report.contains(phase), "falta «{phase}»: {report}");
    }
    let tokens = row(&report, "Tokens:")[1].parse::<usize>().unwrap();
    assert!(tokens > 10, "{report}");
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_counts_a_wrong_type_against_both_types() {
    // «Madrid» anotado a propósito como organización: el modelo dirá LUGAR.
    let dir = sample_corpus(
        r#"[
            {"tipo": "NOMBRE_PERSONA", "texto": "Carlos García"},
            {"tipo": "ID_DOCUMENTO_PERSONA", "texto": "12345678Z"},
            {"tipo": "ORGANIZACION", "texto": "Madrid"},
            {"tipo": "TELEFONO", "texto": "+34 612 345 678"}
        ]"#,
    );
    let output = cli(&["evaluar", dir.path().to_str().unwrap(), "--detalle"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    // Sin precisión (no detectó ninguna organización), tapada por el LUGAR.
    assert_eq!(
        row(&report, "ORGANIZACION"),
        ["ORGANIZACION", "1", "0", "0", "—", "0,00", "1,00"]
    );
    assert_eq!(
        row(&report, "LUGAR"),
        ["LUGAR", "0", "1", "0", "0,00", "—", "—"]
    );
    assert_eq!(
        row(&report, "TOTAL"),
        ["TOTAL", "4", "4", "3", "0,75", "0,75", "1,00"]
    );
    assert!(
        report.contains("No detectada: ORGANIZACION «Madrid»"),
        "{report}"
    );
    assert!(report.contains("De más: LUGAR «Madrid»"), "{report}");
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_without_detalle_does_not_list_entities() {
    let dir = sample_corpus(r#"[{"tipo": "ORGANIZACION", "texto": "Madrid"}]"#);
    let output = cli(&["evaluar", dir.path().to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stdout(&output).contains("«Madrid»"));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_passes_model_strategy_and_threshold() {
    // Con umbral 1 no queda ninguna entidad: nada correcto, nada detectado.
    let dir = sample_corpus(SAMPLE_ENTITIES);
    let folder = dir.path().to_str().unwrap();
    let output = cli(&["evaluar", folder, "--umbral", "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(row(&stdout(&output), "TOTAL")[1..4], ["4", "0", "0"]);

    let output = cli(&[
        "evaluar",
        folder,
        "--modelo",
        "modelo",
        "--decodificacion",
        "media",
        "--umbral",
        "0",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("Modelo: modelo"), "{report}");
    assert!(report.contains("Decodificación: media"), "{report}");
    assert!(report.contains("Umbral: 0,00"), "{report}");
    assert_eq!(row(&report, "TOTAL")[1..4], ["4", "4", "4"]);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_finishes_on_the_synthetic_corpus() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus/texto");
    let output = cli(&["evaluar", corpus.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(row(&stdout(&output), "TOTAL").len() == 7);
}

#[test]
fn hilos_must_be_a_positive_number() {
    let (_dir, path) = sample_file();
    let file = path.to_str().unwrap();
    for value in ["0", "dos", "-1"] {
        for command in ["entidades", "evaluar"] {
            let output = cli(&[command, file, "--hilos", value]);
            assert_eq!(output.status.code(), Some(2), "{command} --hilos {value}");
            assert!(stderr(&output).contains("Uso:"));
        }
    }
    assert!(stderr(&cli(&[])).contains("--hilos <n>"));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_accepts_hilos_and_reports_tokens() {
    let (_dir, path) = sample_file();
    let output = cli(&[
        "entidades",
        path.to_str().unwrap(),
        "--hilos",
        "1",
        "--tiempos",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let timings = stderr(&output);
    let tokens = timings
        .lines()
        .find_map(|l| l.strip_prefix("Tokens:"))
        .unwrap_or_else(|| panic!("falta «Tokens:»: {timings}"));
    assert!(tokens.trim().parse::<usize>().unwrap() > 10, "{timings}");
    assert!(timings.contains("Hilos:            1"), "{timings}");
}

#[test]
fn contexto_must_be_documento_or_parrafo() {
    let (_dir, path) = sample_file();
    let file = path.to_str().unwrap();
    for command in ["entidades", "evaluar"] {
        for args in [
            vec![command, file, "--contexto"],
            vec![command, file, "--contexto", "frase"],
        ] {
            let output = cli(&args);
            assert_eq!(output.status.code(), Some(2), "{args:?}");
            assert!(stderr(&output).contains("Uso:"));
        }
    }
    assert!(stderr(&cli(&[])).contains("--contexto ambos|documento|parrafo"));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entidades_accepts_both_contexts() {
    let (_dir, path) = sample_file();
    for context in ["ambos", "documento", "parrafo"] {
        let output = cli(&["entidades", path.to_str().unwrap(), "--contexto", context]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(
            parse_entities(&stdout(&output), SAMPLE).len(),
            4,
            "{context}"
        );
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn evaluar_reports_the_context() {
    let dir = sample_corpus(SAMPLE_ENTITIES);
    let output = cli(&[
        "evaluar",
        dir.path().to_str().unwrap(),
        "--contexto",
        "parrafo",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Contexto: parrafo"));
    let output = cli(&["evaluar", dir.path().to_str().unwrap()]);
    assert!(stdout(&output).contains("Contexto: ambos"));
}
