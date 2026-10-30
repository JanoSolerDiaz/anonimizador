//! Escritor DOCX (PRD, «Escritura»): sustituciones sobre el XML original con
//! el mapa de fragmentos, metadatos y autores vacíos, sustitución literal en
//! enlaces y códigos de campo, y el resto del paquete copiado tal cual.

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use nucleo::Error;
use nucleo::formatos::Replacement;
use nucleo::formatos::docx::DocxDocument;

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(name)
}

fn original(name: &str) -> Vec<u8> {
    std::fs::read(corpus(&format!("{name}.docx"))).unwrap()
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

/// Entradas del ZIP en orden: nombre, método de compresión y contenido.
fn entries(bytes: &[u8]) -> Vec<(String, zip::CompressionMethod, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            (file.name().unwrap().into_owned(), file.compression(), data)
        })
        .collect()
}

fn part(bytes: &[u8], name: &str) -> String {
    let (_, _, data) = entries(bytes)
        .into_iter()
        .find(|e| e.0 == name)
        .unwrap_or_else(|| panic!("falta {name}"));
    String::from_utf8(data).unwrap()
}

fn segment_index(document: &DocxDocument, part: &str, contains: &str) -> usize {
    document
        .segments()
        .iter()
        .position(|s| s.part == part && s.text.contains(contains))
        .unwrap_or_else(|| panic!("ningún segmento de {part} con {contains:?}"))
}

/// Posición en caracteres de `needle` dentro de `text`.
fn char_range(text: &str, needle: &str) -> (usize, usize) {
    let byte = text.find(needle).unwrap();
    let start = text[..byte].chars().count();
    (start, start + needle.chars().count())
}

/// Una sustitución por cada aparición de cada entidad esperada, las largas
/// primero y sin solapes, y el texto que debe quedar en cada segmento.
fn replace_entities(
    document: &DocxDocument,
    entities: &[(String, String)],
) -> (Vec<Replacement>, Vec<String>) {
    let mut sorted: Vec<&(String, String)> = entities.iter().collect();
    sorted.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
    let mut replacements = Vec::new();
    let mut texts = Vec::new();
    for (index, segment) in document.segments().iter().enumerate() {
        let chars: Vec<char> = segment.text.chars().collect();
        let mut taken = vec![false; chars.len()];
        let mut found: Vec<(usize, usize, String)> = Vec::new();
        for (value, marker) in &sorted {
            let needle: Vec<char> = value.chars().collect();
            let mut start = 0;
            while start + needle.len() <= chars.len() {
                let end = start + needle.len();
                if chars[start..end] == needle[..] && !taken[start..end].iter().any(|t| *t) {
                    taken[start..end].iter_mut().for_each(|t| *t = true);
                    found.push((start, end, marker.clone()));
                    start = end;
                } else {
                    start += 1;
                }
            }
        }
        found.sort();
        let mut text = String::new();
        let mut at = 0;
        for (start, end, marker) in found {
            text.extend(&chars[at..start]);
            text.push_str(&marker);
            at = end;
            replacements.push(Replacement {
                segment: index,
                start,
                end,
                text: marker,
            });
        }
        text.extend(&chars[at..]);
        texts.push(text);
    }
    (replacements, texts)
}

fn corpus_entities(name: &str) -> Vec<(String, String)> {
    expected(name)["entidades"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                e["texto"].as_str().unwrap().to_owned(),
                format!("[{}_{:04}]", e["tipo"].as_str().unwrap(), i + 1),
            )
        })
        .collect()
}

/// DOCX mínimo con el cuerpo dado y otras partes.
fn docx_with(body: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let document = format!("<w:document {w}><w:body>{body}</w:body></w:document>");
    let mut parts = vec![
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", document.as_str()),
    ];
    parts.extend_from_slice(extra);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn without_replacements_only_author_parts_change() {
    for name in CORPUS {
        let bytes = original(name);
        let output = DocxDocument::from_bytes(&bytes)
            .unwrap()
            .write(&[], &[])
            .unwrap();
        let before = entries(&bytes);
        let after = entries(&output);
        let names = |e: &[(String, zip::CompressionMethod, Vec<u8>)]| {
            e.iter().map(|x| (x.0.clone(), x.1)).collect::<Vec<_>>()
        };
        assert_eq!(names(&before), names(&after), "{name}: entradas u orden");
        for ((entry, _, old), (_, _, new)) in before.iter().zip(&after) {
            let text = String::from_utf8_lossy(old);
            let has_authors = entry.starts_with("docProps/")
                || text.contains("w:author=\"")
                || text.contains("w:initials=\"");
            if !has_authors {
                assert!(old == new, "{name}: {entry} ha cambiado");
            }
        }
    }
}

#[test]
fn without_replacements_the_text_reads_back_the_same() {
    for name in CORPUS {
        let document = DocxDocument::from_bytes(&original(name)).unwrap();
        let output = DocxDocument::from_bytes(&document.write(&[], &[]).unwrap()).unwrap();
        assert_eq!(output.segments(), document.segments(), "{name}");
        assert_eq!(output.field_codes(), document.field_codes(), "{name}");
        assert_eq!(output.external_links(), document.external_links(), "{name}");
        assert_eq!(output.warnings(), document.warnings(), "{name}");
    }
}

#[test]
fn author_metadata_and_revision_authors_are_blanked() {
    for name in CORPUS {
        let bytes = original(name);
        let output = DocxDocument::from_bytes(&bytes)
            .unwrap()
            .write(&[], &[])
            .unwrap();
        let reread = DocxDocument::from_bytes(&output).unwrap();
        assert!(reread.author_metadata().is_empty(), "{name}");
        assert!(reread.revision_authors().is_empty(), "{name}");
        let json = expected(name);
        for value in json["metadatos_autor"]
            .as_array()
            .unwrap()
            .iter()
            .chain(json["autores_revision"].as_array().unwrap())
        {
            let value = value.as_str().unwrap();
            for (entry, _, data) in entries(&output) {
                assert!(
                    !String::from_utf8_lossy(&data).contains(value),
                    "{name}: {entry} todavía tiene {value}"
                );
            }
        }
    }
    // Los elementos siguen ahí, vacíos.
    let output = DocxDocument::from_bytes(&original("es-01-contrato-arrendamiento"))
        .unwrap()
        .write(&[], &[])
        .unwrap();
    let core = part(&output, "docProps/core.xml");
    assert!(core.contains("<dc:creator></dc:creator>"), "{core}");
    assert!(
        core.contains("<cp:lastModifiedBy></cp:lastModifiedBy>"),
        "{core}"
    );
    assert!(core.contains("<cp:revision>2</cp:revision>"), "{core}");
    assert!(part(&output, "docProps/app.xml").contains("<Company></Company>"));
}

#[test]
fn a_marker_over_a_name_split_in_three_fragments_takes_the_first_format() {
    let bytes = original("es-01-contrato-arrendamiento");
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let index = segment_index(&document, "word/document.xml", "de una parte doña");
    let text = &document.segments()[index].text;
    let (start, end) = char_range(text, "Lucía Ferrer Ibáñez");
    let marker = "[NOMBRE_PERSONA_0001]";
    let output = document
        .write(
            &[Replacement {
                segment: index,
                start,
                end,
                text: marker.into(),
            }],
            &[],
        )
        .unwrap();

    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(
        reread.segments()[index].text,
        text.replace("Lucía Ferrer Ibáñez", marker)
    );
    let xml = part(&output, "word/document.xml");
    // El marcador queda en la carrera en negrita del primer fragmento; las
    // otras dos se quedan sin texto.
    assert!(
        xml.contains(&format!(
            "<w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{marker}</w:t>"
        )),
        "{xml}"
    );
    assert!(
        xml.contains("<w:rPr><w:b/><w:i/></w:rPr><w:t></w:t>"),
        "{xml}"
    );
    assert!(
        xml.contains("<w:rPr><w:color w:val=\"C00000\"/></w:rPr><w:t></w:t>"),
        "{xml}"
    );
    // Lo demás de la parte no cambia.
    let old = part(&bytes, "word/document.xml");
    let rest = |x: &str| x.split("doña").next().unwrap().to_owned();
    assert_eq!(rest(&xml), rest(&old));
}

#[test]
fn every_entity_of_the_corpus_is_replaced_and_the_rest_stays() {
    for name in CORPUS {
        let document = DocxDocument::from_bytes(&original(name)).unwrap();
        let entities = corpus_entities(name);
        let (replacements, texts) = replace_entities(&document, &entities);
        assert!(!replacements.is_empty(), "{name}");
        let output = document.write(&replacements, &[]).unwrap();
        let reread = DocxDocument::from_bytes(&output).unwrap();
        let got: Vec<&str> = reread.segments().iter().map(|s| s.text.as_str()).collect();
        assert_eq!(got, texts, "{name}");
        for segment in reread.segments() {
            for (value, _) in &entities {
                assert!(
                    !segment.text.contains(value.as_str()),
                    "{name}: {:?} sigue en {}",
                    value,
                    segment.part
                );
            }
        }
    }
}

#[test]
fn the_title_of_the_document_is_replaced_too() {
    let document = DocxDocument::from_bytes(&original("es-01-contrato-arrendamiento")).unwrap();
    let index = segment_index(&document, "docProps/core.xml", "Lucía");
    let (start, end) = char_range(&document.segments()[index].text, "Lucía Ferrer Ibáñez");
    let output = document
        .write(
            &[Replacement {
                segment: index,
                start,
                end,
                text: "[NOMBRE_PERSONA_0001]".into(),
            }],
            &[],
        )
        .unwrap();
    let core = part(&output, "docProps/core.xml");
    assert!(
        core.contains("<dc:title>Contrato de arrendamiento con [NOMBRE_PERSONA_0001]</dc:title>"),
        "{core}"
    );
    assert!(
        core.contains("<dc:subject>Arrendamiento</dc:subject>"),
        "{core}"
    );
}

#[test]
fn a_fragment_left_with_a_space_at_an_edge_gets_xml_space_preserve() {
    let bytes = docx_with(
        "<w:p><w:r><w:t>Firma Ana Ló</w:t></w:r><w:r><w:t>pez y Luis</w:t></w:r></w:p>",
        &[],
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let (start, end) = char_range(&document.segments()[0].text, "Ana López");
    let output = document
        .write(
            &[Replacement {
                segment: 0,
                start,
                end,
                text: "[P]".into(),
            }],
            &[],
        )
        .unwrap();
    let xml = part(&output, "word/document.xml");
    assert!(xml.contains("<w:t>Firma [P]</w:t>"), "{xml}");
    assert!(
        xml.contains("<w:t xml:space=\"preserve\"> y Luis</w:t>"),
        "{xml}"
    );
    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(reread.segments()[0].text, "Firma [P] y Luis");
}

#[test]
fn special_characters_are_escaped_in_the_output() {
    let bytes = docx_with(
        "<w:p><w:r><w:t>Smith &amp; Wesson &lt;3</w:t></w:r></w:p>",
        &[],
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let output = document
        .write(
            &[Replacement {
                segment: 0,
                start: 0,
                end: 5,
                text: "<A&B>".into(),
            }],
            &[],
        )
        .unwrap();
    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(reread.segments()[0].text, "<A&B> & Wesson <3");
}

#[test]
fn a_replacement_over_a_line_break_takes_the_break_with_it() {
    let document = DocxDocument::from_bytes(&original("es-01-contrato-arrendamiento")).unwrap();
    let index = segment_index(&document, "word/document.xml", "Domicilio:");
    let value = "Calle de la Paz 14, 3.º B 46003 Valencia";
    let (start, end) = char_range(&document.segments()[index].text, value);
    let output = document
        .write(
            &[Replacement {
                segment: index,
                start,
                end,
                text: "[DIRECCION_POSTAL_0001]".into(),
            }],
            &[],
        )
        .unwrap();
    let reread = DocxDocument::from_bytes(&output).unwrap();
    // El salto de línea de en medio se va con la dirección.
    assert_eq!(
        reread.segments()[index].text,
        "Domicilio: [DIRECCION_POSTAL_0001]"
    );
}

#[test]
fn link_targets_and_field_codes_get_the_literal_replacements() {
    let bytes = original("es-01-contrato-arrendamiento");
    let literals = [
        (
            "lucia.ferrer@correo-ejemplo.es".to_owned(),
            "[EMAIL_0001]".to_owned(),
        ),
        (
            "pablo.navarro@correo-ejemplo.es".to_owned(),
            "[EMAIL_0002]".to_owned(),
        ),
    ];
    let output = DocxDocument::from_bytes(&bytes)
        .unwrap()
        .write(&[], &literals)
        .unwrap();
    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(reread.external_links()[0].target, "mailto:[EMAIL_0001]");
    assert_eq!(
        reread.field_codes()[0].text,
        " HYPERLINK \"mailto:[EMAIL_0002]\" "
    );
    // Las relaciones internas no se tocan.
    let rels = part(&output, "word/_rels/document.xml.rels");
    let old = part(&bytes, "word/_rels/document.xml.rels");
    assert_eq!(
        rels.replace("[EMAIL_0001]", "lucia.ferrer@correo-ejemplo.es"),
        old
    );
}

#[test]
fn the_people_part_loses_names_and_addresses() {
    let w15 = "xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"";
    let people = format!(
        "<w15:people {w15}><w15:person w15:author=\"Rachel Turner\">\
         <w15:presenceInfo w15:providerId=\"AD\" w15:userId=\"rachel.turner@correo-ejemplo.com\"/>\
         </w15:person></w15:people>"
    );
    let bytes = docx_with(
        "<w:p><w:r><w:t>Hola</w:t></w:r></w:p>",
        &[("word/people.xml", &people)],
    );
    let output = DocxDocument::from_bytes(&bytes)
        .unwrap()
        .write(&[], &[])
        .unwrap();
    let xml = part(&output, "word/people.xml");
    assert!(!xml.contains("Rachel"), "{xml}");
    assert!(!xml.contains("rachel.turner"), "{xml}");
    assert!(xml.contains("w15:author=\"\""), "{xml}");
    assert!(xml.contains("w15:userId=\"\""), "{xml}");
    assert!(xml.contains("w15:providerId=\"AD\""), "{xml}");
}

#[test]
fn invalid_replacements_are_rejected() {
    let document = DocxDocument::from_bytes(&original("es-01-contrato-arrendamiento")).unwrap();
    let length = document.segments()[0].text.chars().count();
    for (segment, start, end) in [(0, 0, length + 1), (document.segments().len(), 0, 1)] {
        let result = document.write(
            &[Replacement {
                segment,
                start,
                end,
                text: "[X]".into(),
            }],
            &[],
        );
        assert!(
            matches!(result, Err(Error::InvalidReplacement(_))),
            "{result:?}"
        );
    }
}

#[test]
fn a_replacement_only_over_unmapped_characters_is_rejected() {
    let bytes = docx_with(
        "<w:p><w:r><w:t>Uno</w:t><w:tab/><w:t>dos</w:t></w:r></w:p>",
        &[],
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let result = document.write(
        &[Replacement {
            segment: 0,
            start: 3,
            end: 4,
            text: "[X]".into(),
        }],
        &[],
    );
    assert!(
        matches!(result, Err(Error::InvalidReplacement(_))),
        "{result:?}"
    );
}

#[test]
fn several_replacements_in_one_fragment_are_all_applied() {
    let bytes = docx_with(
        "<w:p><w:r><w:t>Ana y Luis viven con Eva</w:t></w:r></w:p>",
        &[],
    );
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let text = document.segments()[0].text.clone();
    let replacements: Vec<Replacement> = ["Ana", "Luis", "Eva"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (start, end) = char_range(&text, name);
            Replacement {
                segment: 0,
                start,
                end,
                text: format!("[P{i}]"),
            }
        })
        .collect();
    let output = document.write(&replacements, &[]).unwrap();
    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(reread.segments()[0].text, "[P0] y [P1] viven con [P2]");
}

#[test]
fn a_fragment_ending_in_a_space_gets_xml_space_preserve() {
    let bytes = docx_with("<w:p><w:r><w:t>Nombre: X</w:t></w:r></w:p>", &[]);
    let document = DocxDocument::from_bytes(&bytes).unwrap();
    let output = document
        .write(
            &[Replacement {
                segment: 0,
                start: 8,
                end: 9,
                text: "Ana ".into(),
            }],
            &[],
        )
        .unwrap();
    let xml = part(&output, "word/document.xml");
    assert!(
        xml.contains("<w:t xml:space=\"preserve\">Nombre: Ana </w:t>"),
        "{xml}"
    );
}

#[test]
fn a_longer_literal_wins_over_one_it_contains() {
    let literals = [
        ("ferrer".to_owned(), "[A]".to_owned()),
        (
            "lucia.ferrer@correo-ejemplo.es".to_owned(),
            "[EMAIL_0001]".to_owned(),
        ),
    ];
    let output = DocxDocument::from_bytes(&original("es-01-contrato-arrendamiento"))
        .unwrap()
        .write(&[], &literals)
        .unwrap();
    let reread = DocxDocument::from_bytes(&output).unwrap();
    assert_eq!(reread.external_links()[0].target, "mailto:[EMAIL_0001]");
}

#[test]
fn an_empty_literal_changes_nothing() {
    let bytes = original("es-01-contrato-arrendamiento");
    let output = DocxDocument::from_bytes(&bytes)
        .unwrap()
        .write(&[], &[(String::new(), "[X]".to_owned())])
        .unwrap();
    for name in ["word/document.xml", "word/_rels/document.xml.rels"] {
        assert_eq!(part(&output, name), part(&bytes, name), "{name}");
    }
}

#[test]
fn internal_relationships_never_get_literal_replacements() {
    let bytes = original("es-01-contrato-arrendamiento");
    let output = DocxDocument::from_bytes(&bytes)
        .unwrap()
        .write(&[], &[("styles.xml".to_owned(), "[X]".to_owned())])
        .unwrap();
    let rels = "word/_rels/document.xml.rels";
    assert_eq!(part(&output, rels), part(&bytes, rels));
}
