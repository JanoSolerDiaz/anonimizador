//! Tabla de tipos de datos: qué marcador en español corresponde a cada
//! etiqueta del modelo y qué tipos puede elegir no sustituir el usuario.
//!
//! La tabla vive en `nucleo/config/tipos.json`, versionada, y se incluye en el
//! binario al compilar. Al cargar un modelo se comprueba que la tabla cubra
//! todas sus etiquetas (`check_model_labels`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::{Error, Result};

const BUILTIN_JSON: &str = include_str!("../../config/tipos.json");

/// Un tipo de dato de la tabla.
#[derive(Debug, Clone, Deserialize)]
pub struct TypeEntry {
    /// Nombre del marcador, por ejemplo `NOMBRE_PERSONA` en `[NOMBRE_PERSONA_0042]`.
    #[serde(rename = "marcador")]
    pub marker: String,
    /// Etiqueta del modelo sin el prefijo BIO, o `None` si solo la asignan las reglas.
    #[serde(rename = "etiqueta_modelo")]
    pub model_label: Option<String>,
    /// Si el usuario puede elegir no sustituir este tipo.
    #[serde(rename = "opcional")]
    pub optional: bool,
}

#[derive(Debug, Deserialize)]
struct TypeFile {
    formato: String,
    version_formato: u32,
    tipos: Vec<TypeEntry>,
}

/// Tabla de tipos validada.
#[derive(Debug)]
pub struct TypeTable {
    entries: Vec<TypeEntry>,
}

impl TypeTable {
    /// La tabla incluida en el binario. Su validez la garantizan las pruebas,
    /// así que un fallo aquí es un error de programación.
    pub fn builtin() -> &'static TypeTable {
        static TABLE: OnceLock<TypeTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            TypeTable::from_json(BUILTIN_JSON).expect("nucleo/config/tipos.json no es válida")
        })
    }

    /// Lee y valida una tabla en formato JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        let invalid = |message: String| Error::InvalidTypeTable(message);
        let file: TypeFile = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
        if file.formato != "anonimizador-tipos" || file.version_formato != 1 {
            return Err(invalid(format!(
                "formato desconocido «{}» versión {}",
                file.formato, file.version_formato
            )));
        }

        let mut markers = BTreeSet::new();
        let mut labels = BTreeSet::new();
        for entry in &file.tipos {
            if !is_valid_marker(&entry.marker) {
                return Err(invalid(format!(
                    "el marcador «{}» solo puede tener mayúsculas sin acentos, números y guiones bajos",
                    entry.marker
                )));
            }
            if !markers.insert(entry.marker.as_str()) {
                return Err(invalid(format!(
                    "el marcador «{}» está repetido",
                    entry.marker
                )));
            }
            if let Some(label) = &entry.model_label
                && !labels.insert(label.as_str())
            {
                return Err(invalid(format!("la etiqueta «{label}» está repetida")));
            }
        }
        Ok(Self {
            entries: file.tipos,
        })
    }

    pub fn markers(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.marker.as_str())
    }

    pub fn model_labels(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|e| e.model_label.as_deref())
    }

    pub fn optional_markers(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|e| e.optional)
            .map(|e| e.marker.as_str())
    }

    pub fn contains_marker(&self, marker: &str) -> bool {
        self.entry(marker).is_some()
    }

    /// `Some(true)` si el tipo es opcional, `None` si no existe.
    pub fn is_optional(&self, marker: &str) -> Option<bool> {
        self.entry(marker).map(|e| e.optional)
    }

    /// Marcador que corresponde a una etiqueta del modelo sin prefijo BIO.
    pub fn marker_for_label(&self, label: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.model_label.as_deref() == Some(label))
            .map(|e| e.marker.as_str())
    }

    /// Comprueba que la tabla cubre todas las etiquetas del modelo. Las
    /// etiquetas deben ser `O` o llevar el prefijo `B-` o `I-`.
    pub fn check_model_labels(&self, labels: &[String]) -> Result<()> {
        let mut without_prefix = Vec::new();
        let mut unknown = BTreeSet::new();
        for label in labels.iter().filter(|l| *l != "O") {
            match label
                .strip_prefix("B-")
                .or_else(|| label.strip_prefix("I-"))
            {
                Some(kind) if self.marker_for_label(kind).is_none() => {
                    unknown.insert(kind);
                }
                Some(_) => {}
                None => without_prefix.push(label.as_str()),
            }
        }
        if !without_prefix.is_empty() {
            return Err(Error::InvalidTypeTable(format!(
                "etiquetas del modelo sin prefijo B- o I-: {}",
                without_prefix.join(", ")
            )));
        }
        if !unknown.is_empty() {
            let unknown: Vec<&str> = unknown.into_iter().collect();
            return Err(Error::InvalidTypeTable(format!(
                "faltan en la tabla estos tipos del modelo: {}",
                unknown.join(", ")
            )));
        }
        Ok(())
    }

    fn entry(&self, marker: &str) -> Option<&TypeEntry> {
        self.entries.iter().find(|e| e.marker == marker)
    }
}

fn is_valid_marker(marker: &str) -> bool {
    !marker.is_empty()
        && !marker.starts_with('_')
        && !marker.ends_with('_')
        && marker
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Lee las etiquetas de `id2label` de un `config.json` de Hugging Face, en el
/// orden de su identificador. Los identificadores deben ser 0, 1, 2… sin huecos,
/// porque son la posición de cada etiqueta en la salida del modelo.
pub fn labels_from_config(config_json: &str) -> Result<Vec<String>> {
    let invalid = |message: String| Error::InvalidModelConfig(message);

    #[derive(Deserialize)]
    struct Config {
        id2label: BTreeMap<String, String>,
    }
    let config: Config = serde_json::from_str(config_json).map_err(|e| invalid(e.to_string()))?;
    if config.id2label.is_empty() {
        return Err(invalid("id2label está vacío".into()));
    }

    let mut by_id = BTreeMap::new();
    for (id, label) in config.id2label {
        let id: usize = id.parse().map_err(|_| {
            invalid(format!(
                "el identificador «{id}» de id2label no es un número"
            ))
        })?;
        by_id.insert(id, label);
    }
    if by_id.keys().copied().ne(0..by_id.len()) {
        return Err(invalid(
            "los identificadores de id2label deben ir de 0 en adelante sin huecos".into(),
        ));
    }
    Ok(by_id.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(tipos: &str) -> Result<TypeTable> {
        TypeTable::from_json(&format!(
            r#"{{"formato": "anonimizador-tipos", "version_formato": 1, "tipos": [{tipos}]}}"#
        ))
    }

    #[test]
    fn a_valid_table_is_accepted() {
        let table = table(
            r#"{"marcador": "NOMBRE_PERSONA", "etiqueta_modelo": "PERSON_NAME", "opcional": false},
               {"marcador": "DNI", "etiqueta_modelo": null, "opcional": false}"#,
        )
        .unwrap();
        assert_eq!(
            table.marker_for_label("PERSON_NAME"),
            Some("NOMBRE_PERSONA")
        );
        assert_eq!(table.marker_for_label("DNI"), None);
    }

    #[test]
    fn repeated_markers_are_rejected() {
        let result = table(
            r#"{"marcador": "LUGAR", "etiqueta_modelo": "LOCATION", "opcional": true},
               {"marcador": "LUGAR", "etiqueta_modelo": "GEO_LOCATION", "opcional": true}"#,
        );
        assert!(matches!(result, Err(Error::InvalidTypeTable(m)) if m.contains("repetido")));
    }

    #[test]
    fn repeated_labels_are_rejected() {
        let result = table(
            r#"{"marcador": "LUGAR", "etiqueta_modelo": "LOCATION", "opcional": true},
               {"marcador": "SITIO", "etiqueta_modelo": "LOCATION", "opcional": true}"#,
        );
        assert!(matches!(result, Err(Error::InvalidTypeTable(m)) if m.contains("repetida")));
    }

    #[test]
    fn markers_with_accents_lowercase_spaces_or_edge_underscores_are_rejected() {
        for marker in [
            "DIRECCIÓN",
            "lugar",
            "NOMBRE PERSONA",
            "_LUGAR",
            "LUGAR_",
            "",
        ] {
            let entry = format!(
                r#"{{"marcador": "{marker}", "etiqueta_modelo": null, "opcional": false}}"#
            );
            assert!(
                matches!(table(&entry), Err(Error::InvalidTypeTable(_))),
                "{marker:?}"
            );
        }
    }

    #[test]
    fn unknown_format_or_version_is_rejected() {
        for json in [
            r#"{"formato": "otra-cosa", "version_formato": 1, "tipos": []}"#,
            r#"{"formato": "anonimizador-tipos", "version_formato": 2, "tipos": []}"#,
            "no es json",
        ] {
            assert!(matches!(
                TypeTable::from_json(json),
                Err(Error::InvalidTypeTable(_))
            ));
        }
    }
}
