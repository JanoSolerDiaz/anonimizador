//! Anonimizar un archivo de principio a fin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jiff::Zoned;

use super::{Detector, OutputFormat};
use crate::almacen::Store;
use crate::formatos::docx::Warnings;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct AnonymizeOptions {
    pub format: OutputFormat,
    /// Modelo que se anota en el almacén; ver [`super::model_id`].
    pub model: String,
    /// Fecha y hora del proceso.
    pub now: Zoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub document_id: String,
    pub output: PathBuf,
    /// Apariciones de cada tipo.
    pub entities: BTreeMap<String, usize>,
    pub warnings: Warnings,
}

pub fn anonymize(
    _input: &Path,
    _dir: &Path,
    _detector: &dyn Detector,
    _store: &mut Store,
    _options: &AnonymizeOptions,
) -> Result<Summary> {
    Err(Error::DocumentNotFound)
}
