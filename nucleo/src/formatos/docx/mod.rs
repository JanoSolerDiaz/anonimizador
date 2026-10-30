//! DOCX: edición del XML original con mapa de fragmentos (PRD, «Formato
//! DOCX»).
//!
//! El extractor lee las partes con texto (cuerpo, encabezados, pies, notas y
//! comentarios) y convierte cada párrafo en un segmento. Un fragmento es el
//! contenido de un `w:t` o un `w:delText`; se numeran por parte, en orden de
//! aparición, y el mapa dice de qué fragmento y de qué posición dentro de él
//! sale cada carácter del segmento. El escritor recorre la parte con el
//! mismo criterio y sustituye el texto de los fragmentos.
//!
//! Los elementos se reconocen por su nombre con prefijo (`w:t`), que es el
//! que escribe Word: `a:t`, por ejemplo, es texto de DrawingML y no entra.

mod escritor;
mod estilos;
mod extractor;
mod texto;

use std::io::{Cursor, Read};

use crate::{Error, Result};

/// Firma de un archivo OLE, que es lo que es un DOCX cifrado con contraseña.
const OLE_SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Tamaño máximo de una parte descomprimida. Protege de un ZIP que se
/// descomprime en gigas a partir de unos pocos bytes.
const MAX_PART_BYTES: u64 = 256 * 1024 * 1024;

const DOCUMENT_PART: &str = "word/document.xml";
const CORE_PART: &str = "docProps/core.xml";
const APP_PART: &str = "docProps/app.xml";

/// Posición de un carácter en el original: fragmento de la parte y carácter
/// dentro del fragmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentPos {
    pub fragment: usize,
    pub offset: usize,
}

/// Lo que hace falta del párrafo para escribir MD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphStyle {
    Normal,
    /// Título de nivel 1 a 9.
    Heading(u8),
    /// Elemento de lista, numerada o con viñetas; `level` empieza en 0.
    ListItem {
        ordered: bool,
        level: u8,
    },
    /// Título, asunto, palabras clave o descripción del documento.
    Metadata,
}

/// Celda de tabla en la que está un párrafo. Las tablas se numeran por
/// parte; la fila y la columna empiezan en 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCell {
    pub table: usize,
    pub row: usize,
    pub column: usize,
}

/// Párrafo con texto. `map` tiene una posición por carácter de `text`;
/// `None` es un tabulador o un salto, que en el texto es un espacio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    /// Parte del ZIP de la que sale, como `word/document.xml`.
    pub part: String,
    pub style: ParagraphStyle,
    pub cell: Option<TableCell>,
    pub map: Vec<Option<FragmentPos>>,
    /// Tabuladores y saltos: posición en `text` y número del elemento en la
    /// parte. Una sustitución que los cubre los borra.
    pub breaks: Vec<(usize, usize)>,
}

/// Destino de un enlace externo (`mailto:`, `https:`…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalLink {
    pub rels_part: String,
    pub id: String,
    pub target: String,
}

/// Contenido de un `w:instrText`, como ` HYPERLINK "mailto:…" `. `index`
/// cuenta los códigos de campo de la parte en orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldCode {
    pub part: String,
    pub index: usize,
    pub text: String,
}

/// Contenido que no se revisa (PRD, «Avisos»).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Warnings {
    pub images: usize,
    pub objects: usize,
}

/// Fragmentos de una parte, en orden.
#[derive(Debug, Clone)]
struct PartText {
    name: String,
    fragments: Vec<String>,
    /// Si cada fragmento es texto borrado con control de cambios.
    deleted: Vec<bool>,
}

/// DOCX leído: el archivo original y lo extraído de él.
#[derive(Debug, Clone)]
pub struct DocxDocument {
    original: Vec<u8>,
    segments: Vec<Segment>,
    parts: Vec<PartText>,
    links: Vec<ExternalLink>,
    field_codes: Vec<FieldCode>,
    author_metadata: Vec<String>,
    revision_authors: Vec<String>,
    warnings: Warnings,
}

impl DocxDocument {
    pub fn from_bytes(bytes: &[u8]) -> Result<DocxDocument> {
        if bytes.starts_with(&OLE_SIGNATURE) {
            return Err(Error::DocxProtected);
        }
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| Error::DocxInvalid("no es un archivo DOCX".into()))?;
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.map(|n| n.into_owned()))
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| Error::DocxInvalid("no es un archivo DOCX".into()))?;
        if !names.iter().any(|n| n == DOCUMENT_PART) {
            return Err(Error::DocxInvalid(
                "no tiene el cuerpo del documento".into(),
            ));
        }
        let mut read = |name: &str| read_part(&mut archive, name);

        let styles = estilos::Styles::read(
            read("word/styles.xml")?.as_deref(),
            read("word/numbering.xml")?.as_deref(),
        )?;
        let mut out = extractor::Collected::default();
        for name in text_parts(&names) {
            if let Some(xml) = read(&name)? {
                extractor::extract_part(&name, &xml, &styles, &mut out)?;
            }
        }
        if let Some(xml) = read(CORE_PART)? {
            extractor::extract_core(CORE_PART, &xml, &mut out)?;
        }
        if let Some(xml) = read(APP_PART)? {
            extractor::extract_app(APP_PART, &xml, &mut out)?;
        }
        for name in names.iter().filter(|n| is_word_rels(n)) {
            if let Some(xml) = read(name)? {
                extractor::extract_links(name, &xml, &mut out)?;
            }
        }

        Ok(DocxDocument {
            original: bytes.to_vec(),
            segments: out.segments,
            parts: out
                .parts
                .into_iter()
                .map(|(name, fragments, deleted)| PartText {
                    name,
                    fragments,
                    deleted,
                })
                .collect(),
            links: out.links,
            field_codes: out.field_codes,
            author_metadata: out.author_metadata,
            revision_authors: out.revision_authors,
            warnings: out.warnings,
        })
    }

    /// Segmentos en orden: cuerpo, encabezados, pies, notas, comentarios y
    /// metadatos.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Texto del fragmento `index` de la parte `part`.
    pub fn fragment(&self, part: &str, index: usize) -> &str {
        let part = self
            .parts
            .iter()
            .find(|p| p.name == part)
            .expect("la parte existe");
        &part.fragments[index]
    }

    pub fn external_links(&self) -> &[ExternalLink] {
        &self.links
    }

    pub fn field_codes(&self) -> &[FieldCode] {
        &self.field_codes
    }

    /// Autor, último en modificar y empresa, sin repetir. Se vacían al escribir.
    pub fn author_metadata(&self) -> &[String] {
        &self.author_metadata
    }

    /// Autores e iniciales de comentarios y del control de cambios, sin repetir.
    pub fn revision_authors(&self) -> &[String] {
        &self.revision_authors
    }

    pub fn warnings(&self) -> Warnings {
        self.warnings
    }
}

/// Partes con texto, en el orden en que se presentan los segmentos.
fn text_parts(names: &[String]) -> Vec<String> {
    let numbered = |prefix: &str| {
        let mut found: Vec<String> = names
            .iter()
            .filter(|n| {
                n.strip_prefix(prefix)
                    .and_then(|rest| rest.strip_suffix(".xml"))
                    .is_some_and(|middle| middle.chars().all(|c| c.is_ascii_digit()))
            })
            .cloned()
            .collect();
        // header2 antes que header10.
        found.sort_by_key(|n| (n.len(), n.clone()));
        found
    };
    let mut parts = vec![DOCUMENT_PART.to_owned()];
    parts.extend(numbered("word/header"));
    parts.extend(numbered("word/footer"));
    for name in [
        "word/footnotes.xml",
        "word/endnotes.xml",
        "word/comments.xml",
    ] {
        if names.iter().any(|n| n == name) {
            parts.push(name.to_owned());
        }
    }
    parts
}

fn is_word_rels(name: &str) -> bool {
    name.strip_prefix("word/_rels/")
        .is_some_and(|rest| rest.ends_with(".rels") && !rest.contains('/'))
}

/// Contenido de una parte como texto, o `None` si no existe.
fn read_part(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(Error::DocxInvalid(format!("no se puede leer {name}"))),
    };
    let mut data = Vec::new();
    file.take(MAX_PART_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| Error::DocxInvalid(format!("no se puede leer {name}")))?;
    if data.len() as u64 > MAX_PART_BYTES {
        return Err(Error::DocxInvalid(format!("{name} es demasiado grande")));
    }
    String::from_utf8(data)
        .map(Some)
        .map_err(|_| Error::DocxInvalid(format!("{name} no está en UTF-8")))
}
