//! MD y TXT desde DOCX (PRD, «Salida MD y TXT desde DOCX»): el cuerpo y las
//! notas, con los cambios como aceptados y las sustituciones aplicadas.

use std::io::{Cursor, Write};
use std::path::Path;

use nucleo::Error;
use nucleo::formatos::Replacement;
use nucleo::formatos::docx::DocxDocument;

fn open(name: &str) -> DocxDocument {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(format!("{name}.docx"));
    DocxDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap()
}

/// DOCX mínimo con el cuerpo dado y otras partes.
fn docx_with(body: &str, extra: &[(&str, &str)]) -> DocxDocument {
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let document = format!("<w:document {w}><w:body>{body}</w:body></w:document>");
    let extra: Vec<(&str, String)> = extra
        .iter()
        .map(|(name, xml)| (*name, xml.replace("W", w)))
        .collect();
    let mut parts = vec![
        ("[Content_Types].xml", "<Types/>".to_owned()),
        ("word/document.xml", document),
    ];
    parts.extend(extra);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    DocxDocument::from_bytes(&zip.finish().unwrap().into_inner()).unwrap()
}

fn paragraph(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

fn list_item(text: &str, num_id: u32, level: u8) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num_id}\"/></w:numPr></w:pPr>\
         <w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

/// Numeración con la lista 1 de viñetas y la 2 numerada, en dos niveles.
const NUMBERING: &str = "<w:numbering W>\
    <w:abstractNum w:abstractNumId=\"0\">\
    <w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"bullet\"/></w:lvl>\
    <w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"bullet\"/></w:lvl></w:abstractNum>\
    <w:abstractNum w:abstractNumId=\"1\">\
    <w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl>\
    <w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"lowerLetter\"/></w:lvl></w:abstractNum>\
    <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
    <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>\
    </w:numbering>";

const ES_01_MARKDOWN: &str = "\
# Contrato de arrendamiento de vivienda

En Valencia, a 3 de marzo de 2026, de una parte doña Lucía Ferrer Ibáñez, con DNI 48291736K, como arrendadora.

## Datos de contacto

- Teléfono: 612 384 907
- Correo: lucia.ferrer@correo-ejemplo.es

## Condiciones

1. Renta mensual de 950 euros.
2. Fianza ingresada en la cuenta ES91 2100 0418 4502 0005 1332.

| Parte | Nombre | DNI |
| --- | --- | --- |
| Arrendadora | Lucía Ferrer Ibáñez | 48291736K |
| Arrendatario | Pablo Navarro Gil | 53720914R |

Para cualquier duda escriba a pablo.navarro@correo-ejemplo.es.

Firma: Pablo Navarro Gil

Domicilio: Calle de la Paz 14, 3.º B 46003 Valencia
";

const ES_01_TEXT: &str = "\
Contrato de arrendamiento de vivienda
En Valencia, a 3 de marzo de 2026, de una parte doña Lucía Ferrer Ibáñez, con DNI 48291736K, como arrendadora.
Datos de contacto
Teléfono: 612 384 907
Correo: lucia.ferrer@correo-ejemplo.es
Condiciones
Renta mensual de 950 euros.
Fianza ingresada en la cuenta ES91 2100 0418 4502 0005 1332.
Parte
Nombre
DNI
Arrendadora
Lucía Ferrer Ibáñez
48291736K
Arrendatario
Pablo Navarro Gil
53720914R
Para cualquier duda escriba a pablo.navarro@correo-ejemplo.es.
Firma: Pablo Navarro Gil
Domicilio: Calle de la Paz 14, 3.º B 46003 Valencia
";

#[test]
fn the_contract_gives_the_expected_markdown() {
    assert_eq!(
        open("es-01-contrato-arrendamiento").markdown(&[]).unwrap(),
        ES_01_MARKDOWN
    );
}

#[test]
fn the_contract_gives_one_paragraph_per_line_in_text() {
    assert_eq!(
        open("es-01-contrato-arrendamiento")
            .plain_text(&[])
            .unwrap(),
        ES_01_TEXT
    );
}

#[test]
fn replacements_are_applied_to_both_outputs() {
    let document = open("es-01-contrato-arrendamiento");
    let replacements: Vec<Replacement> = document
        .segments()
        .iter()
        .enumerate()
        .filter_map(|(index, segment)| {
            let byte = segment.text.find("Lucía Ferrer Ibáñez")?;
            let start = segment.text[..byte].chars().count();
            Some(Replacement {
                segment: index,
                start,
                end: start + "Lucía Ferrer Ibáñez".chars().count(),
                text: "[NOMBRE_PERSONA_0001]".into(),
            })
        })
        .collect();
    let markdown = document.markdown(&replacements).unwrap();
    assert_eq!(
        markdown,
        ES_01_MARKDOWN.replace("Lucía Ferrer Ibáñez", "[NOMBRE_PERSONA_0001]")
    );
    let text = document.plain_text(&replacements).unwrap();
    assert_eq!(
        text,
        ES_01_TEXT.replace("Lucía Ferrer Ibáñez", "[NOMBRE_PERSONA_0001]")
    );
}

#[test]
fn notes_go_at_the_end_and_headers_and_footers_are_left_out() {
    let document = open("ca-01-informe-auditoria");
    let markdown = document.markdown(&[]).unwrap();
    assert!(
        markdown.ends_with(
            "\n\n## Notas\n\n\
             Segons l'acta signada per Núria Vidal Roca.\n\n\
             Expedient enviat a nuria.vidal@correu-exemple.cat.\n"
        ),
        "{markdown}"
    );
    let text = document.plain_text(&[]).unwrap();
    assert!(
        text.ends_with(
            "\n\nNotas\n\
             Segons l'acta signada per Núria Vidal Roca.\n\
             Expedient enviat a nuria.vidal@correu-exemple.cat.\n"
        ),
        "{text}"
    );
    for part in ["word/header1.xml", "word/footer1.xml"] {
        for segment in document.segments().iter().filter(|s| s.part == part) {
            assert!(!markdown.contains(segment.text.trim()), "{part}");
            assert!(!text.contains(segment.text.trim()), "{part}");
        }
    }
}

#[test]
fn tracked_changes_appear_accepted_and_comments_and_metadata_are_left_out() {
    let document = open("en-01-employment-review");
    for output in [
        document.markdown(&[]).unwrap(),
        document.plain_text(&[]).unwrap(),
    ] {
        assert!(output.contains("Reviewed by Emma Clarke."), "{output}");
        assert!(!output.contains("Oliver Hart"), "{output}");
        for segment in document
            .segments()
            .iter()
            .filter(|s| s.part == "word/comments.xml" || s.part == "docProps/core.xml")
        {
            assert!(
                !output.contains(segment.text.trim()),
                "{}: {output}",
                segment.part
            );
        }
    }
}

#[test]
fn a_document_without_notes_has_no_notes_section() {
    let document = open("es-01-contrato-arrendamiento");
    assert!(!document.markdown(&[]).unwrap().contains("Notas"));
    assert!(!document.plain_text(&[]).unwrap().contains("Notas"));
}

#[test]
fn a_replacement_over_deleted_text_disappears_with_it() {
    let document = open("en-01-employment-review");
    let index = document
        .segments()
        .iter()
        .position(|s| s.text.contains("Oliver Hart"))
        .unwrap();
    let text = &document.segments()[index].text;
    let start = text[..text.find("Oliver Hart").unwrap()].chars().count();
    let replacements = [Replacement {
        segment: index,
        start,
        end: start + "Oliver Hart".chars().count(),
        text: "[NOMBRE_PERSONA_0002]".into(),
    }];
    let markdown = document.markdown(&replacements).unwrap();
    assert!(!markdown.contains("NOMBRE_PERSONA_0002"), "{markdown}");
    assert!(markdown.contains("Reviewed by Emma Clarke."), "{markdown}");
}

#[test]
fn a_fully_deleted_paragraph_is_left_out() {
    let document = docx_with(
        &format!(
            "{}<w:p><w:del w:id=\"1\" w:author=\"A\"><w:r><w:delText>Borrado entero</w:delText></w:r></w:del></w:p>{}",
            paragraph("Uno"),
            paragraph("Dos")
        ),
        &[],
    );
    assert_eq!(document.markdown(&[]).unwrap(), "Uno\n\nDos\n");
    assert_eq!(document.plain_text(&[]).unwrap(), "Uno\nDos\n");
}

#[test]
fn nested_lists_are_indented_and_numbered_by_level() {
    let body = [
        list_item("Primero", 2, 0),
        list_item("Detalle a", 2, 1),
        list_item("Detalle b", 2, 1),
        list_item("Segundo", 2, 0),
        list_item("Viñeta", 1, 1),
        list_item("Tercero", 2, 0),
        paragraph("Fin de la lista"),
        list_item("Otra vez uno", 2, 0),
    ]
    .concat();
    let document = docx_with(&body, &[("word/numbering.xml", NUMBERING)]);
    assert_eq!(
        document.markdown(&[]).unwrap(),
        "1. Primero\n    1. Detalle a\n    2. Detalle b\n2. Segundo\n    - Viñeta\n3. Tercero\n\n\
         Fin de la lista\n\n1. Otra vez uno\n"
    );
}

#[test]
fn markdown_signs_at_the_start_of_a_paragraph_are_escaped() {
    let body = [
        paragraph("# no es un título"),
        paragraph("1. no es una lista"),
        paragraph("12) tampoco"),
        paragraph("- ni esto"),
        paragraph("> ni una cita"),
        paragraph("Texto normal con # y 1. dentro"),
    ]
    .concat();
    let document = docx_with(&body, &[]);
    assert_eq!(
        document.markdown(&[]).unwrap(),
        "\\# no es un título\n\n1\\. no es una lista\n\n12\\) tampoco\n\n\\- ni esto\n\n\
         \\> ni una cita\n\nTexto normal con # y 1. dentro\n"
    );
    // En TXT no se escapa nada.
    assert!(
        document
            .plain_text(&[])
            .unwrap()
            .starts_with("# no es un título\n1. no es una lista\n")
    );
}

#[test]
fn table_cells_join_their_paragraphs_and_escape_bars() {
    let cell = |paragraphs: &[&str]| {
        format!(
            "<w:tc>{}</w:tc>",
            paragraphs.iter().map(|p| paragraph(p)).collect::<String>()
        )
    };
    let body = format!(
        "<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr><w:tr>{}</w:tr></w:tbl>",
        cell(&["Clave"]),
        cell(&["Valor"]),
        cell(&["a|b"]),
        cell(&["línea uno", "línea dos"]),
        cell(&["sola"]),
    );
    let document = docx_with(&body, &[]);
    assert_eq!(
        document.markdown(&[]).unwrap(),
        "| Clave | Valor |\n| --- | --- |\n| a\\|b | línea uno línea dos |\n| sola |  |\n"
    );
}

#[test]
fn spaces_at_the_edges_of_a_paragraph_are_trimmed() {
    let document = docx_with(&paragraph("  con espacios  "), &[]);
    assert_eq!(document.markdown(&[]).unwrap(), "con espacios\n");
    assert_eq!(document.plain_text(&[]).unwrap(), "con espacios\n");
}

#[test]
fn invalid_replacements_are_rejected() {
    let document = open("es-01-contrato-arrendamiento");
    let replacements = [Replacement {
        segment: 0,
        start: 0,
        end: 10_000,
        text: "[X]".into(),
    }];
    assert!(matches!(
        document.markdown(&replacements),
        Err(Error::InvalidReplacement(_))
    ));
    assert!(matches!(
        document.plain_text(&replacements),
        Err(Error::InvalidReplacement(_))
    ));
}

#[test]
fn a_sublist_starts_again_at_one_under_each_item() {
    let body = [
        list_item("Primero", 2, 0),
        list_item("Detalle a", 2, 1),
        list_item("Detalle b", 2, 1),
        list_item("Segundo", 2, 0),
        list_item("Detalle c", 2, 1),
    ]
    .concat();
    let document = docx_with(&body, &[("word/numbering.xml", NUMBERING)]);
    assert_eq!(
        document.markdown(&[]).unwrap(),
        "1. Primero\n    1. Detalle a\n    2. Detalle b\n2. Segundo\n    1. Detalle c\n"
    );
}

#[test]
fn two_tables_with_an_empty_paragraph_between_stay_apart() {
    let table = |a: &str, b: &str| {
        format!(
            "<w:tbl><w:tr><w:tc>{}</w:tc></w:tr><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
            paragraph(a),
            paragraph(b)
        )
    };
    let body = format!("{}<w:p/>{}", table("A", "1"), table("B", "2"));
    let document = docx_with(&body, &[]);
    assert_eq!(
        document.markdown(&[]).unwrap(),
        "| A |\n| --- |\n| 1 |\n\n| B |\n| --- |\n| 2 |\n"
    );
}
