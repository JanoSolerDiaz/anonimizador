//! Nombres de los archivos de salida y escritura atómica.
//!
//! La salida se llama `descripción_sufijo.ext` (PRD, «Nombres de archivo»):
//! la descripción sale del nombre original y el sufijo dice la operación. Si
//! el nombre ya existe se añade `_2`, `_3`: nunca se sobrescribe nada.
//!
//! Todo se escribe primero en un temporal de la misma carpeta y se renombra
//! al final. Si la escritura falla, el temporal se borra y no queda nada a
//! medias (PRD, «Escritura atómica»).

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::Result;

/// Longitud máxima de la descripción, en caracteres.
const MAX_DESCRIPTION_CHARS: usize = 80;

/// Descripción cuando el nombre original no deja ninguna letra ni cifra.
const FALLBACK_DESCRIPTION: &str = "documento";

/// Operación que produjo el archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suffix {
    Anonymized,
    Deanonymized,
    Dictionary,
}

impl Suffix {
    pub fn as_str(self) -> &'static str {
        match self {
            Suffix::Anonymized => "anonimizado",
            Suffix::Deanonymized => "desanonimizado",
            Suffix::Dictionary => "diccionario",
        }
    }
}

/// Descripción a partir del nombre original, sin carpeta ni extensión: en
/// minúsculas, sin acentos y con cualquier otro carácter convertido en un
/// guion. «Contrato Cliente.docx» da «contrato-cliente».
pub fn description(original: &Path) -> String {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plain: String = stem
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase();
    let joined = plain
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let cut: String = joined.chars().take(MAX_DESCRIPTION_CHARS).collect();
    let cut = cut.trim_end_matches('-');
    if cut.is_empty() {
        FALLBACK_DESCRIPTION.to_string()
    } else {
        cut.to_string()
    }
}

/// Primera ruta libre de `dir` para `descripción_sufijo.ext`, probando
/// después `_2`, `_3`…
pub fn free_path(dir: &Path, description: &str, suffix: Suffix, ext: &str) -> PathBuf {
    let suffix = suffix.as_str();
    let mut path = dir.join(format!("{description}_{suffix}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{description}_{suffix}_{n}.{ext}"));
        n += 1;
    }
    path
}

/// Escribe un archivo nuevo en `dir` con el nombre libre que toque y
/// devuelve su ruta. `write` recibe el destino; si devuelve un error, no
/// queda ningún archivo.
pub fn save_new(
    dir: &Path,
    description: &str,
    suffix: Suffix,
    ext: &str,
    write: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<PathBuf> {
    let temp = write_temp(dir, write)?;
    let path = free_path(dir, description, suffix, ext);
    temp.persist(&path)?;
    Ok(path)
}

/// Escribe `path` entero de una vez: o queda el contenido nuevo completo, o
/// se queda el que había. Sustituye el archivo si ya existe.
pub fn write_atomic(path: &Path, write: impl FnOnce(&mut dyn Write) -> Result<()>) -> Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    write_temp(dir, write)?.persist(path)
}

/// Temporal escrito y volcado a disco, que se borra solo si no se renombra.
struct TempFile {
    path: Option<PathBuf>,
}

impl TempFile {
    fn persist(mut self, target: &Path) -> Result<()> {
        let path = self.path.take().expect("el temporal ya se renombró");
        if let Err(error) = fs::rename(&path, target) {
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
        Ok(())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn write_temp(dir: &Path, write: impl FnOnce(&mut dyn Write) -> Result<()>) -> Result<TempFile> {
    let (file, path) = create_temp(dir)?;
    let temp = TempFile { path: Some(path) };
    let mut out = BufWriter::new(file);
    write(&mut out)?;
    let file = out.into_inner().map_err(|e| e.into_error())?;
    file.sync_all()?;
    Ok(temp)
}

/// Crea un temporal con un nombre que no existía en `dir`.
fn create_temp(dir: &Path) -> Result<(File, PathBuf)> {
    loop {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).map_err(|e| std::io::Error::other(e.to_string()))?;
        let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = dir.join(format!(".anonimizador-{hex}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
