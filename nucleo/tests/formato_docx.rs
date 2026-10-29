//! Extractor DOCX con mapa de fragmentos (PRD, «Extracción con mapa»).
//! Casi todo se comprueba con el corpus sintético de `tests/corpus/docx`.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use nucleo::Error;
use nucleo::formatos::docx::{DocxDocument, ParagraphStyle, Segment, TableCell};

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(name)
}

fn open(name: &str) -> DocxDocument {
    DocxDocument::from_bytes(&std::fs::read(corpus(&format!("{name}.docx"))).unwrap()).unwrap()
}

fn expected(name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(corpus(&format!("{name}.json"))).unwrap()).unwrap()
}

const CORPUS: [&str; 4] = [
    "es-01-contrato-arrendamiento",
    "ca-01-informe-auditoria",
    "en-01-employment-review",
    "fr-01-fiche-client",
];

fn texts<'a>(segments: impl IntoIterator<Item = &'a Segment>) -> Vec<&'a str> {
    segments.into_iter().map(|s| s.text.as_str()).collect()
}

fn in_part<'a>(document: &'a DocxDocument, part: &str) -> Vec<&'a Segment> {
    document
        .segments()
        .iter()
        .filter(|s| s.part == part)
        .collect()
}

fn strings(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

/// DOCX mínimo con el cuerpo dado y, si se pasa, una hoja de estilos.
fn docx_with(body: &str, styles: Option<&str>) -> Vec<u8> {
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let mut parts = vec![
        ("[Content_Types].xml", "<Types/>".to_owned()),
        (
            "word/document.xml",
            format!("<w:document {w}><w:body>{body}</w:body></w:document>"),
        ),
    ];
    if let Some(styles) = styles {
        parts.push((
            "word/styles.xml",
            format!("<w:styles {w}>{styles}</w:styles>"),
        ));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn each_paragraph_of_the_body_is_a_segment_in_order() {
    let document = open("es-01-contrato-arrendamiento");
    assert_eq!(
        texts(in_part(&document, "word/document.xml")),
        [
            "Contrato de arrendamiento de vivienda",
            "En Valencia, a 3 de marzo de 2026, de una parte doña Lucía Ferrer Ibáñez, con DNI 48291736K, como arrendadora.",
            "Datos de contacto",
            "Teléfono: 612 384 907",
            "Correo: lucia.ferrer@correo-ejemplo.es",
            "Condiciones",
            "Renta mensual de 950 euros.",
            "Fianza ingresada en la cuenta ES91 2100 0418 4502 0005 1332.",
            "Parte",
            "Nombre",
            "DNI",
            "Arrendadora",
            "Lucía Ferrer Ibáñez",
            "48291736K",
            "Arrendatario",
            "Pablo Navarro Gil",
            "53720914R",
            "Para cualquier duda escriba a pablo.navarro@correo-ejemplo.es.",
            "Firma: Pablo Navarro Gil",
            "Domicilio: Calle de la Paz 14, 3.º B 46003 Valencia",
        ]
    );
}

#[test]
fn title_subject_keywords_and_description_are_segments_too() {
    let document = open("es-01-contrato-arrendamiento");
    // La descripción está vacía: no hay nada que analizar.
    assert_eq!(
        texts(in_part(&document, "docProps/core.xml")),
        [
            "Contrato de arrendamiento con Lucía Ferrer Ibáñez",
            "Arrendamiento",
            "contrato; vivienda"
        ]
    );
    let last = document.segments().last().unwrap();
    assert_eq!(last.style, ParagraphStyle::Metadata);
}

#[test]
fn every_expected_entity_is_in_the_extracted_text() {
    for name in CORPUS {
        let document = open(name);
        let all = texts(document.segments()).join("\n");
        for entity in expected(name)["entidades"].as_array().unwrap() {
            let text = entity["texto"].as_str().unwrap();
            assert!(all.contains(text), "{name}: falta «{text}» en\n{all}");
        }
    }
}

#[test]
fn every_mapped_character_points_to_its_fragment() {
    for name in CORPUS {
        let document = open(name);
        for segment in document.segments() {
            let chars: Vec<char> = segment.text.chars().collect();
            assert_eq!(chars.len(), segment.map.len(), "{name}: {:?}", segment.text);
            for (c, position) in chars.iter().zip(&segment.map) {
                match position {
                    Some(p) => {
                        let fragment = document.fragment(&segment.part, p.fragment);
                        assert_eq!(
                            fragment.chars().nth(p.offset),
                            Some(*c),
                            "{name}: {:?}",
                            segment.text
                        );
                    }
                    // Tabuladores y saltos: un espacio sin fragmento.
                    None => assert_eq!(*c, ' ', "{name}: {:?}", segment.text),
                }
            }
        }
    }
}

#[test]
fn a_name_split_in_three_fragments_maps_to_the_three() {
    let document = open("es-01-contrato-arrendamiento");
    let segment = &in_part(&document, "word/document.xml")[1];
    let start = segment.text.find("Lucía").unwrap();
    let start = segment.text[..start].chars().count();
    let name: Vec<_> = segment.map[start..start + "Lucía Ferrer Ibáñez".chars().count()]
        .iter()
        .map(|p| p.unwrap())
        .collect();
    let fragments: Vec<usize> = name
        .iter()
        .map(|p| p.fragment)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(fragments.len(), 3, "{name:?}");
    let parts: Vec<&str> = fragments
        .iter()
        .map(|&f| document.fragment("word/document.xml", f))
        .collect();
    assert_eq!(parts, ["Lucía ", "Fer", "rer Ibáñez"]);
    assert_eq!(name[0].offset, 0);
}

#[test]
fn tabs_and_breaks_are_unmapped_spaces() {
    let document = open("es-01-contrato-arrendamiento");
    let body = in_part(&document, "word/document.xml");
    let signature = body.iter().find(|s| s.text.starts_with("Firma:")).unwrap();
    assert_eq!(signature.map[6], None);
    assert!(signature.map[5].is_some() && signature.map[7].is_some());
    let address = body
        .iter()
        .find(|s| s.text.starts_with("Domicilio:"))
        .unwrap();
    let space = address.text.find(" 46003").unwrap();
    assert_eq!(address.map[address.text[..space].chars().count()], None);
}

#[test]
fn headings_lists_and_table_cells_are_recorded() {
    let document = open("es-01-contrato-arrendamiento");
    let body = in_part(&document, "word/document.xml");
    assert_eq!(body[0].style, ParagraphStyle::Heading(1));
    assert_eq!(body[2].style, ParagraphStyle::Heading(2));
    assert_eq!(
        body[3].style,
        ParagraphStyle::ListItem {
            ordered: false,
            level: 0
        }
    );
    assert_eq!(
        body[6].style,
        ParagraphStyle::ListItem {
            ordered: true,
            level: 0
        }
    );
    assert_eq!(body[1].style, ParagraphStyle::Normal);
    assert_eq!(body[1].cell, None);
    assert_eq!(
        body[12].cell,
        Some(TableCell {
            table: 0,
            row: 1,
            column: 1
        })
    );
    assert_eq!(
        body[16].cell,
        Some(TableCell {
            table: 0,
            row: 2,
            column: 2
        })
    );
}

#[test]
fn headers_footers_and_notes_are_their_own_parts() {
    let document = open("ca-01-informe-auditoria");
    assert_eq!(
        texts(in_part(&document, "word/header1.xml")),
        ["Informe confidencial, redactat per Jordi Puig Soler"]
    );
    assert_eq!(
        texts(in_part(&document, "word/footer1.xml")),
        ["Contacte: 677 123 450"]
    );
    assert_eq!(
        texts(in_part(&document, "word/footnotes.xml")),
        [" Segons l'acta signada per Núria Vidal Roca."]
    );
    assert_eq!(
        texts(in_part(&document, "word/endnotes.xml")),
        [" Expedient enviat a nuria.vidal@correu-exemple.cat."]
    );
}

#[test]
fn comments_and_tracked_changes_are_extracted() {
    let document = open("en-01-employment-review");
    assert_eq!(
        texts(in_part(&document, "word/comments.xml")),
        [" Please confirm the phone number +44 7700 900123 with HR."]
    );
    let body = texts(in_part(&document, "word/document.xml"));
    assert!(
        body.contains(&"Reviewed by Oliver Hart Emma Clarke."),
        "{body:?}"
    );
}

#[test]
fn a_text_box_is_its_own_segment() {
    let document = open("fr-01-fiche-client");
    assert_eq!(
        texts(in_part(&document, "word/document.xml")),
        [
            "Fiche client",
            "Cliente : Camille Lefèvre, née à Lyon.",
            "Contact : camille.lefevre@exemple-courriel.fr, 06 12 34 56 78",
            "Adresse : 15 rue des Lilas, 69003 Lyon",
            "IBAN : FR76 3000 6000 0112 3456 7890 189",
        ]
    );
}

#[test]
fn images_and_embedded_objects_are_counted() {
    for name in CORPUS {
        let warnings = open(name).warnings();
        let json = expected(name);
        assert_eq!(warnings.images as u64, json["imagenes"], "{name}");
        assert_eq!(warnings.objects as u64, json["objetos"], "{name}");
    }
}

#[test]
fn author_metadata_and_revision_authors_are_collected() {
    for name in CORPUS {
        let document = open(name);
        let json = expected(name);
        let authors: BTreeSet<String> = document.author_metadata().iter().cloned().collect();
        assert_eq!(authors, strings(&json["metadatos_autor"]), "{name}");
        let revisions: BTreeSet<String> = document.revision_authors().iter().cloned().collect();
        assert_eq!(revisions, strings(&json["autores_revision"]), "{name}");
    }
}

#[test]
fn link_targets_and_field_codes_are_collected() {
    let document = open("es-01-contrato-arrendamiento");
    let links = document.external_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].rels_part, "word/_rels/document.xml.rels");
    assert_eq!(links[0].id, "rIdEnlace1");
    assert_eq!(links[0].target, "mailto:lucia.ferrer@correo-ejemplo.es");
    let codes = document.field_codes();
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].part, "word/document.xml");
    assert_eq!(
        codes[0].text,
        " HYPERLINK \"mailto:pablo.navarro@correo-ejemplo.es\" "
    );
}

#[test]
fn xml_entities_are_decoded_in_the_text_and_the_map() {
    let bytes = docx_with(
        "<w:p><w:r><w:t>Smith &amp; Wesson, Jos&#233; &lt;3</w:t></w:r></w:p>",
        None,
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let segment = &document.segments()[0];
    assert_eq!(segment.text, "Smith & Wesson, José <3");
    assert_eq!(
        document.fragment("word/document.xml", 0),
        "Smith & Wesson, José <3"
    );
    assert!(
        segment
            .map
            .iter()
            .enumerate()
            .all(|(i, p)| p.unwrap().offset == i)
    );
}

#[test]
fn headings_are_found_by_style_name_whatever_the_style_id() {
    // En un Word en castellano, el estilo «heading 1» se llama «Ttulo1».
    let bytes = docx_with(
        "<w:p><w:pPr><w:pStyle w:val=\"Ttulo1\"/></w:pPr><w:r><w:t>Cláusulas</w:t></w:r></w:p>",
        Some(
            "<w:style w:type=\"paragraph\" w:styleId=\"Ttulo1\"><w:name w:val=\"heading 1\"/></w:style>",
        ),
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    assert_eq!(document.segments()[0].style, ParagraphStyle::Heading(1));
}

#[test]
fn empty_paragraphs_are_not_segments() {
    let bytes = docx_with(
        "<w:p/><w:p><w:r><w:t xml:space=\"preserve\">   </w:t></w:r></w:p><w:p><w:r><w:tab/></w:r></w:p><w:p><w:r><w:t>Texto</w:t></w:r></w:p>",
        None,
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    assert_eq!(texts(document.segments()), ["Texto"]);
    // Los fragmentos se cuentan igual: el de los espacios es el 0.
    assert_eq!(document.segments()[0].map[0].unwrap().fragment, 1);
}

#[test]
fn a_protected_docx_is_a_clear_error() {
    let bytes = std::fs::read(corpus("es-02-protegido.docx")).unwrap();
    let error = DocxDocument::from_bytes(&bytes).unwrap_err();
    assert!(matches!(error, Error::DocxProtected), "{error:?}");
    assert!(error.to_string().contains("contraseña"), "{error}");
}

#[test]
fn something_that_is_not_a_docx_is_rejected() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("otra-cosa.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"hola").unwrap();
    let no_document = zip.finish().unwrap().into_inner();
    let broken_xml = docx_with("<w:p><w:r><w:t>Ana Gil</w:r></w:p>", None);
    for bytes in [b"no es un zip".to_vec(), no_document, broken_xml] {
        let error = DocxDocument::from_bytes(&bytes).unwrap_err();
        assert!(matches!(error, Error::DocxInvalid(_)), "{error:?}");
        assert!(!error.to_string().contains("Ana"), "{error}");
    }
}
