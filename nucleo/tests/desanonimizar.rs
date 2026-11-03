//! Desanonimizar (PRD, «Desanonimización»): búsqueda tolerante de
//! marcadores, valores del almacén y de los diccionarios importados,
//! conflictos entre fuentes e ida y vuelta con la anonimización.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use jiff::Zoned;
use nucleo::almacen::Store;
use nucleo::formatos::docx::DocxDocument;
use nucleo::formatos::pdf::DEFAULT_RESOLUTION;
use nucleo::motor::Entity;
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::desanonimizar::{
    Conflict, MarkerSource, Outcome, deanonymize, deanonymize_text,
};
use nucleo::operaciones::marcadores::{FoundMarker, find_markers};
use nucleo::operaciones::{Detector, OutputFormat};
use nucleo::{Error, Result};

const CORPUS: [&str; 4] = [
    "es-01-contrato-arrendamiento",
    "ca-01-informe-auditoria",
    "en-01-employment-review",
    "fr-01-fiche-client",
];

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/docx")
        .join(name)
}

/// Almacén con datos de tres documentos.
fn store() -> Store {
    let mut store = Store::new();
    store.assign("NOMBRE_PERSONA", "PERSON", "Ana López", "doc-1");
    store.assign("EMAIL", "EMAIL", "ana@correo-ejemplo.es", "doc-2");
    store.assign("LUGAR", "LOCATION", "Valencia", "doc-3");
    store
}

/// Diccionario exportado por otra persona.
fn dictionary(entries: &[(&str, &str, &str, Option<&str>)]) -> Vec<u8> {
    let entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|(marker, kind, value, short)| {
            let mut entry = serde_json::json!({ "marcador": marker, "tipo": kind, "valor": value });
            if let Some(short) = short {
                entry["forma_corta"] = (*short).into();
            }
            entry
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "formato": "anonimizador-diccionario",
        "version_formato": 1,
        "documento": {
            "id": "doc-de-otra",
            "creado": "2026-10-20T09:00:00+02:00",
            "salida": { "nombre": "informe_anonimizado.docx", "formato": "docx" }
        },
        "entradas": entries
    }))
    .unwrap()
}

fn restore(
    text: &str,
    store: &Store,
    source: Option<&MarkerSource>,
) -> (String, Vec<String>, Vec<String>) {
    match deanonymize_text(text, store, source).unwrap() {
        Outcome::Done((text, report)) => (text, report.restored, report.missing),
        Outcome::Conflicts(conflicts) => panic!("conflicto inesperado: {conflicts:?}"),
    }
}

fn keys(found: &[FoundMarker]) -> Vec<(usize, usize, &str)> {
    found
        .iter()
        .map(|f| (f.start, f.end, f.key.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------
// Búsqueda tolerante

#[test]
fn markers_are_found_whatever_their_case_spaces_or_digits() {
    let text =
        "[NOMBRE_PERSONA_0042] [nombre_persona_0042] [ NOMBRE_PERSONA_0042 ] [Nombre_Persona_42]";
    let found = find_markers(text);
    assert_eq!(
        keys(&found),
        [
            (0, 21, "[NOMBRE_PERSONA_0042]"),
            (22, 43, "[NOMBRE_PERSONA_0042]"),
            (44, 67, "[NOMBRE_PERSONA_0042]"),
            (68, 87, "[NOMBRE_PERSONA_0042]"),
        ]
    );
    assert!(found.iter().all(|f| !f.short));
}

#[test]
fn positions_are_characters() {
    let found = find_markers("Señoría: [EMAIL_0001].");
    assert_eq!(keys(&found), [(9, 21, "[EMAIL_0001]")]);
}

#[test]
fn short_forms_are_found_too() {
    let found = find_markers("Firma [P42] y [ p7 ].");
    assert_eq!(keys(&found), [(6, 11, "[P42]"), (14, 20, "[P7]")]);
    assert!(found.iter().all(|f| f.short));
}

#[test]
fn brackets_that_are_not_markers_are_ignored() {
    for text in [
        "[1]",
        "[nota]",
        "[]",
        "[ ]",
        "[_0042]",
        "[ _0042]",
        "[NOMBRE PERSONA_0042]",
        "[NOMBRE_PERSONA_]",
        "[NOMBRE_PERSONA_0042",
        "NOMBRE_PERSONA_0042]",
        "[NOMBRE-PERSONA_0042]",
        "[NOMBRE_PERSONA_0042x]",
        "[NOMBRE_PERSONA_99999999999999999999]",
    ] {
        assert!(find_markers(text).is_empty(), "{text}");
    }
}

#[test]
fn a_bracket_inside_another_does_not_hide_the_marker() {
    let found = find_markers("[[EMAIL_0001]]");
    assert_eq!(keys(&found), [(1, 13, "[EMAIL_0001]")]);
}

// ---------------------------------------------------------------------------
// Texto pegado

#[test]
fn markers_from_three_documents_are_restored() {
    let (text, restored, missing) = restore(
        "[NOMBRE_PERSONA_0001] escribe a [EMAIL_0001] desde [LUGAR_0001].\n\nOtra vez [NOMBRE_PERSONA_0001].",
        &store(),
        None,
    );
    assert_eq!(
        text,
        "Ana López escribe a ana@correo-ejemplo.es desde Valencia.\n\nOtra vez Ana López."
    );
    assert_eq!(
        restored,
        ["[NOMBRE_PERSONA_0001]", "[EMAIL_0001]", "[LUGAR_0001]"]
    );
    assert!(missing.is_empty());
}

#[test]
fn altered_markers_are_restored() {
    let (text, restored, _) = restore(
        "Hola [ nombre_persona_1 ], de [Lugar_0001].",
        &store(),
        None,
    );
    assert_eq!(text, "Hola Ana López, de Valencia.");
    assert_eq!(restored, ["[NOMBRE_PERSONA_0001]", "[LUGAR_0001]"]);
}

#[test]
fn markers_in_no_source_stay_and_are_reported() {
    let (text, restored, missing) = restore(
        "[NOMBRE_PERSONA_0099], [nombre_persona_0099], [ANEXO_0002], [P3] y [1].",
        &store(),
        None,
    );
    assert_eq!(
        text,
        "[NOMBRE_PERSONA_0099], [nombre_persona_0099], [ANEXO_0002], [P3] y [1]."
    );
    assert!(restored.is_empty());
    // Solo los de un tipo conocido, sin repetir. Una forma corta o un tipo
    // desconocido pueden ser texto normal entre corchetes.
    assert_eq!(missing, ["[NOMBRE_PERSONA_0099]"]);
}

#[test]
fn imported_dictionaries_are_a_source() {
    let mut store = store();
    store
        .import_dictionary(
            &dictionary(&[("[TELEFONO_0003]", "TELEFONO", "612 384 907", None)]),
            "de-otra_diccionario.json",
        )
        .unwrap();
    let (text, restored, _) = restore("Llame al [TELEFONO_0003].", &store, None);
    assert_eq!(text, "Llame al 612 384 907.");
    assert_eq!(restored, ["[TELEFONO_0003]"]);
}

#[test]
fn short_forms_are_looked_up_in_the_sources() {
    let mut store = store();
    store
        .import_dictionary(
            &dictionary(&[(
                "[NOMBRE_PERSONA_0042]",
                "NOMBRE_PERSONA",
                "Luis Mas",
                Some("[P42]"),
            )]),
            "de-otra_diccionario.json",
        )
        .unwrap();
    let (text, restored, _) = restore("Firma: [p42].", &store, None);
    assert_eq!(text, "Firma: Luis Mas.");
    assert_eq!(restored, ["[P42]"]);
}

#[test]
fn the_same_value_in_two_sources_is_no_conflict() {
    let mut store = store();
    store
        .import_dictionary(
            &dictionary(&[("[NOMBRE_PERSONA_0001]", "NOMBRE_PERSONA", "Ana López", None)]),
            "de-otra_diccionario.json",
        )
        .unwrap();
    let (text, _, _) = restore("[NOMBRE_PERSONA_0001]", &store, None);
    assert_eq!(text, "Ana López");
}

#[test]
fn two_values_in_two_sources_are_a_conflict_until_a_source_is_chosen() {
    let mut store = store();
    store
        .import_dictionary(
            &dictionary(&[
                ("[NOMBRE_PERSONA_0001]", "NOMBRE_PERSONA", "Luis Mas", None),
                ("[EMAIL_0007]", "EMAIL", "luis@correo-ejemplo.es", None),
            ]),
            "de-otra_diccionario.json",
        )
        .unwrap();
    let text = "[NOMBRE_PERSONA_0001] y [EMAIL_0007]. Otra vez [nombre_persona_0001].";
    let dictionary_source = MarkerSource::Dictionary("de-otra_diccionario.json".into());
    match deanonymize_text(text, &store, None).unwrap() {
        Outcome::Conflicts(conflicts) => assert_eq!(
            conflicts,
            [Conflict {
                marker: "[NOMBRE_PERSONA_0001]".into(),
                sources: vec![MarkerSource::Store, dictionary_source.clone()],
            }]
        ),
        Outcome::Done(_) => panic!("debía haber un conflicto"),
    }
    let (chosen, _, _) = restore(text, &store, Some(&dictionary_source));
    assert_eq!(
        chosen,
        "Luis Mas y luis@correo-ejemplo.es. Otra vez Luis Mas."
    );
    let (own, _, _) = restore(text, &store, Some(&MarkerSource::Store));
    assert_eq!(
        own,
        "Ana López y luis@correo-ejemplo.es. Otra vez Ana López."
    );
}

// ---------------------------------------------------------------------------
// Archivos

/// Detector que encuentra los valores esperados del corpus.
struct Fake(Vec<(String, String)>);

impl Fake {
    fn for_corpus(name: &str) -> Fake {
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(corpus(&format!("{name}.json"))).unwrap())
                .unwrap();
        let mut values: Vec<(String, String)> = json["entidades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["texto"].as_str().unwrap().to_owned(),
                    e["tipo"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        values.sort_by_key(|(text, _)| std::cmp::Reverse(text.chars().count()));
        Fake(values)
    }
}

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            let chars: Vec<char> = text.chars().collect();
            let mut taken = vec![false; chars.len()];
            for (value, kind) in &self.0 {
                let needle: Vec<char> = value.chars().collect();
                let mut start = 0;
                while start + needle.len() <= chars.len() {
                    let end = start + needle.len();
                    if chars[start..end] == needle[..] && !taken[start..end].iter().any(|t| *t) {
                        taken[start..end].iter_mut().for_each(|t| *t = true);
                        entities.push(Entity {
                            segment,
                            start,
                            end,
                            kind: kind.clone(),
                            label: kind.clone(),
                            text: value.clone(),
                            score: 0.99,
                        });
                        start = end;
                    } else {
                        start += 1;
                    }
                }
            }
        }
        Ok(entities)
    }
}

fn anonymize_options(format: OutputFormat) -> AnonymizeOptions {
    AnonymizeOptions {
        format,
        model: "modelo".into(),
        now: Zoned::now(),
        spreadsheet_amounts: false,
        pdf_resolution: DEFAULT_RESOLUTION,
    }
}

fn done<T: std::fmt::Debug>(outcome: Outcome<T>) -> T {
    match outcome {
        Outcome::Done(value) => value,
        Outcome::Conflicts(conflicts) => panic!("conflicto inesperado: {conflicts:?}"),
    }
}

/// Texto de todas las partes procesadas: segmentos, enlaces y códigos.
fn processed_text(bytes: &[u8]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let document = DocxDocument::from_bytes(bytes).unwrap();
    (
        document.segments().iter().map(|s| s.text.clone()).collect(),
        document
            .external_links()
            .iter()
            .map(|l| l.target.clone())
            .collect(),
        document
            .field_codes()
            .iter()
            .map(|f| f.text.clone())
            .collect(),
    )
}

#[test]
fn the_corpus_comes_back_exactly_after_a_round_trip() {
    for name in CORPUS {
        let input = corpus(&format!("{name}.docx"));
        let anonymized_dir = tempfile::tempdir().unwrap();
        let restored_dir = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = anonymize(
            &input,
            anonymized_dir.path(),
            &Fake::for_corpus(name),
            &mut store,
            &anonymize_options(OutputFormat::Docx),
        )
        .unwrap();
        let (output, report) =
            done(deanonymize(&summary.output, restored_dir.path(), &store, None).unwrap());
        assert_eq!(
            output,
            restored_dir
                .path()
                .join(format!("{name}-anonimizado_desanonimizado.docx"))
        );
        assert!(report.missing.is_empty(), "{name}: {:?}", report.missing);
        assert_eq!(
            processed_text(&std::fs::read(&output).unwrap()),
            processed_text(&std::fs::read(&input).unwrap()),
            "{name}"
        );
    }
}

#[test]
fn text_and_markdown_come_back_in_their_own_format() {
    let dir = tempfile::tempdir().unwrap();
    let original = "# Para Ana López\r\n\r\nGracias desde Valencia.\r\n";
    let fake = Fake(vec![
        ("Ana López".into(), "NOMBRE_PERSONA".into()),
        ("Valencia".into(), "LUGAR".into()),
    ]);
    for (file, format) in [
        ("nota.md", OutputFormat::Markdown),
        ("nota.txt", OutputFormat::Text),
    ] {
        let input = dir.path().join(file);
        std::fs::write(&input, original).unwrap();
        let out = tempfile::tempdir().unwrap();
        let mut store = Store::new();
        let summary = anonymize(
            &input,
            out.path(),
            &fake,
            &mut store,
            &anonymize_options(format),
        )
        .unwrap();
        let back = tempfile::tempdir().unwrap();
        let (output, _) = done(deanonymize(&summary.output, back.path(), &store, None).unwrap());
        let extension = Path::new(file).extension().unwrap().to_str().unwrap();
        assert_eq!(
            output,
            back.path()
                .join(format!("nota-anonimizado_desanonimizado.{extension}"))
        );
        assert_eq!(std::fs::read_to_string(&output).unwrap(), original);
    }
}

/// DOCX mínimo con el cuerpo dado, guardado en `dir`.
fn docx_file(dir: &Path, body: &str) -> PathBuf {
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in [
        ("[Content_Types].xml", "<Types/>".to_owned()),
        (
            "word/document.xml",
            format!("<w:document {w}><w:body>{body}</w:body></w:document>"),
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    let path = dir.join("editado.docx");
    std::fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
    path
}

#[test]
fn a_marker_split_in_two_fragments_of_a_docx_is_restored() {
    let dir = tempfile::tempdir().unwrap();
    let input = docx_file(
        dir.path(),
        "<w:p><w:r><w:t xml:space=\"preserve\">Firma: [NOMBRE_PERS</w:t></w:r>\
         <w:r><w:rPr><w:b/></w:rPr><w:t>ONA_0001] en [lugar_1].</w:t></w:r></w:p>",
    );
    let out = tempfile::tempdir().unwrap();
    let (output, report) = done(deanonymize(&input, out.path(), &store(), None).unwrap());
    assert_eq!(output, out.path().join("editado_desanonimizado.docx"));
    let document = DocxDocument::from_bytes(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(document.segments()[0].text, "Firma: Ana López en Valencia.");
    assert_eq!(report.restored, ["[NOMBRE_PERSONA_0001]", "[LUGAR_0001]"]);
}

#[test]
fn a_conflict_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("nota.txt");
    std::fs::write(&input, "[NOMBRE_PERSONA_0001]\n").unwrap();
    let mut store = store();
    store
        .import_dictionary(
            &dictionary(&[("[NOMBRE_PERSONA_0001]", "NOMBRE_PERSONA", "Luis Mas", None)]),
            "de-otra_diccionario.json",
        )
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let outcome = deanonymize(&input, out.path(), &store, None).unwrap();
    assert!(matches!(outcome, Outcome::Conflicts(_)), "{outcome:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

#[test]
fn an_unknown_format_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("informe.rtf");
    std::fs::write(&input, "{\\rtf1 [NOMBRE_PERSONA_0001]}").unwrap();
    let error = deanonymize(&input, dir.path(), &store(), None).unwrap_err();
    assert!(matches!(error, Error::UnsupportedFormat(_)), "{error:?}");
}

#[test]
fn the_original_file_does_not_change() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("nota.txt");
    std::fs::write(&input, "Hola [NOMBRE_PERSONA_0001].\n").unwrap();
    let out = tempfile::tempdir().unwrap();
    done(deanonymize(&input, out.path(), &store(), None).unwrap());
    assert_eq!(
        std::fs::read_to_string(&input).unwrap(),
        "Hola [NOMBRE_PERSONA_0001].\n"
    );
}

#[test]
fn odd_names_between_brackets_are_not_markers() {
    let long = format!("[{}_0001]", "A".repeat(70));
    for text in [
        "[A1_0042]",
        "[NOMBRE__0042]",
        "[_NOMBRE_0042]",
        "[NOMBRE_PERSONA__0042]",
        "[P4X2]",
        long.as_str(),
    ] {
        assert!(find_markers(text).is_empty(), "{text}");
    }
}

#[test]
fn short_forms_of_the_own_entries_are_restored() {
    let json = serde_json::json!({
        "formato": "anonimizador-almacen",
        "version_formato": 1,
        "contadores": { "NOMBRE_PERSONA": 42 },
        "entradas": [{
            "marcador": "[NOMBRE_PERSONA_0042]",
            "forma_corta": "[P42]",
            "tipo": "NOMBRE_PERSONA",
            "etiqueta_modelo": "PERSON",
            "valor": "Luis Mas",
            "clave_normalizada": "luis mas",
            "variantes": ["Luis Mas"],
            "documentos": ["doc-pdf"]
        }],
        "documentos": [],
        "importados": []
    });
    let store = Store::from_json(&serde_json::to_vec(&json).unwrap()).unwrap();
    let (text, restored, _) = restore("Firma: [ p42 ].", &store, None);
    assert_eq!(text, "Firma: Luis Mas.");
    assert_eq!(restored, ["[P42]"]);
}
