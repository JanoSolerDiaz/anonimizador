//! Pruebas de localización y comprobación de recursos, sin bibliotecas
//! nativas. Cada prueba crea su propia carpeta temporal.

use std::fs;
use std::path::{Path, PathBuf};

use nucleo::Error;
use nucleo::recursos::{MODEL_FILES, Resources, platform_dir};

/// Crea todos los archivos que `check_files` espera, vacíos.
fn create_all_files(resources: &Resources) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = MODEL_FILES
        .iter()
        .map(|name| resources.model_dir().join(name))
        .collect();
    files.push(resources.onnxruntime_path());
    files.push(resources.pdfium_path());
    for file in &files {
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, b"").unwrap();
    }
    files
}

#[test]
fn platform_dir_matches_the_compilation_target() {
    let expected = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "win-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "mac-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "mac-x64"
    } else {
        "no-soportada"
    };
    assert_eq!(platform_dir(), expected);
}

#[test]
fn paths_are_built_under_the_root() {
    let resources = Resources::new("raiz");

    assert_eq!(resources.root(), Path::new("raiz"));
    assert_eq!(resources.model_dir(), Path::new("raiz").join("modelo"));
    assert_eq!(
        resources.model_path(),
        Path::new("raiz").join("modelo").join("model_quantized.onnx")
    );
    assert_eq!(
        resources.onnxruntime_path().parent().unwrap(),
        Path::new("raiz").join("onnxruntime").join(platform_dir())
    );
    assert_eq!(
        resources.pdfium_dir(),
        Path::new("raiz").join("pdfium").join(platform_dir())
    );
    assert_eq!(resources.pdfium_path().parent().unwrap(), resources.pdfium_dir());
}

#[test]
fn native_library_names_follow_the_platform_convention() {
    let resources = Resources::new("raiz");
    let ort = resources.onnxruntime_path();
    let pdfium = resources.pdfium_path();
    let (ort_name, pdfium_name) = if cfg!(target_os = "windows") {
        ("onnxruntime.dll", "pdfium.dll")
    } else {
        ("libonnxruntime.dylib", "libpdfium.dylib")
    };
    assert_eq!(ort.file_name().unwrap(), ort_name);
    assert_eq!(pdfium.file_name().unwrap(), pdfium_name);
}

#[test]
fn check_files_passes_when_every_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let resources = Resources::new(dir.path());
    create_all_files(&resources);

    assert!(resources.check_files().is_ok());
}

#[test]
fn check_files_reports_each_missing_file_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let resources = Resources::new(dir.path());
    let files = create_all_files(&resources);

    for missing in &files {
        fs::remove_file(missing).unwrap();
        match resources.check_files() {
            Err(Error::ResourceMissing(path)) => assert_eq!(&path, missing),
            other => panic!("se esperaba ResourceMissing({missing:?}), llegó {other:?}"),
        }
        fs::write(missing, b"").unwrap();
    }
}

#[test]
fn check_files_rejects_a_directory_in_place_of_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let resources = Resources::new(dir.path());
    create_all_files(&resources);
    let model = resources.model_path();
    fs::remove_file(&model).unwrap();
    fs::create_dir(&model).unwrap();

    assert!(matches!(
        resources.check_files(),
        Err(Error::ResourceMissing(path)) if path == model
    ));
}

#[test]
fn locate_from_finds_recursos_in_an_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    let recursos = dir.path().join("recursos");
    let deep = dir.path().join("a").join("b").join("c");
    fs::create_dir_all(&recursos).unwrap();
    fs::create_dir_all(&deep).unwrap();

    let found = Resources::locate_from([deep]).unwrap();

    assert_eq!(found.root(), recursos);
}

#[test]
fn locate_from_prefers_the_first_start_that_has_recursos() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::create_dir(first.path().join("recursos")).unwrap();
    fs::create_dir(second.path().join("recursos")).unwrap();

    let found =
        Resources::locate_from([first.path().to_path_buf(), second.path().to_path_buf()]).unwrap();

    assert_eq!(found.root(), first.path().join("recursos"));
}

#[test]
fn locate_from_skips_a_start_without_recursos() {
    let empty = tempfile::tempdir().unwrap();
    let with_resources = tempfile::tempdir().unwrap();
    fs::create_dir(with_resources.path().join("recursos")).unwrap();

    let found = Resources::locate_from([
        empty.path().to_path_buf(),
        with_resources.path().to_path_buf(),
    ])
    .unwrap();

    assert_eq!(found.root(), with_resources.path().join("recursos"));
}

#[test]
fn locate_from_ignores_a_file_named_recursos() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("recursos"), b"no soy una carpeta").unwrap();

    // Sin ninguna carpeta `recursos/` en los ancestros de la carpeta temporal.
    let result = Resources::locate_from([dir.path().to_path_buf()]);

    match result {
        Err(Error::ResourcesDirNotFound) => {}
        Ok(found) => assert_ne!(found.root(), dir.path().join("recursos")),
        Err(other) => panic!("error inesperado: {other:?}"),
    }
}

#[test]
fn locate_from_without_starts_fails_clearly() {
    let result = Resources::locate_from(std::iter::empty::<PathBuf>());
    assert!(matches!(result, Err(Error::ResourcesDirNotFound)));
}
