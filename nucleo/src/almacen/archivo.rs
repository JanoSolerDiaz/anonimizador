//! Archivo del almacén en disco (PRD, «Archivo del almacén»).
//!
//! El archivo es la firma `ANONALMC` (8 bytes), la versión del archivo
//! (1 byte), un nonce aleatorio (12 bytes) y el JSON del almacén cifrado con
//! AES-256-GCM, con su etiqueta al final. La firma y la versión van como
//! datos asociados: cambiar cualquier byte se detecta al descifrar.
//!
//! La clave se genera la primera vez que se guarda y vive en el almacén de
//! credenciales del sistema. Se escribe con un temporal y un renombrado, y
//! se guarda una copia `.bak` del anterior hasta que la nueva termina. Si
//! falta la clave, el archivo antiguo no se toca.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use zeroize::Zeroizing;

use super::Store;
use crate::formatos::salida::write_atomic;
use crate::{Error, Result};

const FILE_MAGIC: &[u8; 8] = b"ANONALMC";
/// Versión del archivo cifrado; la del JSON de dentro va aparte.
const FILE_VERSION: u8 = 1;
const HEADER_LEN: usize = FILE_MAGIC.len() + 1;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Servicio con el que se guarda la clave en el almacén de credenciales.
pub const KEYRING_SERVICE: &str = "anonimizador";
const KEYRING_ACCOUNT: &str = "clave-almacen";

/// Clave de 256 bits. Se borra de la memoria al soltarla y nunca se muestra.
#[derive(Clone)]
pub struct StoreKey(Zeroizing<[u8; KEY_LEN]>);

impl StoreKey {
    /// Clave aleatoria nueva.
    pub fn generate() -> Result<StoreKey> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        getrandom::fill(bytes.as_mut()).map_err(random_error)?;
        Ok(StoreKey(bytes))
    }

    /// Clave a partir de sus 32 bytes, o `None` si no miden eso.
    pub fn from_bytes(bytes: &[u8]) -> Option<StoreKey> {
        let array: [u8; KEY_LEN] = bytes.try_into().ok()?;
        Some(StoreKey(Zeroizing::new(array)))
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(self.0.as_slice()).expect("la clave mide 32 bytes")
    }
}

impl fmt::Debug for StoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StoreKey(oculta)")
    }
}

/// Dónde se guarda la clave del almacén.
pub trait KeyProvider {
    /// Clave guardada, o `None` si aún no hay ninguna.
    fn load(&self) -> Result<Option<StoreKey>>;
    /// Guarda la clave, sustituyendo la que hubiera.
    fn save(&self, key: &StoreKey) -> Result<()>;
}

/// Clave en memoria, para las pruebas.
#[derive(Default)]
pub struct MemoryKey {
    key: Mutex<Option<StoreKey>>,
}

impl MemoryKey {
    /// Sin clave todavía.
    pub fn new() -> MemoryKey {
        MemoryKey::default()
    }

    pub fn with_key(key: StoreKey) -> MemoryKey {
        MemoryKey {
            key: Mutex::new(Some(key)),
        }
    }
}

impl fmt::Debug for MemoryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MemoryKey { .. }")
    }
}

impl KeyProvider for MemoryKey {
    fn load(&self) -> Result<Option<StoreKey>> {
        Ok(self.key.lock().expect("clave en memoria").clone())
    }

    fn save(&self, key: &StoreKey) -> Result<()> {
        *self.key.lock().expect("clave en memoria") = Some(key.clone());
        Ok(())
    }
}

/// Clave en el Administrador de credenciales de Windows o en el Llavero de
/// Mac, con el servicio `anonimizador`.
#[derive(Debug)]
pub struct SystemKeyring {
    account: String,
}

impl Default for SystemKeyring {
    fn default() -> Self {
        SystemKeyring::with_account(KEYRING_ACCOUNT)
    }
}

impl SystemKeyring {
    pub fn new() -> SystemKeyring {
        SystemKeyring::default()
    }

    /// Con otra cuenta, para que las pruebas no toquen la clave de verdad.
    pub fn with_account(account: &str) -> SystemKeyring {
        SystemKeyring {
            account: account.to_owned(),
        }
    }

    /// Borra la clave. Si no había ninguna, no hace nada.
    pub fn delete(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error(&error)),
        }
    }

    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, &self.account).map_err(|e| keyring_error(&e))
    }
}

impl KeyProvider for SystemKeyring {
    fn load(&self) -> Result<Option<StoreKey>> {
        match self.entry()?.get_secret() {
            Ok(secret) => {
                let secret = Zeroizing::new(secret);
                StoreKey::from_bytes(&secret)
                    .map(Some)
                    .ok_or_else(|| Error::Keyring("la clave guardada está dañada".into()))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keyring_error(&error)),
        }
    }

    fn save(&self, key: &StoreKey) -> Result<()> {
        self.entry()?
            .set_secret(key.0.as_slice())
            .map_err(|e| keyring_error(&e))
    }
}

/// Carpeta de la aplicación dentro de la carpeta de datos del usuario.
const APP_DIR: &str = "anonimizador";
/// Nombre del archivo del almacén.
const STORE_FILE: &str = "almacen.anon";

/// Ruta del almacén en la carpeta de datos del usuario (PRD, «Archivo del
/// almacén»): `%APPDATA%` en Windows y `~/Library/Application Support` en
/// Mac. En Linux, que solo se usa para desarrollar, `$XDG_DATA_HOME` o
/// `~/.local/share`. `None` si el sistema no dice dónde está esa carpeta.
pub fn default_path() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local").join("share")))
    };
    Some(base?.join(APP_DIR).join(STORE_FILE))
}

/// Lee el almacén de `path`. Si no existe, devuelve uno vacío; si solo
/// queda la copia `.bak`, lee la copia.
pub fn load(path: &Path, keys: &dyn KeyProvider) -> Result<Store> {
    let Some(bytes) = read_existing(path)? else {
        return Ok(Store::new());
    };
    check_header(&bytes)?;
    let key = keys.load()?.ok_or(Error::StoreKeyMissing)?;
    let json = decrypt(&key, &bytes)?;
    Store::from_json(&json)
}

/// Cifra y guarda el almacén entero en `path`. La primera vez crea la clave.
/// Si ya hay un archivo y falta la clave, da error sin tocar nada.
pub fn save(path: &Path, store: &Store, keys: &dyn KeyProvider) -> Result<()> {
    save_with(path, store, keys, |path, bytes| {
        write_atomic(path, |out| Ok(out.write_all(bytes)?))
    })
}

/// `save` con la escritura final aparte, para poder simular que falla.
fn save_with(
    path: &Path,
    store: &Store,
    keys: &dyn KeyProvider,
    write: impl FnOnce(&Path, &[u8]) -> Result<()>,
) -> Result<()> {
    let backup = backup_path(path);
    let key = match keys.load()? {
        Some(key) => key,
        None if !path.exists() && !backup.exists() => {
            let key = StoreKey::generate()?;
            keys.save(&key)?;
            key
        }
        None => return Err(Error::StoreKeyMissing),
    };
    let json = Zeroizing::new(store.to_json());
    let bytes = encrypt(&key, &json)?;

    if path.exists() {
        fs::copy(path, &backup)?;
    }
    write(path, &bytes)?;
    match fs::remove_file(&backup) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// `almacen.anon` → `almacen.anon.bak`.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".bak");
    PathBuf::from(name)
}

fn read_existing(path: &Path) -> Result<Option<Vec<u8>>> {
    for candidate in [path.to_path_buf(), backup_path(path)] {
        match fs::read(&candidate) {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

fn check_header(bytes: &[u8]) -> Result<()> {
    if bytes.len() < HEADER_LEN + NONCE_LEN || !bytes.starts_with(FILE_MAGIC) {
        return Err(Error::StoreInvalid("el archivo no es un almacén".into()));
    }
    let version = bytes[FILE_MAGIC.len()];
    if version > FILE_VERSION {
        return Err(Error::StoreVersion(version.into()));
    }
    Ok(())
}

fn encrypt(key: &StoreKey, json: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(random_error)?;
    let mut out = Vec::with_capacity(HEADER_LEN + NONCE_LEN + json.len() + 16);
    out.extend_from_slice(FILE_MAGIC);
    out.push(FILE_VERSION);
    let sealed = key
        .cipher()
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: json,
                aad: &out[..HEADER_LEN],
            },
        )
        .map_err(|_| std::io::Error::other("no se ha podido cifrar el almacén"))?;
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Descifra un archivo con la cabecera ya comprobada.
fn decrypt(key: &StoreKey, bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let (header, rest) = bytes.split_at(HEADER_LEN);
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    let nonce: [u8; NONCE_LEN] = nonce.try_into().expect("el nonce mide 12 bytes");
    key.cipher()
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: sealed,
                aad: header,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::StoreUndecryptable)
}

fn random_error(error: getrandom::Error) -> Error {
    std::io::Error::other(format!(
        "no se han podido generar números aleatorios: {error}"
    ))
    .into()
}

/// Solo la clase de fallo: algunos errores de `keyring` llevan dentro los
/// bytes del secreto.
fn keyring_error(error: &keyring::Error) -> Error {
    let what = match error {
        keyring::Error::NoStorageAccess(_) => "el sistema no da acceso; puede que esté bloqueado",
        keyring::Error::NoDefaultStore => "no hay ninguno disponible en este equipo",
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            "la clave guardada está dañada"
        }
        keyring::Error::PlatformFailure(_) => "el sistema ha devuelto un fallo",
        _ => "error inesperado",
    };
    Error::Keyring(what.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_the_new_write_fails_the_backup_stays_with_the_previous_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("almacen.anon");
        let keys = MemoryKey::new();
        let mut store = Store::new();
        store.assign("EMAIL", "B-EMAIL", "ana@ejemplo.es", "doc-1");
        save(&path, &store, &keys).unwrap();
        let previous = fs::read(&path).unwrap();

        let error = save_with(&path, &Store::new(), &keys, |_, _| {
            Err(std::io::Error::other("disco lleno").into())
        })
        .unwrap_err();
        assert!(matches!(error, Error::Io(_)));
        assert_eq!(fs::read(backup_path(&path)).unwrap(), previous);
        assert_eq!(load(&path, &keys).unwrap().to_json(), store.to_json());
    }
}
