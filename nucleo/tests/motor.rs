//! Motor completo con los modelos reales: tokenización, troceado, inferencia
//! por lotes y decodificación.
//!
//! Cada modelo se carga una sola vez y lo comparten todas las pruebas.

use std::sync::OnceLock;

use nucleo::Error;
use nucleo::motor::decodificacion::WordStrategy;
use nucleo::motor::tokenizacion::TextTokenizer;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::recursos::Resources;

/// Modelos que se prueban. Si se vuelve a comparar otro, se añade aquí.
const MODELS: [&str; 1] = ["modelo"];
const STRATEGIES: [WordStrategy; 2] = [WordStrategy::FirstSubtoken, WordStrategy::WordMean];

fn resources() -> Resources {
    Resources::locate().expect("no se encuentra la carpeta recursos/")
}

fn engine(name: &str) -> &'static Engine {
    static V1: OnceLock<Engine> = OnceLock::new();
    static V2: OnceLock<Engine> = OnceLock::new();
    let cell = if name == "modelo" { &V1 } else { &V2 };
    cell.get_or_init(|| {
        let resources = resources();
        Engine::load(&resources, &resources.model(name), EngineOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    })
}

fn options(strategy: WordStrategy) -> EngineOptions {
    let mut options = EngineOptions::default();
    options.decode.strategy = strategy;
    options
}

/// (texto, tipo) de cada entidad, y comprobación de que `text` coincide con
/// el segmento en las posiciones indicadas.
fn summary(segments: &[&str], entities: &[Entity]) -> Vec<(String, String)> {
    entities
        .iter()
        .map(|e| {
            let chars: Vec<char> = segments[e.segment].chars().collect();
            let slice: String = chars[e.start..e.end].iter().collect();
            assert_eq!(slice, e.text, "el texto no coincide con sus posiciones");
            (e.text.clone(), e.kind.clone())
        })
        .collect()
}

#[test]
fn a_missing_model_folder_is_reported_before_loading_anything() {
    let dir = tempfile::tempdir().unwrap();
    let resources = Resources::new(dir.path());
    let result = Engine::load(
        &resources,
        &resources.model("modelo"),
        EngineOptions::default(),
    );
    assert!(matches!(result, Err(Error::ResourceMissing(_))));
}

#[test]
fn default_options() {
    let options = EngineOptions::default();
    assert_eq!(options.batch_size, 8);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    assert_eq!(options.threads, (cores / 2).max(1));
    assert_eq!(options.decode.strategy, WordStrategy::FirstSubtoken);
    assert_eq!(options.decode.threshold, 0.5);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn detects_name_document_place_and_phone_with_exact_offsets() {
    let segments = ["Carlos García, DNI 12345678Z, Madrid, +34 612 345 678."];
    let expected = [
        ("Carlos García", "NOMBRE_PERSONA"),
        ("12345678Z", "ID_DOCUMENTO_PERSONA"),
        ("Madrid", "LUGAR"),
        ("+34 612 345 678", "TELEFONO"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(t, k)| (t.to_string(), k.to_string()))
        .collect();
    // v1 con las opciones del PRD, con las dos estrategias.
    for strategy in STRATEGIES {
        let entities = engine("modelo")
            .detect_with(&segments, &options(strategy))
            .unwrap();
        assert_eq!(summary(&segments, &entities), expected, "v1, {strategy:?}");
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn entities_carry_segment_label_and_score() {
    let segments = ["Sin datos aquí.", "Escribe a Lucía Ferrer."];
    for name in MODELS {
        let entities = engine(name).detect(&segments).unwrap();
        let lucia = entities
            .iter()
            .find(|e| e.text == "Lucía Ferrer")
            .unwrap_or_else(|| panic!("{name}: {entities:?}"));
        assert_eq!(lucia.segment, 1);
        assert_eq!((lucia.start, lucia.end), (10, 22));
        assert_eq!(lucia.label, "PERSON_NAME");
        assert!(
            lucia.score >= 0.5 && lucia.score <= 1.0,
            "{name}: {}",
            lucia.score
        );
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn empty_and_blank_segments_do_not_fail() {
    let segments = ["", "   ", "\n\t", "Hola."];
    for name in MODELS {
        assert!(engine(name).detect(&segments).unwrap().is_empty(), "{name}");
        assert!(
            engine(name).detect::<&str>(&[]).unwrap().is_empty(),
            "{name}"
        );
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn batching_and_padding_do_not_change_the_result() {
    // Segmentos de longitudes muy distintas: en lotes de 8 se rellenan hasta
    // el más largo; en lotes de 1, no. El resultado debe ser el mismo.
    let long = "Marta Vidal firmó el acuerdo en Girona. ".repeat(40);
    let segments = [
        "Llama a Pedro Ruiz.",
        long.as_str(),
        "Vive en Sevilla.",
        "El correo es pedro.ruiz@ejemplo.es",
    ];
    // El modelo está cuantizado a INT8 de forma dinámica: la escala se calcula
    // con todo el lote, así que lo que acompaña a una ventana cambia un poco
    // sus probabilidades. Las entidades deben ser las mismas; la puntuación
    // puede variar algo más de una décima (medido: hasta 0,12 en v1).
    for name in MODELS {
        let batched = engine(name).detect(&segments).unwrap();
        let one_by_one = EngineOptions {
            batch_size: 1,
            ..EngineOptions::default()
        };
        let single = engine(name).detect_with(&segments, &one_by_one).unwrap();
        assert_eq!(batched.len(), single.len(), "{name}");
        for (a, b) in batched.iter().zip(&single) {
            assert_eq!(
                (a.segment, a.start, a.end, &a.kind),
                (b.segment, b.start, b.end, &b.kind),
                "{name}"
            );
            assert!(
                (a.score - b.score).abs() < 0.2,
                "{name}: {} frente a {}",
                a.score,
                b.score
            );
        }
    }
}

/// Texto de más de 512 tokens con un nombre en la zona de solape entre la
/// primera y la segunda ventana (tokens 382 a 509).
fn text_with_name_in_overlap(name: &str) -> String {
    let resources = resources();
    let tokenizer = TextTokenizer::load(&resources.model(name).tokenizer_path()).unwrap();
    let filler = "El informe describe el procedimiento general de la oficina sin cambios. ";
    let sentence = "La solicitud la firma Carlos García Pérez. ";
    for repeat in 1..100 {
        let before = filler.repeat(repeat);
        let text = format!("{before}{sentence}{}", filler.repeat(30));
        let name_char = before.chars().count() + "La solicitud la firma ".chars().count();
        let tokens = tokenizer.encode(&text).unwrap();
        let name_token = tokens.iter().position(|t| t.end > name_char).unwrap();
        if (420..470).contains(&name_token) {
            return text;
        }
    }
    panic!("{name}: no se ha podido colocar el nombre en el solape");
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn an_entity_in_the_overlap_is_detected_once() {
    for name in MODELS {
        let text = text_with_name_in_overlap(name);
        let entities = engine(name).detect(&[text.as_str()]).unwrap();
        let carlos: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.text.contains("Carlos"))
            .collect();
        assert_eq!(carlos.len(), 1, "{name}: {carlos:?}");
        assert_eq!(carlos[0].text, "Carlos García Pérez", "{name}");
        assert_eq!(carlos[0].kind, "NOMBRE_PERSONA", "{name}");
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn an_amount_with_decimal_comma_is_detected_whole() {
    // Antes la coma partía «189,90» en dos palabras y v1 dejaba los
    // céntimos a la vista.
    let segments = ["Importe total: 189,90 €"];
    let entities = engine("modelo").detect(&segments).unwrap();
    assert_eq!(
        summary(&segments, &entities),
        [("189,90 €".to_owned(), "IMPORTE".to_owned())]
    );
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn detect_timed_counts_the_text_tokens() {
    let segments = [
        "Carlos García vive en Madrid.",
        "",
        "Llamadle al 612 345 678.",
    ];
    for name in MODELS {
        let tokenizer = TextTokenizer::load(&resources().model(name).tokenizer_path()).unwrap();
        let expected: usize = segments
            .iter()
            .map(|s| tokenizer.encode(s).unwrap().len())
            .sum();
        let (_, timings) = engine(name)
            .detect_timed(&segments, &EngineOptions::default())
            .unwrap();
        assert!(expected > 0);
        assert_eq!(timings.tokens, expected, "{name}");
    }
}
