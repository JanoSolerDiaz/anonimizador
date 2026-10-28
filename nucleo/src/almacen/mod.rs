//! Almacén de correspondencias marcador → valor original.
//!
//! Esta parte es la estructura una vez descifrada (PRD, «Estructura, una vez
//! descifrado»): contadores por tipo, entradas, documentos procesados y
//! diccionarios importados. El cifrado, la caducidad y exportar e importar
//! llegan en las tareas siguientes del hito 3.
//!
//! La clave de un dato es su valor normalizado, sin el tipo (ver
//! `DECISIONES.md`). Ningún error lleva valores del almacén.

use std::collections::{BTreeMap, HashMap};

use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::motor::marcadores::{format_marker, normalize};
use crate::{Error, Result};

/// Valor del campo `formato`.
pub const STORE_FORMAT: &str = "anonimizador-almacen";
/// Versión del formato que escribe esta aplicación.
pub const STORE_FORMAT_VERSION: u32 = 1;

/// Un dato con su marcador.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreEntry {
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
    #[serde(rename = "etiqueta_modelo")]
    model_label: String,
    #[serde(rename = "valor")]
    value: String,
    #[serde(rename = "clave_normalizada")]
    normalized_key: String,
    #[serde(rename = "variantes")]
    variants: Vec<String>,
    #[serde(rename = "documentos")]
    documents: Vec<String>,
}

impl StoreEntry {
    pub fn marker(&self) -> &str {
        &self.marker
    }

    /// Forma corta (`[P42]`); solo existe si se usó en un PDF.
    pub fn short_form(&self) -> Option<&str> {
        self.short_form.as_deref()
    }

    /// Tipo con el que se creó el marcador.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Etiqueta del modelo de la primera aparición.
    pub fn model_label(&self) -> &str {
        &self.model_label
    }

    /// Forma de la primera aparición; es la que se restaura.
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn normalized_key(&self) -> &str {
        &self.normalized_key
    }

    /// Formas literales distintas encontradas, en orden de aparición.
    pub fn variants(&self) -> &[String] {
        &self.variants
    }

    /// Identificadores de los documentos donde aparece, sin repetir.
    pub fn documents(&self) -> &[String] {
        &self.documents
    }
}

/// Archivo original de un documento procesado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    #[serde(rename = "nombre")]
    pub name: String,
    pub sha256: String,
}

/// Archivo de salida de un documento procesado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputFile {
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "formato")]
    pub format: String,
}

/// Documento procesado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: String,
    /// Fecha y hora con desfase horario: `2026-10-22T10:14:03+02:00`.
    #[serde(rename = "creado")]
    pub created: String,
    #[serde(rename = "caduca")]
    pub expires: Date,
    #[serde(rename = "version_app")]
    pub app_version: String,
    #[serde(rename = "modelo")]
    pub model: String,
    #[serde(rename = "origen")]
    pub origin: SourceFile,
    #[serde(rename = "salida")]
    pub output: OutputFile,
    #[serde(rename = "marcadores")]
    pub markers: Vec<String>,
}

/// Almacén en memoria.
#[derive(Debug, Default)]
pub struct Store {
    counters: BTreeMap<String, u32>,
    entries: Vec<StoreEntry>,
    documents: Vec<DocumentRecord>,
    /// Diccionarios importados; su estructura llega con la tarea 6.
    imported: Vec<Value>,
    by_key: HashMap<String, usize>,
    by_marker: HashMap<String, usize>,
}

/// Cabecera que se lee antes que el resto, para distinguir un archivo que no
/// es un almacén de uno de una versión más nueva.
#[derive(Deserialize)]
struct Header {
    formato: String,
    version_formato: u32,
}

#[derive(Serialize)]
struct StoreJsonOut<'a> {
    formato: &'static str,
    version_formato: u32,
    contadores: &'a BTreeMap<String, u32>,
    entradas: &'a [StoreEntry],
    documentos: &'a [DocumentRecord],
    importados: &'a [Value],
}

#[derive(Deserialize)]
struct StoreJsonIn {
    contadores: BTreeMap<String, u32>,
    entradas: Vec<StoreEntry>,
    documentos: Vec<DocumentRecord>,
    importados: Vec<Value>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lee el JSON descifrado y comprueba que es coherente.
    pub fn from_json(bytes: &[u8]) -> Result<Store> {
        let header: Header = serde_json::from_slice(bytes).map_err(json_error)?;
        if header.formato != STORE_FORMAT {
            return Err(Error::StoreInvalid("el archivo no es un almacén".into()));
        }
        if header.version_formato > STORE_FORMAT_VERSION {
            return Err(Error::StoreVersion(header.version_formato));
        }
        let data: StoreJsonIn = serde_json::from_slice(bytes).map_err(json_error)?;
        let mut store = Store {
            counters: data.contadores,
            entries: data.entradas,
            documents: data.documentos,
            imported: data.importados,
            ..Store::default()
        };
        for (index, entry) in store.entries.iter().enumerate() {
            if store
                .by_key
                .insert(entry.normalized_key.clone(), index)
                .is_some()
            {
                return Err(Error::StoreInvalid(
                    "dos entradas tienen el mismo valor".into(),
                ));
            }
            if store
                .by_marker
                .insert(entry.marker.clone(), index)
                .is_some()
            {
                return Err(Error::StoreInvalid(
                    "dos entradas tienen el mismo marcador".into(),
                ));
            }
            if let Some(number) = marker_number(&entry.marker, &entry.kind)
                && store.counter(&entry.kind) < number
            {
                return Err(Error::StoreInvalid(
                    "un contador va por detrás de sus marcadores".into(),
                ));
            }
        }
        Ok(store)
    }

    /// JSON para cifrar y guardar.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(&StoreJsonOut {
            formato: STORE_FORMAT,
            version_formato: STORE_FORMAT_VERSION,
            contadores: &self.counters,
            entradas: &self.entries,
            documentos: &self.documents,
            importados: &self.imported,
        })
        .expect("el almacén siempre se puede convertir a JSON")
    }

    /// Devuelve el marcador del valor y anota el documento en su entrada. Si
    /// el valor normalizado ya tiene marcador, lo reutiliza aunque `kind` sea
    /// otro; si no, crea el siguiente de `kind`. `kind` debe ser un marcador
    /// de la tabla de tipos y `model_label`, la etiqueta del modelo.
    pub fn assign(&mut self, kind: &str, model_label: &str, literal: &str, document: &str) -> &str {
        let key = normalize(literal);
        let index = match self.by_key.get(&key) {
            Some(&index) => {
                let entry = &mut self.entries[index];
                if !entry.variants.iter().any(|v| v == literal) {
                    entry.variants.push(literal.to_owned());
                }
                if !entry.documents.iter().any(|d| d == document) {
                    entry.documents.push(document.to_owned());
                }
                index
            }
            None => {
                let counter = self.counters.entry(kind.to_owned()).or_insert(0);
                *counter += 1;
                let marker = format_marker(kind, *counter);
                let index = self.entries.len();
                self.by_key.insert(key.clone(), index);
                self.by_marker.insert(marker.clone(), index);
                self.entries.push(StoreEntry {
                    marker,
                    short_form: None,
                    kind: kind.to_owned(),
                    model_label: model_label.to_owned(),
                    value: literal.to_owned(),
                    normalized_key: key,
                    variants: vec![literal.to_owned()],
                    documents: vec![document.to_owned()],
                });
                index
            }
        };
        &self.entries[index].marker
    }

    /// Guarda un documento procesado. Si ya había uno con el mismo
    /// identificador, lo sustituye.
    pub fn record_document(&mut self, document: DocumentRecord) {
        match self.documents.iter_mut().find(|d| d.id == document.id) {
            Some(existing) => *existing = document,
            None => self.documents.push(document),
        }
    }

    /// Último número usado para el tipo, o 0 si aún no hay ninguno.
    pub fn counter(&self, kind: &str) -> u32 {
        self.counters.get(kind).copied().unwrap_or(0)
    }

    /// Entradas en orden de creación.
    pub fn entries(&self) -> impl Iterator<Item = &StoreEntry> {
        self.entries.iter()
    }

    pub fn entry_for_marker(&self, marker: &str) -> Option<&StoreEntry> {
        self.by_marker
            .get(marker)
            .map(|&index| &self.entries[index])
    }

    /// Documentos procesados, en orden de registro.
    pub fn documents(&self) -> &[DocumentRecord] {
        &self.documents
    }
}

/// Número de un marcador `[TIPO_0042]` del tipo `kind`, o `None` si no tiene
/// esa forma.
fn marker_number(marker: &str, kind: &str) -> Option<u32> {
    marker
        .strip_prefix('[')?
        .strip_prefix(kind)?
        .strip_prefix('_')?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Error de lectura del JSON sin el texto de serde_json, que puede citar
/// valores del almacén. Solo la clase de error y la posición.
fn json_error(error: serde_json::Error) -> Error {
    use serde_json::error::Category;
    let what = match error.classify() {
        Category::Io => "no se ha podido leer",
        Category::Syntax => "el JSON está mal formado",
        Category::Data => "falta un campo o tiene un tipo inesperado",
        Category::Eof => "el archivo está incompleto",
    };
    Error::StoreInvalid(format!(
        "{what} (línea {}, columna {})",
        error.line(),
        error.column()
    ))
}
