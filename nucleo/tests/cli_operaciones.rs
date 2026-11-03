//! Órdenes `anonimizar`, `desanonimizar` y `almacen` de la CLI, ejecutando
//! el binario real. El almacén va siempre en una carpeta temporal y la clave
//! sale de la variable de prueba, que solo aceptan las compilaciones de
//! depuración: el almacén de credenciales del sistema no se toca.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use nucleo::almacen::archivo::{MemoryKey, StoreKey, load, save};
use nucleo::almacen::{DocumentRecord, OutputFile, SourceFile, Store};

const TEST_KEY_ENV: &str = "ANONIMIZADOR_CLAVE_PRUEBA";
const TEST_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const OTHER_KEY: &str = "ff0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1eff";

fn key(hex: &str) -> MemoryKey {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    MemoryKey::with_key(StoreKey::from_bytes(&bytes).unwrap())
}

fn cli_with_key(args: &[&str], hex: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(args)
        .env_remove(nucleo::recursos::RESOURCES_ENV)
        .env(TEST_KEY_ENV, hex)
        .output()
        .expect("no se ha podido ejecutar la CLI")
}

fn cli(args: &[&str]) -> Output {
    cli_with_key(args, TEST_KEY)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(name)
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Carpeta de trabajo con un almacén que tiene un documento con Ana López y
/// su correo.
struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Workspace {
        let workspace = Workspace {
            dir: tempfile::tempdir().unwrap(),
        };
        let mut store = Store::new();
        store.assign("NOMBRE_PERSONA", "PERSON", "Ana López", "doc-1");
        store.assign("EMAIL", "EMAIL", "ana@correo-ejemplo.es", "doc-1");
        store.record_document(record("doc-1", "2099-01-01", "carta.txt"));
        workspace.save(&store);
        workspace
    }

    fn store_path(&self) -> PathBuf {
        self.dir.path().join("almacen.anon")
    }

    fn store(&self) -> Store {
        load(&self.store_path(), &key(TEST_KEY)).unwrap()
    }

    fn save(&self, store: &Store) {
        save(&self.store_path(), store, &key(TEST_KEY)).unwrap();
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn out(&self) -> PathBuf {
        let path = self.dir.path().join("salida");
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

fn record(id: &str, expires: &str, origin: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.into(),
        created: "2026-10-30T10:00:00+01:00".into(),
        expires: expires.parse().unwrap(),
        app_version: "0.1.0".into(),
        model: "modelo".into(),
        origin: SourceFile {
            name: origin.into(),
            sha256: "00".repeat(32),
        },
        output: OutputFile {
            name: format!("{}_anonimizado.txt", origin.trim_end_matches(".txt")),
            format: "txt".into(),
        },
        markers: vec!["[NOMBRE_PERSONA_0001]".into(), "[EMAIL_0001]".into()],
    }
}

fn dictionary(marker: &str, value: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "formato": "anonimizador-diccionario",
        "version_formato": 1,
        "documento": {
            "id": "doc-de-otra",
            "creado": "2026-10-20T09:00:00+02:00",
            "salida": { "nombre": "informe_anonimizado.txt", "formato": "txt" }
        },
        "entradas": [{ "marcador": marker, "tipo": "NOMBRE_PERSONA", "valor": value }]
    }))
    .unwrap()
}

#[test]
fn usage_mentions_the_new_commands() {
    let text = stderr(&cli(&[]));
    for command in [
        "cli anonimizar <archivos…>",
        "cli desanonimizar <archivos…>",
        "cli desanonimizar --texto",
        "cli almacen listar",
    ] {
        assert!(text.contains(command), "{command}: {text}");
    }
}

// ---------------------------------------------------------------- anonimizar

#[test]
fn anonimizar_rejects_bad_arguments_before_opening_anything() {
    let w = Workspace::new();
    let file = w.file("nota.txt", "Hola.\n");
    let out = w.out();
    for args in [
        vec!["anonimizar"],
        vec!["anonimizar", s(&file)],
        vec!["anonimizar", "--salida", s(&out)],
        vec!["anonimizar", s(&file), "--salida"],
        vec![
            "anonimizar",
            s(&file),
            "--salida",
            s(&out),
            "--formato",
            "rtf",
        ],
        vec!["anonimizar", s(&file), "--salida", s(&out), "--ppp", "alta"],
        vec!["anonimizar", s(&file), "--salida", s(&out), "--almacen"],
        vec!["anonimizar", s(&file), "--salida", s(&out), "--rapido"],
        vec!["anonimizar", s(&file), "--salida", s(&out), "--umbral", "2"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("Uso:"), "{args:?}");
    }
}

#[test]
fn anonimizar_rejects_a_resolution_out_of_range() {
    let w = Workspace::new();
    let file = w.file("nota.txt", "Hola.\n");
    let out = w.out();
    for resolution in ["100", "301"] {
        let output = cli(&[
            "anonimizar",
            s(&file),
            "--salida",
            s(&out),
            "--ppp",
            resolution,
        ]);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            stderr(&output).contains("La resolución debe estar entre 150 y 300 ppp"),
            "{}",
            stderr(&output)
        );
    }
}

#[test]
fn a_batch_goes_on_after_each_error_and_explains_it() {
    let w = Workspace::new();
    let corrupt = w.file("roto.docx", "esto no es un Word");
    let protected = w.dir.path().join("protegido.docx");
    std::fs::copy(corpus("es-02-protegido.docx"), &protected).unwrap();
    let rtf = w.file("informe.rtf", "{\\rtf1 Hola}");
    let text = w.file("nota.txt", "Hola.\n");
    let out = w.out();
    let output = cli(&[
        "anonimizar",
        s(&corrupt),
        s(&protected),
        s(&rtf),
        s(&text),
        "--salida",
        s(&out),
        "--formato",
        "docx",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let errors = stderr(&output);
    for expected in [
        "roto.docx: El archivo no es un documento de Word válido",
        "protegido.docx: El documento está protegido con contraseña",
        "informe.rtf: El formato «rtf» no se admite",
        "nota.txt: Un archivo TXT no se puede convertir a DOCX.",
    ] {
        assert!(errors.contains(expected), "{expected}: {errors}");
    }
    assert!(
        stdout(&output).contains("Anonimizados: 0 de 4."),
        "{}",
        stdout(&output)
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    // El almacén sigue con su documento.
    assert_eq!(w.store().documents().len(), 1);
}

#[test]
fn a_store_that_cannot_be_opened_stops_everything() {
    let w = Workspace::new();
    let file = w.file("nota.txt", "Hola.\n");
    let out = w.out();
    let output = cli_with_key(
        &[
            "anonimizar",
            s(&file),
            "--salida",
            s(&out),
            "--almacen",
            s(&w.store_path()),
        ],
        OTHER_KEY,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("no se puede descifrar"),
        "{}",
        stderr(&output)
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn with_the_real_model_a_batch_processes_the_good_files_and_explains_the_rest() {
    let w = Workspace::new();
    let corrupt = w.file("roto.docx", "esto no es un Word");
    let protected = w.dir.path().join("protegido.docx");
    std::fs::copy(corpus("es-02-protegido.docx"), &protected).unwrap();
    let good = w.dir.path().join("contrato.docx");
    std::fs::copy(corpus("es-01-contrato-arrendamiento.docx"), &good).unwrap();
    let out = w.out();
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args([
            "anonimizar",
            s(&corrupt),
            s(&good),
            s(&protected),
            "--salida",
            s(&out),
            "--almacen",
            s(&w.store_path()),
        ])
        .env(TEST_KEY_ENV, TEST_KEY)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let errors = stderr(&output);
    assert!(errors.contains("roto.docx: "), "{errors}");
    assert!(errors.contains("protegido.docx: "), "{errors}");
    let text = stdout(&output);
    assert!(text.contains("contrato_anonimizado.docx"), "{text}");
    assert!(text.contains("NOMBRE_PERSONA: "), "{text}");
    assert!(text.contains("Anonimizados: 1 de 3."), "{text}");
    assert!(out.join("contrato_anonimizado.docx").exists());
    let store = w.store();
    assert_eq!(store.documents().len(), 2);
    assert_eq!(store.documents()[1].origin.name, "contrato.docx");

    // Y vuelve.
    let back = w.dir.path().join("vuelta");
    std::fs::create_dir_all(&back).unwrap();
    let output = cli(&[
        "desanonimizar",
        s(&out.join("contrato_anonimizado.docx")),
        "--salida",
        s(&back),
        "--almacen",
        s(&w.store_path()),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        back.join("contrato-anonimizado_desanonimizado.docx")
            .exists()
    );
}

// ------------------------------------------------------------- desanonimizar

#[test]
fn desanonimizar_restores_pasted_text() {
    let w = Workspace::new();
    let output = cli(&[
        "desanonimizar",
        "--texto",
        "Escriba a [ nombre_persona_1 ] en [EMAIL_0001]. [NOMBRE_PERSONA_0009]",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Escriba a Ana López en ana@correo-ejemplo.es. [NOMBRE_PERSONA_0009]\n"
    );
    let report = stderr(&output);
    assert!(report.contains("Restaurados: 2"), "{report}");
    assert!(
        report.contains("Sin fuente: [NOMBRE_PERSONA_0009]"),
        "{report}"
    );
}

#[test]
fn desanonimizar_writes_each_file_in_its_format() {
    let w = Workspace::new();
    let file = w.file("respuesta.md", "# Para [NOMBRE_PERSONA_0001]\n");
    let missing = w.dir.path().join("no-existe.txt");
    let out = w.out();
    let output = cli(&[
        "desanonimizar",
        s(&file),
        s(&missing),
        "--salida",
        s(&out),
        "--almacen",
        s(&w.store_path()),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("no-existe.txt: "),
        "{}",
        stderr(&output)
    );
    let text = stdout(&output);
    assert!(text.contains("respuesta_desanonimizado.md"), "{text}");
    assert!(text.contains("Desanonimizados: 1 de 2."), "{text}");
    assert_eq!(
        std::fs::read_to_string(out.join("respuesta_desanonimizado.md")).unwrap(),
        "# Para Ana López\n"
    );
}

#[test]
fn a_conflict_asks_for_a_source_and_fuente_settles_it() {
    let w = Workspace::new();
    let mut store = w.store();
    store
        .import_dictionary(
            &dictionary("[NOMBRE_PERSONA_0001]", "Luis Mas"),
            "de-otra_diccionario.json",
        )
        .unwrap();
    w.save(&store);
    let base = ["desanonimizar", "--texto", "Hola [NOMBRE_PERSONA_0001]."];
    let store_path = w.store_path();
    let with = |extra: &[&str]| {
        let mut args: Vec<&str> = base.to_vec();
        args.extend_from_slice(&["--almacen", s(&store_path)]);
        args.extend_from_slice(extra);
        cli(&args)
    };

    let output = with(&[]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("[NOMBRE_PERSONA_0001]"), "{message}");
    assert!(message.contains("propio"), "{message}");
    assert!(message.contains("de-otra_diccionario.json"), "{message}");
    assert!(message.contains("--fuente"), "{message}");
    assert!(stdout(&output).is_empty());

    let output = with(&["--fuente", "propio"]);
    assert_eq!(stdout(&output), "Hola Ana López.\n");
    let output = with(&["--fuente", "de-otra_diccionario.json"]);
    assert_eq!(stdout(&output), "Hola Luis Mas.\n");
}

#[test]
fn desanonimizar_rejects_bad_arguments() {
    let w = Workspace::new();
    let file = w.file("nota.txt", "Hola.\n");
    for args in [
        vec!["desanonimizar"],
        vec!["desanonimizar", s(&file)],
        vec!["desanonimizar", "--texto"],
        vec!["desanonimizar", "--texto", "a", s(&file), "--salida", "x"],
        vec!["desanonimizar", "--texto", "a", "--fuente"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("Uso:"), "{args:?}");
    }
}

// ------------------------------------------------------------------- almacen

#[test]
fn listar_shows_documents_without_any_value() {
    let w = Workspace::new();
    let mut store = w.store();
    store
        .import_dictionary(
            &dictionary("[NOMBRE_PERSONA_0007]", "Luis Mas"),
            "de-otra.json",
        )
        .unwrap();
    w.save(&store);
    let output = cli(&["almacen", "listar", "--almacen", s(&w.store_path())]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for expected in [
        "doc-1",
        "carta.txt",
        "carta_anonimizado.txt",
        "2099-01-01",
        "2 marcadores",
        "de-otra.json",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    for value in ["Ana López", "ana@correo-ejemplo.es", "Luis Mas"] {
        assert!(!text.contains(value), "{value}: {text}");
    }
}

#[test]
fn listar_an_empty_store_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nuevo.anon");
    let output = cli(&["almacen", "listar", "--almacen", s(&path)]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("El almacén está vacío."));
    // Mirar no crea el archivo.
    assert!(!path.exists());
}

#[test]
fn the_store_lives_in_the_user_data_folder_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["almacen", "listar"])
        .env(TEST_KEY_ENV, TEST_KEY)
        .env("APPDATA", dir.path())
        .env("HOME", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let expected = if cfg!(target_os = "macos") {
        dir.path()
            .join("Library/Application Support/anonimizador/almacen.anon")
    } else {
        dir.path().join("anonimizador").join("almacen.anon")
    };
    assert!(
        stdout(&output).contains(&format!("Almacén: {}", expected.display())),
        "{}",
        stdout(&output)
    );
}

#[test]
fn exportar_and_importar_move_a_dictionary_between_stores() {
    let w = Workspace::new();
    let out = w.out();
    let output = cli(&[
        "almacen",
        "exportar",
        "doc-1",
        "--salida",
        s(&out),
        "--almacen",
        s(&w.store_path()),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("Este archivo contiene los datos originales sin cifrar."),
        "{}",
        stderr(&output)
    );
    let exported = out.join("carta-anonimizado_diccionario.json");
    assert!(exported.exists(), "{}", stdout(&output));

    let other = tempfile::tempdir().unwrap();
    let other_store = other.path().join("otro.anon");
    let output = cli(&[
        "almacen",
        "importar",
        s(&exported),
        "--almacen",
        s(&other_store),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let imported = load(&other_store, &key(TEST_KEY)).unwrap();
    assert_eq!(imported.imported().len(), 1);
    assert_eq!(
        imported.imported()[0].name(),
        "carta-anonimizado_diccionario.json"
    );
}

#[test]
fn olvidar_removes_a_document_and_complains_about_unknown_ones() {
    let w = Workspace::new();
    let output = cli(&[
        "almacen",
        "olvidar",
        "doc-1",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let store = w.store();
    assert!(store.documents().is_empty());
    assert_eq!(store.entries().count(), 0);

    let output = cli(&[
        "almacen",
        "olvidar",
        "doc-9",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Ese documento no está en el almacén."));
}

#[test]
fn vaciar_needs_confirmation() {
    let w = Workspace::new();
    let output = cli(&["almacen", "vaciar", "--almacen", s(&w.store_path())]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--confirmar"),
        "{}",
        stderr(&output)
    );
    assert_eq!(w.store().documents().len(), 1);

    let output = cli(&[
        "almacen",
        "vaciar",
        "--confirmar",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let store = w.store();
    assert!(store.documents().is_empty());
    // Los contadores no bajan.
    assert_eq!(store.counter("NOMBRE_PERSONA"), 1);
}

#[test]
fn purgar_and_every_opening_remove_expired_documents() {
    let w = Workspace::new();
    let mut store = w.store();
    store.record_document(record("doc-viejo", "2020-01-01", "vieja.txt"));
    w.save(&store);
    let output = cli(&["almacen", "purgar", "--almacen", s(&w.store_path())]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("Documentos caducados borrados: 1."),
        "{}",
        stdout(&output)
    );
    assert_eq!(w.store().documents().len(), 1);

    // Abrir el almacén para otra cosa también purga.
    let mut store = w.store();
    store.record_document(record("doc-viejo", "2020-01-01", "vieja.txt"));
    w.save(&store);
    cli(&[
        "desanonimizar",
        "--texto",
        "x",
        "--almacen",
        s(&w.store_path()),
    ]);
    assert_eq!(w.store().documents().len(), 1);
}

#[test]
fn almacen_rejects_unknown_subcommands_and_missing_arguments() {
    let w = Workspace::new();
    let store = w.store_path();
    for args in [
        vec!["almacen"],
        vec!["almacen", "borrar"],
        vec!["almacen", "exportar", "--almacen", s(&store)],
        vec!["almacen", "exportar", "doc-1", "--almacen", s(&store)],
        vec!["almacen", "importar", "--almacen", s(&store)],
        vec!["almacen", "olvidar", "--almacen", s(&store)],
        vec!["almacen", "listar", "sobra", "--almacen", s(&store)],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("Uso:"), "{args:?}");
    }
}
