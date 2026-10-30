//! Anonimizar un archivo de principio a fin: extraer, detectar, asignar
//! marcadores en el almacén, sustituir, escribir la salida sin dejarla a
//! medias y registrar el documento.
//!
//! El almacén solo se cambia en memoria; guardarlo es cosa de quien llama.
//! Si algo falla después de asignar los marcadores, el documento se olvida:
//! no quedan valores en el almacén sin un documento que los haga caducar.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use jiff::Zoned;
use sha2::{Digest, Sha256};

use super::{Detector, InputFormat, OutputFormat};
use crate::almacen::{DocumentRecord, OutputFile, SourceFile, Store};
use crate::formatos::Replacement;
use crate::formatos::docx::{DocxDocument, Warnings};
use crate::formatos::salida::{Suffix, description, save_new};
use crate::formatos::texto::TextDocument;
use crate::{Error, Result, VERSION};

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

/// Documento de entrada ya leído.
enum Source {
    Docx(DocxDocument),
    Text(TextDocument),
}

/// Anonimiza `input` y deja la salida en `dir` con el nombre
/// `descripción_anonimizado.ext`, sin sobrescribir nada. DOCX sale en DOCX,
/// MD o TXT; MD y TXT, en MD o TXT.
pub fn anonymize(
    input: &Path,
    dir: &Path,
    detector: &dyn Detector,
    store: &mut Store,
    options: &AnonymizeOptions,
) -> Result<Summary> {
    let bytes = std::fs::read(input)?;
    let format = InputFormat::detect(input, &bytes)?;
    if format != InputFormat::Docx && options.format == OutputFormat::Docx {
        return Err(Error::UnsupportedConversion {
            from: format.label(),
            to: options.format.label(),
        });
    }
    let source = match format {
        InputFormat::Docx => Source::Docx(DocxDocument::from_bytes(&bytes)?),
        InputFormat::Markdown | InputFormat::Text => {
            Source::Text(TextDocument::from_bytes(&bytes)?)
        }
    };
    let segments: Vec<&str> = match &source {
        Source::Docx(document) => document
            .segments()
            .iter()
            .map(|s| s.text.as_str())
            .collect(),
        Source::Text(document) => document.segments(),
    };
    let mut entities = detector.detect(&segments)?;
    // Los marcadores se numeran en orden de aparición.
    entities.sort_by_key(|e| (e.segment, e.start));

    let id = uuid::Uuid::new_v4().to_string();
    let mut replacements = Vec::with_capacity(entities.len());
    let mut markers: Vec<String> = Vec::new();
    let mut literals: Vec<(String, String)> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entity in &entities {
        let marker = store
            .assign(&entity.kind, &entity.label, &entity.text, &id)
            .to_owned();
        *counts.entry(entity.kind.clone()).or_default() += 1;
        if !markers.contains(&marker) {
            markers.push(marker.clone());
        }
        if !literals.iter().any(|(value, _)| *value == entity.text) {
            literals.push((entity.text.clone(), marker.clone()));
        }
        replacements.push(Replacement {
            segment: entity.segment,
            start: entity.start,
            end: entity.end,
            text: marker,
        });
    }

    let written =
        output_bytes(&source, options.format, &replacements, &literals).and_then(|data| {
            save_new(
                dir,
                &description(input),
                Suffix::Anonymized,
                options.format.extension(),
                |out: &mut dyn Write| Ok(out.write_all(&data)?),
            )
        });
    let output = match written {
        Ok(path) => path,
        Err(error) => {
            store.forget(&id);
            return Err(error);
        }
    };

    store.record_document(DocumentRecord {
        id: id.clone(),
        created: options.now.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        expires: store.expiry_from(options.now.date()),
        app_version: VERSION.to_owned(),
        model: options.model.clone(),
        origin: SourceFile {
            name: file_name(input),
            sha256: sha256_hex(&bytes),
        },
        output: OutputFile {
            name: file_name(&output),
            format: options.format.extension().to_owned(),
        },
        markers,
    });
    let warnings = match &source {
        Source::Docx(document) => document.warnings(),
        Source::Text(_) => Warnings::default(),
    };
    tracing::info!(
        documento = %id,
        formato = options.format.extension(),
        entidades = entities.len(),
        "documento anonimizado"
    );
    Ok(Summary {
        document_id: id,
        output,
        entities: counts,
        warnings,
    })
}

fn output_bytes(
    source: &Source,
    format: OutputFormat,
    replacements: &[Replacement],
    literals: &[(String, String)],
) -> Result<Vec<u8>> {
    match (source, format) {
        (Source::Docx(document), OutputFormat::Docx) => document.write(replacements, literals),
        (Source::Docx(document), OutputFormat::Markdown) => {
            document.markdown(replacements).map(String::into_bytes)
        }
        (Source::Docx(document), OutputFormat::Text) => {
            document.plain_text(replacements).map(String::into_bytes)
        }
        // MD y TXT son el mismo texto: solo cambia la extensión.
        (Source::Text(document), _) => document.write(replacements),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
