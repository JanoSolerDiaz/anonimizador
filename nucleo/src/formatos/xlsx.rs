//! XLSX: edición del XML original, como en DOCX y PPTX.
//!
//! Cada texto distinto es un segmento: un texto compartido (`xl/sharedStrings.xml`,
//! que pueden usar muchas celdas), el texto en línea de una celda, el
//! resultado de texto de una fórmula y el valor de una celda numérica
//! (teléfonos o documentos escritos como número, fechas, importes). Los
//! segmentos van hoja a hoja, fila a fila, detrás del nombre de su hoja, para
//! que la lectura seguida del motor vea juntas las celdas de una fila. Detrás
//! van los textos compartidos que no usa ninguna celda, los comentarios, los
//! cuadros de texto y gráficos y los metadatos.
//!
//! Un número sustituido pasa a ser texto: la celda pierde su valor numérico
//! y una fórmula que lo usara deja de calcularlo. Una fecha se analiza tal
//! como se ve (`14/02/2025`), no como el número de serie que guarda Excel.
//!
//! Si el nombre de una hoja lleva un dato, la hoja se renombra con el
//! marcador sin corchetes, que Excel no admite, y el nombre nuevo se cambia
//! en las fórmulas, los nombres definidos, los gráficos, las tablas dinámicas,
//! los vínculos y `docProps/app.xml`.

use std::collections::{HashMap, HashSet};

use quick_xml::Reader;
use quick_xml::escape::partial_escape;
use quick_xml::events::{BytesStart, Event};

use crate::Result;
use crate::formatos::Replacement;
use crate::formatos::ooxml::drawingml::{self, ALT_TEXTS, BREAKS, CACHED_VALUES, FRAGMENTS};
use crate::formatos::ooxml::reescritura::{
    Rules, apply_literals, rebuild, rewrite, sorted_literals, splice, write_package,
};
use crate::formatos::ooxml::texto::markdown_table;
use crate::formatos::ooxml::{
    APP_AUTHOR_FIELDS, APP_PART, CORE_AUTHOR_FIELDS, CORE_PART, CORE_TEXT_FIELDS, CUSTOM_PART,
    ExternalLink, Office, PROPERTY_TEXTS, Package, Warnings, append_text, attribute, element_texts,
    external_links, push_unique, relationships, rels_of, xml_error,
};
use crate::formatos::pptx::{SegmentMap, apply_to_fragments, replaced_texts};

pub use crate::formatos::ooxml::drawingml::FragmentPos;

const WORKBOOK_PART: &str = "xl/workbook.xml";
/// Longitud máxima del nombre de una hoja.
const MAX_SHEET_NAME: usize = 31;
/// Elementos de una hoja cuyo texto es una fórmula.
const FORMULAS: [&str; 6] = ["f", "formula1", "formula2", "formula", "definedName", "c:f"];
/// Encabezados y pies de página de una hoja.
const HEADERS_FOOTERS: [&str; 6] = [
    "oddHeader",
    "oddFooter",
    "evenHeader",
    "evenFooter",
    "firstHeader",
    "firstFooter",
];
const TRUE: &str = "VERDADERO";
const FALSE: &str = "FALSO";

/// Qué es un segmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XlsxKind {
    /// Texto de una o más celdas: compartido, en línea o resultado de fórmula.
    Cell,
    /// Valor de una celda numérica, tal como se ve.
    Number,
    /// Nombre de una hoja.
    SheetName,
    /// Comentario, clásico o moderno.
    Comment,
    /// Encabezado o pie de página, con sus códigos de formato (`&L`, `&C`…).
    HeaderFooter,
    /// Cuadro de texto, forma o gráfico.
    Drawing,
    /// Título, asunto, palabras clave o descripción del libro.
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxSegment {
    pub text: String,
    pub part: String,
    pub kind: XlsxKind,
    /// Una posición por carácter de `text`; vacío en números y nombres de
    /// hoja, que no tienen fragmentos.
    pub map: Vec<Option<FragmentPos>>,
    pub breaks: Vec<(usize, usize)>,
}

/// Lo que se ve en una celda.
#[derive(Debug, Clone)]
enum Content {
    /// Texto de un segmento, o el original si no tiene segmento (vacío).
    Text {
        segment: Option<usize>,
        original: String,
    },
    /// Valor que no se analiza: verdadero, falso o un error.
    Fixed(String),
}

#[derive(Debug, Clone)]
struct Cell {
    row: usize,
    column: usize,
    content: Content,
}

#[derive(Debug, Clone)]
struct Sheet {
    name: String,
    part: String,
    name_segment: usize,
    cells: Vec<Cell>,
}

/// Texto compartido leído: texto y mapa a los fragmentos de la parte.
struct Shared {
    text: String,
    map: Vec<Option<FragmentPos>>,
}

#[derive(Debug, Clone)]
pub struct XlsxDocument {
    original: Vec<u8>,
    segments: Vec<XlsxSegment>,
    sheets: Vec<Sheet>,
    /// Fragmentos de cada parte con texto.
    fragments: HashMap<String, Vec<String>>,
    /// Segmento numérico → parte y número de la celda en la parte.
    numbers: HashMap<usize, (String, usize)>,
    /// Segmento numérico → título de su columna, que se le da al modelo como
    /// contexto («Teléfono: 654218903»).
    headers: HashMap<usize, String>,
    /// Partes que se reescriben con sus reglas.
    shared_part: Option<String>,
    links: Vec<ExternalLink>,
    literal_texts: Vec<String>,
    author_metadata: Vec<String>,
    comment_authors: Vec<String>,
    warnings: Warnings,
}

impl XlsxDocument {
    pub fn from_bytes(bytes: &[u8]) -> Result<XlsxDocument> {
        let mut package = Package::open(bytes, Office::Excel)?;
        let names = package.names().to_vec();
        let mut document = XlsxDocument {
            original: bytes.to_vec(),
            segments: Vec::new(),
            sheets: Vec::new(),
            fragments: HashMap::new(),
            numbers: HashMap::new(),
            headers: HashMap::new(),
            shared_part: None,
            links: Vec::new(),
            literal_texts: Vec::new(),
            author_metadata: Vec::new(),
            comment_authors: Vec::new(),
            warnings: Warnings::default(),
        };

        let workbook = package.read(WORKBOOK_PART)?.unwrap_or_default();
        let book = read_workbook(&workbook)?;
        document.literal_texts.extend(book.defined_names);
        let rels_name = rels_of(WORKBOOK_PART);
        let rels = match package.read(&rels_name)? {
            Some(xml) => relationships(Office::Excel, &rels_name, &xml)?,
            None => Vec::new(),
        };
        let target = |kind: &str| {
            rels.iter()
                .find(|(_, k, _)| k == kind)
                .map(|(_, _, t)| t.clone())
        };
        let styles = match target("styles") {
            Some(part) => package.read(&part)?,
            None => None,
        };
        let dates = read_date_styles(styles.as_deref().unwrap_or_default())?;

        // Textos compartidos: se leen ahora y se hacen segmento la primera
        // vez que los usa una celda.
        let mut shared: Vec<Shared> = Vec::new();
        if let Some(part) = target("sharedStrings")
            && let Some(xml) = package.read(&part)?
        {
            let (strings, fragments) = read_shared_strings(&part, &xml)?;
            shared = strings;
            document.fragments.insert(part.clone(), fragments);
            document.shared_part = Some(part);
        }
        let mut shared_segment: Vec<Option<usize>> = vec![None; shared.len()];

        for (name, id) in book.sheets {
            let Some(part) = rels
                .iter()
                .find(|(rid, _, _)| *rid == id)
                .map(|(_, _, t)| t.clone())
            else {
                continue;
            };
            let name_segment = document.segments.len();
            document.segments.push(XlsxSegment {
                text: name.clone(),
                part: WORKBOOK_PART.to_owned(),
                kind: XlsxKind::SheetName,
                map: Vec::new(),
                breaks: Vec::new(),
            });
            let xml = package.read(&part)?.unwrap_or_default();
            let read = read_sheet(&part, &xml, &dates, book.date1904)?;
            document.literal_texts.extend(read.literal_texts);
            let mut cells = Vec::new();
            for cell in read.cells {
                let content = match cell.value {
                    Value::Shared(index) => match shared.get(index) {
                        Some(s) => {
                            if shared_segment[index].is_none() && !s.text.trim().is_empty() {
                                shared_segment[index] = Some(document.segments.len());
                                document.segments.push(XlsxSegment {
                                    text: s.text.clone(),
                                    part: document.shared_part.clone().unwrap_or_default(),
                                    kind: XlsxKind::Cell,
                                    map: s.map.clone(),
                                    breaks: Vec::new(),
                                });
                            }
                            Content::Text {
                                segment: shared_segment[index],
                                original: s.text.clone(),
                            }
                        }
                        None => Content::Fixed(String::new()),
                    },
                    Value::Fragments { text, map } => {
                        let segment = (!text.trim().is_empty()).then(|| {
                            document.segments.push(XlsxSegment {
                                text: text.clone(),
                                part: part.clone(),
                                kind: XlsxKind::Cell,
                                map,
                                breaks: Vec::new(),
                            });
                            document.segments.len() - 1
                        });
                        Content::Text {
                            segment,
                            original: text,
                        }
                    }
                    Value::Number { text, ordinal } => {
                        let segment = document.segments.len();
                        document.numbers.insert(segment, (part.clone(), ordinal));
                        document.segments.push(XlsxSegment {
                            text: text.clone(),
                            part: part.clone(),
                            kind: XlsxKind::Number,
                            map: Vec::new(),
                            breaks: Vec::new(),
                        });
                        Content::Text {
                            segment: Some(segment),
                            original: text,
                        }
                    }
                    Value::Fixed(text) => Content::Fixed(text),
                };
                cells.push(Cell {
                    row: cell.row,
                    column: cell.column,
                    content,
                });
            }
            for (text, map) in read.headers {
                document.segments.push(XlsxSegment {
                    text,
                    part: part.clone(),
                    kind: XlsxKind::HeaderFooter,
                    map,
                    breaks: Vec::new(),
                });
            }
            // El título de cada columna: el texto de la primera fila.
            let first_row = cells.iter().map(|c| c.row).min();
            let titles: HashMap<usize, String> = cells
                .iter()
                .filter(|c| Some(c.row) == first_row)
                .filter_map(|c| match &c.content {
                    Content::Text { segment, original }
                        if !original.trim().is_empty()
                            && !segment.is_some_and(|s| document.numbers.contains_key(&s)) =>
                    {
                        Some((c.column, original.trim().to_owned()))
                    }
                    _ => None,
                })
                .collect();
            for c in cells.iter().filter(|c| Some(c.row) != first_row) {
                if let Content::Text {
                    segment: Some(s), ..
                } = &c.content
                    && document.numbers.contains_key(s)
                    && let Some(title) = titles.get(&c.column)
                {
                    document.headers.insert(*s, title.clone());
                }
            }
            document.fragments.insert(part.clone(), read.fragments);
            document.sheets.push(Sheet {
                name,
                part,
                name_segment,
                cells,
            });
        }
        // Textos compartidos que no usa ninguna celda: siguen en el archivo.
        for (index, s) in shared.into_iter().enumerate() {
            if shared_segment[index].is_none() && !s.text.trim().is_empty() {
                document.segments.push(XlsxSegment {
                    text: s.text,
                    part: document.shared_part.clone().unwrap_or_default(),
                    kind: XlsxKind::Cell,
                    map: s.map,
                    breaks: Vec::new(),
                });
            }
        }

        for name in &names {
            if is_comments(name) {
                let xml = package.read(name)?.unwrap_or_default();
                document.add_comments(name, &xml)?;
            } else if name.starts_with("xl/threadedComments/") && name.ends_with(".xml") {
                let xml = package.read(name)?.unwrap_or_default();
                document.add_threaded_comments(name, &xml)?;
            } else if name.starts_with("xl/persons/") && name.ends_with(".xml") {
                let xml = package.read(name)?.unwrap_or_default();
                for key in ["displayName", "userId"] {
                    for value in crate::formatos::ooxml::attribute_values(
                        Office::Excel,
                        name,
                        &xml,
                        "person",
                        key,
                    )? {
                        push_unique(&mut document.comment_authors, value);
                    }
                }
            }
        }
        for name in names.iter().filter(|n| is_drawing(n)) {
            let xml = package.read(name)?.unwrap_or_default();
            let found = drawingml::extract(Office::Excel, name, &xml)?;
            document.warnings.images += found.images;
            for p in found.paragraphs {
                document.segments.push(XlsxSegment {
                    text: p.text,
                    part: name.clone(),
                    kind: XlsxKind::Drawing,
                    map: p.map,
                    breaks: p.breaks,
                });
            }
            document.literal_texts.extend(found.literal_texts);
            document.fragments.insert(name.clone(), found.fragments);
        }
        for name in names.iter().filter(|n| is_pivot_cache(n)) {
            let xml = package.read(name)?.unwrap_or_default();
            for (element, key) in [("s", "v"), ("cacheField", "name")] {
                document
                    .literal_texts
                    .extend(crate::formatos::ooxml::attribute_values(
                        Office::Excel,
                        name,
                        &xml,
                        element,
                        key,
                    )?);
            }
        }
        for name in names.iter().filter(|n| is_table(n)) {
            let xml = package.read(name)?.unwrap_or_default();
            document
                .literal_texts
                .extend(crate::formatos::ooxml::attribute_values(
                    Office::Excel,
                    name,
                    &xml,
                    "tableColumn",
                    "name",
                )?);
        }
        if let Some(xml) = package.read(CORE_PART)? {
            document.add_core(&xml)?;
        }
        if let Some(xml) = package.read(APP_PART)? {
            for (_, text) in element_texts(Office::Excel, APP_PART, &xml, &APP_AUTHOR_FIELDS)? {
                push_unique(&mut document.author_metadata, text);
            }
        }
        for name in names.iter().filter(|n| is_xl_rels(n)) {
            if let Some(xml) = package.read(name)? {
                document
                    .links
                    .extend(external_links(Office::Excel, name, &xml)?);
            }
        }
        document.warnings.objects = names
            .iter()
            .filter(|n| n.starts_with("xl/embeddings/"))
            .count();
        Ok(document)
    }

    fn add_comments(&mut self, part: &str, xml: &str) -> Result<()> {
        let mut reader = Reader::from_str(xml);
        let mut fragments = Vec::new();
        let mut current: Option<(String, Vec<Option<FragmentPos>>)> = None;
        let mut reading: Option<String> = None;
        let mut author: Option<String> = None;
        let mut phonetic = 0usize;
        loop {
            let event = reader
                .read_event()
                .map_err(|_| xml_error(Office::Excel, part, &reader))?;
            match &event {
                Event::Start(e) => match e.name().as_ref() {
                    "comment" => current = Some((String::new(), Vec::new())),
                    "rPh" => phonetic += 1,
                    "t" if phonetic == 0 => reading = Some(String::new()),
                    "author" => author = Some(String::new()),
                    _ => {}
                },
                Event::Empty(e) if e.name().as_ref() == "t" && phonetic == 0 => {
                    fragments.push(String::new());
                }
                Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                    if let Some(text) = reading.as_mut().or(author.as_mut()) {
                        append_text(text, &event)
                            .map_err(|_| xml_error(Office::Excel, part, &reader))?;
                    }
                }
                Event::End(e) => match e.name().as_ref() {
                    "t" => {
                        if let Some(text) = reading.take() {
                            let index = fragments.len();
                            if let Some((all, map)) = &mut current {
                                for (offset, c) in text.chars().enumerate() {
                                    all.push(c);
                                    map.push(Some(FragmentPos {
                                        fragment: index,
                                        offset,
                                    }));
                                }
                            }
                            fragments.push(text);
                        }
                    }
                    "rPh" => phonetic = phonetic.saturating_sub(1),
                    "author" => {
                        if let Some(name) = author.take() {
                            push_unique(&mut self.comment_authors, name);
                        }
                    }
                    "comment" => {
                        if let Some((text, map)) = current.take()
                            && !text.trim().is_empty()
                        {
                            self.segments.push(XlsxSegment {
                                text,
                                part: part.to_owned(),
                                kind: XlsxKind::Comment,
                                map,
                                breaks: Vec::new(),
                            });
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        self.fragments.insert(part.to_owned(), fragments);
        Ok(())
    }

    fn add_threaded_comments(&mut self, part: &str, xml: &str) -> Result<()> {
        let mut fragments = Vec::new();
        for (_, text) in element_texts(Office::Excel, part, xml, &["text"])? {
            let index = fragments.len();
            if !text.trim().is_empty() {
                self.segments.push(XlsxSegment {
                    map: (0..text.chars().count())
                        .map(|offset| {
                            Some(FragmentPos {
                                fragment: index,
                                offset,
                            })
                        })
                        .collect(),
                    text: text.clone(),
                    part: part.to_owned(),
                    kind: XlsxKind::Comment,
                    breaks: Vec::new(),
                });
            }
            fragments.push(text);
        }
        self.fragments.insert(part.to_owned(), fragments);
        Ok(())
    }

    fn add_core(&mut self, xml: &str) -> Result<()> {
        let names = [&CORE_TEXT_FIELDS[..], &CORE_AUTHOR_FIELDS[..]].concat();
        let mut fragments = Vec::new();
        for (name, text) in element_texts(Office::Excel, CORE_PART, xml, &names)? {
            if CORE_AUTHOR_FIELDS.contains(&name.as_str()) {
                push_unique(&mut self.author_metadata, text);
                continue;
            }
            let index = fragments.len();
            if !text.trim().is_empty() {
                self.segments.push(XlsxSegment {
                    map: (0..text.chars().count())
                        .map(|offset| {
                            Some(FragmentPos {
                                fragment: index,
                                offset,
                            })
                        })
                        .collect(),
                    text: text.clone(),
                    part: CORE_PART.to_owned(),
                    kind: XlsxKind::Metadata,
                    breaks: Vec::new(),
                });
            }
            fragments.push(text);
        }
        self.fragments.insert(CORE_PART.to_owned(), fragments);
        Ok(())
    }

    pub fn segments(&self) -> &[XlsxSegment] {
        &self.segments
    }

    /// Texto que se le da al modelo para cada segmento y cuántos caracteres
    /// de contexto lleva delante. Un número solo («654218903») no le dice
    /// nada al modelo: se le pasa con el título de su columna
    /// («Teléfono: 654218903»). El contexto no es parte del segmento: una
    /// entidad que caiga en él no cuenta.
    pub fn detection_texts(&self) -> Vec<(String, usize)> {
        self.segments
            .iter()
            .enumerate()
            .map(|(index, s)| match self.headers.get(&index) {
                Some(title) => {
                    let prefix = format!("{title}: ");
                    let length = prefix.chars().count();
                    (prefix + &s.text, length)
                }
                None => (s.text.clone(), 0),
            })
            .collect()
    }

    pub fn external_links(&self) -> &[ExternalLink] {
        &self.links
    }

    /// Textos que solo admiten sustitución literal: destinos de enlaces,
    /// fórmulas (los textos entre comillas), nombres definidos, vínculos de
    /// las celdas, valores en caché de gráficos y tablas dinámicas, columnas
    /// de las tablas y texto alternativo.
    pub fn literal_texts(&self) -> Vec<&str> {
        self.links
            .iter()
            .map(|l| l.target.as_str())
            .chain(self.literal_texts.iter().map(String::as_str))
            .collect()
    }

    /// Autor, último en modificar, empresa y responsable. Se vacían al
    /// escribir.
    pub fn author_metadata(&self) -> &[String] {
        &self.author_metadata
    }

    /// Autores de los comentarios y personas de los comentarios modernos,
    /// con su identificador. Se vacían al escribir.
    pub fn comment_authors(&self) -> &[String] {
        &self.comment_authors
    }

    pub fn warnings(&self) -> Warnings {
        self.warnings
    }

    /// El libro con las sustituciones aplicadas sobre los segmentos y las
    /// literales (`valor`, `nuevo`) en lo demás.
    pub fn write(
        &self,
        replacements: &[Replacement],
        literals: &[(String, String)],
    ) -> Result<Vec<u8>> {
        let texts = replaced_texts(self.segments.iter().map(|s| s.text.as_str()), replacements)?;
        // Las de segmentos con fragmentos van al mapa; números y nombres de
        // hoja se tratan aparte.
        let with_fragments: Vec<Replacement> = replacements
            .iter()
            .filter(|r| !self.segments[r.segment].map.is_empty())
            .cloned()
            .collect();
        let segments: Vec<SegmentMap> = self
            .segments
            .iter()
            .map(|s| (s.part.as_str(), s.map.as_slice(), s.breaks.as_slice()))
            .collect();
        let (fragments, breaks) = apply_to_fragments(&segments, &with_fragments, |part, index| {
            self.fragments[part][index].as_str()
        })?;
        let mut numbers: HashMap<&str, HashMap<usize, String>> = HashMap::new();
        for r in replacements {
            if let Some((part, ordinal)) = self.numbers.get(&r.segment) {
                numbers
                    .entry(part.as_str())
                    .or_default()
                    .insert(*ordinal, texts[r.segment].clone());
            }
        }
        let renames = self.renames(&texts, replacements);
        let literals = sorted_literals(literals);
        // En `docProps/app.xml` los nombres de hoja van enteros y primero.
        let mut property_literals: Vec<(String, String)> = renames
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect();
        property_literals.extend(literals.iter().cloned());
        let property_literals = sorted_literals(&property_literals);
        let transform = |element: &str, key: Option<&str>, text: &str| -> String {
            match (element, key) {
                ("sheet", Some("name")) | ("worksheetSource", Some("sheet")) => renames
                    .get(text)
                    .cloned()
                    .unwrap_or_else(|| text.to_owned()),
                _ => rewrite_formula(text, &renames, &literals),
            }
        };
        let none = HashMap::new();
        let rules_for = |name: &str| -> Option<Rules> {
            let new_fragments = Some(fragments.get(name).unwrap_or(&none));
            if name == WORKBOOK_PART {
                Some(Rules {
                    transformed: &["definedName"],
                    transformed_attributes: &[("sheet", "name")],
                    transform: Some(&transform),
                    ..Rules::default()
                })
            } else if self.shared_part.as_deref() == Some(name) {
                Some(Rules {
                    fragments: &["t"],
                    skip_inside: &["rPh"],
                    preserve_space: true,
                    new_fragments,
                    ..Rules::default()
                })
            } else if is_comments(name) {
                Some(Rules {
                    fragments: &["t"],
                    skip_inside: &["rPh"],
                    preserve_space: true,
                    new_fragments,
                    blank: &["author"],
                    ..Rules::default()
                })
            } else if name.starts_with("xl/threadedComments/") {
                Some(Rules {
                    fragments: &["text"],
                    new_fragments,
                    ..Rules::default()
                })
            } else if name.starts_with("xl/persons/") {
                Some(Rules {
                    blank_attributes: &["displayName", "userId"],
                    ..Rules::default()
                })
            } else if is_drawing(name) {
                Some(Rules {
                    fragments: &FRAGMENTS,
                    new_fragments,
                    removed_breaks: breaks.get(name),
                    breaks: &BREAKS,
                    literal: &CACHED_VALUES,
                    literal_attributes: &ALT_TEXTS,
                    literals: &literals,
                    transformed: &["c:f"],
                    transform: Some(&transform),
                    ..Rules::default()
                })
            } else if is_pivot_cache(name) {
                Some(Rules {
                    literal_attributes: &[("s", "v"), ("cacheField", "name")],
                    literals: &literals,
                    transformed_attributes: &[("worksheetSource", "sheet")],
                    transform: Some(&transform),
                    ..Rules::default()
                })
            } else if is_table(name) {
                Some(Rules {
                    literal_attributes: &[("tableColumn", "name")],
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == CORE_PART {
                Some(Rules {
                    fragments: &CORE_TEXT_FIELDS,
                    new_fragments,
                    blank: &CORE_AUTHOR_FIELDS,
                    ..Rules::default()
                })
            } else if name == APP_PART || name == CUSTOM_PART {
                Some(Rules {
                    blank: &APP_AUTHOR_FIELDS,
                    literal: &PROPERTY_TEXTS,
                    literals: &property_literals,
                    ..Rules::default()
                })
            } else if is_xl_rels(name) {
                Some(Rules {
                    link_targets: true,
                    literals: &literals,
                    ..Rules::default()
                })
            } else {
                None
            }
        };
        let sheets: HashSet<&str> = self.sheets.iter().map(|s| s.part.as_str()).collect();
        write_package(
            &self.original,
            Office::Excel,
            |name| sheets.contains(name) || rules_for(name).is_some(),
            |name, xml| {
                if sheets.contains(name) {
                    rewrite_sheet(
                        name,
                        xml,
                        fragments.get(name).unwrap_or(&none),
                        numbers.get(name).unwrap_or(&none),
                        &literals,
                        &transform,
                    )
                } else {
                    match rules_for(name) {
                        Some(rules) => rewrite(Office::Excel, name, xml, &rules),
                        None => Ok(None),
                    }
                }
            },
        )
    }

    /// Nombre antiguo → nombre nuevo de las hojas que cambian: los
    /// marcadores sin corchetes, como mucho 31 caracteres y sin repetir
    /// ningún otro nombre.
    fn renames(&self, texts: &[String], replacements: &[Replacement]) -> HashMap<String, String> {
        let changed: HashSet<usize> = replacements
            .iter()
            .map(|r| r.segment)
            .filter(|s| self.segments[*s].kind == XlsxKind::SheetName)
            .collect();
        let mut taken: Vec<String> = self
            .sheets
            .iter()
            .filter(|s| !changed.contains(&s.name_segment))
            .map(|s| s.name.to_lowercase())
            .collect();
        let mut renames = HashMap::new();
        for sheet in &self.sheets {
            if !changed.contains(&sheet.name_segment) {
                continue;
            }
            let mut name = sheet_name(&texts[sheet.name_segment]);
            if taken.contains(&name.to_lowercase()) {
                let base = name.clone();
                let mut n = 2;
                loop {
                    let suffix = format!(" ({n})");
                    let room = MAX_SHEET_NAME - suffix.chars().count();
                    name = base.chars().take(room).collect::<String>() + &suffix;
                    if !taken.contains(&name.to_lowercase()) {
                        break;
                    }
                    n += 1;
                }
            }
            taken.push(name.to_lowercase());
            if name != sheet.name {
                renames.insert(sheet.name.clone(), name);
            }
        }
        renames
    }

    /// Markdown: una sección por hoja con sus celdas en una tabla.
    pub fn markdown(&self, replacements: &[Replacement]) -> Result<String> {
        let sections: Vec<String> = self
            .grids(replacements)?
            .into_iter()
            .map(|(name, rows)| {
                let table = markdown_table(&rows);
                if table.is_empty() {
                    format!("## {name}")
                } else {
                    format!("## {name}\n\n{table}")
                }
            })
            .collect();
        Ok(if sections.is_empty() {
            String::new()
        } else {
            sections.join("\n\n") + "\n"
        })
    }

    /// Texto: el nombre de cada hoja y una línea por fila, con las celdas
    /// separadas por tabuladores.
    pub fn plain_text(&self, replacements: &[Replacement]) -> Result<String> {
        let sections: Vec<String> = self
            .grids(replacements)?
            .into_iter()
            .map(|(name, rows)| {
                let mut lines = vec![name];
                lines.extend(rows.into_iter().map(|row| {
                    row.iter()
                        .map(|c| c.replace(['\t', '\n', '\r'], " "))
                        .collect::<Vec<_>>()
                        .join("\t")
                }));
                lines.join("\n")
            })
            .collect();
        Ok(if sections.is_empty() {
            String::new()
        } else {
            sections.join("\n\n") + "\n"
        })
    }

    /// Nombre (ya cambiado) y filas con texto de cada hoja.
    fn grids(&self, replacements: &[Replacement]) -> Result<Vec<(String, Vec<Vec<String>>)>> {
        let texts = replaced_texts(self.segments.iter().map(|s| s.text.as_str()), replacements)?;
        let renames = self.renames(&texts, replacements);
        let mut grids = Vec::new();
        for sheet in &self.sheets {
            let name = renames
                .get(&sheet.name)
                .cloned()
                .unwrap_or_else(|| sheet.name.clone());
            let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
            for cell in &sheet.cells {
                let text = match &cell.content {
                    Content::Text {
                        segment: Some(s), ..
                    } => texts[*s].clone(),
                    Content::Text { original, .. } => original.clone(),
                    Content::Fixed(text) => text.clone(),
                };
                if text.trim().is_empty() {
                    continue;
                }
                if rows.last().is_none_or(|(r, _)| *r != cell.row) {
                    rows.push((cell.row, Vec::new()));
                }
                let row = &mut rows.last_mut().expect("recién añadida").1;
                if row.len() <= cell.column {
                    row.resize(cell.column + 1, String::new());
                }
                row[cell.column] = text;
            }
            // Las columnas vacías a la izquierda de todas las filas no salen.
            let first = rows
                .iter()
                .filter_map(|(_, row)| row.iter().position(|c| !c.is_empty()))
                .min()
                .unwrap_or(0);
            let rows = rows
                .into_iter()
                .map(|(_, row)| row[first..].to_vec())
                .collect();
            grids.push((name, rows));
        }
        Ok(grids)
    }
}

/// Nombre de hoja a partir del texto sustituido: los marcadores sin
/// corchetes y, si no cabe en 31 caracteres, solo los marcadores que quepan.
fn sheet_name(replaced: &str) -> String {
    let bare: String = replaced
        .chars()
        .filter(|c| !matches!(c, '[' | ']'))
        .collect();
    if bare.chars().count() <= MAX_SHEET_NAME {
        return bare;
    }
    let markers: Vec<&str> = replaced
        .split('[')
        .skip(1)
        .filter_map(|rest| rest.split_once(']').map(|(m, _)| m))
        .collect();
    let mut name = String::new();
    for marker in markers {
        let next = if name.is_empty() {
            marker.to_owned()
        } else {
            format!("{name} {marker}")
        };
        if next.chars().count() > MAX_SHEET_NAME {
            break;
        }
        name = next;
    }
    if name.is_empty() {
        bare.chars().take(MAX_SHEET_NAME).collect()
    } else {
        name
    }
}

/// La fórmula con las hojas renombradas y las literales aplicadas dentro de
/// los textos entre comillas. Fuera de las comillas solo hay referencias,
/// funciones y números, que no se tocan.
pub(crate) fn rewrite_formula(
    formula: &str,
    renames: &HashMap<String, String>,
    literals: &[(String, String)],
) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            quote @ ('"' | '\'') => {
                // Hasta la comilla de cierre; dos comillas seguidas son una.
                let mut inner = String::new();
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == quote {
                        if chars.get(j + 1) == Some(&quote) {
                            inner.push(quote);
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    inner.push(chars[j]);
                    j += 1;
                }
                let closed = j < chars.len();
                let escape = |s: &str| s.replace(quote, &format!("{quote}{quote}"));
                if quote == '"' {
                    out.push('"');
                    out.push_str(&escape(&apply_literals(&inner, literals)));
                    if closed {
                        out.push('"');
                    }
                } else {
                    let name = renames.get(&inner).map_or(inner.as_str(), String::as_str);
                    out.push('\'');
                    out.push_str(&escape(name));
                    if closed {
                        out.push('\'');
                    }
                }
                i = j + 1;
            }
            c if c.is_alphabetic() || c == '_' || c == '\\' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '.' | '\\'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match renames.get(&word) {
                    Some(new) if chars.get(i) == Some(&'!') => {
                        out.push('\'');
                        out.push_str(&new.replace('\'', "''"));
                        out.push('\'');
                    }
                    _ => out.push_str(&word),
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

struct Workbook {
    /// Nombre e identificador de relación de cada hoja, en orden.
    sheets: Vec<(String, String)>,
    defined_names: Vec<String>,
    date1904: bool,
}

fn read_workbook(xml: &str) -> Result<Workbook> {
    let mut reader = Reader::from_str(xml);
    let mut book = Workbook {
        sheets: Vec::new(),
        defined_names: Vec::new(),
        date1904: false,
    };
    let mut defined: Option<String> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(Office::Excel, WORKBOOK_PART, &reader))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                "sheet" => {
                    if let (Some(name), Some(id)) = (attribute(e, "name"), attribute(e, "r:id")) {
                        book.sheets.push((name, id));
                    }
                }
                "workbookPr" => {
                    book.date1904 =
                        matches!(attribute(e, "date1904").as_deref(), Some("1" | "true"));
                }
                "definedName" if matches!(event, Event::Start(_)) => defined = Some(String::new()),
                _ => {}
            },
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some(text) = &mut defined {
                    append_text(text, &event)
                        .map_err(|_| xml_error(Office::Excel, WORKBOOK_PART, &reader))?;
                }
            }
            Event::End(e) if e.name().as_ref() == "definedName" => {
                book.defined_names.extend(defined.take());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(book)
}

/// Cómo se ve una celda numérica según su estilo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumberLook {
    Plain,
    Date,
    DateTime,
    Time,
}

/// Aspecto de cada estilo de celda (`cellXfs`), en orden.
fn read_date_styles(xml: &str) -> Result<Vec<NumberLook>> {
    let mut reader = Reader::from_str(xml);
    let mut formats: HashMap<u32, String> = HashMap::new();
    let mut looks = Vec::new();
    let mut in_cell_xfs = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(Office::Excel, "xl/styles.xml", &reader))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                "numFmt" => {
                    if let (Some(id), Some(code)) = (
                        attribute(e, "numFmtId").and_then(|v| v.parse().ok()),
                        attribute(e, "formatCode"),
                    ) {
                        formats.insert(id, code);
                    }
                }
                "cellXfs" if matches!(event, Event::Start(_)) => in_cell_xfs = true,
                "xf" if in_cell_xfs => {
                    let id: u32 = attribute(e, "numFmtId")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    looks.push(look_of(id, formats.get(&id).map(String::as_str)));
                }
                _ => {}
            },
            Event::End(e) if e.name().as_ref() == "cellXfs" => in_cell_xfs = false,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(looks)
}

/// Aspecto de un formato de número: los integrados de fecha y hora de Excel
/// y los propios que tienen día, mes y año u horas.
fn look_of(id: u32, code: Option<&str>) -> NumberLook {
    match id {
        14..=17 | 27..=31 | 34..=36 | 50..=58 => return NumberLook::Date,
        22 => return NumberLook::DateTime,
        18..=21 | 32 | 33 | 45..=47 => return NumberLook::Time,
        _ => {}
    }
    let Some(code) = code else {
        return NumberLook::Plain;
    };
    // Sin lo que va entre comillas, entre corchetes ni escapado.
    let mut plain = String::new();
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                }
            }
            // Un color o una condición; `[h]` o `[mm]` son tiempo transcurrido.
            '[' => {
                let inner: String = chars.by_ref().take_while(|d| *d != ']').collect();
                if !inner.is_empty()
                    && inner
                        .chars()
                        .all(|d| matches!(d.to_ascii_lowercase(), 'h' | 'm' | 's'))
                {
                    plain.push_str(&inner.to_ascii_lowercase());
                }
            }
            '\\' => {
                chars.next();
            }
            c => plain.push(c.to_ascii_lowercase()),
        }
    }
    let date = plain.contains('y') || plain.contains('d');
    let time = plain.contains('h') || plain.contains('s');
    match (date, time) {
        (true, true) => NumberLook::DateTime,
        (true, false) => NumberLook::Date,
        (false, true) => NumberLook::Time,
        (false, false) => NumberLook::Plain,
    }
}

/// Número tal como se ve: las fechas en `dd/mm/aaaa`, los enteros sin
/// notación científica.
fn render_number(raw: &str, look: NumberLook, date1904: bool) -> String {
    let Ok(value) = raw.trim().parse::<f64>() else {
        return raw.to_owned();
    };
    if look != NumberLook::Plain && value >= 0.0 {
        let days = value.trunc() as i64;
        let seconds = ((value - value.trunc()) * 86_400.0).round() as i64;
        // El 1900 de Excel cuenta un 29 de febrero que no existió.
        let (base, days) = if date1904 {
            (jiff::civil::date(1904, 1, 1), days)
        } else if days < 60 {
            (jiff::civil::date(1899, 12, 31), days)
        } else {
            (jiff::civil::date(1899, 12, 30), days)
        };
        if let Ok(date) = base.checked_add(jiff::Span::new().days(days)) {
            let time = format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            let day = date.strftime("%d/%m/%Y").to_string();
            return match look {
                NumberLook::Date => day,
                NumberLook::DateTime => format!("{day} {}", &time[..5]),
                _ => time,
            };
        }
    }
    if value.fract() == 0.0 && value.abs() < 1e17 && raw.contains(['E', 'e']) {
        return format!("{}", value as i64);
    }
    raw.trim().to_owned()
}

/// Valor de una celda leída.
enum Value {
    Shared(usize),
    Fragments {
        text: String,
        map: Vec<Option<FragmentPos>>,
    },
    Number {
        text: String,
        ordinal: usize,
    },
    Fixed(String),
}

struct ReadCell {
    row: usize,
    column: usize,
    value: Value,
}

struct ReadSheet {
    cells: Vec<ReadCell>,
    headers: Vec<(String, Vec<Option<FragmentPos>>)>,
    fragments: Vec<String>,
    literal_texts: Vec<String>,
}

/// Celda abierta mientras se recorre la hoja.
struct OpenCell {
    row: usize,
    column: usize,
    kind: Option<String>,
    style: usize,
    ordinal: usize,
    value: Option<String>,
    text: String,
    map: Vec<Option<FragmentPos>>,
    has_inline: bool,
}

/// Qué texto se está leyendo dentro de la hoja.
enum Reading {
    Nothing,
    /// Fragmento: un `t` de un texto en línea, el `v` de un resultado de
    /// texto o un encabezado o pie.
    Fragment(String),
    /// El `v` de una celda que no es texto.
    Value(String),
    /// Fórmula u otro texto que solo admite literales.
    Literal(String),
}

fn read_sheet(part: &str, xml: &str, looks: &[NumberLook], date1904: bool) -> Result<ReadSheet> {
    let mut reader = Reader::from_str(xml);
    let mut out = ReadSheet {
        cells: Vec::new(),
        headers: Vec::new(),
        fragments: Vec::new(),
        literal_texts: Vec::new(),
    };
    let mut row = 0usize;
    let mut last_column: Option<usize> = None;
    let mut ordinal = 0usize;
    let mut cell: Option<OpenCell> = None;
    let mut phonetic = 0usize;
    let mut in_inline = false;
    let mut reading = Reading::Nothing;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(Office::Excel, part, &reader))?;
        let empty = matches!(event, Event::Empty(_));
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name();
                match name.as_ref() {
                    "row" => {
                        row = attribute(e, "r")
                            .and_then(|r| r.parse().ok())
                            .unwrap_or(row + 1);
                        last_column = None;
                    }
                    "c" => {
                        let (r, column) = match attribute(e, "r").and_then(|r| cell_position(&r)) {
                            Some((r, c)) => (r, c),
                            None => (row, last_column.map_or(0, |c| c + 1)),
                        };
                        last_column = Some(column);
                        let open = OpenCell {
                            row: r,
                            column,
                            kind: attribute(e, "t"),
                            style: attribute(e, "s").and_then(|s| s.parse().ok()).unwrap_or(0),
                            ordinal,
                            value: None,
                            text: String::new(),
                            map: Vec::new(),
                            has_inline: false,
                        };
                        ordinal += 1;
                        if empty {
                            out.cells.extend(finish_cell(open, looks, date1904));
                        } else {
                            cell = Some(open);
                        }
                    }
                    "is" if !empty => {
                        in_inline = true;
                        if let Some(c) = &mut cell {
                            c.has_inline = true;
                        }
                    }
                    "rPh" if !empty => phonetic += 1,
                    "t" if in_inline && phonetic == 0 => {
                        if empty {
                            out.fragments.push(String::new());
                        } else {
                            reading = Reading::Fragment(String::new());
                        }
                    }
                    "v" if cell.is_some() => {
                        let text_result =
                            cell.as_ref().and_then(|c| c.kind.as_deref()) == Some("str");
                        if text_result {
                            if empty {
                                out.fragments.push(String::new());
                            } else {
                                reading = Reading::Fragment(String::new());
                            }
                        } else if !empty {
                            reading = Reading::Value(String::new());
                        }
                    }
                    n if HEADERS_FOOTERS.contains(&n) => {
                        if empty {
                            out.fragments.push(String::new());
                        } else {
                            reading = Reading::Fragment(String::new());
                        }
                    }
                    n if FORMULAS.contains(&n) && !empty => {
                        reading = Reading::Literal(String::new())
                    }
                    "hyperlink" => {
                        for key in ["display", "tooltip", "location"] {
                            out.literal_texts.extend(attribute(e, key));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Reading::Fragment(text) | Reading::Value(text) | Reading::Literal(text) =
                    &mut reading
                {
                    append_text(text, &event)
                        .map_err(|_| xml_error(Office::Excel, part, &reader))?;
                }
            }
            Event::End(e) => {
                let name = e.name();
                match (
                    name.as_ref(),
                    std::mem::replace(&mut reading, Reading::Nothing),
                ) {
                    ("t" | "v", Reading::Fragment(text)) => {
                        let index = out.fragments.len();
                        if let Some(c) = &mut cell {
                            for (offset, ch) in text.chars().enumerate() {
                                c.text.push(ch);
                                c.map.push(Some(FragmentPos {
                                    fragment: index,
                                    offset,
                                }));
                            }
                        }
                        out.fragments.push(text);
                    }
                    ("v", Reading::Value(text)) => {
                        if let Some(c) = &mut cell {
                            c.value = Some(text);
                        }
                    }
                    (n, Reading::Fragment(text)) if HEADERS_FOOTERS.contains(&n) => {
                        let index = out.fragments.len();
                        if !text.trim().is_empty() {
                            let map = (0..text.chars().count())
                                .map(|offset| {
                                    Some(FragmentPos {
                                        fragment: index,
                                        offset,
                                    })
                                })
                                .collect();
                            out.headers.push((text.clone(), map));
                        }
                        out.fragments.push(text);
                    }
                    (n, Reading::Literal(text)) if FORMULAS.contains(&n) => {
                        if !text.is_empty() {
                            out.literal_texts.push(text);
                        }
                    }
                    (_, other) => reading = other,
                }
                match name.as_ref() {
                    "is" => in_inline = false,
                    "rPh" => phonetic = phonetic.saturating_sub(1),
                    "c" => {
                        if let Some(open) = cell.take() {
                            out.cells.extend(finish_cell(open, looks, date1904));
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn finish_cell(cell: OpenCell, looks: &[NumberLook], date1904: bool) -> Option<ReadCell> {
    let value = match cell.kind.as_deref() {
        Some("s") => Value::Shared(cell.value?.trim().parse().ok()?),
        Some("inlineStr") | Some("str")
            if cell.has_inline || cell.kind.as_deref() == Some("str") =>
        {
            Value::Fragments {
                text: cell.text,
                map: cell.map,
            }
        }
        Some("b") => Value::Fixed(
            if cell.value?.trim() == "1" {
                TRUE
            } else {
                FALSE
            }
            .to_owned(),
        ),
        Some("e") => Value::Fixed(cell.value?),
        _ => {
            let raw = cell.value?;
            if raw.trim().is_empty() {
                return None;
            }
            let look = looks.get(cell.style).copied().unwrap_or(NumberLook::Plain);
            Value::Number {
                text: render_number(&raw, look, date1904),
                ordinal: cell.ordinal,
            }
        }
    };
    Some(ReadCell {
        row: cell.row,
        column: cell.column,
        value,
    })
}

/// Fila (desde 1) y columna (desde 0) de una referencia como `B12`.
fn cell_position(reference: &str) -> Option<(usize, usize)> {
    let letters: String = reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let row: usize = reference[letters.len()..].parse().ok()?;
    if letters.is_empty() {
        return None;
    }
    let column = letters.chars().fold(0usize, |acc, c| {
        acc * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
    });
    Some((row, column - 1))
}

/// Textos compartidos y sus fragmentos. Cada `si` es un texto; sus
/// fragmentos son los `t` que no están en una lectura fonética.
fn read_shared_strings(part: &str, xml: &str) -> Result<(Vec<Shared>, Vec<String>)> {
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut fragments = Vec::new();
    let mut current: Option<Shared> = None;
    let mut reading: Option<String> = None;
    let mut phonetic = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| xml_error(Office::Excel, part, &reader))?;
        match &event {
            Event::Start(e) => match e.name().as_ref() {
                "si" => {
                    current = Some(Shared {
                        text: String::new(),
                        map: Vec::new(),
                    })
                }
                "rPh" => phonetic += 1,
                "t" if phonetic == 0 => reading = Some(String::new()),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "si" => strings.push(Shared {
                    text: String::new(),
                    map: Vec::new(),
                }),
                "t" if phonetic == 0 => fragments.push(String::new()),
                _ => {}
            },
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some(text) = &mut reading {
                    append_text(text, &event)
                        .map_err(|_| xml_error(Office::Excel, part, &reader))?;
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "t" => {
                    if let Some(text) = reading.take() {
                        let index = fragments.len();
                        if let Some(s) = &mut current {
                            for (offset, c) in text.chars().enumerate() {
                                s.text.push(c);
                                s.map.push(Some(FragmentPos {
                                    fragment: index,
                                    offset,
                                }));
                            }
                        }
                        fragments.push(text);
                    }
                }
                "rPh" => phonetic = phonetic.saturating_sub(1),
                "si" => strings.extend(current.take()),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok((strings, fragments))
}

/// Reescribe una hoja: fragmentos (textos en línea, resultados de texto,
/// encabezados y pies), números sustituidos, fórmulas y vínculos.
fn rewrite_sheet(
    part: &str,
    xml: &str,
    new_fragments: &HashMap<usize, String>,
    numbers: &HashMap<usize, String>,
    literals: &[(String, String)],
    transform: &dyn Fn(&str, Option<&str>, &str) -> String,
) -> Result<Option<String>> {
    /// Celda abierta: tipo, si tiene fórmula, texto nuevo si es un número
    /// que cambia y su etiqueta de apertura.
    struct OpenCell<'x> {
        kind: Option<String>,
        formula: bool,
        new: Option<String>,
        tag: (usize, usize, BytesStart<'x>),
    }
    /// Elemento con texto abierto.
    enum Open {
        Fragment(usize),
        Number { start: usize },
        Formula,
    }
    let mut reader = Reader::from_str(xml);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut fragment = 0usize;
    let mut ordinal = 0usize;
    let mut phonetic = 0usize;
    let mut in_inline = false;
    let mut cell: Option<OpenCell> = None;
    // Qué es, dónde acaba su etiqueta de apertura y su texto.
    let mut open: Option<(Open, usize, String)> = None;
    loop {
        let before = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|_| xml_error(Office::Excel, part, &reader))?;
        let after = reader.buffer_position() as usize;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name();
                let name = name.as_ref();
                match name {
                    "c" => {
                        let new = numbers.get(&ordinal).cloned();
                        ordinal += 1;
                        if !empty {
                            cell = Some(OpenCell {
                                kind: attribute(&e, "t"),
                                formula: false,
                                new,
                                tag: (before, after, e.clone()),
                            });
                        }
                    }
                    "f" => {
                        if let Some(c) = &mut cell {
                            c.formula = true;
                        }
                        if !empty {
                            open = Some((Open::Formula, after, String::new()));
                        }
                    }
                    "is" if !empty => in_inline = true,
                    "rPh" if !empty => phonetic += 1,
                    "t" if in_inline && phonetic == 0 => {
                        fragment += 1;
                        if !empty {
                            open = Some((Open::Fragment(fragment - 1), after, String::new()));
                        }
                    }
                    "v" => {
                        let text_result =
                            cell.as_ref().and_then(|c| c.kind.as_deref()) == Some("str");
                        if text_result {
                            fragment += 1;
                            if !empty {
                                open = Some((Open::Fragment(fragment - 1), after, String::new()));
                            }
                        } else if cell.as_ref().is_some_and(|c| c.new.is_some()) && !empty {
                            open = Some((Open::Number { start: before }, after, String::new()));
                        }
                    }
                    n if HEADERS_FOOTERS.contains(&n) => {
                        fragment += 1;
                        if !empty {
                            open = Some((Open::Fragment(fragment - 1), after, String::new()));
                        }
                    }
                    n if FORMULAS.contains(&n) && !empty => {
                        open = Some((Open::Formula, after, String::new()));
                    }
                    "hyperlink" => {
                        let mut set: Vec<(&str, String)> = Vec::new();
                        for key in ["display", "tooltip"] {
                            if let Some(value) = attribute(&e, key) {
                                let new = apply_literals(&value, literals);
                                if new != value {
                                    set.push((key, new));
                                }
                            }
                        }
                        if let Some(value) = attribute(&e, "location") {
                            let new = transform("hyperlink", Some("location"), &value);
                            if new != value {
                                set.push(("location", new));
                            }
                        }
                        if !set.is_empty() {
                            let set: Vec<(&str, &str)> =
                                set.iter().map(|(k, v)| (*k, v.as_str())).collect();
                            edits.push((before, after, rebuild(&e, &set, empty)?));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(_) | Event::GeneralRef(_) | Event::CData(_) => {
                if let Some((_, _, text)) = &mut open {
                    append_text(text, &event)
                        .map_err(|_| xml_error(Office::Excel, part, &reader))?;
                }
            }
            Event::End(e) => {
                let name = e.name();
                let name = name.as_ref();
                match name {
                    "is" => in_inline = false,
                    "rPh" => phonetic = phonetic.saturating_sub(1),
                    _ => {}
                }
                if let Some((kind, tag_end, text)) = open.take() {
                    match kind {
                        Open::Fragment(index) => {
                            if let Some(new) = new_fragments.get(&index) {
                                edits.push((tag_end, before, partial_escape(new).into_owned()));
                            }
                        }
                        // Una fórmula conserva su `v` con el texto nuevo; un
                        // número fijo pasa a texto en línea.
                        Open::Number { start } => {
                            if let Some(c) = &cell
                                && let Some(new) = &c.new
                            {
                                if c.formula {
                                    edits.push((tag_end, before, partial_escape(new).into_owned()));
                                } else {
                                    edits.push((start, after, inline_string(new)));
                                }
                            }
                        }
                        Open::Formula => {
                            let new = transform(name, None, &text);
                            if new != text {
                                edits.push((tag_end, before, partial_escape(&new).into_owned()));
                            }
                        }
                    }
                }
                if name == "c"
                    && let Some(c) = cell.take()
                    && c.new.is_some()
                {
                    let kind = if c.formula { "str" } else { "inlineStr" };
                    let (start, end, tag) = c.tag;
                    edits.push((start, end, rebuild(&tag, &[("t", kind)], false)?));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(splice(xml, edits))
}

/// Texto en línea de una celda, con `xml:space` si empieza o acaba en
/// espacio.
fn inline_string(text: &str) -> String {
    let space = if text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) {
        " xml:space=\"preserve\""
    } else {
        ""
    };
    format!("<is><t{space}>{}</t></is>", partial_escape(text))
}

fn is_comments(name: &str) -> bool {
    name.strip_prefix("xl/comments")
        .is_some_and(|rest| rest.ends_with(".xml") && !rest.contains('/'))
}

fn is_drawing(name: &str) -> bool {
    (name.starts_with("xl/drawings/") || name.starts_with("xl/charts/"))
        && name.ends_with(".xml")
        && !name.contains("/_rels/")
}

fn is_pivot_cache(name: &str) -> bool {
    name.starts_with("xl/pivotCache/") && name.ends_with(".xml") && !name.contains("/_rels/")
}

fn is_table(name: &str) -> bool {
    name.starts_with("xl/tables/") && name.ends_with(".xml") && !name.contains("/_rels/")
}

fn is_xl_rels(name: &str) -> bool {
    name.starts_with("xl/") && name.contains("/_rels/") && name.ends_with(".rels")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_rename_sheets_and_replace_only_inside_quotes() {
        let renames = HashMap::from([
            (
                "Nómina Marta".to_owned(),
                "Nómina NOMBRE_PERSONA_0001".to_owned(),
            ),
            ("Marta".to_owned(), "NOMBRE_PERSONA_0001".to_owned()),
        ]);
        let literals = vec![("Marta".to_owned(), "[NOMBRE_PERSONA_0001]".to_owned())];
        assert_eq!(
            rewrite_formula(
                "'Nómina Marta'!A1+Marta!B2&\"Hola Marta\"&MARTA(1)",
                &renames,
                &literals
            ),
            "'Nómina NOMBRE_PERSONA_0001'!A1+'NOMBRE_PERSONA_0001'!B2&\"Hola [NOMBRE_PERSONA_0001]\"&MARTA(1)"
        );
        assert_eq!(
            rewrite_formula("\"a\"\"b\"", &renames, &literals),
            "\"a\"\"b\""
        );
        assert_eq!(
            rewrite_formula("'O''Brien'!A1", &HashMap::new(), &[]),
            "'O''Brien'!A1"
        );
    }

    #[test]
    fn dates_and_big_numbers_are_shown_as_excel_shows_them() {
        assert_eq!(
            render_number("45702", NumberLook::Date, false),
            "14/02/2025"
        );
        assert_eq!(
            render_number("45702.5", NumberLook::DateTime, false),
            "14/02/2025 12:00"
        );
        assert_eq!(render_number("0.25", NumberLook::Time, false), "06:00:00");
        assert_eq!(render_number("1", NumberLook::Date, true), "02/01/1904");
        assert_eq!(
            render_number("4.5320151128303E+15", NumberLook::Plain, false),
            "4532015112830300"
        );
        assert_eq!(render_number("1250.5", NumberLook::Plain, false), "1250.5");
        assert_eq!(look_of(164, Some("dd/mm/yyyy")), NumberLook::Date);
        assert_eq!(look_of(165, Some("#,##0.00 \"€\"")), NumberLook::Plain);
        assert_eq!(look_of(166, Some("[h]:mm")), NumberLook::Time);
    }

    #[test]
    fn cell_references_are_read() {
        assert_eq!(cell_position("B12"), Some((12, 1)));
        assert_eq!(cell_position("AA3"), Some((3, 26)));
        assert_eq!(cell_position("12"), None);
    }
}
