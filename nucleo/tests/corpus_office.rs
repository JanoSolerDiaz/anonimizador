//! Corpus sintético de PowerPoint y Excel (`tests/corpus/pptx` y
//! `tests/corpus/xlsx`). Lo genera `nucleo/examples/generar_corpus_office.rs`;
//! aquí se comprueba que el generador es determinista, que el corpus guardado
//! es el que genera y que cada paquete está completo y con XML bien formado.

#[path = "../examples/generar_corpus_office.rs"]
#[allow(dead_code)]
mod generador;

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus")
}

fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for folder in ["pptx", "xlsx"] {
        for entry in fs::read_dir(dir.join(folder)).unwrap() {
            let path = entry.unwrap().path();
            let name = format!("{folder}/{}", path.file_name().unwrap().to_string_lossy());
            files.push((name, fs::read(&path).unwrap()));
        }
    }
    files.sort();
    files
}

fn read_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            (file.name().unwrap().into_owned(), data)
        })
        .collect()
}

/// Valores del atributo `attribute` de los elementos `element`. Falla si el
/// XML está mal formado o deja etiquetas abiertas.
fn parse_xml(name: &str, data: &[u8], element: &str, attribute: &str) -> Vec<String> {
    let mut reader = Reader::from_reader(data);
    let mut found = Vec::new();
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                depth += 1;
                if e.local_name().as_ref() == element {
                    for a in e.attributes() {
                        let a = a.unwrap_or_else(|err| panic!("{name}: {err}"));
                        if a.key.local_name().as_ref() == attribute {
                            found.push(a.value.into_owned());
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) if e.local_name().as_ref() == element => {
                for a in e.attributes() {
                    let a = a.unwrap_or_else(|err| panic!("{name}: {err}"));
                    if a.key.local_name().as_ref() == attribute {
                        found.push(a.value.into_owned());
                    }
                }
            }
            Ok(Event::End(_)) => depth -= 1,
            Ok(_) => {}
            Err(error) => panic!("{name}: XML mal formado: {error}"),
        }
        buffer.clear();
    }
    assert_eq!(depth, 0, "{name}: quedan etiquetas abiertas");
    found
}

#[test]
fn the_generator_is_deterministic_and_matches_the_saved_corpus() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    generador::generate(first.path()).unwrap();
    generador::generate(second.path()).unwrap();
    let generated = files_in(first.path());
    assert_eq!(
        generated,
        files_in(second.path()),
        "dos ejecuciones difieren"
    );
    let saved = files_in(&corpus_dir());
    let names = |files: &[(String, Vec<u8>)]| files.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
    assert_eq!(names(&saved), names(&generated));
    for ((name, saved), (_, generated)) in saved.iter().zip(&generated) {
        // Git puede guardar los JSON con otros saltos de línea.
        let normalize = |bytes: &[u8]| -> Vec<u8> {
            if name.ends_with(".json") {
                String::from_utf8_lossy(bytes)
                    .replace("\r\n", "\n")
                    .into_bytes()
            } else {
                bytes.to_vec()
            }
        };
        assert!(
            normalize(saved) == normalize(generated),
            "{name} no es el que genera el generador; vuelva a ejecutar \
             `cargo run -p nucleo --example generar_corpus_office`"
        );
    }
}

#[test]
fn every_package_is_complete_and_its_xml_well_formed() {
    for (label, bytes) in files_in(&corpus_dir()) {
        if label.ends_with(".json") {
            continue;
        }
        let entries = read_entries(&bytes);
        let names: BTreeSet<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            entries[0].0, "[Content_Types].xml",
            "{label}: debe ir primero"
        );
        for (name, data) in &entries {
            if name.ends_with(".xml") || name.ends_with(".rels") {
                parse_xml(&format!("{label}/{name}"), data, "", "");
            }
        }
        // Cada parte declarada existe, y cada parte XML está declarada.
        let types = &entries[0].1;
        let declared = parse_xml(&label, types, "Override", "PartName");
        for part in &declared {
            assert!(names.contains(&part[1..]), "{label}: falta {part}");
        }
        for (name, _) in &entries {
            if name.ends_with(".xml") && name != "[Content_Types].xml" {
                assert!(
                    declared.iter().any(|d| d[1..] == **name),
                    "{label}: {name} sin tipo"
                );
            }
        }
        // Cada relación interna apunta a una parte que existe.
        for (name, data) in entries.iter().filter(|e| e.0.ends_with(".rels")) {
            let folder = name.rsplit_once("_rels/").unwrap().0.trim_end_matches('/');
            let targets = parse_xml(name, data, "Relationship", "Target");
            // Los externos (`mailto:`) no son partes del paquete.
            for target in targets.into_iter().filter(|t| !t.contains(':')) {
                let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
                for piece in target.split('/') {
                    match piece {
                        ".." => {
                            parts.pop();
                        }
                        piece => parts.push(piece),
                    }
                }
                let resolved = parts.join("/");
                assert!(
                    names.contains(resolved.as_str()),
                    "{label}: {name} apunta a {resolved}, que no existe"
                );
            }
        }
    }
}
