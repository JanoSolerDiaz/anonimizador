//! CLI para pruebas y mediciones del núcleo.
//!
//! Uso:
//!   cli version
//!   cli comprobar-recursos [carpeta] [--modelo <nombre>]...
//!   cli entidades <archivo> [opciones]
//!   cli evaluar <carpeta> [opciones]

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use nucleo::evaluacion::{Evaluation, Span, load_corpus};
use nucleo::motor::decodificacion::WordStrategy;
use nucleo::motor::opcionales::SkippedTypes;
use nucleo::motor::texto::split_paragraphs;
use nucleo::motor::{ContextMode, Engine, EngineOptions, Timings};
use nucleo::recursos::{DEFAULT_MODEL, Resources};

const USAGE: &str = "Uso:
  cli version
  cli comprobar-recursos [carpeta] [--modelo <nombre>]...
  cli entidades <archivo> [opciones]
      --modelo <nombre>              carpeta del modelo dentro de recursos/ (por defecto: modelo)
      --decodificacion primer|media  estrategia de palabra (por defecto: primer)
      --umbral <0..1>                puntuación mínima de una entidad (por defecto: 0,5)
      --hilos <n>                    hilos de cada pasada del modelo (por defecto: la mitad de los núcleos)
      --contexto ambos|documento|parrafo
                                     el modelo lee los párrafos aislados, seguidos o de las dos
                                     formas, y se tapa lo que encuentre cualquiera (por defecto: ambos)
      --no-sustituir <T1,T2>         tipos opcionales que no se sustituyen
      --tiempos                      tiempo de cada fase en la salida de error
  cli evaluar <carpeta> [opciones]
      --modelo, --decodificacion, --umbral, --hilos y --contexto como en entidades
      --detalle                      lista las entidades no detectadas y las de más";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("version") => {
            println!("anonimizador {}", nucleo::VERSION);
            ExitCode::SUCCESS
        }
        Some("comprobar-recursos") => match CheckArgs::parse(&args[1..]) {
            Some(check) => check_resources(check),
            None => usage(),
        },
        Some("entidades") => match EntitiesArgs::parse(&args[1..]) {
            Ok(entities) => print_entities(entities),
            Err(ArgsError::Usage) => usage(),
            Err(ArgsError::Invalid(error)) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        },
        Some("evaluar") => match EvaluateArgs::parse(&args[1..]) {
            Ok(evaluate) => evaluate_corpus(evaluate),
            Err(_) => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

/// Argumentos de `comprobar-recursos`.
struct CheckArgs<'a> {
    dir: Option<&'a str>,
    models: Vec<&'a str>,
}

impl<'a> CheckArgs<'a> {
    fn parse(args: &'a [String]) -> Option<Self> {
        let mut dir = None;
        let mut models = Vec::new();
        let mut args = args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            match arg {
                "--modelo" => models.push(args.next()?),
                _ if arg.starts_with("--") => return None,
                _ if dir.is_none() => dir = Some(arg),
                _ => return None,
            }
        }
        if models.is_empty() {
            models.push(DEFAULT_MODEL);
        }
        Some(Self { dir, models })
    }
}

fn check_resources(check: CheckArgs) -> ExitCode {
    let resources = match check.dir {
        Some(dir) => Ok(Resources::new(dir)),
        None => Resources::locate(),
    };
    let result = resources.and_then(|r| {
        println!("Recursos: {}", r.root().display());
        let models: Vec<_> = check.models.iter().map(|name| r.model(name)).collect();
        r.smoke_test_models(&models)
    });
    match result {
        Ok(report) => {
            for model in &report.models {
                println!("Modelo «{}» cargado.", model.name);
                println!("  Entradas: {}", model.inputs.join(", "));
                println!("  Salidas:  {}", model.outputs.join(", "));
            }
            println!(
                "PDFium cargado. PDF de prueba con {} página(s).",
                report.pdf_pages
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Entidad tal como se imprime, con los campos en este orden.
#[derive(serde::Serialize)]
struct EntityJson<'a> {
    segmento: usize,
    inicio: usize,
    fin: usize,
    tipo: &'a str,
    etiqueta: &'a str,
    texto: &'a str,
    puntuacion: f64,
}

enum ArgsError {
    /// Faltan argumentos o sobran: se muestra el uso.
    Usage,
    /// Los argumentos tienen la forma correcta pero un valor no vale.
    Invalid(nucleo::Error),
}

/// Lee una opción del motor común a `entidades` y `evaluar`. Devuelve
/// `Ok(false)` si `arg` no es una de ellas.
fn parse_engine_option<'a>(
    arg: &str,
    args: &mut impl Iterator<Item = &'a str>,
    model: &mut &'a str,
    options: &mut EngineOptions,
) -> Result<bool, ArgsError> {
    match arg {
        "--modelo" => *model = args.next().ok_or(ArgsError::Usage)?,
        "--decodificacion" => {
            options.decode.strategy = match args.next() {
                Some("primer") => WordStrategy::FirstSubtoken,
                Some("media") => WordStrategy::WordMean,
                _ => return Err(ArgsError::Usage),
            }
        }
        "--umbral" => {
            options.decode.threshold = args
                .next()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|t| (0.0..=1.0).contains(t))
                .ok_or(ArgsError::Usage)?
        }
        "--contexto" => {
            options.context = match args.next() {
                Some("ambos") => ContextMode::Combined,
                Some("documento") => ContextMode::Document,
                Some("parrafo") => ContextMode::Paragraph,
                _ => return Err(ArgsError::Usage),
            }
        }
        "--hilos" => {
            options.threads = args
                .next()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|&n| n > 0)
                .ok_or(ArgsError::Usage)?
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Argumentos de `entidades`.
struct EntitiesArgs<'a> {
    file: &'a str,
    model: &'a str,
    options: EngineOptions,
    timings: bool,
}

impl<'a> EntitiesArgs<'a> {
    fn parse(args: &'a [String]) -> Result<Self, ArgsError> {
        let mut file = None;
        let mut model = DEFAULT_MODEL;
        let mut options = EngineOptions::default();
        let mut timings = false;
        let mut args = args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            if parse_engine_option(arg, &mut args, &mut model, &mut options)? {
                continue;
            }
            match arg {
                "--no-sustituir" => {
                    let list = args.next().ok_or(ArgsError::Usage)?;
                    let kinds: Vec<&str> = list
                        .split(',')
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .collect();
                    options.skipped = SkippedTypes::new(&kinds).map_err(ArgsError::Invalid)?;
                }
                "--tiempos" => timings = true,
                _ if arg.starts_with("--") => return Err(ArgsError::Usage),
                _ if file.is_none() => file = Some(arg),
                _ => return Err(ArgsError::Usage),
            }
        }
        Ok(Self {
            file: file.ok_or(ArgsError::Usage)?,
            model,
            options,
            timings,
        })
    }
}

fn print_entities(args: EntitiesArgs) -> ExitCode {
    let text = match std::fs::read_to_string(args.file) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("No se puede leer el archivo «{}»: {e}", args.file);
            return ExitCode::FAILURE;
        }
    };
    let segments: Vec<&str> = split_paragraphs(&text).iter().map(|p| p.text).collect();
    if segments.is_empty() {
        println!("[]");
        return ExitCode::SUCCESS;
    }

    let total = Instant::now();
    let result = Resources::locate().and_then(|resources| {
        let load = Instant::now();
        let engine = Engine::load(
            &resources,
            &resources.model(args.model),
            args.options.clone(),
        )?;
        let load = load.elapsed();
        engine
            .detect_timed(&segments, &args.options)
            .map(|(entities, timings)| (entities, load, timings))
    });
    let (entities, load, timings) = match result {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let json: Vec<EntityJson> = entities
        .iter()
        .map(|e| EntityJson {
            segmento: e.segment,
            inicio: e.start,
            fin: e.end,
            tipo: &e.kind,
            etiqueta: &e.label,
            texto: &e.text,
            puntuacion: (f64::from(e.score) * 10_000.0).round() / 10_000.0,
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json).expect("la lista de entidades siempre se serializa")
    );

    if args.timings {
        eprint!(
            "{}",
            timings_report(load, &timings, total.elapsed(), args.options.threads)
        );
    }
    ExitCode::SUCCESS
}

/// Tiempo de cada fase, una por línea, con los tokens analizados por
/// segundo de análisis (sin contar la carga del modelo).
fn timings_report(load: Duration, timings: &Timings, total: Duration, threads: usize) -> String {
    let ms = |d: Duration| format!("{:.1} ms", d.as_secs_f64() * 1000.0);
    let mut report: String = [
        ("Carga del modelo: ", load),
        ("Tokenización:     ", timings.tokenization),
        ("Inferencia:       ", timings.inference),
        ("Decodificación:   ", timings.decoding),
        ("Total:            ", total),
    ]
    .iter()
    .map(|(phase, d)| format!("{phase}{}\n", ms(*d)))
    .collect();
    let analysis = (timings.tokenization + timings.inference + timings.decoding).as_secs_f64();
    report.push_str(&format!("Hilos:            {threads}\n"));
    report.push_str(&format!("Tokens:           {}\n", timings.tokens));
    if analysis > 0.0 {
        report.push_str(&format!(
            "Tokens/segundo:   {:.0}\n",
            timings.tokens as f64 / analysis
        ));
    }
    report
}

/// Argumentos de `evaluar`.
struct EvaluateArgs<'a> {
    dir: &'a str,
    model: &'a str,
    options: EngineOptions,
    detail: bool,
}

impl<'a> EvaluateArgs<'a> {
    fn parse(args: &'a [String]) -> Result<Self, ArgsError> {
        let mut dir = None;
        let mut model = DEFAULT_MODEL;
        let mut options = EngineOptions::default();
        let mut detail = false;
        let mut args = args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            if parse_engine_option(arg, &mut args, &mut model, &mut options)? {
                continue;
            }
            match arg {
                "--detalle" => detail = true,
                _ if arg.starts_with("--") => return Err(ArgsError::Usage),
                _ if dir.is_none() => dir = Some(arg),
                _ => return Err(ArgsError::Usage),
            }
        }
        Ok(Self {
            dir: dir.ok_or(ArgsError::Usage)?,
            model,
            options,
            detail,
        })
    }
}

/// Número con dos decimales y coma, o una raya si no hay datos.
fn decimal(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |v| format!("{v:.2}").replace('.', ","))
}

fn evaluate_corpus(args: EvaluateArgs) -> ExitCode {
    let corpus = match load_corpus(Path::new(args.dir)) {
        Ok(corpus) => corpus,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let total = Instant::now();
    let engine = Resources::locate().and_then(|resources| {
        Engine::load(
            &resources,
            &resources.model(args.model),
            args.options.clone(),
        )
    });
    let engine = match engine {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let load = total.elapsed();

    let mut evaluation = Evaluation::default();
    let mut timings = Timings::default();
    let mut details = String::new();
    for text in &corpus {
        let paragraphs = split_paragraphs(&text.text);
        let segments: Vec<&str> = paragraphs.iter().map(|p| p.text).collect();
        let (entities, spent) = match engine.detect_timed(&segments, &args.options) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("{}: {e}", text.name);
                return ExitCode::FAILURE;
            }
        };
        timings.tokenization += spent.tokenization;
        timings.inference += spent.inference;
        timings.decoding += spent.decoding;
        timings.tokens += spent.tokens;

        // De posiciones dentro del párrafo a posiciones en el texto completo.
        let detected: Vec<Span> = entities
            .iter()
            .map(|e| {
                let offset = paragraphs[e.segment].start;
                Span {
                    start: offset + e.start,
                    end: offset + e.end,
                    kind: e.kind.clone(),
                }
            })
            .collect();
        let comparison = evaluation.add(&text.text, &text.expected, &detected);
        if !(comparison.missed.is_empty() && comparison.spurious.is_empty()) {
            let chars: Vec<char> = text.text.chars().collect();
            let show = |s: &Span| -> String {
                chars[s.start..s.end]
                    .iter()
                    .map(|&c| if c.is_whitespace() { ' ' } else { c })
                    .collect()
            };
            details.push_str(&format!("\n{}\n", text.name));
            for s in &comparison.missed {
                details.push_str(&format!("  No detectada: {} «{}»\n", s.kind, show(s)));
            }
            for s in &comparison.spurious {
                details.push_str(&format!("  De más: {} «{}»\n", s.kind, show(s)));
            }
        }
    }

    let strategy = match args.options.decode.strategy {
        WordStrategy::FirstSubtoken => "primer",
        WordStrategy::WordMean => "media",
    };
    let totals = evaluation.total();
    println!("Modelo: {}", args.model);
    println!("Decodificación: {strategy}");
    println!(
        "Contexto: {}",
        match args.options.context {
            ContextMode::Combined => "ambos",
            ContextMode::Document => "documento",
            ContextMode::Paragraph => "parrafo",
        }
    );
    println!(
        "Umbral: {}",
        decimal(Some(f64::from(args.options.decode.threshold)))
    );
    println!("Textos: {}", corpus.len());
    println!("Entidades esperadas: {}", totals.expected);
    println!();
    println!(
        "{:<24}{:>11}{:>12}{:>11}{:>11}{:>15}{:>9}",
        "Tipo", "Esperadas", "Detectadas", "Correctas", "Precisión", "Exhaustividad", "Tapadas"
    );
    let rows = evaluation
        .by_kind()
        .iter()
        .map(|(kind, counts)| (kind.as_str(), *counts))
        .chain(std::iter::once(("TOTAL", totals)));
    for (kind, c) in rows {
        println!(
            "{:<24}{:>11}{:>12}{:>11}{:>11}{:>15}{:>9}",
            kind,
            c.expected,
            c.detected,
            c.correct,
            decimal(c.precision()),
            decimal(c.recall()),
            decimal(c.coverage())
        );
    }
    println!();
    print!(
        "{}",
        timings_report(load, &timings, total.elapsed(), args.options.threads)
    );
    if args.detail {
        print!("\nDiferencias:\n{details}");
    }
    ExitCode::SUCCESS
}
