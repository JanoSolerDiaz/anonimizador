//! Garantías de seguridad de la configuración de Tauri (PRD §Seguridad y
//! privacidad). Si alguien abre la CSP, añade permisos o activa el
//! actualizador, estas pruebas fallan.

use std::path::Path;

use serde_json::Value;

const ALLOWED_PERMISSIONS: [&str; 2] = ["core:event:default", "core:window:default"];

fn read_json(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{relative}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn config() -> Value {
    read_json("tauri.conf.json")
}

fn sources(csp: &Value, directive: &str) -> Vec<String> {
    csp[directive]
        .as_str()
        .unwrap_or_else(|| panic!("falta la directiva {directive}"))
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn all_sources(csp: &Value) -> Vec<(String, String)> {
    csp.as_object()
        .expect("la CSP debe ser un objeto de directivas")
        .iter()
        .flat_map(|(directive, value)| {
            value
                .as_str()
                .unwrap()
                .split_whitespace()
                .map(move |s| (directive.clone(), s.to_owned()))
        })
        .collect()
}

#[test]
fn identifier_is_the_approved_one() {
    assert_eq!(config()["identifier"], "com.cuatroochenta.anonimizador");
}

#[test]
fn csp_connect_src_only_allows_ipc() {
    let csp = &config()["app"]["security"]["csp"];
    let mut connect = sources(csp, "connect-src");
    connect.sort();
    assert_eq!(connect, ["'self'", "http://ipc.localhost", "ipc:"]);
}

#[test]
fn csp_has_no_external_origin() {
    let csp = &config()["app"]["security"]["csp"];
    for (directive, source) in all_sources(csp) {
        let external = (source.contains("://") && source != "http://ipc.localhost")
            || source.contains('*')
            || source == "https:"
            || source == "http:";
        assert!(!external, "{directive} permite un origen externo: {source}");
    }
}

#[test]
fn csp_forbids_inline_and_eval_scripts_and_styles() {
    let csp = &config()["app"]["security"]["csp"];
    for directive in ["default-src", "script-src", "style-src"] {
        for source in sources(csp, directive) {
            assert!(
                source != "'unsafe-inline'" && source != "'unsafe-eval'",
                "{directive} no puede permitir {source}"
            );
        }
    }
}

#[test]
fn csp_blocks_objects_and_framing() {
    let csp = &config()["app"]["security"]["csp"];
    assert_eq!(sources(csp, "object-src"), ["'none'"]);
    assert_eq!(sources(csp, "frame-ancestors"), ["'none'"]);
}

#[test]
fn dev_csp_only_adds_the_local_dev_server() {
    let dev = &config()["app"]["security"]["devCsp"];
    for (directive, source) in all_sources(dev) {
        if source.contains("://") {
            assert!(
                source == "http://ipc.localhost" || source.starts_with("ws://localhost:"),
                "devCsp {directive} permite un origen no local: {source}"
            );
        }
        assert!(!source.contains('*'), "devCsp {directive}: {source}");
    }
}

#[test]
fn prototype_is_frozen() {
    assert_eq!(config()["app"]["security"]["freezePrototype"], true);
}

#[test]
fn updater_is_disabled() {
    let config = config();
    assert!(
        config["plugins"]["updater"].is_null(),
        "el actualizador debe estar desactivado"
    );
    assert_eq!(config["bundle"]["createUpdaterArtifacts"], false);
}

#[test]
fn capabilities_only_grant_allowed_permissions() {
    let capability = read_json("capabilities/default.json");
    let permissions = capability["permissions"].as_array().unwrap();
    for permission in permissions {
        let name = permission
            .as_str()
            .expect("los permisos con alcance se revisan a mano");
        assert!(
            ALLOWED_PERMISSIONS.contains(&name),
            "permiso no autorizado: {name}"
        );
    }
}

#[test]
fn capabilities_apply_to_the_main_window() {
    let config = config();
    let windows = config["app"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["label"], "main");
    let capability = read_json("capabilities/default.json");
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
}

#[test]
fn no_other_capability_files_exist() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let files: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, ["default.json"]);
}

#[test]
fn single_instance_is_the_first_plugin_registered() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap();
    let first_plugin = source
        .find(".plugin(")
        .expect("debe registrarse al menos un plugin");
    assert!(
        source[first_plugin..].starts_with(".plugin(tauri_plugin_single_instance::init("),
        "single-instance debe ser el primer plugin"
    );
    assert!(
        source.contains(r#"get_webview_window("main")"#),
        "la segunda instancia debe enfocar la ventana main"
    );
}
