//! CLI para pruebas y mediciones del núcleo.
//!
//! Uso:
//!   cli version
//!   cli comprobar-recursos [carpeta] [--modelo <nombre>]...
//!   cli entidades <archivo> [opciones]

use std::process::ExitCode;
use std::time::{Duration, Instant};

use nucleo::motor::decodificacion::WordStrategy;
use nucleo::motor::opcionales::SkippedTypes;
use nucleo::motor::texto::split_paragraphs;
use nucleo::motor::{Engine, EngineOptions};
use nucleo::recursos::{DEFAULT_MODEL, Resources};

const USAGE: &str = "Uso:
  cli version
  cli comprobar-recursos [carpeta] [--modelo <nombre>]...
  cli entidades <archivo> [opciones]
      --modelo <nombre>              carpeta del modelo dentro de recursos/ (por defecto: modelo)
      --decodificacion primer|media  estrategia de palabra (por defecto: primer)
      --umbral <0..1>                puntuación mínima de una entidad (por defecto: 0,5)
      --no-sustituir <T1,T2>         tipos opcionales que no se sustituyen
      --tiempos                      tiempo de cada fase en la salida de error";

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
            match arg {
                "--modelo" => model = args.next().ok_or(ArgsError::Usage)?,
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
        let ms = |d: Duration| format!("{:.1} ms", d.as_secs_f64() * 1000.0);
        eprintln!("Carga del modelo: {}", ms(load));
        eprintln!("Tokenización:     {}", ms(timings.tokenization));
        eprintln!("Inferencia:       {}", ms(timings.inference));
        eprintln!("Decodificación:   {}", ms(timings.decoding));
        eprintln!("Total:            {}", ms(total.elapsed()));
    }
    ExitCode::SUCCESS
}
