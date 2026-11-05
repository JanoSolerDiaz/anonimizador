//! Motor completo con los modelos reales: tokenización, troceado, inferencia
//! por lotes y decodificación.
//!
//! Cada modelo se carga una sola vez y lo comparten todas las pruebas.

use std::sync::OnceLock;

use nucleo::Error;
use nucleo::motor::decodificacion::WordStrategy;
use nucleo::motor::tokenizacion::TextTokenizer;
use nucleo::motor::{ContextMode, Engine, EngineOptions, Entity};
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
    assert_eq!(options.context, ContextMode::Combined);
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

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn the_card_rule_covers_cards_the_model_only_partly_detects() {
    // v1 solo marca «5369»; la regla de tarjetas tapa el número entero.
    let segments = [
        "Internal note: the customer read out card number 5369 2714 6382 9011, security code 905, over the phone.",
    ];
    let entities = engine("modelo").detect(&segments).unwrap();
    let found = summary(&segments, &entities);
    assert!(
        found.contains(&(
            "5369 2714 6382 9011".to_owned(),
            "TARJETA_BANCARIA".to_owned()
        )),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|(text, _)| text != "5369 2714 6382 9011"
            && "5369 2714 6382 9011".contains(text.as_str())),
        "quedan trozos de la tarjeta: {found:?}"
    );
}

// ---------------------------------------------------------------- contexto

fn with_context(context: ContextMode) -> EngineOptions {
    EngineOptions {
        context,
        ..EngineOptions::default()
    }
}

/// Párrafos de un texto del corpus sintético.
fn corpus_paragraphs(name: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/corpus/texto")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    nucleo::motor::texto::split_paragraphs(&text)
        .iter()
        .map(|p| p.text.to_owned())
        .collect()
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn a_short_paragraph_uses_the_context_of_its_neighbours() {
    // Solo, «Refiere un trastorno depresivo…» no da ningún dato de salud.
    // Con el resto del informe alrededor, sí.
    let paragraphs = corpus_paragraphs("es-07-valoracion-social.txt");
    let segments: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
    let target = segments
        .iter()
        .position(|s| s.contains("trastorno depresivo"))
        .unwrap();
    let health_in_target = |context| {
        engine("modelo")
            .detect_with(&segments, &with_context(context))
            .unwrap()
            .into_iter()
            .filter(|e| e.segment == target && e.kind == "DATOS_SALUD")
            .map(|e| e.text)
            .collect::<Vec<_>>()
    };
    assert!(health_in_target(ContextMode::Paragraph).is_empty());
    assert_eq!(
        health_in_target(ContextMode::Document),
        ["trastorno depresivo"]
    );
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn paragraph_context_is_the_same_as_each_segment_alone() {
    let paragraphs = corpus_paragraphs("es-02-informe-medico.txt");
    let segments: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
    // En lotes de uno, para que cada segmento vaya solo en su pasada como
    // cuando se analiza aparte: la cuantización INT8 mueve un poco las
    // puntuaciones según qué textos comparten lote.
    let one_by_one = EngineOptions {
        batch_size: 1,
        ..with_context(ContextMode::Paragraph)
    };
    let together = engine("modelo")
        .detect_with(&segments, &one_by_one)
        .unwrap();
    let mut alone = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        for mut e in engine("modelo")
            .detect_with(&[*segment], &with_context(ContextMode::Paragraph))
            .unwrap()
        {
            e.segment = i;
            alone.push(e);
        }
    }
    let strip = |entities: Vec<Entity>| -> Vec<(usize, usize, usize, String)> {
        entities
            .into_iter()
            .map(|e| (e.segment, e.start, e.end, e.kind))
            .collect()
    };
    assert_eq!(strip(together), strip(alone));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn document_context_over_several_windows_keeps_each_entity_in_its_segment() {
    let sentence = "El contrato lo firmó Carlos García Pérez en la notaría de Madrid el pasado martes, ante dos testigos y con toda la documentación en regla.";
    let segments = vec![sentence; 55];
    let (entities, timings) = engine("modelo")
        .detect_timed(&segments, &with_context(ContextMode::Document))
        .unwrap();
    assert!(timings.tokens > 1500, "solo {} tokens", timings.tokens);
    // Posición en caracteres: «firmó» lleva una letra de dos bytes.
    let start = sentence[..sentence.find("Carlos").unwrap()].chars().count();
    for segment in 0..segments.len() {
        let names: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.segment == segment && e.kind == "NOMBRE_PERSONA")
            .collect();
        assert_eq!(names.len(), 1, "segmento {segment}: {names:?}");
        assert_eq!(names[0].text, "Carlos García Pérez", "segmento {segment}");
        assert_eq!(names[0].start, start, "segmento {segment}");
    }
    summary(&segments, &entities);
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn combined_context_covers_what_either_reading_finds() {
    // «trastorno depresivo» solo lo tapa la lectura seguida; «Marchand», en
    // «Mme Marchand», solo la lectura párrafo a párrafo.
    // `covered(archivo, párrafo, palabra, modo)`: la palabra del párrafo que
    // contiene `párrafo` queda tapada entera.
    let covered = |file: &str, paragraph: &str, word: &str, context: ContextMode| -> bool {
        let paragraphs = corpus_paragraphs(file);
        let segments: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
        let target = segments.iter().position(|s| s.contains(paragraph)).unwrap();
        let text = segments[target];
        let start = text[..text.find(word).unwrap()].chars().count();
        let end = start + word.chars().count();
        let entities = engine("modelo")
            .detect_with(&segments, &with_context(context))
            .unwrap();
        (start..end).all(|i| {
            entities
                .iter()
                .any(|e| e.segment == target && e.start <= i && i < e.end)
        })
    };
    let health = (
        "es-07-valoracion-social.txt",
        "trastorno depresivo",
        "depresivo",
    );
    let name = ("fr-02-compte-rendu.txt", "Mme Marchand", "Marchand");
    assert!(!covered(
        health.0,
        health.1,
        health.2,
        ContextMode::Paragraph
    ));
    assert!(!covered(name.0, name.1, name.2, ContextMode::Document));
    assert!(covered(health.0, health.1, health.2, ContextMode::Combined));
    assert!(covered(name.0, name.1, name.2, ContextMode::Combined));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn progress_goes_up_to_one_and_gives_the_same_entities() {
    let engine = engine("modelo");
    let segments: Vec<String> = (0..40)
        .map(|n| format!("El cliente número {n} es Marta Lozano Ibáñez, con DNI 48291736Q."))
        .collect();
    let seen = std::sync::Mutex::new(Vec::new());
    let with_progress = engine
        .detect_with_progress(&segments, &|done| seen.lock().unwrap().push(done))
        .unwrap();
    assert_eq!(with_progress, engine.detect(&segments).unwrap());
    let seen = seen.into_inner().unwrap();
    assert!(seen.len() > 2, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert_eq!(seen.first(), Some(&0.0));
    assert_eq!(seen.last(), Some(&1.0));
    // Con la lectura `ambos`, la primera acaba en la mitad.
    assert!(seen.contains(&0.5), "{seen:?}");
}
