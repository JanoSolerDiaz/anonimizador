//! Lo que comparten DOCX, PPTX y XLSX: los tres son un ZIP con partes XML
//! («Office Open XML»). Aquí están la lectura segura del paquete, las ayudas
//! para leer XML, la reescritura de una parte por reglas y los metadatos de
//! `docProps`.

pub(crate) mod reescritura;

use std::io::{Cursor, Read};

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::{Error, Result};

/// Firma de un archivo OLE: lo es un documento de Office cifrado con
/// contraseña y lo es también un archivo de Office 97-2003.
pub(crate) const OLE_SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Tamaño máximo de una parte descomprimida. Protege de un ZIP que se
/// descomprime en gigas a partir de unos pocos bytes.
const MAX_PART_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) const CORE_PART: &str = "docProps/core.xml";
pub(crate) const APP_PART: &str = "docProps/app.xml";
pub(crate) const CUSTOM_PART: &str = "docProps/custom.xml";

/// Elementos de `docProps/core.xml` que se analizan como texto.
pub(crate) const CORE_TEXT_FIELDS: [&str; 4] =
    ["dc:title", "dc:subject", "cp:keywords", "dc:description"];
/// Metadatos de autor que se vacían.
pub(crate) const CORE_AUTHOR_FIELDS: [&str; 2] = ["dc:creator", "cp:lastModifiedBy"];
pub(crate) const APP_AUTHOR_FIELDS: [&str; 2] = ["Company", "Manager"];
/// Textos de `docProps/app.xml` (títulos de las partes: el título del
/// documento, los de las diapositivas, los nombres de las hojas) y de
/// `docProps/custom.xml` que reciben las sustituciones literales.
pub(crate) const PROPERTY_TEXTS: [&str; 2] = ["vt:lpstr", "vt:lpwstr"];

/// Aplicación de Office de un paquete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Office {
    Word,
    PowerPoint,
    Excel,
}

impl Office {
    pub fn app(self) -> &'static str {
        match self {
            Office::Word => "Word",
            Office::PowerPoint => "PowerPoint",
            Office::Excel => "Excel",
        }
    }

    /// Para los mensajes de error: «no es un documento de Word válido».
    pub fn valid_noun(self) -> &'static str {
        match self {
            Office::Word => "un documento de Word válido",
            Office::PowerPoint => "una presentación de PowerPoint válida",
            Office::Excel => "un libro de Excel válido",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Office::Word => "docx",
            Office::PowerPoint => "pptx",
            Office::Excel => "xlsx",
        }
    }

    /// Extensión del formato binario de Office 97-2003.
    pub fn old_extension(self) -> &'static str {
        match self {
            Office::Word => "doc",
            Office::PowerPoint => "ppt",
            Office::Excel => "xls",
        }
    }

    /// Parte que tiene que estar en el paquete.
    pub(crate) fn main_part(self) -> &'static str {
        match self {
            Office::Word => "word/document.xml",
            Office::PowerPoint => "ppt/presentation.xml",
            Office::Excel => "xl/workbook.xml",
        }
    }

    /// Aplicación de un paquete por las partes que tiene.
    pub(crate) fn of_package(bytes: &[u8]) -> Option<Office> {
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
        [Office::Word, Office::PowerPoint, Office::Excel]
            .into_iter()
            .find(|office| archive.index_for_name(office.main_part()).is_some())
    }
}

/// Contenido que no se revisa (PRD, «Avisos»).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Warnings {
    pub images: usize,
    pub objects: usize,
}

/// Paquete abierto para leer sus partes.
pub(crate) struct Package<'a> {
    office: Office,
    archive: zip::ZipArchive<Cursor<&'a [u8]>>,
    names: Vec<String>,
}

impl<'a> Package<'a> {
    /// Abre el paquete y comprueba que tiene su parte principal. Un archivo
    /// OLE es un documento cifrado con contraseña.
    pub fn open(bytes: &'a [u8], office: Office) -> Result<Package<'a>> {
        if bytes.starts_with(&OLE_SIGNATURE) {
            return Err(Error::Protected(office));
        }
        let not_office = || {
            invalid(
                office,
                format!("no es un archivo {}", office.extension().to_uppercase()),
            )
        };
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| not_office())?;
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.map(|n| n.into_owned()))
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| not_office())?;
        if !names.iter().any(|n| n == office.main_part()) {
            return Err(invalid(office, missing_main(office).into()));
        }
        Ok(Package {
            office,
            archive,
            names,
        })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Contenido de una parte como texto, o `None` si no existe.
    pub fn read(&mut self, name: &str) -> Result<Option<String>> {
        read_part(&mut self.archive, self.office, name)
    }
}

fn missing_main(office: Office) -> &'static str {
    match office {
        Office::Word => "no tiene el cuerpo del documento",
        Office::PowerPoint => "no tiene la presentación",
        Office::Excel => "no tiene el libro",
    }
}

/// Contenido de una parte como texto, o `None` si no existe.
pub(crate) fn read_part(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    office: Office,
    name: &str,
) -> Result<Option<String>> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(invalid(office, format!("no se puede leer {name}"))),
    };
    let mut data = Vec::new();
    file.take(MAX_PART_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| invalid(office, format!("no se puede leer {name}")))?;
    if data.len() as u64 > MAX_PART_BYTES {
        return Err(invalid(office, format!("{name} es demasiado grande")));
    }
    String::from_utf8(data)
        .map(Some)
        .map_err(|_| invalid(office, format!("{name} no está en UTF-8")))
}

pub(crate) fn invalid(office: Office, detail: String) -> Error {
    Error::InvalidOffice(office, detail)
}

/// Error de XML con la parte y la posición, sin el texto del documento.
pub(crate) fn xml_error<R>(office: Office, part: &str, reader: &Reader<R>) -> Error {
    invalid(
        office,
        format!(
            "{part} no es XML válido (posición {})",
            reader.error_position()
        ),
    )
}

/// Añade a `text` el texto de un evento, con las entidades resueltas.
pub(crate) fn append_text(text: &mut String, event: &Event<'_>) -> std::result::Result<(), ()> {
    match event {
        Event::Text(e) => text.push_str(&e.xml10_content()),
        Event::CData(e) => text.push_str(&e.xml10_content()),
        Event::GeneralRef(e) => {
            if e.is_char_ref() {
                text.push(e.resolve_char_ref().ok().flatten().ok_or(())?);
            } else {
                text.push_str(resolve_predefined_entity(&e.xml10_content()).ok_or(())?);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Valor de un atributo con las entidades resueltas.
pub(crate) fn attribute(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// Texto de cada elemento llamado como alguno de `names`, en orden.
pub(crate) fn element_texts(
    office: Office,
    part: &str,
    xml: &str,
    names: &[&str],
) -> Result<Vec<(String, String)>> {
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    let mut current: Option<(String, String)> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(office, part, &reader))?;
        match &event {
            Event::Start(e) if names.contains(&e.name().as_ref()) => {
                current = Some((e.name().as_ref().to_owned(), String::new()));
            }
            Event::Empty(e) if names.contains(&e.name().as_ref()) => {
                found.push((e.name().as_ref().to_owned(), String::new()));
            }
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some((_, text)) = &mut current {
                    append_text(text, &event).map_err(|_| xml_error(office, part, &reader))?;
                }
            }
            Event::End(e)
                if current
                    .as_ref()
                    .is_some_and(|(n, _)| n == e.name().as_ref()) =>
            {
                found.extend(current.take());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}

/// Valor de un atributo de cada elemento llamado `element`, en orden.
pub(crate) fn attribute_values(
    office: Office,
    part: &str,
    xml: &str,
    element: &str,
    name: &str,
) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| xml_error(office, part, &reader))?
        {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == element => {
                found.extend(attribute(&e, name));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}

/// Destino de un enlace externo (`mailto:`, `https:`…) de un archivo de
/// relaciones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalLink {
    pub rels_part: String,
    pub id: String,
    pub target: String,
}

/// Destinos de los enlaces externos de un archivo de relaciones.
pub(crate) fn external_links(office: Office, part: &str, xml: &str) -> Result<Vec<ExternalLink>> {
    let mut reader = Reader::from_str(xml);
    let mut links = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| xml_error(office, part, &reader))?
        {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "Relationship" => {
                if attribute(&e, "TargetMode").as_deref() == Some("External")
                    && let (Some(id), Some(target)) = (attribute(&e, "Id"), attribute(&e, "Target"))
                {
                    links.push(ExternalLink {
                        rels_part: part.to_owned(),
                        id,
                        target,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(links)
}

/// Relaciones de un archivo `.rels`: identificador, tipo (la última parte de
/// la URI) y destino resuelto desde la carpeta de la parte.
pub(crate) fn relationships(
    office: Office,
    rels_part: &str,
    xml: &str,
) -> Result<Vec<(String, String, String)>> {
    // `ppt/slides/_rels/slide1.xml.rels` habla de rutas relativas a `ppt/slides`.
    let folder = rels_part
        .rsplit_once("_rels/")
        .map_or("", |(folder, _)| folder.trim_end_matches('/'));
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| xml_error(office, rels_part, &reader))?
        {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "Relationship" => {
                if attribute(&e, "TargetMode").as_deref() == Some("External") {
                    continue;
                }
                if let (Some(id), Some(kind), Some(target)) = (
                    attribute(&e, "Id"),
                    attribute(&e, "Type"),
                    attribute(&e, "Target"),
                ) {
                    let kind = kind.rsplit('/').next().unwrap_or_default().to_owned();
                    found.push((id, kind, resolve(folder, &target)));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(found)
}

/// Ruta de una parte a partir de la carpeta de quien la nombra.
pub(crate) fn resolve(folder: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_owned();
    }
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            piece => parts.push(piece),
        }
    }
    parts.join("/")
}

/// Archivo de relaciones de una parte: `ppt/slides/slide1.xml` →
/// `ppt/slides/_rels/slide1.xml.rels`.
pub(crate) fn rels_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Partes con un nombre `prefijoN.xml`, ordenadas por `N`.
pub(crate) fn numbered_parts(names: &[String], prefix: &str) -> Vec<String> {
    let number = |name: &String| -> Option<u64> {
        name.strip_prefix(prefix)?
            .strip_suffix(".xml")
            .filter(|middle| !middle.is_empty() && middle.chars().all(|c| c.is_ascii_digit()))?
            .parse()
            .ok()
    };
    let mut found: Vec<(u64, String)> = names
        .iter()
        .filter_map(|n| Some((number(n)?, n.clone())))
        .collect();
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

/// Archivos de relaciones de las partes de una carpeta, sin entrar en
/// subcarpetas: `ppt/slides/_rels/*.rels`.
pub(crate) fn is_rels_in(name: &str, folder: &str) -> bool {
    name.strip_prefix(folder)
        .and_then(|rest| rest.strip_prefix("/_rels/"))
        .is_some_and(|rest| rest.ends_with(".rels") && !rest.contains('/'))
}

pub(crate) fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.contains(&value) {
        list.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_resolved_from_the_folder_of_the_part() {
        assert_eq!(
            resolve("ppt/slides", "../notesSlides/notesSlide1.xml"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert_eq!(
            resolve("xl", "worksheets/sheet1.xml"),
            "xl/worksheets/sheet1.xml"
        );
        assert_eq!(
            resolve("xl", "/xl/sharedStrings.xml"),
            "xl/sharedStrings.xml"
        );
        assert_eq!(resolve("", "ppt/presentation.xml"), "ppt/presentation.xml");
    }

    #[test]
    fn rels_and_numbered_parts() {
        assert_eq!(
            rels_of("ppt/slides/slide1.xml"),
            "ppt/slides/_rels/slide1.xml.rels"
        );
        let names: Vec<String> = [
            "ppt/slides/slide10.xml",
            "ppt/slides/slide2.xml",
            "ppt/slides/slideLayout.xml",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            numbered_parts(&names, "ppt/slides/slide"),
            ["ppt/slides/slide2.xml", "ppt/slides/slide10.xml"]
        );
        assert!(is_rels_in("word/_rels/document.xml.rels", "word"));
        assert!(!is_rels_in("word/glossary/_rels/document.xml.rels", "word"));
    }
}
