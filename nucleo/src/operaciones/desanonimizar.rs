//! Desanonimizar.

use std::path::{Path, PathBuf};

use crate::almacen::Store;
use crate::{Error, Result};

/// De dónde sale el valor de un marcador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerSource {
    Store,
    Dictionary(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub marker: String,
    pub sources: Vec<MarkerSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub restored: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    Done(T),
    Conflicts(Vec<Conflict>),
}

pub fn deanonymize_text(
    _text: &str,
    _store: &Store,
    _source: Option<&MarkerSource>,
) -> Result<Outcome<(String, Report)>> {
    Err(Error::DocumentNotFound)
}

pub fn deanonymize(
    _input: &Path,
    _dir: &Path,
    _store: &Store,
    _source: Option<&MarkerSource>,
) -> Result<Outcome<(PathBuf, Report)>> {
    Err(Error::DocumentNotFound)
}
