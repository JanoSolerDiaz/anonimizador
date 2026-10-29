//! Genera el corpus DOCX sintético de `tests/corpus/docx`.
//!
//! Cada DOCX es mínimo pero válido y reúne casos difíciles del formato:
//! nombres partidos en fragmentos con formato distinto, tablas, cuadros de
//! texto, encabezados y pies, notas, comentarios, control de cambios,
//! enlaces `mailto:`, códigos de campo, metadatos de autor, una imagen y un
//! objeto incrustado. Junto a cada uno va un JSON con lo que debe detectarse.
//! Todos los datos son inventados.
//!
//! La salida es determinista: fechas fijas dentro del ZIP y el mismo orden
//! de partes. Uso: `cargo run -p nucleo --example generar_corpus_docx [carpeta]`.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus/docx"));
    fs::create_dir_all(&dir)?;
    generate(&dir)?;
    println!("Corpus DOCX generado en {}", dir.display());
    Ok(())
}

/// Escribe todos los DOCX del corpus y sus JSON en `dir`.
pub fn generate(dir: &Path) -> std::io::Result<()> {
    for (name, docx, expected) in [contrato_es(), informe_ca(), review_en(), fiche_fr()] {
        fs::write(dir.join(format!("{name}.docx")), docx.build()?)?;
        write_json(&dir.join(format!("{name}.json")), &expected)?;
    }
    fs::write(dir.join("es-02-protegido.docx"), protected_header())?;
    write_json(
        &dir.join("es-02-protegido.json"),
        &json!({
            "protegido": true,
            "nota": "Solo la cabecera de un archivo OLE, que es lo que tiene un DOCX con contraseña. Basta para reconocerlo; Word no lo abre."
        }),
    )
}

fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn entities(list: &[(&str, &str)]) -> Value {
    Value::Array(
        list.iter()
            .map(|(kind, text)| json!({ "tipo": kind, "texto": text }))
            .collect(),
    )
}

// ------------------------------------------------------------ documentos

fn contrato_es() -> (&'static str, Docx, Value) {
    let pablo_mail = "pablo.navarro@correo-ejemplo.es";
    let body = [
        heading(1, "Contrato de arrendamiento de vivienda"),
        // El nombre y el DNI van partidos en fragmentos con formato distinto.
        p(&[
            run("En "),
            run("Valencia"),
            run(", a 3 de marzo de 2026, de una parte doña "),
            styled("<w:b/>", "Lucía "),
            styled("<w:b/><w:i/>", "Fer"),
            styled("<w:color w:val=\"C00000\"/>", "rer Ibáñez"),
            run(", con DNI "),
            run("4829"),
            styled("<w:u w:val=\"single\"/>", "1736K"),
            run(", como arrendadora."),
        ]),
        heading(2, "Datos de contacto"),
        list_item(1, &[run("Teléfono: 612 384 907")]),
        list_item(
            1,
            &[
                run("Correo: "),
                hyperlink("rIdEnlace1", "lucia.ferrer@correo-ejemplo.es"),
            ],
        ),
        heading(2, "Condiciones"),
        list_item(2, &[run("Renta mensual de 950 euros.")]),
        list_item(
            2,
            &[run(
                "Fianza ingresada en la cuenta ES91 2100 0418 4502 0005 1332.",
            )],
        ),
        table(&[
            &["Parte", "Nombre", "DNI"],
            &["Arrendadora", "Lucía Ferrer Ibáñez", "48291736K"],
            &["Arrendatario", "Pablo Navarro Gil", "53720914R"],
        ]),
        p(&[
            run("Para cualquier duda escriba a "),
            field(&format!(" HYPERLINK \"mailto:{pablo_mail}\" "), pablo_mail),
            run("."),
        ]),
        p(&[r(&[t("Firma:"), TAB.into(), t("Pablo Navarro Gil")])]),
        p(&[r(&[
            t("Domicilio: Calle de la Paz 14, 3.º B"),
            BR.into(),
            t("46003 Valencia"),
        ])]),
    ]
    .concat();
    let docx = Docx {
        body,
        lists: true,
        links: vec!["mailto:lucia.ferrer@correo-ejemplo.es".into()],
        core: Core {
            title: "Contrato de arrendamiento con Lucía Ferrer Ibáñez",
            subject: "Arrendamiento",
            keywords: "contrato; vivienda",
            description: "",
            creator: "Marta Salvador Ruiz",
            last_modified_by: "Andrés Molina Peña",
            company: "Inmobiliaria Ejemplo Levante SL",
        },
        ..Docx::default()
    };
    let expected = json!({
        "entidades": entities(&[
            ("LUGAR", "Valencia"),
            ("NOMBRE_PERSONA", "Lucía Ferrer Ibáñez"),
            ("ID_DOCUMENTO_PERSONA", "48291736K"),
            ("TELEFONO", "612 384 907"),
            ("EMAIL", "lucia.ferrer@correo-ejemplo.es"),
            ("IMPORTE", "950 euros"),
            ("CUENTA_BANCARIA", "ES91 2100 0418 4502 0005 1332"),
            ("NOMBRE_PERSONA", "Pablo Navarro Gil"),
            ("ID_DOCUMENTO_PERSONA", "53720914R"),
            ("EMAIL", pablo_mail),
            ("DIRECCION_POSTAL", "Calle de la Paz 14, 3.º B 46003 Valencia"),
        ]),
        "metadatos_autor": ["Marta Salvador Ruiz", "Andrés Molina Peña", "Inmobiliaria Ejemplo Levante SL"],
        "autores_revision": [],
        "imagenes": 0,
        "objetos": 0
    });
    ("es-01-contrato-arrendamiento", docx, expected)
}

fn informe_ca() -> (&'static str, Docx, Value) {
    let body = [
        heading(1, "Informe d'auditoria"),
        p(&[
            run("La reunió es va fer a Girona amb "),
            styled("<w:u w:val=\"single\"/>", "Núria "),
            styled("<w:u w:val=\"single\"/><w:b/>", "Vidal Roca"),
            run("."),
            note_reference("footnoteReference", 1),
        ]),
        p(&[
            run("L'expedient es va tancar el 14 de maig de 2026."),
            note_reference("endnoteReference", 1),
        ]),
    ]
    .concat();
    let docx = Docx {
        body,
        header: Some(p(&[run(
            "Informe confidencial, redactat per Jordi Puig Soler",
        )])),
        footer: Some(p(&[run("Contacte: 677 123 450")])),
        footnotes: vec![p(&[
            note_mark("footnoteRef"),
            run(" Segons l'acta signada per Núria Vidal Roca."),
        ])],
        endnotes: vec![p(&[
            note_mark("endnoteRef"),
            run(" Expedient enviat a nuria.vidal@correu-exemple.cat."),
        ])],
        core: Core {
            title: "Informe d'auditoria",
            subject: "",
            keywords: "",
            description: "Revisió anual",
            creator: "Pere Martí Font",
            last_modified_by: "Pere Martí Font",
            company: "Auditors Exemple SCP",
        },
        ..Docx::default()
    };
    let expected = json!({
        "entidades": entities(&[
            ("NOMBRE_PERSONA", "Jordi Puig Soler"),
            ("TELEFONO", "677 123 450"),
            ("LUGAR", "Girona"),
            ("NOMBRE_PERSONA", "Núria Vidal Roca"),
            ("EMAIL", "nuria.vidal@correu-exemple.cat"),
        ]),
        "metadatos_autor": ["Pere Martí Font", "Auditors Exemple SCP"],
        "autores_revision": [],
        "imagenes": 0,
        "objetos": 0
    });
    ("ca-01-informe-auditoria", docx, expected)
}

fn review_en() -> (&'static str, Docx, Value) {
    let reviser = "author=\"Samuel Price\" w:date=\"2026-04-03T09:00:00Z\"";
    let body = [
        heading(1, "Annual performance review"),
        p(&[
            run("Employee: "),
            "<w:commentRangeStart w:id=\"0\"/>".to_owned(),
            run("Daniel Brooks"),
            "<w:commentRangeEnd w:id=\"0\"/>".to_owned(),
            "<w:r><w:commentReference w:id=\"0\"/></w:r>".to_owned(),
            run("."),
        ]),
        // Control de cambios: un nombre borrado y otro insertado.
        p(&[
            run("Reviewed by "),
            format!("<w:del w:id=\"1\" w:{reviser}><w:r><w:delText xml:space=\"preserve\">Oliver Hart </w:delText></w:r></w:del>"),
            format!("<w:ins w:id=\"2\" w:{reviser}>{}</w:ins>", run("Emma Clarke")),
            run("."),
        ]),
        p(&[r(&[t("Salary:"), TAB.into(), t("£48,500 per year")])]),
        p(&[r(&[
            t("Home address: 22 Baker Lane"),
            BR.into(),
            t("Leeds LS1 4DT"),
        ])]),
    ]
    .concat();
    let docx = Docx {
        body,
        comments: vec![Comment {
            author: "Rachel Turner",
            initials: "RT",
            text: "Please confirm the phone number +44 7700 900123 with HR.",
        }],
        core: Core {
            title: "Performance review",
            subject: "",
            keywords: "",
            description: "",
            creator: "Rachel Turner",
            last_modified_by: "Samuel Price",
            company: "Example Analytics Ltd",
        },
        ..Docx::default()
    };
    let expected = json!({
        "entidades": entities(&[
            ("NOMBRE_PERSONA", "Daniel Brooks"),
            ("NOMBRE_PERSONA", "Oliver Hart"),
            ("NOMBRE_PERSONA", "Emma Clarke"),
            ("IMPORTE", "£48,500"),
            ("DIRECCION_POSTAL", "22 Baker Lane Leeds LS1 4DT"),
            ("TELEFONO", "+44 7700 900123"),
        ]),
        "metadatos_autor": ["Rachel Turner", "Samuel Price", "Example Analytics Ltd"],
        "autores_revision": ["Rachel Turner", "RT", "Samuel Price"],
        "imagenes": 0,
        "objetos": 0
    });
    ("en-01-employment-review", docx, expected)
}

fn fiche_fr() -> (&'static str, Docx, Value) {
    let body = [
        heading(1, "Fiche client"),
        p(&[
            run("Cliente : "),
            styled("<w:i/>", "Camille "),
            styled("<w:b/>", "Lefèvre"),
            run(", née à Lyon."),
        ]),
        p(&[text_box(&p(&[run(
            "Contact : camille.lefevre@exemple-courriel.fr, 06 12 34 56 78",
        )]))]),
        p(&[image("rIdImagen1")]),
        p(&[embedded_object("rIdVista1", "rIdObjeto1")]),
        p(&[run("Adresse : 15 rue des Lilas, 69003 Lyon")]),
        p(&[run("IBAN : FR76 3000 6000 0112 3456 7890 189")]),
    ]
    .concat();
    let docx = Docx {
        body,
        image: true,
        object: true,
        core: Core {
            title: "Fiche client",
            subject: "",
            keywords: "",
            description: "",
            creator: "Julien Moreau",
            last_modified_by: "Julien Moreau",
            company: "Exemple Conseil SARL",
        },
        ..Docx::default()
    };
    let expected = json!({
        "entidades": entities(&[
            ("NOMBRE_PERSONA", "Camille Lefèvre"),
            ("LUGAR", "Lyon"),
            ("EMAIL", "camille.lefevre@exemple-courriel.fr"),
            ("TELEFONO", "06 12 34 56 78"),
            ("DIRECCION_POSTAL", "15 rue des Lilas, 69003 Lyon"),
            ("CUENTA_BANCARIA", "FR76 3000 6000 0112 3456 7890 189"),
        ]),
        "metadatos_autor": ["Julien Moreau", "Exemple Conseil SARL"],
        "autores_revision": [],
        "imagenes": 1,
        "objetos": 1
    });
    ("fr-01-fiche-client", docx, expected)
}

/// Cabecera de un archivo OLE (formato de archivo compuesto), que es lo que
/// es un DOCX cifrado con contraseña. Sin directorio ni flujos.
fn protected_header() -> Vec<u8> {
    let mut header = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    header.extend_from_slice(&[0; 16]); // CLSID
    header.extend_from_slice(&[0x3E, 0x00, 0x03, 0x00]); // versión 3.62
    header.extend_from_slice(&[0xFE, 0xFF, 0x09, 0x00, 0x06, 0x00]); // orden, sectores
    header.resize(512, 0xFF);
    header
}

// ------------------------------------------------------- piezas de XML

const NAMESPACES: &str = concat!(
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" ",
    "xmlns:v=\"urn:schemas-microsoft-com:vml\" ",
    "xmlns:o=\"urn:schemas-microsoft-com:office:office\""
);
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const TAB: &str = "<w:tab/>";
const BR: &str = "<w:br/>";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `w:t` con `xml:space="preserve"` si empieza o acaba en espacio.
fn t(text: &str) -> String {
    t_named("w:t", text)
}

fn t_named(tag: &str, text: &str) -> String {
    let space = if text.starts_with(' ') || text.ends_with(' ') {
        " xml:space=\"preserve\""
    } else {
        ""
    };
    format!("<{tag}{space}>{}</{tag}>", escape(text))
}

fn r(content: &[String]) -> String {
    format!("<w:r>{}</w:r>", content.concat())
}

fn run(text: &str) -> String {
    r(&[t(text)])
}

fn styled(properties: &str, text: &str) -> String {
    format!("<w:r><w:rPr>{properties}</w:rPr>{}</w:r>", t(text))
}

fn p(runs: &[String]) -> String {
    format!("<w:p>{}</w:p>", runs.concat())
}

fn heading(level: u8, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr>{}</w:p>",
        run(text)
    )
}

/// Párrafo de lista: `num` 1 es de viñetas y 2, numerada.
fn list_item(num: u8, runs: &[String]) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr>{}</w:p>",
        runs.concat()
    )
}

fn hyperlink(id: &str, text: &str) -> String {
    format!(
        "<w:hyperlink r:id=\"{id}\" w:history=\"1\"><w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr>{}</w:r></w:hyperlink>",
        t(text)
    )
}

/// Código de campo con su resultado visible.
fn field(instruction: &str, result: &str) -> String {
    [
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>".to_owned(),
        r(&[t_named("w:instrText", instruction)]),
        "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>".to_owned(),
        styled("<w:rStyle w:val=\"Hyperlink\"/>", result),
        "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>".to_owned(),
    ]
    .concat()
}

fn table(rows: &[&[&str]]) -> String {
    let columns = rows[0].len();
    let border = "w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"";
    let borders: String = ["top", "left", "bottom", "right", "insideH", "insideV"]
        .iter()
        .map(|side| format!("<w:{side} {border}/>"))
        .collect();
    let grid = "<w:gridCol w:w=\"3000\"/>".repeat(columns);
    let body: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .map(|cell| {
                    format!(
                        "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
                        p(&[run(cell)])
                    )
                })
                .collect();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect();
    format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>"
    )
}

fn note_reference(kind: &str, id: u32) -> String {
    format!(
        "<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{kind} w:id=\"{id}\"/></w:r>"
    )
}

fn note_mark(kind: &str) -> String {
    format!("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{kind}/></w:r>")
}

/// Cuadro de texto en VML, que Word lee en todas sus versiones.
fn text_box(paragraphs: &str) -> String {
    format!(
        "<w:r><w:pict><v:rect style=\"width:320pt;height:40pt\"><v:textbox><w:txbxContent>{paragraphs}</w:txbxContent></v:textbox></v:rect></w:pict></w:r>"
    )
}

fn image(id: &str) -> String {
    let size = "cx=\"457200\" cy=\"457200\"";
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent {size}/><wp:docPr id=\"1\" name=\"Imagen 1\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"imagen1.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext {size}/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

/// Hoja de Excel incrustada, con una imagen de vista previa.
fn embedded_object(preview: &str, object: &str) -> String {
    format!(
        "<w:r><w:object w:dxaOrig=\"2880\" w:dyaOrig=\"1080\"><v:shape id=\"objeto1\" style=\"width:144pt;height:54pt\" o:ole=\"\"><v:imagedata r:id=\"{preview}\" o:title=\"\"/></v:shape><o:OLEObject Type=\"Embed\" ProgID=\"Excel.Sheet.12\" ShapeID=\"objeto1\" DrawAspect=\"Content\" ObjectID=\"_1700000001\" r:id=\"{object}\"/></w:object></w:r>"
    )
}

/// PNG de 1 × 1 píxel.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0xcf, 0x50,
    0x0f, 0x00, 0x03, 0x86, 0x01, 0x80, 0x5a, 0x34, 0x7d, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

// ------------------------------------------------------------ paquete

#[derive(Default)]
struct Core {
    title: &'static str,
    subject: &'static str,
    keywords: &'static str,
    description: &'static str,
    creator: &'static str,
    last_modified_by: &'static str,
    company: &'static str,
}

struct Comment {
    author: &'static str,
    initials: &'static str,
    text: &'static str,
}

/// Contenido de un DOCX. El cuerpo usa los identificadores de relación
/// fijos que pone `build` (`rIdEnlace1`, `rIdImagen1`…).
#[derive(Default)]
struct Docx {
    body: String,
    header: Option<String>,
    footer: Option<String>,
    footnotes: Vec<String>,
    endnotes: Vec<String>,
    comments: Vec<Comment>,
    links: Vec<String>,
    lists: bool,
    image: bool,
    object: bool,
    core: Core,
}

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

impl Docx {
    fn build(&self) -> std::io::Result<Vec<u8>> {
        let mut types = vec![
            ("/word/document.xml", format!("{WML}.document.main+xml")),
            ("/word/styles.xml", format!("{WML}.styles+xml")),
        ];
        let mut rels = vec![("rIdStyles", "styles", "styles.xml".to_owned(), false)];
        let mut parts: Vec<(String, Vec<u8>)> = vec![("word/styles.xml".into(), styles().into())];

        if self.lists {
            types.push(("/word/numbering.xml", format!("{WML}.numbering+xml")));
            rels.push(("rIdNumbering", "numbering", "numbering.xml".into(), false));
            parts.push(("word/numbering.xml".into(), numbering().into()));
        }
        let mut section = String::new();
        if let Some(header) = &self.header {
            types.push(("/word/header1.xml", format!("{WML}.header+xml")));
            rels.push(("rIdHeader1", "header", "header1.xml".into(), false));
            parts.push(("word/header1.xml".into(), root("w:hdr", header).into()));
            section.push_str("<w:headerReference w:type=\"default\" r:id=\"rIdHeader1\"/>");
        }
        if let Some(footer) = &self.footer {
            types.push(("/word/footer1.xml", format!("{WML}.footer+xml")));
            rels.push(("rIdFooter1", "footer", "footer1.xml".into(), false));
            parts.push(("word/footer1.xml".into(), root("w:ftr", footer).into()));
            section.push_str("<w:footerReference w:type=\"default\" r:id=\"rIdFooter1\"/>");
        }
        for (kind, notes, id) in [
            ("footnote", &self.footnotes, "rIdFootnotes"),
            ("endnote", &self.endnotes, "rIdEndnotes"),
        ] {
            if notes.is_empty() {
                continue;
            }
            let plural = format!("{kind}s");
            types.push((
                if kind == "footnote" {
                    "/word/footnotes.xml"
                } else {
                    "/word/endnotes.xml"
                },
                format!("{WML}.{plural}+xml"),
            ));
            rels.push((
                id,
                if kind == "footnote" {
                    "footnotes"
                } else {
                    "endnotes"
                },
                format!("{plural}.xml"),
                false,
            ));
            parts.push((format!("word/{plural}.xml"), notes_part(kind, notes).into()));
        }
        if !self.comments.is_empty() {
            types.push(("/word/comments.xml", format!("{WML}.comments+xml")));
            rels.push(("rIdComments", "comments", "comments.xml".into(), false));
            parts.push((
                "word/comments.xml".into(),
                comments_part(&self.comments).into(),
            ));
        }
        let link_ids: Vec<String> = (1..=self.links.len())
            .map(|n| format!("rIdEnlace{n}"))
            .collect();
        for (id, target) in link_ids.iter().zip(&self.links) {
            rels.push((id, "hyperlink", escape(target), true));
        }
        if self.image {
            rels.push(("rIdImagen1", "image", "media/imagen1.png".into(), false));
            parts.push(("word/media/imagen1.png".into(), PNG.to_vec()));
        }
        if self.object {
            rels.push((
                "rIdVista1",
                "image",
                "media/vista-objeto1.png".into(),
                false,
            ));
            rels.push((
                "rIdObjeto1",
                "package",
                "embeddings/Microsoft_Excel_Worksheet1.xlsx".into(),
                false,
            ));
            parts.push(("word/media/vista-objeto1.png".into(), PNG.to_vec()));
            parts.push((
                "word/embeddings/Microsoft_Excel_Worksheet1.xlsx".into(),
                worksheet()?,
            ));
        }

        section.push_str(
            "<w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1417\" w:right=\"1701\" w:bottom=\"1417\" w:left=\"1701\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/>",
        );
        let document = format!(
            "{DECLARATION}<w:document {NAMESPACES}><w:body>{}<w:sectPr>{section}</w:sectPr></w:body></w:document>",
            self.body
        );
        let document_rels: String = rels
            .iter()
            .map(|(id, kind, target, external)| {
                let mode = if *external {
                    " TargetMode=\"External\""
                } else {
                    ""
                };
                format!(
                    "<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"{mode}/>"
                )
            })
            .collect();

        types.push((
            "/docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml".into(),
        ));
        types.push((
            "/docProps/app.xml",
            "application/vnd.openxmlformats-officedocument.extended-properties+xml".into(),
        ));
        let mut defaults = String::from(
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        );
        if self.image || self.object {
            defaults.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
        }
        if self.object {
            defaults.push_str("<Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>");
        }
        let overrides: String = types
            .iter()
            .map(|(part, kind)| format!("<Override PartName=\"{part}\" ContentType=\"{kind}\"/>"))
            .collect();

        let mut all: Vec<(String, Vec<u8>)> = vec![
            (
                "[Content_Types].xml".into(),
                format!("{DECLARATION}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">{defaults}{overrides}</Types>").into(),
            ),
            ("_rels/.rels".into(), package_rels().into()),
            ("docProps/core.xml".into(), core_part(&self.core).into()),
            ("docProps/app.xml".into(), app_part(self.core.company).into()),
            ("word/document.xml".into(), document.into()),
            (
                "word/_rels/document.xml.rels".into(),
                relationships(&document_rels).into(),
            ),
        ];
        all.extend(parts);
        zip_parts(&all)
    }
}

fn root(tag: &str, content: &str) -> String {
    format!("{DECLARATION}<{tag} {NAMESPACES}>{content}</{tag}>")
}

fn relationships(content: &str) -> String {
    format!(
        "{DECLARATION}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{content}</Relationships>"
    )
}

fn package_rels() -> String {
    relationships(concat!(
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
        "<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>"
    ))
}

fn core_part(core: &Core) -> String {
    format!(
        "{DECLARATION}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:subject>{}</dc:subject><dc:creator>{}</dc:creator><cp:keywords>{}</cp:keywords><dc:description>{}</dc:description><cp:lastModifiedBy>{}</cp:lastModifiedBy><cp:revision>2</cp:revision><dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-03-03T09:00:00Z</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">2026-03-04T10:30:00Z</dcterms:modified></cp:coreProperties>",
        escape(core.title),
        escape(core.subject),
        escape(core.creator),
        escape(core.keywords),
        escape(core.description),
        escape(core.last_modified_by),
    )
}

fn app_part(company: &str) -> String {
    format!(
        "{DECLARATION}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><Application>Microsoft Office Word</Application><Company>{}</Company></Properties>",
        escape(company)
    )
}

fn styles() -> String {
    root(
        "w:styles",
        concat!(
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>",
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>",
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"26\"/></w:rPr></w:style>",
            "<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:rPr><w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/></w:rPr></w:style>"
        ),
    )
}

fn numbering() -> String {
    let level = |format: &str, text: &str| {
        format!(
            "<w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl>"
        )
    };
    root(
        "w:numbering",
        &format!(
            "<w:abstractNum w:abstractNumId=\"0\">{}</w:abstractNum><w:abstractNum w:abstractNumId=\"1\">{}</w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>",
            level("bullet", "•"),
            level("decimal", "%1.")
        ),
    )
}

/// Notas al pie o al final, con los dos separadores que Word espera.
fn notes_part(kind: &str, notes: &[String]) -> String {
    let separators = format!(
        "<w:{kind} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}><w:{kind} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>"
    );
    let content: String = notes
        .iter()
        .enumerate()
        .map(|(i, note)| format!("<w:{kind} w:id=\"{}\">{note}</w:{kind}>", i + 1))
        .collect();
    root(&format!("w:{kind}s"), &format!("{separators}{content}"))
}

fn comments_part(comments: &[Comment]) -> String {
    let content: String = comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "<w:comment w:id=\"{i}\" w:author=\"{}\" w:date=\"2026-04-02T10:00:00Z\" w:initials=\"{}\">{}</w:comment>",
                escape(c.author),
                escape(c.initials),
                p(&[note_mark("annotationRef"), run(&format!(" {}", c.text))])
            )
        })
        .collect();
    root("w:comments", &content)
}

/// Hoja de cálculo mínima para el objeto incrustado.
fn worksheet() -> std::io::Result<Vec<u8>> {
    let sml = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            format!("{DECLARATION}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>").into(),
        ),
        (
            "_rels/.rels".into(),
            relationships("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>").into(),
        ),
        (
            "xl/workbook.xml".into(),
            format!("{DECLARATION}<workbook xmlns=\"{sml}\" xmlns:r=\"{REL}\"><sheets><sheet name=\"Hoja1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>").into(),
        ),
        (
            "xl/_rels/workbook.xml.rels".into(),
            relationships(&format!("<Relationship Id=\"rId1\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet1.xml\"/>")).into(),
        ),
        (
            "xl/worksheets/sheet1.xml".into(),
            format!("{DECLARATION}<worksheet xmlns=\"{sml}\"><sheetData><row r=\"1\"><c r=\"A1\"><v>1250</v></c></row></sheetData></worksheet>").into(),
        ),
    ];
    zip_parts(&parts)
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
