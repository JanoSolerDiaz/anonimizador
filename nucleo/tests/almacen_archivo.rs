//! Archivo del almacén (PRD, «Archivo del almacén»): cifrado con AES-256-GCM,
//! clave en el almacén de credenciales, escritura atómica con copia `.bak`.
//! Si falta la clave o el archivo no descifra, error claro y el archivo
//! antiguo no se toca.

use std::fs;
use std::path::{Path, PathBuf};

use nucleo::Error;
use nucleo::almacen::Store;
use nucleo::almacen::archivo::{KeyProvider, MemoryKey, StoreKey, SystemKeyring, load, save};

fn store_with_data() -> Store {
    let mut store = Store::new();
    store.assign("NOMBRE_PERSONA", "B-NOMBRE", "Juan Pérez", "doc-1");
    store.assign("EMAIL", "B-EMAIL", "juan.perez@ejemplo.es", "doc-1");
    store
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("almacen.anon")
}

fn bak_path(dir: &Path) -> PathBuf {
    dir.join("almacen.anon.bak")
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[test]
fn without_a_file_the_store_starts_empty_and_no_key_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKey::new();
    let store = load(&store_path(dir.path()), &keys).unwrap();
    assert_eq!(store.entries().count(), 0);
    assert!(keys.load().unwrap().is_none(), "leer no crea la clave");
}

#[test]
fn the_first_save_creates_the_key_and_the_store_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    let store = store_with_data();
    save(&path, &store, &keys).unwrap();
    assert!(keys.load().unwrap().is_some(), "la clave se crea al guardar");

    let again = load(&path, &keys).unwrap();
    assert_eq!(again.to_json(), store.to_json());

    // Una segunda vez reutiliza la clave: se sigue pudiendo leer.
    save(&path, &again, &keys).unwrap();
    assert_eq!(load(&path, &keys).unwrap().to_json(), store.to_json());
}

#[test]
fn the_file_on_disk_has_no_readable_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    save(&path, &store_with_data(), &MemoryKey::new()).unwrap();
    let bytes = fs::read(&path).unwrap();
    for text in [
        "Juan",
        "juan perez",
        "ejemplo.es",
        "NOMBRE_PERSONA",
        "marcador",
        "anonimizador-almacen",
    ] {
        assert!(!contains(&bytes, text), "el archivo deja leer «{text}»");
    }
}

#[test]
fn two_saves_of_the_same_store_give_different_bytes() {
    // Cada escritura usa un nonce nuevo.
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    let store = store_with_data();
    save(&path, &store, &keys).unwrap();
    let first = fs::read(&path).unwrap();
    save(&path, &store, &keys).unwrap();
    assert_ne!(fs::read(&path).unwrap(), first);
}

#[test]
fn another_key_cannot_decrypt_it_and_the_file_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    save(&path, &store_with_data(), &MemoryKey::new()).unwrap();
    let before = fs::read(&path).unwrap();

    let other = MemoryKey::with_key(StoreKey::generate().unwrap());
    let error = load(&path, &other).unwrap_err();
    assert!(matches!(error, Error::StoreUndecryptable), "{error:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn any_changed_byte_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    save(&path, &store_with_data(), &keys).unwrap();
    let original = fs::read(&path).unwrap();

    // La cabecera, el nonce, el texto cifrado y la etiqueta final.
    for index in [8, 12, original.len() / 2, original.len() - 1] {
        let mut changed = original.clone();
        changed[index] ^= 0x01;
        fs::write(&path, &changed).unwrap();
        let error = load(&path, &keys).unwrap_err();
        assert!(
            matches!(error, Error::StoreUndecryptable | Error::StoreVersion(_)),
            "byte {index}: {error:?}"
        );
    }
}

#[test]
fn a_cut_file_or_another_file_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    save(&path, &store_with_data(), &keys).unwrap();
    let original = fs::read(&path).unwrap();

    fs::write(&path, &original[..original.len() - 10]).unwrap();
    assert!(matches!(
        load(&path, &keys),
        Err(Error::StoreUndecryptable)
    ));
    fs::write(&path, &original[..5]).unwrap();
    assert!(matches!(load(&path, &keys), Err(Error::StoreInvalid(_))));
    fs::write(&path, b"{\"formato\": \"anonimizador-almacen\"}").unwrap();
    assert!(matches!(load(&path, &keys), Err(Error::StoreInvalid(_))));
}

#[test]
fn a_newer_file_version_is_reported_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    save(&path, &store_with_data(), &keys).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    // El byte que sigue a la firma de 8 bytes es la versión del archivo.
    bytes[8] = 2;
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(load(&path, &keys), Err(Error::StoreVersion(2))));
}

#[test]
fn a_missing_key_is_a_clear_error_and_nothing_is_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    save(&path, &store_with_data(), &MemoryKey::new()).unwrap();
    let before = fs::read(&path).unwrap();

    let no_key = MemoryKey::new();
    let error = load(&path, &no_key).unwrap_err();
    assert!(matches!(error, Error::StoreKeyMissing), "{error:?}");
    assert!(error.to_string().contains("clave"), "{error}");

    // Guardar sin la clave no crea otra ni pisa el archivo antiguo.
    let error = save(&path, &Store::new(), &no_key).unwrap_err();
    assert!(matches!(error, Error::StoreKeyMissing), "{error:?}");
    assert!(no_key.load().unwrap().is_none());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn a_successful_save_leaves_neither_backup_nor_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    save(&path, &store_with_data(), &keys).unwrap();
    save(&path, &Store::new(), &keys).unwrap();
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["almacen.anon"]);
}

#[test]
fn an_interrupted_write_keeps_the_previous_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    let store = store_with_data();
    save(&path, &store, &keys).unwrap();

    // Lo que queda si el proceso muere a mitad: la copia `.bak` y un
    // temporal sin renombrar.
    fs::copy(&path, bak_path(dir.path())).unwrap();
    fs::write(dir.path().join(".anonimizador-0123456789abcdef.tmp"), b"a medias").unwrap();
    assert_eq!(load(&path, &keys).unwrap().to_json(), store.to_json());

    // La siguiente escritura termina bien y retira la copia.
    save(&path, &store, &keys).unwrap();
    assert!(!bak_path(dir.path()).exists());
}

#[test]
fn if_only_the_backup_survived_it_is_loaded() {
    // Si el archivo principal hubiera desaparecido, la copia `.bak` es el
    // último almacén completo.
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let keys = MemoryKey::new();
    let store = store_with_data();
    save(&path, &store, &keys).unwrap();
    fs::rename(&path, bak_path(dir.path())).unwrap();
    assert_eq!(load(&path, &keys).unwrap().to_json(), store.to_json());
}

#[test]
fn errors_never_contain_store_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    save(&path, &store_with_data(), &MemoryKey::new()).unwrap();
    let errors = [
        load(&path, &MemoryKey::new()).unwrap_err(),
        load(&path, &MemoryKey::with_key(StoreKey::generate().unwrap())).unwrap_err(),
    ];
    for error in errors {
        let text = format!("{error} {error:?}");
        assert!(!text.contains("Juan"), "{text}");
    }
}

#[test]
fn the_key_never_shows_in_debug_output() {
    let key = StoreKey::generate().unwrap();
    let keys = MemoryKey::with_key(key);
    let text = format!("{keys:?}");
    assert!(!text.chars().any(|c| c.is_ascii_digit()), "{text}");
}

/// Toca el almacén de credenciales real del sistema. Usa una cuenta propia
/// de pruebas y la borra al terminar.
#[test]
#[ignore = "usa el almacén de credenciales real del sistema"]
fn the_system_keyring_keeps_the_key() {
    let keys = SystemKeyring::with_account("prueba-anonimizador-almacen");
    keys.delete().unwrap();
    assert!(keys.load().unwrap().is_none());

    let dir = tempfile::tempdir().unwrap();
    let path = store_path(dir.path());
    let store = store_with_data();
    save(&path, &store, &keys).unwrap();
    assert!(keys.load().unwrap().is_some());
    assert_eq!(load(&path, &keys).unwrap().to_json(), store.to_json());

    keys.delete().unwrap();
    assert!(matches!(load(&path, &keys), Err(Error::StoreKeyMissing)));
}
