//! Contrato entre `scripts/recursos.lock.json` (lo que descargan los scripts)
//! y `nucleo::recursos` (lo que busca la app). Si uno cambia sin el otro, la
//! app no encontraría sus recursos.

use std::path::Path;

use nucleo::recursos::{MODEL_FILES, Resources, platform_dir};
use serde_json::Value;

fn lock() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/recursos.lock.json");
    let text = std::fs::read_to_string(&path).expect("no se puede leer recursos.lock.json");
    serde_json::from_str(&text).expect("recursos.lock.json no es JSON válido")
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Nombres de destino de un binario para la plataforma actual.
fn binary_outputs(lock: &Value, name: &str) -> Vec<String> {
    let binary = lock["binarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["nombre"] == name)
        .unwrap_or_else(|| panic!("falta el binario {name} en el lock"));
    binary["plataformas"][platform_dir()]["archivos"]
        .as_object()
        .unwrap_or_else(|| panic!("{name} no tiene paquete para {}", platform_dir()))
        .values()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_lock_downloads_every_model_file_the_app_needs() {
    let lock = lock();
    let downloaded: Vec<&str> = lock["modelo"]["archivos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["nombre"].as_str().unwrap())
        .collect();
    for needed in MODEL_FILES {
        assert!(
            downloaded.contains(&needed),
            "el lock no descarga {needed}: {downloaded:?}"
        );
    }
}

#[test]
fn the_lock_puts_the_model_where_the_app_looks() {
    let lock = lock();
    assert_eq!(lock["modelo"]["destino"], "recursos/modelo");
    assert_eq!(file_name(&Resources::new("recursos").model_dir()), "modelo");
}

#[test]
fn onnxruntime_library_name_matches_for_this_platform() {
    let lock = lock();
    let expected = file_name(&Resources::new("recursos").onnxruntime_path());
    assert!(binary_outputs(&lock, "onnxruntime").contains(&expected));
}

#[test]
fn pdfium_library_name_matches_for_this_platform() {
    let lock = lock();
    let expected = file_name(&Resources::new("recursos").pdfium_path());
    assert!(binary_outputs(&lock, "pdfium").contains(&expected));
}

#[test]
fn every_hash_is_a_lowercase_sha256() {
    let lock = lock();
    let mut hashes: Vec<String> = lock["modelo"]["archivos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["sha256"].as_str().unwrap().to_owned())
        .collect();
    for binary in lock["binarios"].as_array().unwrap() {
        for package in binary["plataformas"].as_object().unwrap().values() {
            hashes.push(package["sha256"].as_str().unwrap().to_owned());
        }
    }
    for hash in hashes {
        assert_eq!(hash.len(), 64, "{hash}");
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{hash}"
        );
    }
}

#[test]
fn model_revision_is_a_fixed_commit_not_a_branch() {
    let lock = lock();
    let revision = lock["modelo"]["revision"].as_str().unwrap();
    assert_eq!(revision.len(), 40, "debe ser un hash de commit: {revision}");
    assert!(revision.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn every_download_uses_https() {
    let lock = lock();
    for binary in lock["binarios"].as_array().unwrap() {
        for package in binary["plataformas"].as_object().unwrap().values() {
            let url = package["url"].as_str().unwrap();
            assert!(url.starts_with("https://"), "{url}");
        }
    }
}
