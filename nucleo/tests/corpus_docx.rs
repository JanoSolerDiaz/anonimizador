//! Corpus DOCX sintético (`tests/corpus/docx`). Lo genera
//! `nucleo/examples/generar_corpus_docx.rs`; aquí se comprueba que el
//! generador es determinista, que el corpus guardado es el que genera y que
//! cada paquete está completo y con XML bien formado.

#[path = "../examples/generar_corpus_docx.rs"]
#[allow(dead_code)]
mod generador;

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus/docx")
}

fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

fn docx_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "docx"))
        .collect();
    paths.sort();
    paths
}

fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

fn read_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            (file.name().to_owned(), data)
        })
        .collect()
}

/// Lee el XML entero; falla si está mal formado. Devuelve los atributos
/// pedidos de cada elemento con ese nombre local.
fn parse_xml(name: &str, data: &[u8], element: &[u8], attribute: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_reader(data);
    let mut found = Vec::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == element => {
                for a in e.attributes() {
                    let a = a.unwrap_or_else(|err| panic!("{name}: {err}"));
                    if a.key.local_name().as_ref() == attribute {
                        found.push(String::from_utf8(a.value.into_owned()).unwrap());
                    }
                }
            }
            Ok(_) => {}
            Err(error) => panic!("{name}: XML mal formado: {error}"),
        }
        buffer.clear();
    }
    found
}

#[test]
fn the_generator_is_deterministic_and_matches_the_saved_corpus() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    generador::generate(first.path()).unwrap();
    generador::generate(second.path()).unwrap();
    let generated = files_in(first.path());
    assert_eq!(generated, files_in(second.path()), "dos ejecuciones difieren");

    let saved = files_in(&corpus_dir());
    let names = |files: &[(String, Vec<u8>)]| files.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
    assert_eq!(names(&saved), names(&generated));
    for ((name, saved), (_, generated)) in saved.iter().zip(&generated) {
        assert!(
            saved == generated,
            "{name} no es el que genera el generador; vuelva a ejecutar \
             `cargo run -p nucleo --example generar_corpus_docx`"
        );
    }
}

#[test]
fn the_corpus_covers_the_four_languages_and_a_protected_file() {
    let names: Vec<String> = docx_paths()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for prefix in ["es-", "ca-", "en-", "fr-"] {
        assert!(
            names.iter().any(|n| n.starts_with(prefix) && !n.contains("protegido")),
            "falta un DOCX que empiece por {prefix}: {names:?}"
        );
    }
    let protected: Vec<&String> = names.iter().filter(|n| n.contains("protegido")).collect();
    assert_eq!(protected.len(), 1, "{names:?}");
    let bytes = fs::read(corpus_dir().join(protected[0])).unwrap();
    // Un DOCX con contraseña es un archivo OLE, no un ZIP.
    assert!(bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]));
}

#[test]
fn every_docx_has_its_expected_entities() {
    for path in docx_paths() {
        let json = path.with_extension("json");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&json).unwrap_or_else(|_| panic!("falta {json:?}")))
                .unwrap();
        if path.to_string_lossy().contains("protegido") {
            assert_eq!(value["protegido"], true, "{json:?}");
            continue;
        }
        let entities = value["entidades"].as_array().unwrap();
        assert!(!entities.is_empty(), "{json:?}");
        for entity in entities {
            assert!(entity["tipo"].is_string() && entity["texto"].is_string(), "{json:?}");
        }
        for key in ["imagenes", "objetos"] {
            assert!(value[key].is_u64(), "{json:?}: falta {key}");
        }
        assert!(value["metadatos_autor"].is_array(), "{json:?}");
        assert!(value["autores_revision"].is_array(), "{json:?}");
    }
}

#[test]
fn every_package_is_complete_and_its_xml_well_formed() {
    for path in docx_paths() {
        let bytes = fs::read(&path).unwrap();
        if !is_zip(&bytes) {
            continue;
        }
        let entries = read_entries(&bytes);
        let names: BTreeSet<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        let label = path.file_name().unwrap().to_string_lossy();
        assert_eq!(entries[0].0, "[Content_Types].xml", "{label}: debe ir primero");

        for (name, data) in &entries {
            if name.ends_with(".xml") || name.ends_with(".rels") {
                parse_xml(&format!("{label}/{name}"), data, b"", b"");
            }
        }

        // Cada parte declarada en [Content_Types].xml existe.
        let types = &entries[0].1;
        for part in parse_xml(&label, types, b"Override", b"PartName") {
            assert!(names.contains(&part[1..]), "{label}: falta {part}");
        }

        // Cada relación interna apunta a una parte que existe.
        for (name, data) in entries.iter().filter(|e| e.0.ends_with(".rels")) {
            let base = name.replace("_rels/", "").replace(".rels", "");
            let folder = Path::new(&base).parent().unwrap().to_string_lossy().into_owned();
            let targets = parse_xml(name, data, b"Relationship", b"Target");
            let modes = parse_xml(name, data, b"Relationship", b"TargetMode");
            let external = modes.iter().filter(|m| *m == "External").count();
            let internal: Vec<&String> = targets
                .iter()
                .filter(|t| !t.starts_with("mailto:") && !t.starts_with("http"))
                .collect();
            assert_eq!(targets.len() - internal.len(), external, "{label}/{name}");
            for target in internal {
                let full = if folder.is_empty() {
                    target.clone()
                } else {
                    format!("{folder}/{target}")
                };
                assert!(names.contains(full.as_str()), "{label}/{name}: falta {full}");
            }
        }
    }
}

#[test]
fn the_corpus_has_every_difficult_case() {
    let mut all = String::new();
    let mut parts = BTreeSet::new();
    for path in docx_paths() {
        let bytes = fs::read(&path).unwrap();
        if !is_zip(&bytes) {
            continue;
        }
        for (name, data) in read_entries(&bytes) {
            parts.insert(name.clone());
            all.push_str(&String::from_utf8_lossy(&data));
        }
    }
    for part in [
        "word/header1.xml",
        "word/footer1.xml",
        "word/footnotes.xml",
        "word/endnotes.xml",
        "word/comments.xml",
        "word/numbering.xml",
        "word/media/imagen1.png",
        "docProps/core.xml",
        "docProps/app.xml",
    ] {
        assert!(parts.contains(part), "ninguna parte {part}");
    }
    for marker in [
        "<w:tbl>",
        "<w:txbxContent>",
        "<w:ins ",
        "<w:delText",
        "<w:instrText",
        "mailto:",
        "<w:hyperlink ",
        "<o:OLEObject ",
        "<w:tab/>",
        "<w:br/>",
        "<dc:creator>",
        "<cp:lastModifiedBy>",
        "<Company>",
        "w:val=\"Heading1\"",
        "<w:numPr>",
    ] {
        assert!(all.contains(marker), "el corpus no tiene {marker}");
    }
}
