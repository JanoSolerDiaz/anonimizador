//! Diccionario de un documento (PRD, «Exportar e importar»).
//!
//! Exportar saca del almacén solo lo necesario para desanonimizar un
//! documento: marcador, tipo y valor. Sin variantes, sin claves normalizadas
//! y sin el nombre original del archivo, que puede llevar datos personales.
//! En v1 va sin cifrar; el aviso lo muestra la interfaz.
//!
//! Importar guarda el diccionario en `importados`, aparte de las entradas
//! propias: los números de marcador de otra persona no coinciden con los
//! propios, así que ni se mezclan ni cuentan para los contadores.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{OutputFile, Store, json_problem};
use crate::{Error, Result};

/// Valor del campo `formato` de un diccionario.
pub const DICTIONARY_FORMAT: &str = "anonimizador-diccionario";
/// Versión del formato de diccionario que escribe esta aplicación.
pub const DICTIONARY_FORMAT_VERSION: u32 = 1;

/// Un marcador con su valor original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    #[serde(rename = "marcador")]
    marker: String,
    #[serde(
        rename = "forma_corta",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    short_form: Option<String>,
    #[serde(rename = "tipo")]
    kind: String,
    #[serde(rename = "valor")]
    value: String,
}

impl DictionaryEntry {
    pub fn marker(&self) -> &str {
        &self.marker
    }

    pub fn short_form(&self) -> Option<&str> {
        self.short_form.as_deref()
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// El documento del que sale un diccionario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryDocument {
    pub id: String,
    #[serde(rename = "creado")]
    pub created: String,
    #[serde(rename = "salida")]
    pub output: OutputFile,
}

/// Diccionario recibido de otra persona, tal como se guarda en el almacén.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedDictionary {
    #[serde(rename = "nombre")]
    name: String,
    #[serde(rename = "documento")]
    document: DictionaryDocument,
    #[serde(rename = "entradas")]
    entries: Vec<DictionaryEntry>,
}

impl ImportedDictionary {
    /// Nombre con el que se importó, normalmente el del archivo.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn document(&self) -> &DictionaryDocument {
        &self.document
    }

    pub fn entries(&self) -> &[DictionaryEntry] {
        &self.entries
    }

    pub fn entry_for_marker(&self, marker: &str) -> Option<&DictionaryEntry> {
        self.entries.iter().find(|e| e.marker == marker)
    }
}

#[derive(Deserialize)]
struct Header {
    formato: String,
    version_formato: u32,
}

#[derive(Serialize)]
struct DictionaryOut<'a> {
    formato: &'static str,
    version_formato: u32,
    documento: DictionaryDocument,
    entradas: &'a [DictionaryEntry],
}

#[derive(Deserialize)]
struct DictionaryIn {
    documento: DictionaryDocument,
    entradas: Vec<DictionaryEntry>,
}

impl Store {
    /// JSON con las entradas del documento `id`, en orden de creación.
    pub fn export_dictionary(&self, id: &str) -> Result<Vec<u8>> {
        let record = self
            .documents
            .iter()
            .find(|d| d.id == id)
            .ok_or(Error::DocumentNotFound)?;
        let entries: Vec<DictionaryEntry> = self
            .entries
            .iter()
            .filter(|e| e.documents.iter().any(|d| d == id))
            .map(|e| DictionaryEntry {
                marker: e.marker.clone(),
                short_form: e.short_form.clone(),
                kind: e.kind.clone(),
                value: e.value.clone(),
            })
            .collect();
        let out = DictionaryOut {
            formato: DICTIONARY_FORMAT,
            version_formato: DICTIONARY_FORMAT_VERSION,
            documento: DictionaryDocument {
                id: record.id.clone(),
                created: record.created.clone(),
                output: record.output.clone(),
            },
            entradas: &entries,
        };
        Ok(serde_json::to_vec_pretty(&out)
            .expect("el diccionario siempre se puede convertir a JSON"))
    }

    /// Guarda un diccionario recibido con el nombre `name`. Si ya había uno
    /// del mismo documento, lo sustituye. Si no es válido, no cambia nada.
    pub fn import_dictionary(&mut self, bytes: &[u8], name: &str) -> Result<()> {
        let invalid = |e| Error::DictionaryInvalid(json_problem(&e));
        let header: Header = serde_json::from_slice(bytes).map_err(invalid)?;
        if header.formato != DICTIONARY_FORMAT {
            return Err(Error::DictionaryInvalid(
                "el archivo no es un diccionario".into(),
            ));
        }
        if header.version_formato > DICTIONARY_FORMAT_VERSION {
            return Err(Error::DictionaryVersion(header.version_formato));
        }
        let data: DictionaryIn = serde_json::from_slice(bytes).map_err(invalid)?;
        let mut seen = HashSet::new();
        if !data.entradas.iter().all(|e| seen.insert(e.marker.as_str())) {
            return Err(Error::DictionaryInvalid(
                "dos entradas tienen el mismo marcador".into(),
            ));
        }
        let imported = ImportedDictionary {
            name: name.to_owned(),
            document: data.documento,
            entries: data.entradas,
        };
        match self
            .imported
            .iter_mut()
            .find(|d| d.document.id == imported.document.id)
        {
            Some(existing) => *existing = imported,
            None => self.imported.push(imported),
        }
        Ok(())
    }

    /// Diccionarios importados, en orden de importación.
    pub fn imported(&self) -> &[ImportedDictionary] {
        &self.imported
    }
}
