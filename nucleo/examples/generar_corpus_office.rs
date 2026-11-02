//! Genera el corpus sintético de PowerPoint y Excel de `tests/corpus/pptx` y
//! `tests/corpus/xlsx`.
//!
//! Cada archivo es completo (tema, patrones, estilos) para que Office lo
//! abra sin pedir reparación, y reúne casos difíciles del formato: nombres
//! partidos en fragmentos con formato distinto, saltos de línea, tablas,
//! notas, comentarios y sus autores, texto alternativo de una imagen,
//! enlaces `mailto:`, una plantilla con un nombre; en Excel, una hoja con un
//! nombre de persona, números (teléfonos, fechas, importes), resultados de
//! fórmula, un nombre definido, una lista de validación, un vínculo y un pie
//! de página. Junto a cada uno va un JSON con lo que debe detectarse. Todos
//! los datos son inventados.
//!
//! La salida es determinista. Uso:
//! `cargo run -p nucleo --example generar_corpus_office [carpeta]`; sin
//! carpeta escribe en `tests/corpus`.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const SML: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT: &str = "application/vnd.openxmlformats-officedocument";

/// PNG de 1 × 1 píxel.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus"));
    generate(&dir)?;
    println!("Corpus de PowerPoint y Excel generado en {}", dir.display());
    Ok(())
}

/// Escribe los PPTX y XLSX del corpus y sus JSON en `dir/pptx` y `dir/xlsx`.
pub fn generate(dir: &Path) -> std::io::Result<()> {
    for (folder, name, bytes, expected) in [
        (
            "pptx",
            "es-01-presentacion-ventas",
            presentation()?,
            presentation_expected(),
        ),
        ("xlsx", "es-01-nominas", workbook()?, workbook_expected()),
    ] {
        let folder = dir.join(folder);
        fs::create_dir_all(&folder)?;
        fs::write(
            folder.join(format!("{name}.{}", folder_ext(&folder))),
            bytes,
        )?;
        let mut json = serde_json::to_vec_pretty(&expected)?;
        json.push(b'\n');
        fs::write(folder.join(format!("{name}.json")), json)?;
    }
    Ok(())
}

fn folder_ext(folder: &Path) -> String {
    folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn entities(list: &[(&str, &str)]) -> Value {
    Value::Array(
        list.iter()
            .map(|(kind, text)| json!({ "tipo": kind, "texto": text }))
            .collect(),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn relationships(items: &[(&str, &str, &str)]) -> String {
    let items: String = items
        .iter()
        .map(|(id, kind, target)| {
            let external = if target.contains(':') {
                " TargetMode=\"External\""
            } else {
                ""
            };
            format!(
                "<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{}\"{external}/>",
                escape(target)
            )
        })
        .collect();
    format!(
        "{DECLARATION}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{items}</Relationships>"
    )
}

fn content_types(defaults: &[(&str, &str)], overrides: &[(&str, String)]) -> String {
    let mut out = format!(
        "{DECLARATION}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>"
    );
    for (extension, kind) in defaults {
        out.push_str(&format!(
            "<Default Extension=\"{extension}\" ContentType=\"{kind}\"/>"
        ));
    }
    for (part, kind) in overrides {
        out.push_str(&format!(
            "<Override PartName=\"/{part}\" ContentType=\"{kind}\"/>"
        ));
    }
    out.push_str("</Types>");
    out
}

fn package_rels(main: &str) -> String {
    relationships(&[
        ("rId1", "officeDocument", main),
        ("rId2", "metadata/core-properties", "docProps/core.xml"),
        ("rId3", "extended-properties", "docProps/app.xml"),
    ])
    .replace(
        &format!("{REL}/metadata/core-properties"),
        "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
    )
}

fn core(title: &str, author: &str) -> String {
    format!(
        "{DECLARATION}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" \
         xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
         <dc:title>{}</dc:title><dc:creator>{}</dc:creator><cp:lastModifiedBy>{}</cp:lastModifiedBy>\
         <dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-03-02T09:00:00Z</dcterms:created>\
         <dcterms:modified xsi:type=\"dcterms:W3CDTF\">2026-03-02T09:00:00Z</dcterms:modified>\
         </cp:coreProperties>",
        escape(title),
        escape(author),
        escape(author)
    )
}

fn app(application: &str, company: &str, heading: &str, titles: &[&str]) -> String {
    let titles_xml: String = titles
        .iter()
        .map(|t| format!("<vt:lpstr>{}</vt:lpstr>", escape(t)))
        .collect();
    format!(
        "{DECLARATION}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" \
         xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
         <Application>{application}</Application>\
         <HeadingPairs><vt:vector size=\"2\" baseType=\"variant\"><vt:variant><vt:lpstr>{heading}</vt:lpstr></vt:variant>\
         <vt:variant><vt:i4>{}</vt:i4></vt:variant></vt:vector></HeadingPairs>\
         <TitlesOfParts><vt:vector size=\"{}\" baseType=\"lpstr\">{titles_xml}</vt:vector></TitlesOfParts>\
         <Company>{}</Company></Properties>",
        titles.len(),
        titles.len(),
        escape(company)
    )
}

/// ZIP con las partes en el orden dado y una fecha fija, para que dos
/// ejecuciones den los mismos bytes.
fn zip_parts(parts: &[(String, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        zip.start_file(name.as_str(), options)
            .map_err(std::io::Error::other)?;
        zip.write_all(data)?;
    }
    Ok(zip.finish().map_err(std::io::Error::other)?.into_inner())
}

// --------------------------------------------------------------- PowerPoint

/// Carrera de texto con propiedades opcionales.
fn run(text: &str, properties: &str) -> String {
    format!(
        "<a:r><a:rPr lang=\"es-ES\" dirty=\"0\"{properties}/><a:t>{}</a:t></a:r>",
        escape(text)
    )
}

fn para(content: &str) -> String {
    format!("<a:p>{content}</a:p>")
}

fn text_shape(id: u32, name: &str, placeholder: &str, xfrm: &str, paragraphs: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>\
         <p:nvPr>{placeholder}</p:nvPr></p:nvSpPr><p:spPr>{xfrm}</p:spPr>\
         <p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"
    )
}

fn xfrm(x: u64, y: u64, cx: u64, cy: u64) -> String {
    format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>")
}

fn tree(shapes: &str) -> String {
    format!(
        "<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/>\
         <a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld>"
    )
}

fn table(rows: &[&[&str]]) -> String {
    let columns = rows[0].len();
    let grid: String = (0..columns)
        .map(|_| "<a:gridCol w=\"3048000\"/>".to_owned())
        .collect();
    let body: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .map(|cell| {
                    format!(
                        "<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>{}</a:txBody><a:tcPr/></a:tc>",
                        para(&run(cell, ""))
                    )
                })
                .collect();
            format!("<a:tr h=\"370840\">{cells}</a:tr>")
        })
        .collect();
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"4\" name=\"Tabla 3\"/>\
         <p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>\
         <p:xfrm><a:off x=\"1524000\" y=\"1981200\"/><a:ext cx=\"9144000\" cy=\"1112520\"/></p:xfrm>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\
         <a:tbl><a:tblPr firstRow=\"1\" bandRow=\"1\"/><a:tblGrid>{grid}</a:tblGrid>{body}</a:tbl>\
         </a:graphicData></a:graphic></p:graphicFrame>"
    )
}

fn sld(kind: &str, shapes: &str, extra: &str) -> String {
    format!(
        "{DECLARATION}<p:{kind} xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\">{}{extra}</p:{kind}>",
        tree(shapes)
    )
}

fn theme(name: &str) -> String {
    let solid = |color: &str| format!("<a:solidFill><a:schemeClr val=\"{color}\"/></a:solidFill>");
    let line = format!("<a:ln w=\"6350\">{}</a:ln>", solid("phClr"));
    let fonts = "<a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>";
    format!(
        "{DECLARATION}<a:theme xmlns:a=\"{A}\" name=\"{name}\"><a:themeElements>\
         <a:clrScheme name=\"Office\"><a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>\
         <a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1><a:dk2><a:srgbClr val=\"44546A\"/></a:dk2>\
         <a:lt2><a:srgbClr val=\"E7E6E6\"/></a:lt2><a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1>\
         <a:accent2><a:srgbClr val=\"ED7D31\"/></a:accent2><a:accent3><a:srgbClr val=\"A5A5A5\"/></a:accent3>\
         <a:accent4><a:srgbClr val=\"FFC000\"/></a:accent4><a:accent5><a:srgbClr val=\"5B9BD5\"/></a:accent5>\
         <a:accent6><a:srgbClr val=\"70AD47\"/></a:accent6><a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink>\
         <a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink></a:clrScheme>\
         <a:fontScheme name=\"Office\"><a:majorFont>{fonts}</a:majorFont><a:minorFont>{fonts}</a:minorFont></a:fontScheme>\
         <a:fmtScheme name=\"Office\"><a:fillStyleLst>{s}{s}{s}</a:fillStyleLst>\
         <a:lnStyleLst>{line}{line}{line}</a:lnStyleLst>\
         <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle>\
         <a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>\
         <a:bgFillStyleLst>{s}{s}{s}</a:bgFillStyleLst></a:fmtScheme></a:themeElements>\
         <a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
        s = solid("phClr")
    )
}

const CLR_MAP: &str = "<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" \
                       accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>";

fn presentation() -> std::io::Result<Vec<u8>> {
    let title_ph = "<p:ph type=\"title\"/>";
    let body_ph = "<p:ph idx=\"1\"/>";
    let slide1 = sld(
        "sld",
        &[
            text_shape(
                2,
                "Título 1",
                title_ph,
                "",
                &para(&[
                    run("Resultados de ventas de ", ""),
                    run("Marta ", " b=\"1\""),
                    run("Lozano", " b=\"1\" i=\"1\""),
                    run(" Ibáñez", ""),
                ]
                .concat()),
            ),
            text_shape(
                3,
                "Marcador de contenido 2",
                body_ph,
                "",
                &[
                    para(&[
                        run("Responsable: Marta Lozano Ibáñez, DNI ", ""),
                        run("4829", ""),
                        run("1736Q", " u=\"sng\""),
                    ]
                    .concat()),
                    para(&run("Teléfono: 654 218 903", "")),
                    para(&format!(
                        "{}<a:r><a:rPr lang=\"es-ES\" dirty=\"0\"><a:hlinkClick r:id=\"rId3\"/></a:rPr><a:t>marta.lozano@correo-ejemplo.es</a:t></a:r>",
                        run("Correo: ", "")
                    )),
                    para(&format!(
                        "{}<a:br><a:rPr lang=\"es-ES\" dirty=\"0\"/></a:br>{}",
                        run("Oficina: Calle del Coso 87,", ""),
                        run("50001 Zaragoza", "")
                    )),
                ]
                .concat(),
            ),
            format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"5\" name=\"Imagen 4\" descr=\"Foto de Marta Lozano Ibáñez\"/>\
                 <p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>\
                 <p:blipFill><a:blip r:embed=\"rId4\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>\
                 <p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
                xfrm(9144000, 4572000, 914400, 914400)
            ),
        ]
        .concat(),
        "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",
    );
    let slide2 = sld(
        "sld",
        &[
            text_shape(
                2,
                "Título 1",
                title_ph,
                "",
                &para(&run("Equipo comercial", "")),
            ),
            table(&[
                &["Nombre", "Cargo", "Teléfono"],
                &["Iván Prieto Salas", "Comercial", "612 384 907"],
                &["Lucía Ferrer Gil", "Directora", "699 112 233"],
            ]),
        ]
        .concat(),
        "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",
    );
    let notes = sld(
        "notes",
        &[
            text_shape(2, "Marcador de imagen de diapositiva 1", "<p:ph type=\"sldImg\"/>", "", ""),
            text_shape(
                3,
                "Marcador de notas 2",
                "<p:ph type=\"body\" idx=\"1\"/>",
                "",
                &para(&run("Recordar llamar a Iván Prieto Salas al 612 384 907.", "")),
            ),
        ]
        .concat()
        .replace("<p:txBody><a:bodyPr/><a:lstStyle/></p:txBody>", "<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"es-ES\"/></a:p></p:txBody>"),
        "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",
    );
    let master = sld(
        "sldMaster",
        &[
            text_shape(
                2,
                "Título",
                title_ph,
                &xfrm(838200, 365125, 10515600, 1325563),
                &para("<a:endParaRPr lang=\"es-ES\"/>"),
            ),
            text_shape(
                3,
                "Pie de página",
                "<p:ph type=\"ftr\" sz=\"quarter\" idx=\"3\"/>",
                &xfrm(4038600, 6356350, 4114800, 365125),
                &para(&run("Preparado por Andrés Molina Peña", "")),
            ),
        ]
        .concat(),
        &format!(
            "{CLR_MAP}<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst>"
        ),
    );
    let layout = sld(
        "sldLayout",
        &text_shape(
            2,
            "Título 1",
            title_ph,
            "",
            &para("<a:endParaRPr lang=\"es-ES\"/>"),
        ),
        "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",
    )
    .replace("<p:sldLayout ", "<p:sldLayout preserve=\"1\" ");
    let notes_master = sld("notesMaster", "", CLR_MAP);
    let presentation = format!(
        "{DECLARATION}<p:presentation xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\" saveSubsetFonts=\"1\">\
         <p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
         <p:notesMasterIdLst><p:notesMasterId r:id=\"rId4\"/></p:notesMasterIdLst>\
         <p:sldIdLst><p:sldId id=\"256\" r:id=\"rId2\"/><p:sldId id=\"257\" r:id=\"rId3\"/></p:sldIdLst>\
         <p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    );
    let comment_authors = format!(
        "{DECLARATION}<p:cmAuthorLst xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\">\
         <p:cmAuthor id=\"1\" name=\"Andrés Molina Peña\" initials=\"AM\" lastIdx=\"1\" clrIdx=\"0\"/></p:cmAuthorLst>"
    );
    let comments = format!(
        "{DECLARATION}<p:cmLst xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\">\
         <p:cm authorId=\"1\" dt=\"2026-03-02T09:00:00.000\" idx=\"1\"><p:pos x=\"10\" y=\"10\"/>\
         <p:text>Confirmar el teléfono de Lucía Ferrer Gil</p:text></p:cm></p:cmLst>"
    );
    let pml = format!("{CT}.presentationml");
    let parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            content_types(
                &[("png", "image/png")],
                &[
                    ("ppt/presentation.xml", format!("{pml}.presentation.main+xml")),
                    ("ppt/slideMasters/slideMaster1.xml", format!("{pml}.slideMaster+xml")),
                    ("ppt/slideLayouts/slideLayout1.xml", format!("{pml}.slideLayout+xml")),
                    ("ppt/slides/slide1.xml", format!("{pml}.slide+xml")),
                    ("ppt/slides/slide2.xml", format!("{pml}.slide+xml")),
                    ("ppt/notesSlides/notesSlide1.xml", format!("{pml}.notesSlide+xml")),
                    ("ppt/notesMasters/notesMaster1.xml", format!("{pml}.notesMaster+xml")),
                    ("ppt/theme/theme1.xml", format!("{CT}.theme+xml")),
                    ("ppt/theme/theme2.xml", format!("{CT}.theme+xml")),
                    ("ppt/presProps.xml", format!("{pml}.presProps+xml")),
                    ("ppt/viewProps.xml", format!("{pml}.viewProps+xml")),
                    ("ppt/tableStyles.xml", format!("{pml}.tableStyles+xml")),
                    ("ppt/commentAuthors.xml", format!("{pml}.commentAuthors+xml")),
                    ("ppt/comments/comment1.xml", format!("{pml}.comments+xml")),
                    ("docProps/core.xml", "application/vnd.openxmlformats-package.core-properties+xml".into()),
                    ("docProps/app.xml", format!("{CT}.extended-properties+xml")),
                ],
            )
            .into(),
        ),
        ("_rels/.rels".into(), package_rels("ppt/presentation.xml").into()),
        ("ppt/presentation.xml".into(), presentation.into()),
        (
            "ppt/_rels/presentation.xml.rels".into(),
            relationships(&[
                ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
                ("rId2", "slide", "slides/slide1.xml"),
                ("rId3", "slide", "slides/slide2.xml"),
                ("rId4", "notesMaster", "notesMasters/notesMaster1.xml"),
                ("rId5", "presProps", "presProps.xml"),
                ("rId6", "viewProps", "viewProps.xml"),
                ("rId7", "theme", "theme/theme1.xml"),
                ("rId8", "tableStyles", "tableStyles.xml"),
                ("rId9", "commentAuthors", "commentAuthors.xml"),
            ])
            .into(),
        ),
        ("ppt/slideMasters/slideMaster1.xml".into(), master.into()),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
            relationships(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "theme", "../theme/theme1.xml"),
            ])
            .into(),
        ),
        ("ppt/slideLayouts/slideLayout1.xml".into(), layout.into()),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
            relationships(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]).into(),
        ),
        ("ppt/slides/slide1.xml".into(), slide1.into()),
        (
            "ppt/slides/_rels/slide1.xml.rels".into(),
            relationships(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "notesSlide", "../notesSlides/notesSlide1.xml"),
                ("rId3", "hyperlink", "mailto:marta.lozano@correo-ejemplo.es"),
                ("rId4", "image", "../media/image1.png"),
            ])
            .into(),
        ),
        ("ppt/slides/slide2.xml".into(), slide2.into()),
        (
            "ppt/slides/_rels/slide2.xml.rels".into(),
            relationships(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "comments", "../comments/comment1.xml"),
            ])
            .into(),
        ),
        ("ppt/notesSlides/notesSlide1.xml".into(), notes.into()),
        (
            "ppt/notesSlides/_rels/notesSlide1.xml.rels".into(),
            relationships(&[
                ("rId1", "notesMaster", "../notesMasters/notesMaster1.xml"),
                ("rId2", "slide", "../slides/slide1.xml"),
            ])
            .into(),
        ),
        ("ppt/notesMasters/notesMaster1.xml".into(), notes_master.into()),
        (
            "ppt/notesMasters/_rels/notesMaster1.xml.rels".into(),
            relationships(&[("rId1", "theme", "../theme/theme2.xml")]).into(),
        ),
        ("ppt/theme/theme1.xml".into(), theme("Tema de Office").into()),
        ("ppt/theme/theme2.xml".into(), theme("Tema de Office").into()),
        ("ppt/media/image1.png".into(), PIXEL.to_vec()),
        (
            "ppt/presProps.xml".into(),
            format!("{DECLARATION}<p:presentationPr xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\"/>").into(),
        ),
        (
            "ppt/viewProps.xml".into(),
            format!("{DECLARATION}<p:viewPr xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:p=\"{P}\"/>").into(),
        ),
        (
            "ppt/tableStyles.xml".into(),
            format!("{DECLARATION}<a:tblStyleLst xmlns:a=\"{A}\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>").into(),
        ),
        ("ppt/commentAuthors.xml".into(), comment_authors.into()),
        ("ppt/comments/comment1.xml".into(), comments.into()),
        ("docProps/core.xml".into(), core("Resultados de Marta Lozano Ibáñez", "Andrés Molina Peña").into()),
        (
            "docProps/app.xml".into(),
            app(
                "Microsoft Office PowerPoint",
                "Distribuciones Ejemplo SL",
                "Títulos de diapositiva",
                &["Resultados de ventas de Marta Lozano Ibáñez", "Equipo comercial"],
            )
            .into(),
        ),
    ];
    zip_parts(&parts)
}

fn presentation_expected() -> Value {
    json!({
        "entidades": entities(&[
            ("NOMBRE_PERSONA", "Marta Lozano Ibáñez"),
            ("ID_DOCUMENTO_PERSONA", "48291736Q"),
            ("TELEFONO", "654 218 903"),
            ("EMAIL", "marta.lozano@correo-ejemplo.es"),
            ("DIRECCION_POSTAL", "Calle del Coso 87, 50001 Zaragoza"),
            ("NOMBRE_PERSONA", "Iván Prieto Salas"),
            ("TELEFONO", "612 384 907"),
            ("NOMBRE_PERSONA", "Lucía Ferrer Gil"),
            ("TELEFONO", "699 112 233"),
            ("NOMBRE_PERSONA", "Andrés Molina Peña"),
        ]),
        "autores": ["Andrés Molina Peña", "AM", "Distribuciones Ejemplo SL"],
        "imagenes": 1,
        "objetos": 0
    })
}

// -------------------------------------------------------------------- Excel

/// Celda con texto compartido.
fn shared(reference: &str, index: usize) -> String {
    format!("<c r=\"{reference}\" t=\"s\"><v>{index}</v></c>")
}

fn number(reference: &str, value: &str, style: u32) -> String {
    format!("<c r=\"{reference}\" s=\"{style}\"><v>{value}</v></c>")
}

fn workbook() -> std::io::Result<Vec<u8>> {
    // Textos compartidos: el nombre de Marta va partido en dos carreras.
    let strings = [
        "Nombre",
        "DNI",
        "Teléfono",
        "Fecha de nacimiento",
        "Importe",
        "Correo",
        "",
        "48291736Q",
        "marta.lozano@correo-ejemplo.es",
        "Iván Prieto Salas",
        "X4721930F",
        "Total nóminas",
        "Revisado por Andrés Molina Peña",
    ];
    let items: String = strings
        .iter()
        .enumerate()
        .map(|(index, text)| {
            if index == 6 {
                "<si><r><t xml:space=\"preserve\">Marta </t></r><r><rPr><b/><sz val=\"11\"/><rFont val=\"Calibri\"/></rPr>\
                 <t>Lozano Ibáñez</t></r></si>"
                    .to_owned()
            } else {
                format!("<si><t>{}</t></si>", escape(text))
            }
        })
        .collect();
    let shared_strings = format!(
        "{DECLARATION}<sst xmlns=\"{SML}\" count=\"{}\" uniqueCount=\"{}\">{items}</sst>",
        strings.len() + 1,
        strings.len()
    );
    let sheet1 = format!(
        "{DECLARATION}<worksheet xmlns=\"{SML}\" xmlns:r=\"{REL}\"><dimension ref=\"A1:F5\"/>\
         <sheetData>\
         <row r=\"1\">{}{}{}{}{}{}</row>\
         <row r=\"2\">{}{}{}{}{}{}</row>\
         <row r=\"3\">{}{}{}{}{}</row>\
         <row r=\"5\"><c r=\"A5\" t=\"str\"><f>A2&amp;\" - \"&amp;C2</f><v>Marta Lozano Ibáñez - 654218903</v></c>\
         <c r=\"C5\"><f>C3</f><v>612384907</v></c></row>\
         </sheetData>\
         <dataValidations count=\"1\"><dataValidation type=\"list\" allowBlank=\"1\" showInputMessage=\"1\" showErrorMessage=\"1\" sqref=\"A7\">\
         <formula1>\"Marta Lozano Ibáñez,Iván Prieto Salas\"</formula1></dataValidation></dataValidations>\
         <hyperlinks><hyperlink ref=\"F2\" r:id=\"rId1\" display=\"marta.lozano@correo-ejemplo.es\"/></hyperlinks>\
         <pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>\
         <headerFooter><oddFooter>&amp;LPreparado por Andrés Molina Peña&amp;RPágina &amp;P</oddFooter></headerFooter>\
         </worksheet>",
        shared("A1", 0),
        shared("B1", 1),
        shared("C1", 2),
        shared("D1", 3),
        shared("E1", 4),
        shared("F1", 5),
        shared("A2", 6),
        shared("B2", 7),
        number("C2", "654218903", 0),
        number("D2", "31198", 1),
        number("E2", "2150.75", 2),
        shared("F2", 8),
        shared("A3", 9),
        shared("B3", 10),
        number("C3", "612384907", 0),
        number("D3", "32918", 1),
        number("E3", "1890", 2),
    );
    let sheet2 = format!(
        "{DECLARATION}<worksheet xmlns=\"{SML}\" xmlns:r=\"{REL}\"><dimension ref=\"A1:B2\"/><sheetData>\
         <row r=\"1\">{}<c r=\"B1\" s=\"2\"><f>SUM('Nómina Marta Lozano'!E2:E3)</f><v>4040.75</v></c></row>\
         <row r=\"2\">{}</row></sheetData>\
         <pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/></worksheet>",
        shared("A1", 11),
        shared("A2", 12),
    );
    let book = format!(
        "{DECLARATION}<workbook xmlns=\"{SML}\" xmlns:r=\"{REL}\"><bookViews><workbookView/></bookViews><sheets>\
         <sheet name=\"Nómina Marta Lozano\" sheetId=\"1\" r:id=\"rId1\"/><sheet name=\"Resumen\" sheetId=\"2\" r:id=\"rId2\"/></sheets>\
         <definedNames><definedName name=\"Telefono_responsable\">'Nómina Marta Lozano'!$C$2</definedName></definedNames>\
         <calcPr calcId=\"191029\"/></workbook>"
    );
    let styles = format!(
        "{DECLARATION}<styleSheet xmlns=\"{SML}\"><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"#,##0.00\\ &quot;€&quot;\"/></numFmts>\
         <fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/><family val=\"2\"/></font></fonts>\
         <fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"3\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
         <xf numFmtId=\"14\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>\
         <xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs>\
         <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>"
    );
    let sml = format!("{CT}.spreadsheetml");
    let parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            content_types(
                &[],
                &[
                    ("xl/workbook.xml", format!("{sml}.sheet.main+xml")),
                    ("xl/worksheets/sheet1.xml", format!("{sml}.worksheet+xml")),
                    ("xl/worksheets/sheet2.xml", format!("{sml}.worksheet+xml")),
                    ("xl/theme/theme1.xml", format!("{CT}.theme+xml")),
                    ("xl/styles.xml", format!("{sml}.styles+xml")),
                    ("xl/sharedStrings.xml", format!("{sml}.sharedStrings+xml")),
                    (
                        "docProps/core.xml",
                        "application/vnd.openxmlformats-package.core-properties+xml".into(),
                    ),
                    ("docProps/app.xml", format!("{CT}.extended-properties+xml")),
                ],
            )
            .into(),
        ),
        ("_rels/.rels".into(), package_rels("xl/workbook.xml").into()),
        ("xl/workbook.xml".into(), book.into()),
        (
            "xl/_rels/workbook.xml.rels".into(),
            relationships(&[
                ("rId1", "worksheet", "worksheets/sheet1.xml"),
                ("rId2", "worksheet", "worksheets/sheet2.xml"),
                ("rId3", "theme", "theme/theme1.xml"),
                ("rId4", "styles", "styles.xml"),
                ("rId5", "sharedStrings", "sharedStrings.xml"),
            ])
            .into(),
        ),
        ("xl/worksheets/sheet1.xml".into(), sheet1.into()),
        (
            "xl/worksheets/_rels/sheet1.xml.rels".into(),
            relationships(&[("rId1", "hyperlink", "mailto:marta.lozano@correo-ejemplo.es")]).into(),
        ),
        ("xl/worksheets/sheet2.xml".into(), sheet2.into()),
        ("xl/theme/theme1.xml".into(), theme("Tema de Office").into()),
        ("xl/styles.xml".into(), styles.into()),
        ("xl/sharedStrings.xml".into(), shared_strings.into()),
        (
            "docProps/core.xml".into(),
            core("Nóminas de marzo", "Andrés Molina Peña").into(),
        ),
        (
            "docProps/app.xml".into(),
            app(
                "Microsoft Excel",
                "Distribuciones Ejemplo SL",
                "Hojas de cálculo",
                &["Nómina Marta Lozano", "Resumen"],
            )
            .into(),
        ),
    ];
    zip_parts(&parts)
}

fn workbook_expected() -> Value {
    json!({
        "entidades": entities(&[
            ("NOMBRE_PERSONA", "Marta Lozano Ibáñez"),
            ("NOMBRE_PERSONA", "Marta Lozano"),
            ("ID_DOCUMENTO_PERSONA", "48291736Q"),
            ("TELEFONO", "654218903"),
            ("FECHA_NACIMIENTO", "31/05/1985"),
            ("IMPORTE", "2150.75"),
            ("EMAIL", "marta.lozano@correo-ejemplo.es"),
            ("NOMBRE_PERSONA", "Iván Prieto Salas"),
            ("ID_DOCUMENTO_PERSONA", "X4721930F"),
            ("TELEFONO", "612384907"),
            ("FECHA_NACIMIENTO", "14/02/1990"),
            ("IMPORTE", "1890"),
            ("NOMBRE_PERSONA", "Andrés Molina Peña"),
        ]),
        "autores": ["Andrés Molina Peña", "Distribuciones Ejemplo SL"],
        "hoja_renombrada": ["Nómina Marta Lozano", "Nómina NOMBRE_PERSONA"],
        "imagenes": 0,
        "objetos": 0
    })
}
