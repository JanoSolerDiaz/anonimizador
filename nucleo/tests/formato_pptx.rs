//! PPTX: lectura de diapositivas, notas, tablas, comentarios, gráficos y
//! metadatos, y escritura sobre el XML original.

use std::io::{Cursor, Read, Write};

use nucleo::Error;
use nucleo::error::Office;
use nucleo::formatos::Replacement;
use nucleo::formatos::pptx::{Place, PptxDocument};

const P: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
                 xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" \
                 xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn zip(parts: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn rels(list: &[(&str, &str, &str)]) -> String {
    let items: String = list
        .iter()
        .map(|(id, kind, target)| {
            if target.starts_with("mailto:") {
                format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\" TargetMode=\"External\"/>")
            } else {
                format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"/>")
            }
        })
        .collect();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{items}</Relationships>"
    )
}

fn shape(body: &str, placeholder: Option<&str>) -> String {
    let ph = placeholder.map_or(String::new(), |t| format!("<p:ph type=\"{t}\"/>"));
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Forma\"/><p:cNvSpPr/><p:nvPr>{ph}</p:nvPr></p:nvSpPr>\
         <p:spPr/><p:txBody><a:bodyPr/>{body}</p:txBody></p:sp>"
    )
}

fn slide(shapes: &str) -> String {
    format!("<p:sld {P}><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sld>")
}

/// Presentación de prueba. El archivo `slide2.xml` es la primera
/// diapositiva y `slide1.xml` la segunda.
fn sample() -> Vec<u8> {
    let first = slide(&format!(
        "{}{}<p:pic><p:nvPicPr><p:cNvPr id=\"5\" name=\"Foto\" descr=\"Foto de Marta Lozano\"/></p:nvPicPr>\
         <p:blipFill><a:blip r:embed=\"rId9\"/></p:blipFill></p:pic>",
        shape(
            "<a:p><a:r><a:t>Contrato de Marta</a:t></a:r></a:p>",
            Some("title")
        ),
        shape(
            "<a:p><a:r><a:t>Firma: Mar</a:t></a:r><a:r><a:rPr b=\"1\"/><a:t>ta Lo</a:t></a:r><a:r><a:t>zano</a:t></a:r></a:p>\
             <a:p><a:r><a:t>Calle del Coso</a:t></a:r><a:br/><a:r><a:t>87, Zaragoza</a:t></a:r></a:p>\
             <a:p><a:r><a:t>Escriba a </a:t></a:r><a:r><a:rPr><a:hlinkClick r:id=\"rId5\"/></a:rPr><a:t>marta@correo.es</a:t></a:r></a:p>",
            None
        ),
    ));
    let second = slide(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"3\" name=\"Tabla\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>\
         <a:graphic><a:graphicData><a:tbl>\
         <a:tr><a:tc><a:txBody><a:p><a:r><a:t>Nombre</a:t></a:r></a:p></a:txBody></a:tc>\
               <a:tc><a:txBody><a:p><a:r><a:t>DNI</a:t></a:r></a:p></a:txBody></a:tc></a:tr>\
         <a:tr><a:tc><a:txBody><a:p><a:r><a:t>Iván Prieto</a:t></a:r></a:p></a:txBody></a:tc>\
               <a:tc><a:txBody><a:p><a:r><a:t>X4721930F</a:t></a:r></a:p></a:txBody></a:tc></a:tr>\
         </a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
    );
    let notes = format!(
        "<p:notes {P}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:notes>",
        shape(
            "<a:p><a:r><a:t>Llamar a Marta Lozano</a:t></a:r></a:p>",
            Some("body")
        )
    );
    let chart = "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" \
                 xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><c:chart>\
                 <c:title><c:tx><c:rich><a:p><a:r><a:t>Ventas de Iván Prieto</a:t></a:r></a:p></c:rich></c:tx></c:title>\
                 <c:plotArea><c:barChart><c:ser><c:cat><c:strRef><c:f>Hoja1!$A$2</c:f><c:strCache><c:pt idx=\"0\">\
                 <c:v>Iván Prieto</c:v></c:pt></c:strCache></c:strRef></c:cat></c:ser></c:barChart></c:plotArea>\
                 </c:chart></c:chartSpace>"
        .to_owned();
    zip(&[
        ("[Content_Types].xml", "<Types/>".to_owned()),
        (
            "ppt/presentation.xml",
            format!("<p:presentation {P}><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId2\"/><p:sldId id=\"257\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>"),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            rels(&[("rId1", "slide", "slides/slide1.xml"), ("rId2", "slide", "slides/slide2.xml")]),
        ),
        ("ppt/slides/slide1.xml", second),
        ("ppt/slides/slide2.xml", first),
        (
            "ppt/slides/_rels/slide2.xml.rels",
            rels(&[
                ("rId3", "notesSlide", "../notesSlides/notesSlide1.xml"),
                ("rId4", "chart", "../charts/chart1.xml"),
                ("rId5", "hyperlink", "mailto:marta@correo.es"),
            ]),
        ),
        ("ppt/notesSlides/notesSlide1.xml", notes),
        ("ppt/charts/chart1.xml", chart),
        (
            "ppt/slideLayouts/slideLayout1.xml",
            format!("<p:sldLayout {P}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sldLayout>",
                shape("<a:p><a:r><a:t>Preparado por Marta Lozano</a:t></a:r></a:p>", Some("ftr"))),
        ),
        (
            "ppt/commentAuthors.xml",
            format!("<p:cmAuthorLst {P}><p:cmAuthor id=\"0\" name=\"Lucía Ferrer\" initials=\"LF\" lastIdx=\"1\" clrIdx=\"0\"/></p:cmAuthorLst>"),
        ),
        (
            "ppt/comments/comment1.xml",
            format!("<p:cmLst {P}><p:cm authorId=\"0\" idx=\"1\"><p:pos x=\"1\" y=\"1\"/><p:text>Revisar el DNI de Iván Prieto</p:text></p:cm></p:cmLst>"),
        ),
        ("ppt/embeddings/oleObject1.bin", "binario".to_owned()),
        (
            "docProps/core.xml",
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
             xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Contrato de Marta</dc:title>\
             <dc:creator>Lucía Ferrer</dc:creator><cp:lastModifiedBy>Lucía Ferrer</cp:lastModifiedBy></cp:coreProperties>"
                .to_owned(),
        ),
        (
            "docProps/app.xml",
            "<Properties xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
             <Company>Inmobiliaria Ejemplo</Company><TitlesOfParts><vt:vector size=\"1\" baseType=\"lpstr\">\
             <vt:lpstr>Contrato de Marta</vt:lpstr></vt:vector></TitlesOfParts></Properties>"
                .to_owned(),
        ),
    ])
}

fn texts(document: &PptxDocument) -> Vec<(&str, Place)> {
    document
        .segments()
        .iter()
        .map(|s| (s.text.as_str(), s.place))
        .collect()
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn find(document: &PptxDocument, text: &str) -> usize {
    document
        .segments()
        .iter()
        .position(|s| s.text == text)
        .unwrap_or_else(|| panic!("no está «{text}»"))
}

#[test]
fn slides_come_in_presentation_order_with_their_charts_and_notes() {
    let document = PptxDocument::from_bytes(&sample()).unwrap();
    let all = texts(&document);
    let slides: Vec<(&str, Place)> = all
        .iter()
        .copied()
        .filter(|(_, place)| *place != Place::Other)
        .collect();
    assert_eq!(
        slides,
        [
            ("Contrato de Marta", Place::Slide(1)),
            ("Firma: Marta Lozano", Place::Slide(1)),
            ("Calle del Coso 87, Zaragoza", Place::Slide(1)),
            ("Escriba a marta@correo.es", Place::Slide(1)),
            ("Ventas de Iván Prieto", Place::Slide(1)),
            ("Llamar a Marta Lozano", Place::Notes(1)),
            ("Nombre", Place::Slide(2)),
            ("DNI", Place::Slide(2)),
            ("Iván Prieto", Place::Slide(2)),
            ("X4721930F", Place::Slide(2)),
        ]
    );
    // Plantillas, comentarios y metadatos también se analizan.
    for other in [
        "Preparado por Marta Lozano",
        "Revisar el DNI de Iván Prieto",
        "Contrato de Marta",
    ] {
        assert!(
            all.iter().any(|(t, p)| *t == other && *p == Place::Other),
            "falta «{other}»"
        );
    }
    let title = &document.segments()[0];
    assert!(title.title);
    let cell = document.segments()[find(&document, "Iván Prieto")]
        .cell
        .unwrap();
    assert_eq!((cell.row, cell.column), (1, 0));
}

#[test]
fn authors_links_alt_texts_and_chart_caches_are_collected() {
    let document = PptxDocument::from_bytes(&sample()).unwrap();
    assert_eq!(
        document.author_metadata(),
        ["Lucía Ferrer", "Inmobiliaria Ejemplo"]
    );
    assert_eq!(document.comment_authors(), ["Lucía Ferrer", "LF"]);
    let literal = document.literal_texts();
    for expected in [
        "mailto:marta@correo.es",
        "Foto de Marta Lozano",
        "Iván Prieto",
    ] {
        assert!(literal.contains(&expected), "falta {expected}: {literal:?}");
    }
    let warnings = document.warnings();
    assert_eq!((warnings.images, warnings.objects), (1, 1));
}

#[test]
fn writing_replaces_text_in_the_original_xml() {
    let original = sample();
    let document = PptxDocument::from_bytes(&original).unwrap();
    let firma = find(&document, "Firma: Marta Lozano");
    let calle = find(&document, "Calle del Coso 87, Zaragoza");
    let tabla = find(&document, "Iván Prieto");
    let replacements = [
        Replacement {
            segment: firma,
            start: 7,
            end: 19,
            text: "[NOMBRE_PERSONA_0001]".into(),
        },
        Replacement {
            segment: calle,
            start: 0,
            end: 17,
            text: "[DIRECCION_0001]".into(),
        },
        Replacement {
            segment: tabla,
            start: 0,
            end: 11,
            text: "[NOMBRE_PERSONA_0002]".into(),
        },
    ];
    let literals = [
        (
            "Marta Lozano".to_owned(),
            "[NOMBRE_PERSONA_0001]".to_owned(),
        ),
        ("Iván Prieto".to_owned(), "[NOMBRE_PERSONA_0002]".to_owned()),
        ("marta@correo.es".to_owned(), "[EMAIL_0001]".to_owned()),
    ];
    let written = document.write(&replacements, &literals).unwrap();

    let again = PptxDocument::from_bytes(&written).unwrap();
    assert_eq!(
        again.segments()[find(&again, "Firma: [NOMBRE_PERSONA_0001]")].place,
        Place::Slide(1)
    );
    // El salto cubierto por la sustitución se va con ella.
    find(&again, "[DIRECCION_0001], Zaragoza");
    find(&again, "[NOMBRE_PERSONA_0002]");

    let slide = part(&written, "ppt/slides/slide2.xml");
    assert!(
        slide.contains(
            "<a:t>Firma: [NOMBRE_PERSONA_0001]</a:t></a:r><a:r><a:rPr b=\"1\"/><a:t></a:t>"
        ),
        "{slide}"
    );
    assert!(!slide.contains("<a:br/>"), "{slide}");
    assert!(
        slide.contains("descr=\"Foto de [NOMBRE_PERSONA_0001]\""),
        "{slide}"
    );
    assert!(
        part(&written, "ppt/slides/_rels/slide2.xml.rels")
            .contains("Target=\"mailto:[EMAIL_0001]\"")
    );
    assert!(part(&written, "ppt/charts/chart1.xml").contains("<c:v>[NOMBRE_PERSONA_0002]</c:v>"));

    // Autores vacíos: metadatos, comentarios.
    assert!(
        again.author_metadata().is_empty(),
        "{:?}",
        again.author_metadata()
    );
    assert!(
        again.comment_authors().is_empty(),
        "{:?}",
        again.comment_authors()
    );
    assert!(part(&written, "docProps/app.xml").contains("<Company></Company>"));
    // Las partes sin cambios se copian tal cual.
    assert_eq!(
        part(&written, "ppt/presentation.xml"),
        part(&original, "ppt/presentation.xml")
    );
}

#[test]
fn markdown_and_text_have_one_section_per_slide_with_notes() {
    let document = PptxDocument::from_bytes(&sample()).unwrap();
    let tabla = find(&document, "Iván Prieto");
    let replacements = [Replacement {
        segment: tabla,
        start: 0,
        end: 11,
        text: "[NOMBRE_PERSONA_0002]".into(),
    }];
    assert_eq!(
        document.markdown(&replacements).unwrap(),
        "## Diapositiva 1\n\n\
         ### Contrato de Marta\n\n\
         Firma: Marta Lozano\n\n\
         Calle del Coso 87, Zaragoza\n\n\
         Escriba a marta@correo.es\n\n\
         Ventas de Iván Prieto\n\n\
         #### Notas\n\n\
         Llamar a Marta Lozano\n\n\
         ## Diapositiva 2\n\n\
         | Nombre | DNI |\n| --- | --- |\n| [NOMBRE_PERSONA_0002] | X4721930F |\n"
    );
    assert_eq!(
        document.plain_text(&replacements).unwrap(),
        "Diapositiva 1\n\
         Contrato de Marta\n\
         Firma: Marta Lozano\n\
         Calle del Coso 87, Zaragoza\n\
         Escriba a marta@correo.es\n\
         Ventas de Iván Prieto\n\
         Notas:\n\
         Llamar a Marta Lozano\n\
         \n\
         Diapositiva 2\n\
         Nombre\tDNI\n\
         [NOMBRE_PERSONA_0002]\tX4721930F\n"
    );
}

#[test]
fn protected_and_broken_files_give_clear_errors() {
    let ole = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    assert!(matches!(
        PptxDocument::from_bytes(&ole).unwrap_err(),
        Error::Protected(Office::PowerPoint)
    ));
    let not_pptx = zip(&[("word/document.xml", "<w:document/>".to_owned())]);
    let error = PptxDocument::from_bytes(&not_pptx).unwrap_err();
    assert_eq!(
        error.to_string(),
        "El archivo no es una presentación de PowerPoint válida: no tiene la presentación."
    );
    let broken = zip(&[
        (
            "ppt/presentation.xml",
            format!(
                "<p:presentation {P}><p:sldIdLst><p:sldId r:id=\"rId1\"/></p:sldIdLst></p:presentation>"
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            rels(&[("rId1", "slide", "slides/slide1.xml")]),
        ),
        ("ppt/slides/slide1.xml", "<p:sld><a:p></p:sld>".to_owned()),
    ]);
    assert!(matches!(
        PptxDocument::from_bytes(&broken).unwrap_err(),
        Error::InvalidOffice(Office::PowerPoint, _)
    ));
}
