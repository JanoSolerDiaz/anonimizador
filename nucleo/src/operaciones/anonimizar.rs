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
use crate::formatos::pdf::{PdfDocument, WriteOptions, verify};
use crate::formatos::pptx::PptxDocument;
use crate::formatos::salida::{Suffix, description, save_new};
use crate::formatos::texto::TextDocument;
use crate::formatos::xlsx::XlsxDocument;
use crate::{Error, Result, VERSION};

/// Tipo de los importes, que en Excel no se sustituyen por defecto.
pub const AMOUNT_KIND: &str = "IMPORTE";

#[derive(Debug, Clone)]
pub struct AnonymizeOptions {
    pub format: OutputFormat,
    /// Modelo que se anota en el almacén; ver [`super::model_id`].
    pub model: String,
    /// Fecha y hora del proceso.
    pub now: Zoned,
    /// En un libro de Excel los importes se dejan como están, salvo que esto
    /// sea `true` (decisión de Jano del 2026-11-02). En los demás formatos
    /// se sustituyen siempre que no se haya desmarcado el tipo.
    pub spreadsheet_amounts: bool,
    /// Resolución de las páginas de PDF que se convierten en imagen, en ppp
    /// (150 a 300; ver [`crate::formatos::pdf::DEFAULT_RESOLUTION`]).
    pub pdf_resolution: u32,
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
    Pdf(PdfDocument),
    Docx(DocxDocument),
    Pptx(PptxDocument),
    Xlsx(XlsxDocument),
    Text(TextDocument),
}

/// Anonimiza `input` y deja la salida en `dir` con el nombre
/// `descripción_anonimizado.ext`, sin sobrescribir nada. DOCX, PPTX y XLSX
/// salen en su formato, en MD o en TXT; MD y TXT, en MD o TXT.
pub fn anonymize(
    input: &Path,
    dir: &Path,
    detector: &dyn Detector,
    store: &mut Store,
    options: &AnonymizeOptions,
) -> Result<Summary> {
    let bytes = std::fs::read(input)?;
    let format = InputFormat::detect(input, &bytes)?;
    if !format.can_become(options.format) {
        return Err(Error::UnsupportedConversion {
            from: format.label(),
            to: options.format.label(),
        });
    }
    let source = match format {
        InputFormat::Pdf => Source::Pdf(PdfDocument::from_bytes(&bytes)?),
        InputFormat::Docx => Source::Docx(DocxDocument::from_bytes(&bytes)?),
        InputFormat::Pptx => Source::Pptx(PptxDocument::from_bytes(&bytes)?),
        InputFormat::Xlsx => Source::Xlsx(XlsxDocument::from_bytes(&bytes)?),
        InputFormat::Markdown | InputFormat::Text => {
            Source::Text(TextDocument::from_bytes(&bytes)?)
        }
    };
    let segments: Vec<&str> = match &source {
        Source::Pdf(document) => document.segments(),
        Source::Docx(document) => document
            .segments()
            .iter()
            .map(|s| s.text.as_str())
            .collect(),
        Source::Pptx(document) => document
            .segments()
            .iter()
            .map(|s| s.text.as_str())
            .collect(),
        Source::Xlsx(document) => document
            .segments()
            .iter()
            .map(|s| s.text.as_str())
            .collect(),
        Source::Text(document) => document.segments(),
    };
    let mut entities = match &source {
        // Los números de Excel se analizan con el título de su columna
        // delante; las entidades vuelven a las posiciones del segmento.
        Source::Xlsx(document) => {
            let with_context = document.detection_texts();
            let texts: Vec<&str> = with_context.iter().map(|(t, _)| t.as_str()).collect();
            let mut entities = detector.detect(&texts)?;
            entities.retain_mut(|e| {
                let prefix = with_context[e.segment].1;
                if e.end <= prefix {
                    return false;
                }
                e.start = e.start.max(prefix) - prefix;
                e.end -= prefix;
                e.text = segments[e.segment]
                    .chars()
                    .skip(e.start)
                    .take(e.end - e.start)
                    .collect();
                true
            });
            entities
        }
        _ => detector.detect(&segments)?,
    };
    if matches!(source, Source::Xlsx(_)) && !options.spreadsheet_amounts {
        entities.retain(|e| e.kind != AMOUNT_KIND);
    }
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

    // En un PDF, cada otra aparición de un valor detectado se tapa también,
    // en cualquier página: la verificación final la buscaría.
    if let Source::Pdf(document) = &source {
        replacements = document.with_literals(&replacements, &literals);
    }
    let mut rasterized_pages = 0;
    let data = match (&source, options.format) {
        (Source::Pdf(document), OutputFormat::Pdf) => document
            .write(
                &replacements,
                &WriteOptions {
                    resolution: options.pdf_resolution,
                },
                &mut |marker: &str| store.short_form(marker),
            )
            .map(|written| {
                rasterized_pages = written.rasterized_pages;
                written.bytes
            }),
        _ => output_bytes(&source, options.format, &replacements, &literals),
    };
    let written = data.and_then(|data| {
        save_new(
            dir,
            &description(input),
            Suffix::Anonymized,
            options.format.extension(),
            |out: &mut dyn Write| Ok(out.write_all(&data)?),
        )
    });
    // Verificación obligatoria del PDF, sobre el archivo ya escrito. Si
    // falla, la salida se borra.
    let written = written.and_then(|path| {
        if options.format != OutputFormat::Pdf {
            return Ok(path);
        }
        let values: Vec<&str> = literals.iter().map(|(value, _)| value.as_str()).collect();
        match std::fs::read(&path)
            .map_err(Error::from)
            .and_then(|bytes| verify(&bytes, &values))
        {
            Ok(()) => Ok(path),
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                tracing::warn!(documento = %id, "la verificación del PDF ha fallado");
                Err(error)
            }
        }
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
        Source::Pdf(document) => Warnings {
            attachments: document.attachments(),
            ..Warnings::default()
        },
        Source::Docx(document) => document.warnings(),
        Source::Pptx(document) => document.warnings(),
        Source::Xlsx(document) => document.warnings(),
        Source::Text(_) => Warnings::default(),
    };
    tracing::info!(
        documento = %id,
        formato = options.format.extension(),
        entidades = entities.len(),
        paginas_rasterizadas = rasterized_pages,
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
    let text = |result: Result<String>| result.map(String::into_bytes);
    match (source, format) {
        // El PDF en PDF lo escribe `anonymize`, que tiene el almacén para
        // las formas cortas.
        (Source::Pdf(document), OutputFormat::Markdown) => text(document.markdown(replacements)),
        (Source::Pdf(document), OutputFormat::Text) => text(document.plain_text(replacements)),
        (Source::Pdf(document), _) => document.docx(replacements),
        (Source::Docx(document), OutputFormat::Markdown) => text(document.markdown(replacements)),
        (Source::Docx(document), OutputFormat::Text) => text(document.plain_text(replacements)),
        (Source::Docx(document), _) => document.write(replacements, literals),
        (Source::Pptx(document), OutputFormat::Markdown) => text(document.markdown(replacements)),
        (Source::Pptx(document), OutputFormat::Text) => text(document.plain_text(replacements)),
        (Source::Pptx(document), _) => document.write(replacements, literals),
        (Source::Xlsx(document), OutputFormat::Markdown) => text(document.markdown(replacements)),
        (Source::Xlsx(document), OutputFormat::Text) => text(document.plain_text(replacements)),
        (Source::Xlsx(document), _) => document.write(replacements, literals),
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
