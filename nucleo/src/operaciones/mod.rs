//! Operaciones completas sobre archivos: anonimizar y desanonimizar. Juntan
//! los extractores y escritores de [`crate::formatos`], el motor y el
//! almacén.

pub mod anonimizar;

use crate::Result;
use crate::motor::{Engine, Entity};

/// Lo que encuentra los datos personales de unos segmentos. Lo hace el
/// motor; las pruebas usan uno falso para no cargar el modelo.
pub trait Detector {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>>;
}

impl Detector for Engine {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        Engine::detect(self, segments)
    }
}

/// Formato del archivo de salida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Docx,
    Markdown,
    Text,
}

/// Modelo tal como se guarda en el almacén: `repositorio@revisión`.
pub fn model_id(name: &str) -> String {
    name.to_owned()
}
