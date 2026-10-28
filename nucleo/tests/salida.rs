//! Nombres de los archivos de salida y escritura atómica (PRD, «Nombres de
//! archivo» y «Escritura atómica»).
//!
//! La salida se llama `descripción_sufijo.ext`, nunca sobrescribe nada y, si
//! la escritura falla, no deja archivos a medias ni temporales.

use std::fs;
use std::path::Path;

use nucleo::Error;
use nucleo::formatos::salida::{Suffix, description, free_path, save_new, write_atomic};

/// Nombres de los archivos de una carpeta, ordenados.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn failure() -> Error {
    Error::Io(std::io::Error::other("fallo simulado"))
}

#[test]
fn the_description_comes_from_the_original_name() {
    let cases = [
        ("Contrato Cliente.docx", "contrato-cliente"),
        ("Informe Médico Ñúñez (v2).PDF", "informe-medico-nunez-v2"),
        ("  __raro__ nombre!!.txt", "raro-nombre"),
        ("acta_reunión_2026.md", "acta-reunion-2026"),
        ("copia.final.docx", "copia-final"),
        ("C:/carpeta/Otra Carpeta/Nota.txt", "nota"),
        ("Çà et là.docx", "ca-et-la"),
    ];
    for (original, expected) in cases {
        assert_eq!(description(Path::new(original)), expected, "{original}");
    }
}

#[test]
fn a_name_without_usable_letters_becomes_documento() {
    for original in ["数据.docx", "¡¿!?.txt", "---.md", ""] {
        assert_eq!(
            description(Path::new(original)),
            "documento",
            "{original:?}"
        );
    }
}

#[test]
fn long_descriptions_are_cut_without_a_trailing_hyphen() {
    let original = format!("{} final.docx", "palabra ".repeat(30));
    let d = description(Path::new(&original));
    assert!(d.chars().count() <= 80, "{d}");
    assert!(d.starts_with("palabra-palabra"), "{d}");
    assert!(!d.ends_with('-'), "{d}");
}

#[test]
fn suffixes_name_the_operation() {
    assert_eq!(Suffix::Anonymized.as_str(), "anonimizado");
    assert_eq!(Suffix::Deanonymized.as_str(), "desanonimizado");
    assert_eq!(Suffix::Dictionary.as_str(), "diccionario");
}

#[test]
fn free_path_adds_2_3_when_the_name_is_taken() {
    let dir = tempfile::tempdir().unwrap();
    let first = free_path(dir.path(), "contrato-cliente", Suffix::Anonymized, "docx");
    assert_eq!(first, dir.path().join("contrato-cliente_anonimizado.docx"));

    fs::write(&first, b"ya estaba").unwrap();
    let second = free_path(dir.path(), "contrato-cliente", Suffix::Anonymized, "docx");
    assert_eq!(
        second,
        dir.path().join("contrato-cliente_anonimizado_2.docx")
    );

    fs::write(&second, b"ya estaba").unwrap();
    let third = free_path(dir.path(), "contrato-cliente", Suffix::Anonymized, "docx");
    assert_eq!(
        third,
        dir.path().join("contrato-cliente_anonimizado_3.docx")
    );

    // Otra extensión no choca.
    let md = free_path(dir.path(), "contrato-cliente", Suffix::Anonymized, "md");
    assert_eq!(md, dir.path().join("contrato-cliente_anonimizado.md"));
}

#[test]
fn save_new_writes_everything_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("nota_anonimizado.txt");
    fs::write(&existing, b"original").unwrap();

    let path = save_new(dir.path(), "nota", Suffix::Anonymized, "txt", |out| {
        out.write_all(b"texto ")?;
        out.write_all(b"anonimizado")?;
        Ok(())
    })
    .unwrap();

    assert_eq!(path, dir.path().join("nota_anonimizado_2.txt"));
    assert_eq!(fs::read(&path).unwrap(), b"texto anonimizado");
    assert_eq!(fs::read(&existing).unwrap(), b"original");
    assert_eq!(
        names(dir.path()),
        ["nota_anonimizado.txt", "nota_anonimizado_2.txt"]
    );
}

#[test]
fn a_failed_save_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("otro.txt"), b"no se toca").unwrap();

    let result = save_new(dir.path(), "nota", Suffix::Anonymized, "txt", |out| {
        out.write_all(b"medio documento")?;
        Err(failure())
    });

    assert!(result.is_err());
    assert_eq!(names(dir.path()), ["otro.txt"]);
}

#[test]
fn a_panic_while_writing_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let result = std::panic::catch_unwind(move || {
        let _ = save_new(&path, "nota", Suffix::Anonymized, "txt", |out| {
            out.write_all(b"medio documento").unwrap();
            panic!("fallo inesperado");
        });
    });
    assert!(result.is_err());
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[test]
fn saving_into_a_missing_folder_fails_without_creating_it() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-existe");
    let result = save_new(&missing, "nota", Suffix::Anonymized, "txt", |_| Ok(()));
    assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
    assert!(!missing.exists());
}

#[test]
fn write_atomic_replaces_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("almacen.bin");
    fs::write(&path, b"version antigua y mas larga").unwrap();

    write_atomic(&path, |out| Ok(out.write_all(b"nueva")?)).unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"nueva");
    assert_eq!(names(dir.path()), ["almacen.bin"]);
}

#[test]
fn a_failed_atomic_write_keeps_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("almacen.bin");
    fs::write(&path, b"version antigua").unwrap();

    let result = write_atomic(&path, |out| {
        out.write_all(b"nue")?;
        Err(failure())
    });

    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"version antigua");
    assert_eq!(names(dir.path()), ["almacen.bin"]);
}
