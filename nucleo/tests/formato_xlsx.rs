//! XLSX: lectura de celdas (textos compartidos, en línea, números, fechas y
//! resultados de fórmula), nombres de hoja, comentarios, encabezados,
//! gráficos y tablas dinámicas, y escritura sobre el XML original.

use std::io::{Cursor, Read, Write};

use nucleo::Error;
use nucleo::error::Office;
use nucleo::formatos::Replacement;
use nucleo::formatos::xlsx::{XlsxDocument, XlsxKind};

const MAIN: &str = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
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
            let external = if target.starts_with("mailto:") {
                " TargetMode=\"External\""
            } else {
                ""
            };
            format!(
                "<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"{external}/>"
            )
        })
        .collect();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{items}</Relationships>"
    )
}

/// Libro de prueba: «Nómina Marta Lozano» y «Resumen».
fn sample() -> Vec<u8> {
    let shared = format!(
        "<sst {MAIN} count=\"8\" uniqueCount=\"7\">\
         <si><t>Nombre</t></si><si><t>Teléfono</t></si><si><t>Alta</t></si><si><t>Importe</t></si>\
         <si><r><t>Mar</t></r><r><rPr><b/></rPr><t>ta Lozano</t></r><rPh sb=\"0\" eb=\"1\"><t>マルタ</t></rPh></si>\
         <si><t>Calle del Coso 87</t></si><si><t xml:space=\"preserve\">Sin usar Lucía </t></si></sst>"
    );
    let sheet1 = format!(
        "<worksheet {MAIN}><sheetData>\
         <row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c><c r=\"C1\" t=\"s\"><v>2</v></c><c r=\"D1\" t=\"s\"><v>3</v></c></row>\
         <row r=\"2\"><c r=\"A2\" t=\"s\"><v>4</v></c><c r=\"B2\"><v>654218903</v></c><c r=\"C2\" s=\"1\"><v>45702</v></c><c r=\"D2\" s=\"0\"><v>1250.5</v></c></row>\
         <row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>Iván Prieto</t></is></c>\
         <c r=\"B3\" t=\"str\"><f>A2&amp;\"\"</f><v>Marta Lozano</v></c>\
         <c r=\"C3\"><f>B2</f><v>654218903</v></c><c r=\"D3\" t=\"b\"><v>1</v></c></row>\
         </sheetData>\
         <dataValidations count=\"1\"><dataValidation type=\"list\" sqref=\"A4\"><formula1>\"Marta Lozano,Iván Prieto\"</formula1></dataValidation></dataValidations>\
         <hyperlinks><hyperlink ref=\"A2\" r:id=\"rId1\" display=\"Marta Lozano\"/>\
         <hyperlink ref=\"A3\" location=\"'Nómina Marta Lozano'!A2\" display=\"Ir\"/></hyperlinks>\
         <headerFooter><oddHeader>&amp;LPreparado por Lucía Ferrer</oddHeader></headerFooter>\
         </worksheet>"
    );
    let sheet2 = format!(
        "<worksheet {MAIN}><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>5</v></c>\
         <c r=\"B1\" t=\"str\"><f>'Nómina Marta Lozano'!A2</f><v>Marta Lozano</v></c></row></sheetData></worksheet>"
    );
    zip(&[
        ("[Content_Types].xml", "<Types/>".to_owned()),
        (
            "xl/workbook.xml",
            format!(
                "<workbook {MAIN}><sheets><sheet name=\"Nómina Marta Lozano\" sheetId=\"1\" r:id=\"rId1\"/>\
                 <sheet name=\"Resumen\" sheetId=\"2\" r:id=\"rId2\"/></sheets>\
                 <definedNames><definedName name=\"Telefono\">'Nómina Marta Lozano'!$B$2</definedName></definedNames></workbook>"
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            rels(&[
                ("rId1", "worksheet", "worksheets/sheet1.xml"),
                ("rId2", "worksheet", "worksheets/sheet2.xml"),
                ("rId3", "sharedStrings", "sharedStrings.xml"),
                ("rId4", "styles", "styles.xml"),
            ]),
        ),
        ("xl/sharedStrings.xml", shared),
        (
            "xl/styles.xml",
            format!("<styleSheet {MAIN}><cellXfs count=\"2\"><xf numFmtId=\"0\"/><xf numFmtId=\"14\"/></cellXfs></styleSheet>"),
        ),
        ("xl/worksheets/sheet1.xml", sheet1),
        ("xl/worksheets/sheet2.xml", sheet2),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            rels(&[("rId1", "hyperlink", "mailto:marta@correo.es"), ("rId2", "comments", "../comments1.xml")]),
        ),
        (
            "xl/comments1.xml",
            format!(
                "<comments {MAIN}><authors><author>Lucía Ferrer</author></authors><commentList>\
                 <comment ref=\"B2\" authorId=\"0\"><text><r><rPr><b/></rPr><t xml:space=\"preserve\">Lucía Ferrer: </t></r>\
                 <r><t>Confirmar con Iván Prieto</t></r></text></comment></commentList></comments>"
            ),
        ),
        (
            "xl/threadedComments/threadedComment1.xml",
            "<ThreadedComments xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments\">\
             <threadedComment ref=\"C2\" id=\"{1}\" personId=\"{9}\"><text>Pregunta a Marta</text></threadedComment></ThreadedComments>"
                .to_owned(),
        ),
        (
            "xl/persons/person.xml",
            "<personList xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments\">\
             <person displayName=\"Lucía Ferrer\" id=\"{9}\" userId=\"lucia@correo.es\" providerId=\"AD\"/></personList>"
                .to_owned(),
        ),
        (
            "xl/charts/chart1.xml",
            "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><c:chart>\
             <c:title><c:tx><c:rich><a:p><a:r><a:t>Ventas de Iván</a:t></a:r></a:p></c:rich></c:tx></c:title>\
             <c:plotArea><c:barChart><c:ser><c:cat><c:strRef><c:f>'Nómina Marta Lozano'!$A$2</c:f><c:strCache>\
             <c:pt idx=\"0\"><c:v>Marta Lozano</c:v></c:pt></c:strCache></c:strRef></c:cat></c:ser></c:barChart></c:plotArea>\
             </c:chart></c:chartSpace>"
                .to_owned(),
        ),
        (
            "xl/pivotCache/pivotCacheDefinition1.xml",
            format!(
                "<pivotCacheDefinition {MAIN}><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:D3\" sheet=\"Nómina Marta Lozano\"/></cacheSource>\
                 <cacheFields count=\"1\"><cacheField name=\"Nombre\"><sharedItems><s v=\"Marta Lozano\"/><s v=\"Iván Prieto\"/></sharedItems></cacheField></cacheFields>\
                 </pivotCacheDefinition>"
            ),
        ),
        (
            "xl/tables/table1.xml",
            format!("<table {MAIN} id=\"1\" name=\"Tabla1\" displayName=\"Tabla1\" ref=\"A1:D3\"><tableColumns count=\"1\"><tableColumn id=\"1\" name=\"Nombre\"/></tableColumns></table>"),
        ),
        (
            "docProps/core.xml",
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
             xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:creator>Lucía Ferrer</dc:creator></cp:coreProperties>"
                .to_owned(),
        ),
        (
            "docProps/app.xml",
            "<Properties xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
             <Company>Inmobiliaria Ejemplo</Company><TitlesOfParts><vt:vector size=\"2\" baseType=\"lpstr\">\
             <vt:lpstr>Nómina Marta Lozano</vt:lpstr><vt:lpstr>Resumen</vt:lpstr></vt:vector></TitlesOfParts></Properties>"
                .to_owned(),
        ),
    ])
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

/// Índice del segmento número `n` (desde 0) con este texto.
fn nth(document: &XlsxDocument, text: &str, n: usize) -> usize {
    document
        .segments()
        .iter()
        .enumerate()
        .filter(|(_, s)| s.text == text)
        .nth(n)
        .unwrap_or_else(|| panic!("no está «{text}» {n}"))
        .0
}

fn replacement(segment: usize, start: usize, end: usize, text: &str) -> Replacement {
    Replacement {
        segment,
        start,
        end,
        text: text.to_owned(),
    }
}

#[test]
fn cells_come_row_by_row_after_the_name_of_their_sheet() {
    let document = XlsxDocument::from_bytes(&sample()).unwrap();
    let segments: Vec<(&str, XlsxKind)> = document
        .segments()
        .iter()
        .map(|s| (s.text.as_str(), s.kind))
        .collect();
    assert_eq!(
        segments,
        [
            ("Nómina Marta Lozano", XlsxKind::SheetName),
            ("Nombre", XlsxKind::Cell),
            ("Teléfono", XlsxKind::Cell),
            ("Alta", XlsxKind::Cell),
            ("Importe", XlsxKind::Cell),
            ("Marta Lozano", XlsxKind::Cell),
            ("654218903", XlsxKind::Number),
            ("14/02/2025", XlsxKind::Number),
            ("1250.5", XlsxKind::Number),
            ("Iván Prieto", XlsxKind::Cell),
            ("Marta Lozano", XlsxKind::Cell),
            ("654218903", XlsxKind::Number),
            ("&LPreparado por Lucía Ferrer", XlsxKind::HeaderFooter),
            ("Resumen", XlsxKind::SheetName),
            ("Calle del Coso 87", XlsxKind::Cell),
            ("Marta Lozano", XlsxKind::Cell),
            ("Sin usar Lucía ", XlsxKind::Cell),
            ("Lucía Ferrer: Confirmar con Iván Prieto", XlsxKind::Comment),
            ("Pregunta a Marta", XlsxKind::Comment),
            ("Ventas de Iván", XlsxKind::Drawing),
        ]
    );
}

#[test]
fn numbers_reach_the_model_with_the_title_of_their_column() {
    let document = XlsxDocument::from_bytes(&sample()).unwrap();
    let texts = document.detection_texts();
    let phone = nth(&document, "654218903", 0);
    assert_eq!(texts[phone], ("Teléfono: 654218903".to_owned(), 10));
    let date = nth(&document, "14/02/2025", 0);
    assert_eq!(texts[date], ("Alta: 14/02/2025".to_owned(), 6));
    // Los textos y los números sin título van tal cual.
    let name = nth(&document, "Marta Lozano", 0);
    assert_eq!(texts[name], ("Marta Lozano".to_owned(), 0));
}

#[test]
fn authors_formulas_and_caches_are_collected() {
    let document = XlsxDocument::from_bytes(&sample()).unwrap();
    assert_eq!(
        document.author_metadata(),
        ["Lucía Ferrer", "Inmobiliaria Ejemplo"]
    );
    assert_eq!(
        document.comment_authors(),
        ["Lucía Ferrer", "lucia@correo.es"]
    );
    let literal = document.literal_texts();
    for expected in [
        "mailto:marta@correo.es",
        "\"Marta Lozano,Iván Prieto\"",
        "Iván Prieto",
        "'Nómina Marta Lozano'!$B$2",
    ] {
        assert!(literal.contains(&expected), "falta {expected}: {literal:?}");
    }
}

fn anonymized() -> (Vec<u8>, XlsxDocument, Vec<Replacement>) {
    let original = sample();
    let document = XlsxDocument::from_bytes(&original).unwrap();
    let replacements = vec![
        replacement(
            nth(&document, "Nómina Marta Lozano", 0),
            7,
            19,
            "[NOMBRE_PERSONA_0001]",
        ),
        replacement(
            nth(&document, "Marta Lozano", 0),
            0,
            12,
            "[NOMBRE_PERSONA_0001]",
        ),
        replacement(nth(&document, "654218903", 0), 0, 9, "[TELEFONO_0001]"),
        replacement(
            nth(&document, "14/02/2025", 0),
            0,
            10,
            "[FECHA_NACIMIENTO_0001]",
        ),
        replacement(
            nth(&document, "Marta Lozano", 1),
            0,
            12,
            "[NOMBRE_PERSONA_0001]",
        ),
        replacement(nth(&document, "654218903", 1), 0, 9, "[TELEFONO_0001]"),
        replacement(
            nth(&document, "Marta Lozano", 2),
            0,
            12,
            "[NOMBRE_PERSONA_0001]",
        ),
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
    (written, document, replacements)
}

#[test]
fn cells_and_shared_strings_are_rewritten_in_place() {
    let (written, _, _) = anonymized();
    let shared = part(&written, "xl/sharedStrings.xml");
    assert!(
        shared.contains("<si><r><t>[NOMBRE_PERSONA_0001]</t></r><r><rPr><b/></rPr><t></t></r><rPh sb=\"0\" eb=\"1\"><t>マルタ</t></rPh></si>"),
        "{shared}"
    );
    let sheet = part(&written, "xl/worksheets/sheet1.xml");
    // Un número sustituido pasa a ser texto en la celda.
    assert!(
        sheet.contains("<c r=\"B2\" t=\"inlineStr\"><is><t>[TELEFONO_0001]</t></is></c>"),
        "{sheet}"
    );
    assert!(
        sheet.contains(
            "<c r=\"C2\" s=\"1\" t=\"inlineStr\"><is><t>[FECHA_NACIMIENTO_0001]</t></is></c>"
        ),
        "{sheet}"
    );
    assert!(
        sheet.contains("<c r=\"D2\" s=\"0\"><v>1250.5</v></c>"),
        "{sheet}"
    );
    // Un resultado de fórmula sustituido pasa a ser texto y la fórmula sigue.
    assert!(
        sheet.contains("<c r=\"C3\" t=\"str\"><f>B2</f><v>[TELEFONO_0001]</v></c>"),
        "{sheet}"
    );
    assert!(
        sheet.contains("<c r=\"B3\" t=\"str\"><f>A2&amp;\"\"</f><v>[NOMBRE_PERSONA_0001]</v></c>"),
        "{sheet}"
    );
    // Literales dentro de las comillas de una fórmula y en los atributos.
    assert!(
        sheet.contains("<formula1>\"[NOMBRE_PERSONA_0001],[NOMBRE_PERSONA_0002]\"</formula1>"),
        "{sheet}"
    );
    assert!(
        sheet.contains("display=\"[NOMBRE_PERSONA_0001]\""),
        "{sheet}"
    );
    assert!(part(&written, "xl/worksheets/_rels/sheet1.xml.rels").contains("mailto:[EMAIL_0001]"));
    let pivot = part(&written, "xl/pivotCache/pivotCacheDefinition1.xml");
    assert!(
        pivot.contains("<s v=\"[NOMBRE_PERSONA_0001]\"/><s v=\"[NOMBRE_PERSONA_0002]\"/>"),
        "{pivot}"
    );
    assert!(part(&written, "xl/charts/chart1.xml").contains("<c:v>[NOMBRE_PERSONA_0001]</c:v>"));
}

#[test]
fn a_sheet_with_personal_data_in_its_name_is_renamed_everywhere() {
    let (written, _, _) = anonymized();
    let new = "Nómina NOMBRE_PERSONA_0001";
    let workbook = part(&written, "xl/workbook.xml");
    assert!(
        workbook.contains(&format!("<sheet name=\"{new}\"")),
        "{workbook}"
    );
    assert!(
        workbook.contains(&format!(
            "<definedName name=\"Telefono\">'{new}'!$B$2</definedName>"
        )),
        "{workbook}"
    );
    assert!(part(&written, "xl/worksheets/sheet2.xml").contains(&format!("<f>'{new}'!A2</f>")));
    let sheet = part(&written, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(&format!("location=\"'{new}'!A2\"")),
        "{sheet}"
    );
    assert!(part(&written, "xl/charts/chart1.xml").contains(&format!("<c:f>'{new}'!$A$2</c:f>")));
    assert!(
        part(&written, "xl/pivotCache/pivotCacheDefinition1.xml")
            .contains(&format!("sheet=\"{new}\""))
    );
    assert!(part(&written, "docProps/app.xml").contains(&format!("<vt:lpstr>{new}</vt:lpstr>")));
    for (_, data) in all_parts(&written) {
        assert!(!data.contains("Marta Lozano"), "queda el nombre: {data}");
    }
}

#[test]
fn authors_are_blanked() {
    let (written, _, _) = anonymized();
    let again = XlsxDocument::from_bytes(&written).unwrap();
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
    assert!(part(&written, "xl/comments1.xml").contains("<author></author>"));
}

#[test]
fn the_written_book_reads_back_with_the_markers() {
    let (written, _, _) = anonymized();
    let again = XlsxDocument::from_bytes(&written).unwrap();
    let texts: Vec<&str> = again.segments().iter().map(|s| s.text.as_str()).collect();
    for expected in [
        "Nómina NOMBRE_PERSONA_0001",
        "[NOMBRE_PERSONA_0001]",
        "[TELEFONO_0001]",
        "[FECHA_NACIMIENTO_0001]",
        "1250.5",
    ] {
        assert!(texts.contains(&expected), "falta {expected}: {texts:?}");
    }
}

#[test]
fn markdown_and_text_have_one_table_per_sheet() {
    let (_, document, replacements) = anonymized();
    assert_eq!(
        document.markdown(&replacements).unwrap(),
        "## Nómina NOMBRE_PERSONA_0001\n\n\
         | Nombre | Teléfono | Alta | Importe |\n\
         | --- | --- | --- | --- |\n\
         | [NOMBRE_PERSONA_0001] | [TELEFONO_0001] | [FECHA_NACIMIENTO_0001] | 1250.5 |\n\
         | Iván Prieto | [NOMBRE_PERSONA_0001] | [TELEFONO_0001] | VERDADERO |\n\n\
         ## Resumen\n\n\
         | Calle del Coso 87 | [NOMBRE_PERSONA_0001] |\n\
         | --- | --- |\n"
    );
    assert_eq!(
        document.plain_text(&replacements).unwrap(),
        "Nómina NOMBRE_PERSONA_0001\n\
         Nombre\tTeléfono\tAlta\tImporte\n\
         [NOMBRE_PERSONA_0001]\t[TELEFONO_0001]\t[FECHA_NACIMIENTO_0001]\t1250.5\n\
         Iván Prieto\t[NOMBRE_PERSONA_0001]\t[TELEFONO_0001]\tVERDADERO\n\
         \n\
         Resumen\n\
         Calle del Coso 87\t[NOMBRE_PERSONA_0001]\n"
    );
}

#[test]
fn a_long_sheet_name_keeps_only_its_markers_and_names_stay_unique() {
    let workbook = format!(
        "<workbook {MAIN}><sheets><sheet name=\"Nómina de Marta Lozano Ibáñez\" sheetId=\"1\" r:id=\"rId1\"/>\
         <sheet name=\"Marta\" sheetId=\"2\" r:id=\"rId2\"/><sheet name=\"Lozano\" sheetId=\"3\" r:id=\"rId3\"/></sheets></workbook>"
    );
    let empty = format!("<worksheet {MAIN}><sheetData/></worksheet>");
    let bytes = zip(&[
        ("xl/workbook.xml", workbook),
        (
            "xl/_rels/workbook.xml.rels",
            rels(&[
                ("rId1", "worksheet", "worksheets/sheet1.xml"),
                ("rId2", "worksheet", "worksheets/sheet2.xml"),
                ("rId3", "worksheet", "worksheets/sheet3.xml"),
            ]),
        ),
        ("xl/worksheets/sheet1.xml", empty.clone()),
        ("xl/worksheets/sheet2.xml", empty.clone()),
        ("xl/worksheets/sheet3.xml", empty),
    ]);
    let document = XlsxDocument::from_bytes(&bytes).unwrap();
    let replacements = [
        replacement(0, 10, 29, "[NOMBRE_PERSONA_0001]"),
        replacement(1, 0, 5, "[NOMBRE_PERSONA_0001]"),
        replacement(2, 0, 6, "[NOMBRE_PERSONA_0001]"),
    ];
    let written = document.write(&replacements, &[]).unwrap();
    let workbook = part(&written, "xl/workbook.xml");
    // «Nómina de NOMBRE_PERSONA_0001» tiene 29 caracteres: cabe.
    assert!(
        workbook.contains("name=\"Nómina de NOMBRE_PERSONA_0001\""),
        "{workbook}"
    );
    // Dos hojas que acabarían igual: la segunda se numera.
    assert!(
        workbook.contains("name=\"NOMBRE_PERSONA_0001\""),
        "{workbook}"
    );
    assert!(
        workbook.contains("name=\"NOMBRE_PERSONA_0001 (2)\""),
        "{workbook}"
    );

    let long = [replacement(
        0,
        10,
        29,
        "[NOMBRE_PERSONA_0001] [ID_DOCUMENTO_PERSONA_0001]",
    )];
    let workbook = part(&document.write(&long, &[]).unwrap(), "xl/workbook.xml");
    // Más de 31 caracteres: solo los marcadores que caben.
    assert!(
        workbook.contains("name=\"NOMBRE_PERSONA_0001\""),
        "{workbook}"
    );
}

#[test]
fn protected_files_give_a_clear_error() {
    let ole = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    assert!(matches!(
        XlsxDocument::from_bytes(&ole).unwrap_err(),
        Error::Protected(Office::Excel)
    ));
}

fn all_parts(bytes: &[u8]) -> Vec<(String, String)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            (file.name().unwrap().into_owned(), text)
        })
        .collect()
}
