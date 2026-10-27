//! Tipos opcionales que el usuario decide no sustituir.
//!
//! Solo los tipos marcados como `opcional` en la tabla de tipos (`CARGO`,
//! `IMPORTE`, `LUGAR` y `COORDENADAS`) se pueden dejar sin sustituir. Por
//! defecto se sustituyen todos.

use std::collections::BTreeSet;

use super::Entity;
use super::tipos::TypeTable;
use crate::{Error, Result};

/// Tipos opcionales desmarcados. Vacío por defecto.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkippedTypes(BTreeSet<String>);

impl SkippedTypes {
    /// Valida que cada tipo existe y es opcional.
    pub fn new<S: AsRef<str>>(kinds: &[S]) -> Result<Self> {
        let table = TypeTable::builtin();
        let mut skipped = BTreeSet::new();
        for kind in kinds.iter().map(AsRef::as_ref) {
            match table.is_optional(kind) {
                Some(true) => {
                    skipped.insert(kind.to_owned());
                }
                Some(false) => {
                    let optional: Vec<&str> = table.optional_markers().collect();
                    return Err(Error::TypeNotOptional(format!(
                        "{kind}. Solo se pueden dejar sin sustituir: {}",
                        optional.join(", ")
                    )));
                }
                None => return Err(Error::UnknownType(kind.to_owned())),
            }
        }
        Ok(Self(skipped))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, kind: &str) -> bool {
        self.0.contains(kind)
    }

    /// Quita las entidades de los tipos desmarcados.
    pub fn retain(&self, entities: &mut Vec<Entity>) {
        entities.retain(|e| !self.contains(&e.kind));
    }
}
