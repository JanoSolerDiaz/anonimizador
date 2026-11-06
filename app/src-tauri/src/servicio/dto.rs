//! Lo que viaja entre la interfaz y el servicio, en JSON. Ningún tipo lleva
//! valores del almacén salvo el texto desanonimizado que pide el usuario.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use nucleo::operaciones::desanonimizar::MarkerSource;
use nucleo::operaciones::{InputFormat, Inspection, OutputFormat};

/// Estado del modelo, que se carga en segundo plano al abrir la app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ModelStatus {
    Loading,
    Ready,
    Failed { message: String },
}

/// Estado del almacén.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum StoreStatus {
    Ready,
    /// No se pudo abrir: se puede empezar uno nuevo.
    Failed {
        message: String,
    },
}

/// Todo lo que la interfaz necesita al arrancar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub model: ModelStatus,
    pub store: StoreStatus,
    pub settings: Settings,
    /// Tipos que se pueden dejar sin sustituir, en el orden de la tabla.
    pub optional_types: Vec<String>,
    /// Si hay un lote en marcha.
    pub busy: bool,
}

/// Lo que se recuerda entre sesiones. No lleva datos de los documentos.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Última carpeta de salida.
    pub last_dir: Option<String>,
    /// Tipos opcionales que el usuario dejó sin sustituir.
    pub skipped: Vec<String>,
}

/// Un archivo de la lista, antes de procesarlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    /// «DOCX», «PDF digital», «PDF escaneado», «No admitido»…
    pub kind: String,
    /// Formato de entrada (`pdf`, `docx`…), si se admite.
    pub format: Option<String>,
    /// Formatos de salida al anonimizar, el propio primero. Vacío si no se
    /// puede anonimizar.
    pub outputs: Vec<String>,
    /// Si se puede desanonimizar.
    pub deanonymizable: bool,
    /// Por qué no se admite, en una frase.
    pub reason: Option<String>,
}

impl FileInfo {
    pub fn new(path: String, name: String, inspection: Inspection) -> FileInfo {
        let base = FileInfo {
            path,
            name,
            kind: String::new(),
            format: None,
            outputs: Vec::new(),
            deanonymizable: false,
            reason: None,
        };
        match inspection {
            Inspection::Supported(format) => FileInfo {
                kind: kind_label(format).to_owned(),
                format: Some(format.extension().to_owned()),
                outputs: format
                    .outputs()
                    .into_iter()
                    .map(|o| o.extension().to_owned())
                    .collect(),
                deanonymizable: format.deanonymizable(),
                ..base
            },
            Inspection::ScannedPdf => FileInfo {
                kind: "PDF escaneado".to_owned(),
                format: Some("pdf".to_owned()),
                reason: Some(nucleo::Error::ScannedPdf.to_string()),
                ..base
            },
            Inspection::Rejected(error) => FileInfo {
                kind: "No admitido".to_owned(),
                reason: Some(error.to_string()),
                ..base
            },
        }
    }
}

fn kind_label(format: InputFormat) -> &'static str {
    match format {
        InputFormat::Pdf => "PDF digital",
        InputFormat::Docx => "DOCX",
        InputFormat::Pptx => "PPTX",
        InputFormat::Xlsx => "XLSX",
        InputFormat::Markdown => "MD",
        InputFormat::Text => "TXT",
    }
}

/// Formato de salida por su extensión.
pub fn output_format(extension: &str) -> Option<OutputFormat> {
    [
        OutputFormat::Pdf,
        OutputFormat::Docx,
        OutputFormat::Pptx,
        OutputFormat::Xlsx,
        OutputFormat::Markdown,
        OutputFormat::Text,
    ]
    .into_iter()
    .find(|f| f.extension() == extension)
}

/// Un archivo de un lote. `id` lo pone la interfaz para reconocer sus
/// eventos.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFile {
    pub id: String,
    pub path: String,
    /// Formato de salida al anonimizar (`pdf`, `docx`, `md`…).
    #[serde(default)]
    pub format: Option<String>,
    /// Fuente elegida para un conflicto al desanonimizar.
    #[serde(default)]
    pub source: Option<Source>,
}

/// Lote de anonimizar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnonymizeJob {
    pub files: Vec<JobFile>,
    pub dir: String,
    #[serde(default)]
    pub skipped: Vec<String>,
    /// En Excel, sustituir también los importes.
    #[serde(default)]
    pub spreadsheet_amounts: bool,
    /// Resolución de las páginas de PDF con datos; 200 si no se dice.
    #[serde(default)]
    pub resolution: Option<u32>,
}

/// Lote de desanonimizar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeanonymizeJob {
    pub files: Vec<JobFile>,
    pub dir: String,
}

/// Fuente de un marcador: las entradas propias o un diccionario importado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    Store,
    Dictionary { name: String },
}

impl From<&MarkerSource> for Source {
    fn from(source: &MarkerSource) -> Source {
        match source {
            MarkerSource::Store => Source::Store,
            MarkerSource::Dictionary(name) => Source::Dictionary { name: name.clone() },
        }
    }
}

impl From<&Source> for MarkerSource {
    fn from(source: &Source) -> MarkerSource {
        match source {
            Source::Store => MarkerSource::Store,
            Source::Dictionary { name } => MarkerSource::Dictionary(name.clone()),
        }
    }
}

/// Un marcador con valores distintos en varias fuentes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub marker: String,
    pub sources: Vec<Source>,
}

/// Avisos de un archivo anonimizado.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningsView {
    pub images: usize,
    pub objects: usize,
    pub attachments: usize,
    pub links: usize,
}

/// En qué va un archivo de un lote.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Phase {
    Queued,
    Reading,
    Analyzing {
        /// Parte hecha, de 0 a 1.
        fraction: f32,
    },
    Writing,
    Verifying,
    Anonymized {
        output: String,
        /// Apariciones de cada tipo.
        entities: BTreeMap<String, usize>,
        warnings: WarningsView,
    },
    Deanonymized {
        output: String,
        restored: Vec<String>,
        missing: Vec<String>,
    },
    /// Hace falta elegir una fuente para este archivo.
    Conflict {
        conflicts: Vec<ConflictView>,
    },
    Failed {
        message: String,
    },
}

/// Evento que el servicio manda a la interfaz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Model {
        status: ModelStatus,
    },
    Progress {
        file: String,
        phase: Phase,
    },
    /// El lote ha terminado y el almacén está guardado (o no, y se dice).
    BatchDone {
        done: usize,
        failed: usize,
        store_error: Option<String>,
    },
    /// El almacén ha cambiado fuera de un lote (purga diaria).
    StoreChanged,
}

/// Resultado de desanonimizar texto pegado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum TextResult {
    Done {
        text: String,
        restored: Vec<String>,
        missing: Vec<String>,
    },
    Conflict {
        conflicts: Vec<ConflictView>,
    },
}

/// Un documento del almacén, sin sus valores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    pub id: String,
    /// Nombre del archivo original.
    pub name: String,
    pub output: String,
    pub created: String,
    pub expires: String,
    /// Marcadores del documento.
    pub entries: usize,
}

/// Un diccionario importado, sin sus valores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryView {
    pub name: String,
    pub document: String,
    pub entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreView {
    /// Días hasta que caduca un documento nuevo.
    pub retention_days: u32,
    /// Si ese plazo lo fija la organización (y el usuario no lo cambia).
    pub retention_by_policy: bool,
    pub documents: Vec<DocumentView>,
    pub dictionaries: Vec<DictionaryView>,
}

/// Diccionario exportado, con el aviso del PRD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub path: String,
    pub warning: String,
}
