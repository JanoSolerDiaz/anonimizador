//! Evaluación de la detección: localizar en cada texto del corpus las
//! entidades esperadas y compararlas con las detectadas. Una entidad es
//! correcta solo si coinciden los dos bordes y el tipo.

use std::path::{Path, PathBuf};

use nucleo::Error;
use nucleo::evaluacion::{Counts, Evaluation, ExpectedEntity, Span, load_corpus, locate};

fn expected(kind: &str, text: &str) -> ExpectedEntity {
    ExpectedEntity {
        kind: kind.to_owned(),
        text: text.to_owned(),
    }
}

fn span(start: usize, end: usize, kind: &str) -> Span {
    Span {
        start,
        end,
        kind: kind.to_owned(),
    }
}

// ---------------------------------------------------------------- localizar

#[test]
fn locate_finds_every_occurrence() {
    let text = "Ana Gil vive en Elx. Ana Gil trabaja en Elx.";
    let spans = locate(
        text,
        &[
            expected("NOMBRE_PERSONA", "Ana Gil"),
            expected("LUGAR", "Elx"),
        ],
    )
    .unwrap();
    assert_eq!(
        spans,
        [
            span(0, 7, "NOMBRE_PERSONA"),
            span(16, 19, "LUGAR"),
            span(21, 28, "NOMBRE_PERSONA"),
            span(40, 43, "LUGAR"),
        ]
    );
}

#[test]
fn locate_positions_are_in_characters() {
    let spans = locate(
        "Sí: Núria Puig, de València.",
        &[expected("LUGAR", "València")],
    )
    .unwrap();
    assert_eq!(spans, [span(19, 27, "LUGAR")]);
}

#[test]
fn locate_ignores_matches_inside_other_words() {
    let text = "Mariana y Ana; Anabel no.";
    let spans = locate(text, &[expected("NOMBRE_PERSONA", "Ana")]).unwrap();
    assert_eq!(spans, [span(10, 13, "NOMBRE_PERSONA")]);
}

#[test]
fn an_occurrence_inside_a_longer_entity_belongs_to_the_longer_one() {
    let text = "Vive en Calle Mayor 5, 46001 Valencia, y trabaja en Valencia.";
    let spans = locate(
        text,
        &[
            expected("LUGAR", "Valencia"),
            expected("DIRECCION_POSTAL", "Calle Mayor 5, 46001 Valencia"),
        ],
    )
    .unwrap();
    assert_eq!(
        spans,
        [span(8, 37, "DIRECCION_POSTAL"), span(52, 60, "LUGAR")]
    );
}

#[test]
fn locate_rejects_text_that_does_not_appear() {
    match locate("Ana Gil", &[expected("NOMBRE_PERSONA", "Ana Ruiz")]) {
        Err(error @ Error::Corpus(_)) => {
            let message = error.to_string();
            assert!(message.contains("«Ana Ruiz»"), "{message}");
        }
        other => panic!("se esperaba Error::Corpus, llegó {other:?}"),
    }
}

#[test]
fn locate_rejects_unknown_types() {
    assert!(matches!(
        locate("Ana Gil", &[expected("PERSONA", "Ana Gil")]),
        Err(Error::UnknownType(kind)) if kind == "PERSONA"
    ));
}

#[test]
fn locate_rejects_empty_text() {
    assert!(matches!(
        locate("Ana Gil", &[expected("NOMBRE_PERSONA", "")]),
        Err(Error::Corpus(_))
    ));
}

#[test]
fn locate_rejects_the_same_text_with_two_types() {
    let result = locate(
        "Ana Gil",
        &[
            expected("NOMBRE_PERSONA", "Ana Gil"),
            expected("ALIAS_PERSONA", "Ana Gil"),
        ],
    );
    assert!(matches!(result, Err(Error::Corpus(_))), "{result:?}");
}

#[test]
fn locate_rejects_entities_that_overlap_without_containing_each_other() {
    let result = locate(
        "Ana Gil Pons",
        &[
            expected("NOMBRE_PERSONA", "Ana Gil"),
            expected("NOMBRE_PERSONA", "Gil Pons"),
        ],
    );
    assert!(matches!(result, Err(Error::Corpus(_))), "{result:?}");
}

// ---------------------------------------------------------------- métrica

/// Texto sin espacios para las pruebas en las que el contenido no importa.
const FILLER: &str = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

fn counts(evaluation: &Evaluation, kind: &str) -> Counts {
    evaluation.by_kind().get(kind).copied().unwrap_or_default()
}

#[test]
fn exact_matches_count_as_correct() {
    let gold = [span(0, 7, "NOMBRE_PERSONA"), span(16, 19, "LUGAR")];
    let mut evaluation = Evaluation::default();
    let comparison = evaluation.add(FILLER, &gold, &gold);
    assert!(comparison.missed.is_empty() && comparison.spurious.is_empty());
    let name = counts(&evaluation, "NOMBRE_PERSONA");
    assert_eq!((name.expected, name.detected, name.correct), (1, 1, 1));
    assert_eq!(name.precision(), Some(1.0));
    assert_eq!(name.recall(), Some(1.0));
}

#[test]
fn a_shifted_border_is_a_miss_and_a_false_positive() {
    let mut evaluation = Evaluation::default();
    let comparison = evaluation.add(
        FILLER,
        &[span(0, 24, "CUENTA_BANCARIA")],
        &[span(5, 24, "CUENTA_BANCARIA")],
    );
    assert_eq!(comparison.missed, [span(0, 24, "CUENTA_BANCARIA")]);
    assert_eq!(comparison.spurious, [span(5, 24, "CUENTA_BANCARIA")]);
    let account = counts(&evaluation, "CUENTA_BANCARIA");
    assert_eq!(
        (account.expected, account.detected, account.correct),
        (1, 1, 0)
    );
    assert_eq!(account.precision(), Some(0.0));
    assert_eq!(account.recall(), Some(0.0));
}

#[test]
fn a_wrong_type_is_a_miss_for_one_type_and_a_false_positive_for_the_other() {
    let mut evaluation = Evaluation::default();
    evaluation.add(
        FILLER,
        &[span(0, 8, "VEHICULO")],
        &[span(0, 8, "REFERENCIA")],
    );
    let vehicle = counts(&evaluation, "VEHICULO");
    assert_eq!(
        (vehicle.expected, vehicle.detected, vehicle.correct),
        (1, 0, 0)
    );
    assert_eq!(vehicle.precision(), None);
    assert_eq!(vehicle.recall(), Some(0.0));
    let reference = counts(&evaluation, "REFERENCIA");
    assert_eq!(
        (reference.expected, reference.detected, reference.correct),
        (0, 1, 0)
    );
    assert_eq!(reference.precision(), Some(0.0));
    assert_eq!(reference.recall(), None);
}

#[test]
fn an_entity_is_covered_when_detections_of_any_type_hide_all_its_letters() {
    // «Carlos García» en dos trozos: el espacio del medio no cuenta.
    // La matrícula con otro tipo también queda tapada.
    // El IBAN sin «ES91» deja letras a la vista.
    let gold = [
        span(0, 13, "NOMBRE_PERSONA"),
        span(20, 28, "VEHICULO"),
        span(30, 54, "CUENTA_BANCARIA"),
    ];
    let detected = [
        span(0, 6, "NOMBRE_PERSONA"),
        span(7, 13, "NOMBRE_PERSONA"),
        span(20, 28, "REFERENCIA"),
        span(35, 54, "CUENTA_BANCARIA"),
    ];
    let text = "Carlos García, coche 1234-BCD, ES91 2100 0418 4502 0005 1332";
    let mut evaluation = Evaluation::default();
    evaluation.add(text, &gold, &detected);
    assert_eq!(counts(&evaluation, "NOMBRE_PERSONA").covered, 1);
    assert_eq!(counts(&evaluation, "VEHICULO").covered, 1);
    assert_eq!(counts(&evaluation, "CUENTA_BANCARIA").covered, 0);
    assert_eq!(counts(&evaluation, "NOMBRE_PERSONA").coverage(), Some(1.0));
    assert_eq!(counts(&evaluation, "REFERENCIA").coverage(), None);
}

#[test]
fn ratios_are_none_without_data() {
    let empty = Counts::default();
    assert_eq!(empty.precision(), None);
    assert_eq!(empty.recall(), None);
    assert_eq!(empty.coverage(), None);
}

#[test]
fn total_adds_every_type_and_every_text() {
    let mut evaluation = Evaluation::default();
    evaluation.add(
        FILLER,
        &[span(0, 7, "NOMBRE_PERSONA"), span(10, 13, "LUGAR")],
        &[span(0, 7, "NOMBRE_PERSONA")],
    );
    evaluation.add(
        FILLER,
        &[span(0, 4, "LUGAR")],
        &[span(0, 4, "LUGAR"), span(6, 9, "IP")],
    );
    let total = evaluation.total();
    assert_eq!((total.expected, total.detected, total.correct), (3, 3, 2));
    assert_eq!(counts(&evaluation, "LUGAR").expected, 2);
    assert_eq!(counts(&evaluation, "LUGAR").correct, 1);
    let kinds: Vec<&String> = evaluation.by_kind().keys().collect();
    assert_eq!(kinds, ["IP", "LUGAR", "NOMBRE_PERSONA"]);
}

#[test]
fn a_repeated_detection_only_matches_once() {
    let mut evaluation = Evaluation::default();
    evaluation.add(
        FILLER,
        &[span(0, 3, "LUGAR")],
        &[span(0, 3, "LUGAR"), span(0, 3, "LUGAR")],
    );
    let place = counts(&evaluation, "LUGAR");
    assert_eq!((place.detected, place.correct), (2, 1));
}

// ---------------------------------------------------------------- corpus

fn write(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

#[test]
fn load_corpus_pairs_each_text_with_its_json() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "b.txt", "Vive en Elx.");
    write(
        dir.path(),
        "b.json",
        r#"{"entidades": [{"tipo": "LUGAR", "texto": "Elx"}]}"#,
    );
    write(dir.path(), "a.txt", "Nada que tapar.");
    write(dir.path(), "a.json", r#"{"entidades": []}"#);
    let corpus = load_corpus(dir.path()).unwrap();
    let names: Vec<&str> = corpus.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(corpus[1].text, "Vive en Elx.");
    assert_eq!(corpus[1].expected, [span(8, 11, "LUGAR")]);
}

#[test]
fn load_corpus_rejects_a_text_without_json() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "Texto.");
    match load_corpus(dir.path()) {
        Err(error @ Error::Corpus(_)) => assert!(error.to_string().contains("a.json")),
        other => panic!("se esperaba Error::Corpus, llegó {other:?}"),
    }
}

#[test]
fn load_corpus_rejects_a_json_without_text() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.json", r#"{"entidades": []}"#);
    match load_corpus(dir.path()) {
        Err(error @ Error::Corpus(_)) => assert!(error.to_string().contains("a.txt")),
        other => panic!("se esperaba Error::Corpus, llegó {other:?}"),
    }
}

#[test]
fn load_corpus_rejects_malformed_json_and_empty_folders() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(load_corpus(dir.path()), Err(Error::Corpus(_))));
    write(dir.path(), "a.txt", "Texto.");
    write(
        dir.path(),
        "a.json",
        r#"{"entidades": [{"tipo": "LUGAR"}]}"#,
    );
    assert!(matches!(load_corpus(dir.path()), Err(Error::Corpus(_))));
}

#[test]
fn load_corpus_of_a_missing_folder_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        load_corpus(&dir.path().join("no-existe")),
        Err(Error::Io(_))
    ));
}

// ---------------------------------------------------------------- corpus real

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/corpus/texto")
}

#[test]
fn the_synthetic_corpus_is_valid_and_covers_four_languages() {
    let corpus = load_corpus(&corpus_dir()).unwrap();
    for language in ["es", "ca", "en", "fr"] {
        let texts = corpus
            .iter()
            .filter(|t| t.name.starts_with(&format!("{language}-")))
            .count();
        assert!(texts >= 8, "{language}: solo {texts} textos");
    }
    // Los identificadores que antes cubrían las reglas deben estar bien
    // representados, porque ahora solo los detecta el modelo.
    let mut evaluation = Evaluation::default();
    for text in &corpus {
        evaluation.add(&text.text, &text.expected, &[]);
    }
    for kind in [
        "ID_DOCUMENTO_PERSONA",
        "CUENTA_BANCARIA",
        "TARJETA_BANCARIA",
        "NOMBRE_PERSONA",
        "EMAIL",
        "TELEFONO",
        "DIRECCION_POSTAL",
    ] {
        let n = counts(&evaluation, kind).expected;
        assert!(n >= 6, "{kind}: solo {n} entidades esperadas");
    }
}

#[test]
fn the_long_text_is_a_single_paragraph_with_many_entities() {
    let corpus = load_corpus(&corpus_dir()).unwrap();
    let long = corpus
        .iter()
        .find(|t| t.name.contains("largo"))
        .expect("falta el texto largo");
    let paragraphs = nucleo::motor::texto::split_paragraphs(&long.text);
    assert_eq!(
        paragraphs.len(),
        1,
        "el texto largo debe ser un solo párrafo"
    );
    assert!(long.text.chars().count() >= 6000);
    assert!(long.expected.len() >= 30);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn the_long_text_puts_entities_where_two_windows_overlap() {
    use nucleo::motor::tokenizacion::TextTokenizer;
    use nucleo::motor::troceado::{MAX_WINDOW_TOKENS, OVERLAP_TOKENS, plan_windows};
    use nucleo::recursos::Resources;

    let corpus = load_corpus(&corpus_dir()).unwrap();
    let long = corpus.iter().find(|t| t.name.contains("largo")).unwrap();
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    for model in ["modelo", "modelo-v2"] {
        let tokenizer = TextTokenizer::load(&resources.model(model).tokenizer_path()).unwrap();
        let tokens = tokenizer.encode(long.text.trim_end()).unwrap();
        let windows = plan_windows(tokens.len(), MAX_WINDOW_TOKENS, OVERLAP_TOKENS);
        assert!(
            windows.len() >= 3,
            "{model}: solo {} ventanas",
            windows.len()
        );
        let in_overlap = long
            .expected
            .iter()
            .filter(|e| {
                // Tokens de la entidad, y cuántas ventanas los contienen todos.
                let inside: Vec<usize> = (0..tokens.len())
                    .filter(|&i| tokens[i].start < e.end && tokens[i].end > e.start)
                    .collect();
                windows
                    .iter()
                    .filter(|w| inside.iter().all(|&i| w.start <= i && i < w.end))
                    .count()
                    >= 2
            })
            .count();
        assert!(
            in_overlap >= 3,
            "{model}: solo {in_overlap} entidades en solapes"
        );
    }
}
