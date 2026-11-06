//! El almacén abierto por la CLI o por la app: su archivo, su clave, la purga
//! de caducados al abrirlo, guardar, exportar e importar diccionarios, y
//! empezar uno nuevo cuando el que hay no se puede abrir (PRD, «Archivo del
//! almacén»).

use std::io::Write;
use std::path::{Path, PathBuf};

use jiff::civil::Date;

use super::archivo::{self, KeyProvider, MemoryKey, StoreKey, SystemKeyring};
use super::{Store, politica};
use crate::formatos::salida::{Suffix, description, save_new};
use crate::{Error, Result};

/// Variable con una clave de prueba en hexadecimal. Solo la aceptan las
/// compilaciones de depuración, para que las pruebas no toquen el almacén de
/// credenciales del sistema; las de `--release` la ignoran.
pub const TEST_KEY_ENV: &str = "ANONIMIZADOR_CLAVE_PRUEBA";

/// Aviso del PRD al exportar un diccionario.
pub const EXPORT_WARNING: &str = "Este archivo contiene los datos originales sin cifrar. Envíalo solo a quien deba desanonimizar el documento, y nunca junto al documento anonimizado.";

/// Lo que se añade al nombre del almacén que no se pudo abrir cuando se
/// empieza uno nuevo: `almacen.anon.no-se-pudo-abrir-2026-11-05`.
pub const SET_ASIDE_SUFFIX: &str = "no-se-pudo-abrir";

/// Clave del almacén: la del sistema, o la de prueba en depuración.
pub type Keys = Box<dyn KeyProvider + Send + Sync>;

/// Almacén abierto, con los documentos caducados ya borrados.
pub struct OpenStore {
    pub path: PathBuf,
    pub store: Store,
    /// Documentos caducados que se borraron al abrirlo.
    pub purged: usize,
    /// Plazo fijado por sistemas, si lo hay.
    policy: Option<u32>,
    /// Si al abrirlo cambió algo (purga o plazos acortados por la política).
    changed: bool,
    keys: Keys,
}

impl OpenStore {
    /// Abre el almacén de `path` con la clave del sistema y borra los
    /// documentos caducados el día `today`.
    pub fn open(path: &Path, today: Date) -> Result<OpenStore> {
        Self::open_with_policy(path, default_keys()?, today, politica::retention_policy())
    }

    /// Como [`OpenStore::open`], con otra clave y sin política.
    pub fn open_with(path: &Path, keys: Keys, today: Date) -> Result<OpenStore> {
        Self::open_with_policy(path, keys, today, None)
    }

    /// Como [`OpenStore::open`], con otra clave y la política `policy`. Con
    /// política, los documentos nuevos caducan a esos días y los guardados
    /// no pasan de esos días desde su creación.
    pub fn open_with_policy(
        path: &Path,
        keys: Keys,
        today: Date,
        policy: Option<u32>,
    ) -> Result<OpenStore> {
        let mut store = archivo::load(path, keys.as_ref())?;
        let mut capped = 0;
        if let Some(days) = policy {
            store = store.with_retention_days(days);
            capped = store.cap_expiry(days);
        }
        let before = store.documents().len();
        store.purge(today);
        let purged = before - store.documents().len();
        Ok(OpenStore {
            path: path.to_path_buf(),
            store,
            purged,
            policy,
            changed: purged > 0 || capped > 0,
            keys,
        })
    }

    /// Plazo fijado por sistemas, si lo hay.
    pub fn policy(&self) -> Option<u32> {
        self.policy
    }

    /// Aparta el almacén de `path` que no se puede abrir, sin borrarlo, y
    /// empieza uno vacío en su sitio. Devuelve el almacén nuevo y dónde quedó
    /// el antiguo (si había uno).
    pub fn start_over(
        path: &Path,
        keys: Keys,
        today: Date,
        policy: Option<u32>,
    ) -> Result<(OpenStore, Option<PathBuf>)> {
        let mut set_aside = None;
        for (from, extension) in [
            (path.to_path_buf(), ""),
            (archivo::backup_path(path), ".bak"),
        ] {
            if from.exists() {
                let to = free_name(path, today, extension);
                std::fs::rename(&from, &to)?;
                if extension.is_empty() {
                    set_aside = Some(to);
                }
            }
        }
        let store = match policy {
            Some(days) => Store::new().with_retention_days(days),
            None => Store::new(),
        };
        let open = OpenStore {
            path: path.to_path_buf(),
            store,
            purged: 0,
            policy,
            changed: false,
            keys,
        };
        open.save()?;
        Ok((open, set_aside))
    }

    /// Borra los documentos que caducan `today` o antes. Devuelve cuántos.
    pub fn purge(&mut self, today: Date) -> usize {
        let before = self.store.documents().len();
        self.store.purge(today);
        before - self.store.documents().len()
    }

    /// Guarda el almacén entero, creando su carpeta si hace falta.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        archivo::save(&self.path, &self.store, self.keys.as_ref())
    }

    /// Guarda solo si al abrirlo cambió algo.
    pub fn save_if_purged(&self) -> Result<()> {
        if self.changed { self.save() } else { Ok(()) }
    }

    /// Exporta el diccionario del documento `id` a `dir`, con el nombre
    /// `descripción_diccionario.json`. Hay que mostrar [`EXPORT_WARNING`].
    pub fn export(&self, id: &str, dir: &Path) -> Result<PathBuf> {
        let bytes = self.store.export_dictionary(id)?;
        let record = self
            .store
            .documents()
            .iter()
            .find(|d| d.id == id)
            .ok_or(Error::DocumentNotFound)?;
        save_new(
            dir,
            &description(Path::new(&record.output.name)),
            Suffix::Dictionary,
            "json",
            |out: &mut dyn Write| Ok(out.write_all(&bytes)?),
        )
    }

    /// Importa el diccionario de `file`. Devuelve el nombre con que queda.
    pub fn import(&mut self, file: &Path) -> Result<String> {
        let bytes = std::fs::read(file)?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.store.import_dictionary(&bytes, &name)?;
        Ok(name)
    }
}

/// La clave del sistema, o en depuración la de [`TEST_KEY_ENV`] si está.
pub fn default_keys() -> Result<Keys> {
    if cfg!(debug_assertions)
        && let Ok(hex) = std::env::var(TEST_KEY_ENV)
    {
        let key = parse_hex(&hex)
            .and_then(|bytes| StoreKey::from_bytes(&bytes))
            .ok_or_else(|| {
                Error::StoreInvalid(format!("{TEST_KEY_ENV} debe tener 64 cifras hexadecimales"))
            })?;
        return Ok(Box::new(MemoryKey::with_key(key)));
    }
    Ok(Box::new(SystemKeyring::new()))
}

/// `almacen.anon.no-se-pudo-abrir-AAAA-MM-DD` (más `extension`), con `_2`,
/// `_3`… si ya existe.
fn free_name(path: &Path, today: Date, extension: &str) -> PathBuf {
    let base = format!(
        "{}.{SET_ASIDE_SUFFIX}-{today}",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut candidate = path.with_file_name(format!("{base}{extension}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{base}_{n}{extension}"));
        n += 1;
    }
    candidate
}

fn parse_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}
