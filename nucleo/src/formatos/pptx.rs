//! PPTX: edición del XML original con mapa de fragmentos, como en DOCX.
//!
//! Las diapositivas se leen en el orden de la presentación. Detrás de cada
//! una van sus diagramas, sus gráficos y sus notas, para que el modelo lea
//! juntos los textos que se ven juntos. Después, las plantillas (patrones y
//! diseños, que pueden llevar un pie con un nombre), los comentarios y el
//! resto de diagramas y gráficos. Cada párrafo de DrawingML es un segmento.
//!
//! Los valores en caché de los gráficos, el texto alternativo de imágenes y
//! formas, los destinos de los enlaces y los títulos de `docProps/app.xml`
//! reciben las sustituciones literales. Se vacían los metadatos de autor y
//! los autores de los comentarios.

use std::collections::{HashMap, HashSet};

use crate::Result;
use crate::formatos::ooxml::drawingml::{self, ALT_TEXTS, BREAKS, CACHED_VALUES, FRAGMENTS};
use crate::formatos::ooxml::reescritura::{Rules, rewrite, sorted_literals, write_package};
use crate::formatos::ooxml::texto::{escape_start, markdown_table};
use crate::formatos::ooxml::{
    APP_AUTHOR_FIELDS, APP_PART, CORE_AUTHOR_FIELDS, CORE_PART, CORE_TEXT_FIELDS, CUSTOM_PART,
    ExternalLink, Office, PROPERTY_TEXTS, Package, Warnings, attribute_values, element_texts,
    external_links, push_unique, relationships, rels_of,
};
use crate::formatos::{Replacement, order_for_writing};

pub use crate::formatos::ooxml::drawingml::{FragmentPos, TableCell};

const PRESENTATION_PART: &str = "ppt/presentation.xml";
const COMMENT_AUTHORS_PART: &str = "ppt/commentAuthors.xml";
const AUTHORS_PART: &str = "ppt/authors.xml";
/// Carpetas con partes de DrawingML que no cuelgan de una diapositiva.
const OTHER_FOLDERS: [&str; 7] = [
    "ppt/slideMasters/",
    "ppt/slideLayouts/",
    "ppt/notesMasters/",
    "ppt/handoutMasters/",
    "ppt/comments/",
    "ppt/diagrams/",
    "ppt/charts/",
];

/// Dónde está un segmento, para escribir MD y TXT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// En la diapositiva `n`, empezando en 1, o en un diagrama o un gráfico
    /// suyo.
    Slide(usize),
    /// En las notas de la diapositiva `n`.
    Notes(usize),
    /// En plantillas, comentarios, metadatos o partes sin diapositiva. No
    /// sale en MD ni en TXT.
    Other,
}

/// Párrafo con texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxSegment {
    pub text: String,
    pub part: String,
    pub place: Place,
    /// Si es el título de la diapositiva.
    pub title: bool,
    pub cell: Option<TableCell>,
    pub map: Vec<Option<FragmentPos>>,
    /// Saltos de línea: posición en `text` y número del salto en la parte.
    pub breaks: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct PptxDocument {
    original: Vec<u8>,
    segments: Vec<PptxSegment>,
    /// Fragmentos de cada parte con texto.
    fragments: HashMap<String, Vec<String>>,
    links: Vec<ExternalLink>,
    literal_texts: Vec<String>,
    author_metadata: Vec<String>,
    comment_authors: Vec<String>,
    warnings: Warnings,
}

impl PptxDocument {
    pub fn from_bytes(bytes: &[u8]) -> Result<PptxDocument> {
        let mut package = Package::open(bytes, Office::PowerPoint)?;
        let names = package.names().to_vec();
        let mut document = PptxDocument {
            original: bytes.to_vec(),
            segments: Vec::new(),
            fragments: HashMap::new(),
            links: Vec::new(),
            literal_texts: Vec::new(),
            author_metadata: Vec::new(),
            comment_authors: Vec::new(),
            warnings: Warnings::default(),
        };

        // Diapositivas en el orden de la presentación, con lo que cuelga de
        // cada una.
        let mut done: HashSet<String> = HashSet::new();
        for (index, slide) in slide_parts(&mut package)?.into_iter().enumerate() {
            let number = index + 1;
            let rels = match package.read(&rels_of(&slide))? {
                Some(xml) => relationships(Office::PowerPoint, &rels_of(&slide), &xml)?,
                None => Vec::new(),
            };
            let related = |kind: &str| -> Vec<String> {
                rels.iter()
                    .filter(|(_, k, _)| k == kind)
                    .map(|(_, _, target)| target.clone())
                    .collect()
            };
            let mut parts = vec![(slide.clone(), Place::Slide(number))];
            for target in related("diagramData").into_iter().chain(related("chart")) {
                parts.push((target, Place::Slide(number)));
            }
            for target in related("notesSlide") {
                parts.push((target, Place::Notes(number)));
            }
            for (part, place) in parts {
                if done.insert(part.clone())
                    && let Some(xml) = package.read(&part)?
                {
                    let images = document.add_part(&part, &xml, place)?;
                    if matches!(place, Place::Slide(_)) {
                        document.warnings.images += images;
                    }
                }
            }
        }
        // Lo demás con DrawingML.
        for name in &names {
            let other = name.ends_with(".xml")
                && !name.contains("/_rels/")
                && (OTHER_FOLDERS.iter().any(|f| name.starts_with(f))
                    || name.starts_with("ppt/slides/")
                    || name.starts_with("ppt/notesSlides/"));
            if other
                && done.insert(name.clone())
                && let Some(xml) = package.read(name)?
            {
                document.add_part(name, &xml, Place::Other)?;
            }
        }

        if let Some(xml) = package.read(CORE_PART)? {
            document.add_core(&xml)?;
        }
        if let Some(xml) = package.read(APP_PART)? {
            for (_, text) in element_texts(Office::PowerPoint, APP_PART, &xml, &APP_AUTHOR_FIELDS)?
            {
                push_unique(&mut document.author_metadata, text);
            }
        }
        for (part, element, keys) in [
            (
                COMMENT_AUTHORS_PART,
                "p:cmAuthor",
                &["name", "initials"][..],
            ),
            (
                AUTHORS_PART,
                "p188:author",
                &["name", "initials", "userId"][..],
            ),
        ] {
            if let Some(xml) = package.read(part)? {
                for key in keys {
                    for value in attribute_values(Office::PowerPoint, part, &xml, element, key)? {
                        push_unique(&mut document.comment_authors, value);
                    }
                }
            }
        }
        for name in names.iter().filter(|n| is_ppt_rels(n)) {
            if let Some(xml) = package.read(name)? {
                document
                    .links
                    .extend(external_links(Office::PowerPoint, name, &xml)?);
            }
        }
        document.warnings.objects = names
            .iter()
            .filter(|n| n.starts_with("ppt/embeddings/"))
            .count();
        Ok(document)
    }

    /// Segmentos de una parte de DrawingML. Devuelve las imágenes que tiene.
    fn add_part(&mut self, part: &str, xml: &str, place: Place) -> Result<usize> {
        let found = drawingml::extract(Office::PowerPoint, part, xml)?;
        for p in found.paragraphs {
            self.segments.push(PptxSegment {
                text: p.text,
                part: part.to_owned(),
                place,
                title: p.title,
                cell: p.cell,
                map: p.map,
                breaks: p.breaks,
            });
        }
        self.literal_texts.extend(found.literal_texts);
        self.fragments.insert(part.to_owned(), found.fragments);
        Ok(found.images)
    }

    /// Título, asunto, palabras clave y descripción como segmentos, y los
    /// metadatos de autor.
    fn add_core(&mut self, xml: &str) -> Result<()> {
        let names = [&CORE_TEXT_FIELDS[..], &CORE_AUTHOR_FIELDS[..]].concat();
        let mut fragments = Vec::new();
        for (name, text) in element_texts(Office::PowerPoint, CORE_PART, xml, &names)? {
            if CORE_AUTHOR_FIELDS.contains(&name.as_str()) {
                push_unique(&mut self.author_metadata, text);
                continue;
            }
            let index = fragments.len();
            if !text.trim().is_empty() {
                self.segments.push(PptxSegment {
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
                    place: Place::Other,
                    title: false,
                    cell: None,
                    breaks: Vec::new(),
                });
            }
            fragments.push(text);
        }
        self.fragments.insert(CORE_PART.to_owned(), fragments);
        Ok(())
    }

    /// Párrafos en orden: diapositivas con sus diagramas, gráficos y notas;
    /// después plantillas, comentarios y metadatos.
    pub fn segments(&self) -> &[PptxSegment] {
        &self.segments
    }

    pub fn external_links(&self) -> &[ExternalLink] {
        &self.links
    }

    /// Textos que solo admiten sustitución literal: destinos de enlaces,
    /// valores en caché de los gráficos y texto alternativo.
    pub fn literal_texts(&self) -> Vec<&str> {
        self.links
            .iter()
            .map(|l| l.target.as_str())
            .chain(self.literal_texts.iter().map(String::as_str))
            .collect()
    }

    /// Autor, último en modificar, empresa y responsable, sin repetir. Se
    /// vacían al escribir.
    pub fn author_metadata(&self) -> &[String] {
        &self.author_metadata
    }

    /// Nombres, iniciales e identificadores de quienes han comentado. Se
    /// vacían al escribir.
    pub fn comment_authors(&self) -> &[String] {
        &self.comment_authors
    }

    pub fn warnings(&self) -> Warnings {
        self.warnings
    }

    /// La presentación con las sustituciones aplicadas sobre los segmentos y
    /// las literales (`valor`, `nuevo`) en lo demás.
    pub fn write(
        &self,
        replacements: &[Replacement],
        literals: &[(String, String)],
    ) -> Result<Vec<u8>> {
        let (fragments, breaks) = self.apply(replacements)?;
        let literals = sorted_literals(literals);
        let none = HashMap::new();
        let rules_for = |name: &str| -> Option<Rules> {
            if name == CORE_PART {
                Some(Rules {
                    fragments: &CORE_TEXT_FIELDS,
                    new_fragments: Some(fragments.get(name).unwrap_or(&none)),
                    blank: &CORE_AUTHOR_FIELDS,
                    ..Rules::default()
                })
            } else if self.fragments.contains_key(name) {
                Some(Rules {
                    fragments: &FRAGMENTS,
                    new_fragments: Some(fragments.get(name).unwrap_or(&none)),
                    removed_breaks: breaks.get(name),
                    breaks: &BREAKS,
                    literal: &CACHED_VALUES,
                    literal_attributes: &ALT_TEXTS,
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == APP_PART || name == CUSTOM_PART {
                Some(Rules {
                    blank: &APP_AUTHOR_FIELDS,
                    literal: &PROPERTY_TEXTS,
                    literals: &literals,
                    ..Rules::default()
                })
            } else if is_ppt_rels(name) {
                Some(Rules {
                    link_targets: true,
                    literals: &literals,
                    ..Rules::default()
                })
            } else if name == COMMENT_AUTHORS_PART {
                Some(Rules {
                    blank_attributes: &["name", "initials"],
                    ..Rules::default()
                })
            } else if name == AUTHORS_PART {
                Some(Rules {
                    blank_attributes: &["name", "initials", "userId"],
                    ..Rules::default()
                })
            } else {
                None
            }
        };
        write_package(
            &self.original,
            Office::PowerPoint,
            |name| rules_for(name).is_some(),
            |name, xml| match rules_for(name) {
                Some(rules) => rewrite(Office::PowerPoint, name, xml, &rules),
                None => Ok(None),
            },
        )
    }

    /// Texto nuevo de cada fragmento que cambia y saltos que se borran, por
    /// parte. Como en DOCX: el marcador va en el primer fragmento cubierto y
    /// de los siguientes se borra la parte cubierta.
    #[allow(clippy::type_complexity)]
    fn apply(
        &self,
        replacements: &[Replacement],
    ) -> Result<(
        HashMap<&str, HashMap<usize, String>>,
        HashMap<&str, HashSet<usize>>,
    )> {
        let segments: Vec<SegmentMap> = self
            .segments
            .iter()
            .map(|s| (s.part.as_str(), s.map.as_slice(), s.breaks.as_slice()))
            .collect();
        apply_to_fragments(&segments, replacements, |part, index| {
            self.fragments[part][index].as_str()
        })
    }

    /// Markdown: una sección por diapositiva, con el título como
    /// encabezado, las tablas como tablas y las notas al final.
    pub fn markdown(&self, replacements: &[Replacement]) -> Result<String> {
        let mut sections = Vec::new();
        for (number, slide, notes) in self.slides(replacements)? {
            let mut blocks = vec![format!("## Diapositiva {number}")];
            blocks.extend(markdown_blocks(&slide));
            if !notes.is_empty() {
                blocks.push("#### Notas".to_owned());
                blocks.extend(markdown_blocks(&notes));
            }
            sections.push(blocks.join("\n\n"));
        }
        Ok(join_sections(&sections, "\n\n"))
    }

    /// Texto: una línea por párrafo, las celdas de una fila separadas por
    /// tabuladores y una línea en blanco entre diapositivas.
    pub fn plain_text(&self, replacements: &[Replacement]) -> Result<String> {
        let mut sections = Vec::new();
        for (number, slide, notes) in self.slides(replacements)? {
            let mut lines = vec![format!("Diapositiva {number}")];
            lines.extend(text_lines(&slide));
            if !notes.is_empty() {
                lines.push("Notas:".to_owned());
                lines.extend(text_lines(&notes));
            }
            sections.push(lines.join("\n"));
        }
        Ok(join_sections(&sections, "\n\n"))
    }

    /// Párrafos visibles de cada diapositiva y de sus notas, ya sustituidos.
    #[allow(clippy::type_complexity)]
    fn slides(
        &self,
        replacements: &[Replacement],
    ) -> Result<Vec<(usize, Vec<Visible>, Vec<Visible>)>> {
        let texts = replaced_texts(self.segments.iter().map(|s| s.text.as_str()), replacements)?;
        let mut slides: Vec<(usize, Vec<Visible>, Vec<Visible>)> = Vec::new();
        for (segment, text) in self.segments.iter().zip(texts) {
            let text = text.trim().to_owned();
            if text.is_empty() {
                continue;
            }
            let (number, notes) = match segment.place {
                Place::Slide(n) => (n, false),
                Place::Notes(n) => (n, true),
                Place::Other => continue,
            };
            if slides.last().is_none_or(|(n, _, _)| *n != number) {
                slides.push((number, Vec::new(), Vec::new()));
            }
            let (_, slide, slide_notes) = slides.last_mut().expect("recién añadida");
            let visible = Visible {
                text,
                title: segment.title,
                cell: segment.cell.map(|c| (segment.part.clone(), c)),
            };
            if notes {
                slide_notes.push(visible)
            } else {
                slide.push(visible)
            }
        }
        Ok(slides)
    }
}

/// Párrafo tal como se ve, ya sustituido.
struct Visible {
    text: String,
    title: bool,
    /// Parte y celda: dos tablas de partes distintas no se juntan.
    cell: Option<(String, TableCell)>,
}

/// Bloques Markdown: títulos, párrafos y tablas.
fn markdown_blocks(paragraphs: &[Visible]) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < paragraphs.len() {
        let p = &paragraphs[i];
        if let Some((part, cell)) = &p.cell {
            let count = paragraphs[i..]
                .iter()
                .take_while(|q| {
                    q.cell
                        .as_ref()
                        .is_some_and(|(qp, qc)| qp == part && qc.table == cell.table)
                })
                .count();
            blocks.push(markdown_table(&grid(&paragraphs[i..i + count])));
            i += count;
            continue;
        }
        blocks.push(if p.title {
            format!("### {}", p.text)
        } else {
            escape_start(&p.text)
        });
        i += 1;
    }
    blocks
}

/// Líneas de texto: las tablas, una fila por línea con las celdas separadas
/// por tabuladores.
fn text_lines(paragraphs: &[Visible]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut i = 0;
    while i < paragraphs.len() {
        let p = &paragraphs[i];
        if let Some((part, cell)) = &p.cell {
            let count = paragraphs[i..]
                .iter()
                .take_while(|q| {
                    q.cell
                        .as_ref()
                        .is_some_and(|(qp, qc)| qp == part && qc.table == cell.table)
                })
                .count();
            lines.extend(
                grid(&paragraphs[i..i + count])
                    .into_iter()
                    .map(|row| row.join("\t")),
            );
            i += count;
            continue;
        }
        lines.push(p.text.clone());
        i += 1;
    }
    lines
}

/// Filas y columnas de una tabla. Los párrafos de una celda se juntan con un
/// espacio.
fn grid(paragraphs: &[Visible]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for p in paragraphs {
        let (_, cell) = p.cell.as_ref().expect("solo párrafos de tabla");
        if rows.len() <= cell.row {
            rows.resize(cell.row + 1, Vec::new());
        }
        let row = &mut rows[cell.row];
        if row.len() <= cell.column {
            row.resize(cell.column + 1, String::new());
        }
        let text = &mut row[cell.column];
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&p.text);
    }
    rows
}

fn join_sections(sections: &[String], separator: &str) -> String {
    if sections.is_empty() {
        String::new()
    } else {
        sections.join(separator) + "\n"
    }
}

/// El texto de cada segmento con sus sustituciones.
pub(crate) fn replaced_texts<'a>(
    texts: impl Iterator<Item = &'a str>,
    replacements: &[Replacement],
) -> Result<Vec<String>> {
    let mut chars: Vec<Vec<char>> = texts.map(|t| t.chars().collect()).collect();
    let lengths: Vec<usize> = chars.iter().map(Vec::len).collect();
    for r in order_for_writing(replacements, &lengths)? {
        chars[r.segment].splice(r.start..r.end, r.text.chars());
    }
    Ok(chars.into_iter().map(|c| c.into_iter().collect()).collect())
}

/// Un segmento visto desde el escritor: su parte, su mapa a los fragmentos
/// y sus saltos.
pub(crate) type SegmentMap<'a> = (&'a str, &'a [Option<FragmentPos>], &'a [(usize, usize)]);

/// Texto nuevo de cada fragmento que cambia y saltos que se borran, por
/// parte. `segments` da, por segmento, su parte, su mapa y sus saltos.
#[allow(clippy::type_complexity)]
pub(crate) fn apply_to_fragments<'a>(
    segments: &[SegmentMap<'a>],
    replacements: &[Replacement],
    fragment: impl Fn(&str, usize) -> &'a str,
) -> Result<(
    HashMap<&'a str, HashMap<usize, String>>,
    HashMap<&'a str, HashSet<usize>>,
)> {
    let lengths: Vec<usize> = segments.iter().map(|s| s.1.len()).collect();
    let mut changed: HashMap<&str, HashMap<usize, Vec<char>>> = HashMap::new();
    let mut removed: HashMap<&str, HashSet<usize>> = HashMap::new();
    for r in order_for_writing(replacements, &lengths)? {
        let (part, map, breaks) = segments[r.segment];
        let positions: Vec<FragmentPos> = map[r.start..r.end].iter().flatten().copied().collect();
        let Some(first) = positions.first().copied() else {
            return Err(crate::Error::InvalidReplacement(format!(
                "los caracteres {}..{} del segmento {} no tienen texto que sustituir",
                r.start, r.end, r.segment
            )));
        };
        let gone = removed.entry(part).or_default();
        for &(at, index) in breaks {
            if (r.start..r.end).contains(&at) {
                gone.insert(index);
            }
        }
        let fragments = changed.entry(part).or_default();
        for group in positions.chunk_by(|a, b| a.fragment == b.fragment) {
            let index = group[0].fragment;
            let chars = fragments
                .entry(index)
                .or_insert_with(|| fragment(part, index).chars().collect());
            let text = if index == first.fragment {
                r.text.as_str()
            } else {
                ""
            };
            chars.splice(
                group[0].offset..=group[group.len() - 1].offset,
                text.chars(),
            );
        }
    }
    let fragments = changed
        .into_iter()
        .map(|(part, fragments)| {
            let fragments = fragments
                .into_iter()
                .map(|(index, chars)| (index, chars.into_iter().collect::<String>()))
                .filter(|(index, text)| text != fragment(part, *index))
                .collect();
            (part, fragments)
        })
        .collect();
    Ok((fragments, removed))
}

/// Diapositivas en el orden de la presentación.
fn slide_parts(package: &mut Package) -> Result<Vec<String>> {
    let rels_name = rels_of(PRESENTATION_PART);
    let rels = match package.read(&rels_name)? {
        Some(xml) => relationships(Office::PowerPoint, &rels_name, &xml)?,
        None => Vec::new(),
    };
    let presentation = package.read(PRESENTATION_PART)?.unwrap_or_default();
    let ids = attribute_values(
        Office::PowerPoint,
        PRESENTATION_PART,
        &presentation,
        "p:sldId",
        "r:id",
    )?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            rels.iter()
                .find(|(rid, kind, _)| rid == id && kind == "slide")
                .map(|(_, _, target)| target.clone())
        })
        .collect())
}

fn is_ppt_rels(name: &str) -> bool {
    name.starts_with("ppt/") && name.contains("/_rels/") && name.ends_with(".rels")
}
