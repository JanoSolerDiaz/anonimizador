//! Almacén de correspondencias marcador → valor original.
//!
//! Esta parte es la estructura una vez descifrada (PRD, «Estructura, una vez
//! descifrado»): contadores por tipo, entradas, documentos procesados y
//! diccionarios importados. El cifrado y el archivo están en [`archivo`];
//! exportar e importar, en [`diccionario`].
//!
//! Al borrar entradas (caducidad, olvidar, vaciar) los contadores no bajan:
//! un documento anonimizado antes puede seguir llevando un marcador borrado,
//! y si se diera a otro valor, al desanonimizarlo saldría el dato equivocado.
//!
//! La clave de un dato es su valor normalizado, sin el tipo (ver
//! `DECISIONES.md`). Ningún error lleva valores del almacén.

pub mod archivo;
pub mod diccionario;

use std::collections::{BTreeMap, HashMap};

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::motor::marcadores::{format_marker, normalize};
use crate::{Error, Result};
use diccionario::ImportedDictionary;

/// Valor del campo `formato`.
pub const STORE_FORMAT: &str = "anonimizador-almacen";
/// Versión del formato que escribe esta aplicación.
pub const STORE_FORMAT_VERSION: u32 = 1;
/// Días hasta que caduca un documento si nadie fija otro plazo.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

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
#[derive(Debug)]
pub struct Store {
    counters: BTreeMap<String, u32>,
    /// Último número de forma corta usado con cada inicial (`N` → 2 tras
    /// `[N2]`). Como `counters`, no baja nunca.
    short_counters: BTreeMap<String, u32>,
    entries: Vec<StoreEntry>,
    documents: Vec<DocumentRecord>,
    imported: Vec<ImportedDictionary>,
    by_key: HashMap<String, usize>,
    by_marker: HashMap<String, usize>,
    /// Plazo de caducidad. No se guarda en el archivo: lo fija quien abre el
    /// almacén (en el hito 6, la política de sistemas).
    retention_days: u32,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            counters: BTreeMap::new(),
            short_counters: BTreeMap::new(),
            entries: Vec::new(),
            documents: Vec::new(),
            imported: Vec::new(),
            by_key: HashMap::new(),
            by_marker: HashMap::new(),
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contadores_cortos: &'a BTreeMap<String, u32>,
    entradas: &'a [StoreEntry],
    documentos: &'a [DocumentRecord],
    importados: &'a [ImportedDictionary],
}

#[derive(Deserialize)]
struct StoreJsonIn {
    contadores: BTreeMap<String, u32>,
    /// Falta en los almacenes que nunca usaron una forma corta.
    #[serde(default)]
    contadores_cortos: BTreeMap<String, u32>,
    entradas: Vec<StoreEntry>,
    documentos: Vec<DocumentRecord>,
    importados: Vec<ImportedDictionary>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// El mismo almacén con otro plazo de caducidad.
    pub fn with_retention_days(mut self, days: u32) -> Self {
        self.retention_days = days;
        self
    }

    pub fn retention_days(&self) -> u32 {
        self.retention_days
    }

    /// Fecha de caducidad de un documento procesado `today`.
    pub fn expiry_from(&self, today: Date) -> Date {
        today.saturating_add(Span::new().days(i64::from(self.retention_days)))
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
            short_counters: data.contadores_cortos,
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
        // Un almacén puede traer formas cortas sin su contador (el ejemplo
        // del PRD, `[P42]`): el contador se pone a su altura para no repetir.
        for entry in &store.entries {
            if let Some((letter, number)) = entry.short_form.as_deref().and_then(short_parts) {
                let counter = store.short_counters.entry(letter).or_insert(0);
                *counter = (*counter).max(number);
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
            contadores_cortos: &self.short_counters,
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

    /// Forma corta del marcador (`[N2]`), para cuando el completo no cabe en
    /// un PDF. Si aún no tiene, se le da la inicial de su tipo con el
    /// siguiente número de esa inicial y queda guardada en la entrada.
    /// `None` si el marcador no está en el almacén.
    pub fn short_form(&mut self, marker: &str) -> Option<String> {
        let &index = self.by_marker.get(marker)?;
        let entry = &mut self.entries[index];
        if let Some(short) = &entry.short_form {
            return Some(short.clone());
        }
        let letter: String = entry.kind.chars().take(1).collect();
        let counter = self.short_counters.entry(letter.clone()).or_insert(0);
        *counter += 1;
        let short = format!("[{letter}{counter}]");
        entry.short_form = Some(short.clone());
        Some(short)
    }

    /// Guarda un documento procesado. Si ya había uno con el mismo
    /// identificador, lo sustituye.
    pub fn record_document(&mut self, document: DocumentRecord) {
        match self.documents.iter_mut().find(|d| d.id == document.id) {
            Some(existing) => *existing = document,
            None => self.documents.push(document),
        }
    }

    /// Borra los documentos que caducan `today` o antes, y las entradas que
    /// ya no usa ningún documento.
    pub fn purge(&mut self, today: Date) {
        let expired: Vec<String> = self
            .documents
            .iter()
            .filter(|d| d.expires <= today)
            .map(|d| d.id.clone())
            .collect();
        self.remove_documents(&expired);
    }

    /// Borra un documento y sus entradas que no usen otros documentos.
    pub fn forget(&mut self, document: &str) {
        self.remove_documents(&[document.to_owned()]);
    }

    /// Lo vacía todo salvo los contadores.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.documents.clear();
        self.imported.clear();
        self.by_key.clear();
        self.by_marker.clear();
    }

    fn remove_documents(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        self.documents.retain(|d| !ids.contains(&d.id));
        for entry in &mut self.entries {
            entry.documents.retain(|d| !ids.contains(d));
        }
        self.entries.retain(|e| !e.documents.is_empty());
        self.reindex();
    }

    /// Rehace los índices después de borrar entradas.
    fn reindex(&mut self) {
        self.by_key.clear();
        self.by_marker.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.by_key.insert(entry.normalized_key.clone(), index);
            self.by_marker.insert(entry.marker.clone(), index);
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

/// Error de lectura del JSON del almacén; ver [`json_problem`].
fn json_error(error: serde_json::Error) -> Error {
    Error::StoreInvalid(json_problem(&error))
}

/// Qué falla en un JSON, sin el texto de serde_json, que puede citar valores
/// del almacén. Solo la clase de error y la posición.
fn json_problem(error: &serde_json::Error) -> String {
    use serde_json::error::Category;
    let what = match error.classify() {
        Category::Io => "no se ha podido leer",
        Category::Syntax => "el JSON está mal formado",
        Category::Data => "falta un campo o tiene un tipo inesperado",
        Category::Eof => "el archivo está incompleto",
    };
    format!(
        "{what} (línea {}, columna {})",
        error.line(),
        error.column()
    )
}

/// Inicial y número de una forma corta: `[N2]` → (`N`, 2).
fn short_parts(short: &str) -> Option<(String, u32)> {
    let inner = short.strip_prefix('[')?.strip_suffix(']')?;
    let digits = inner.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let letters = &inner[..inner.len() - digits.len()];
    if letters.is_empty() {
        return None;
    }
    Some((letters.to_owned(), digits.parse().ok()?))
}
