//! Órdenes `cli anonimizar` y `cli desanonimizar`. Un error en un archivo
//! no detiene el resto del lote; el almacén se guarda una vez, al final.

use std::cell::OnceCell;
use std::path::Path;
use std::process::ExitCode;

use jiff::Zoned;
use nucleo::formatos::pdf::{DEFAULT_RESOLUTION, MAX_RESOLUTION, MIN_RESOLUTION};
use nucleo::motor::opcionales::SkippedTypes;
use nucleo::motor::{Engine, EngineOptions, Entity};
use nucleo::operaciones::anonimizar::{AnonymizeOptions, anonymize};
use nucleo::operaciones::desanonimizar::{
    Conflict, MarkerSource, Outcome, Report, deanonymize, deanonymize_text,
};
use nucleo::operaciones::{Detector, InputFormat, OutputFormat, model_id};
use nucleo::recursos::{DEFAULT_MODEL, Resources};

use super::almacen::OpenStore;
use super::{ArgsError, parse_engine_option};

/// El motor se carga la primera vez que hace falta detectar: un lote en el
/// que ningún archivo llega tan lejos no carga el modelo.
struct LazyEngine<'a> {
    model: &'a str,
    options: EngineOptions,
    engine: OnceCell<Engine>,
}

impl Detector for LazyEngine<'_> {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        let engine = match self.engine.get() {
            Some(engine) => engine,
            None => {
                let resources = Resources::locate()?;
                let engine = Engine::load(
                    &resources,
                    &resources.model(self.model),
                    self.options.clone(),
                )?;
                self.engine.get_or_init(|| engine)
            }
        };
        Engine::detect(engine, segments)
    }
}

/// Argumentos de `anonimizar`.
struct AnonymizeArgs<'a> {
    files: Vec<&'a str>,
    dir: &'a str,
    format: Option<OutputFormat>,
    store: Option<&'a str>,
    model: &'a str,
    options: EngineOptions,
    /// En Excel, sustituir también los importes.
    spreadsheet_amounts: bool,
    /// Resolución de las páginas de PDF con datos.
    resolution: u32,
}

impl<'a> AnonymizeArgs<'a> {
    fn parse(args: &'a [String]) -> Result<Self, ArgsError> {
        let mut files = Vec::new();
        let (mut dir, mut format, mut store) = (None, None, None);
        let mut spreadsheet_amounts = false;
        let mut resolution = DEFAULT_RESOLUTION;
        let mut model = DEFAULT_MODEL;
        let mut options = EngineOptions::default();
        let mut args = args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            if parse_engine_option(arg, &mut args, &mut model, &mut options)? {
                continue;
            }
            match arg {
                "--salida" => dir = Some(args.next().ok_or(ArgsError::Usage)?),
                "--almacen" => store = Some(args.next().ok_or(ArgsError::Usage)?),
                "--formato" => {
                    format = Some(match args.next() {
                        Some("pdf") => OutputFormat::Pdf,
                        Some("docx") => OutputFormat::Docx,
                        Some("pptx") => OutputFormat::Pptx,
                        Some("xlsx") => OutputFormat::Xlsx,
                        Some("md") => OutputFormat::Markdown,
                        Some("txt") => OutputFormat::Text,
                        _ => return Err(ArgsError::Usage),
                    })
                }
                "--sustituir-importes" => spreadsheet_amounts = true,
                "--ppp" => {
                    let value = args.next().ok_or(ArgsError::Usage)?;
                    resolution = value.parse().map_err(|_| ArgsError::Usage)?;
                    if !(MIN_RESOLUTION..=MAX_RESOLUTION).contains(&resolution) {
                        return Err(ArgsError::Invalid(nucleo::Error::InvalidResolution(
                            resolution,
                        )));
                    }
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
                _ if arg.starts_with("--") => return Err(ArgsError::Usage),
                _ => files.push(arg),
            }
        }
        if files.is_empty() {
            return Err(ArgsError::Usage);
        }
        Ok(Self {
            files,
            dir: dir.ok_or(ArgsError::Usage)?,
            format,
            store,
            model,
            options,
            spreadsheet_amounts,
            resolution,
        })
    }
}

pub fn anonymize_files(args: &[String]) -> ExitCode {
    let args = match AnonymizeArgs::parse(args) {
        Ok(args) => args,
        Err(error) => return super::args_error(error),
    };
    let mut open = match OpenStore::open(args.store) {
        Ok(open) => open,
        Err(message) => return fail(&message),
    };
    let engine = LazyEngine {
        model: args.model,
        options: args.options.clone(),
        engine: OnceCell::new(),
    };
    let model = model_id(args.model);
    let mut done = 0;
    for file in &args.files {
        let path = Path::new(file);
        let options = AnonymizeOptions {
            format: args.format.unwrap_or_else(|| same_format(path)),
            model: model.clone(),
            now: Zoned::now(),
            spreadsheet_amounts: args.spreadsheet_amounts,
            pdf_resolution: args.resolution,
        };
        match anonymize(
            path,
            Path::new(args.dir),
            &engine,
            &mut open.store,
            &options,
        ) {
            Ok(summary) => {
                done += 1;
                println!("{file} → {}", summary.output.display());
                if summary.entities.is_empty() {
                    println!("  Sin datos personales detectados.");
                } else {
                    let counts: Vec<String> = summary
                        .entities
                        .iter()
                        .map(|(kind, n)| format!("{kind}: {n}"))
                        .collect();
                    println!("  {}", counts.join(", "));
                }
                if summary.warnings.images > 0 {
                    println!(
                        "  El documento tiene {} imágenes. Su contenido no se ha revisado.",
                        summary.warnings.images
                    );
                }
                if summary.warnings.objects > 0 {
                    println!(
                        "  El documento tiene {} objetos incrustados. Su contenido no se ha revisado.",
                        summary.warnings.objects
                    );
                }
                if summary.warnings.attachments > 0 {
                    println!(
                        "  El PDF tenía {} archivos adjuntos. Se han quitado de la salida.",
                        summary.warnings.attachments
                    );
                }
            }
            Err(error) => eprintln!("{file}: {error}"),
        }
    }
    println!("Anonimizados: {done} de {}.", args.files.len());
    if let Err(message) = open.save() {
        return fail(&message);
    }
    if done == args.files.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Sin `--formato`, cada archivo sale en su formato.
fn same_format(path: &Path) -> OutputFormat {
    // Un paquete de Office se reconoce por su contenido: hace falta entero.
    let bytes = std::fs::read(path).unwrap_or_default();
    InputFormat::detect(path, &bytes)
        .map(InputFormat::same_output)
        .unwrap_or(OutputFormat::Text)
}

/// Qué desanonimizar: archivos o texto pegado.
enum Input<'a> {
    Files { files: Vec<&'a str>, dir: &'a str },
    Text(&'a str),
}

struct DeanonymizeArgs<'a> {
    input: Input<'a>,
    source: Option<MarkerSource>,
    store: Option<&'a str>,
}

impl<'a> DeanonymizeArgs<'a> {
    fn parse(args: &'a [String]) -> Result<Self, ArgsError> {
        let mut files = Vec::new();
        let (mut dir, mut text, mut source, mut store) = (None, None, None, None);
        let mut args = args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            match arg {
                "--salida" => dir = Some(args.next().ok_or(ArgsError::Usage)?),
                "--texto" => text = Some(args.next().ok_or(ArgsError::Usage)?),
                "--almacen" => store = Some(args.next().ok_or(ArgsError::Usage)?),
                "--fuente" => {
                    source = Some(match args.next().ok_or(ArgsError::Usage)? {
                        "propio" => MarkerSource::Store,
                        name => MarkerSource::Dictionary(name.to_owned()),
                    })
                }
                _ if arg.starts_with("--") => return Err(ArgsError::Usage),
                _ => files.push(arg),
            }
        }
        let input = match (text, dir) {
            (Some(text), None) if files.is_empty() => Input::Text(text),
            (None, Some(dir)) if !files.is_empty() => Input::Files { files, dir },
            _ => return Err(ArgsError::Usage),
        };
        Ok(Self {
            input,
            source,
            store,
        })
    }
}

pub fn deanonymize_files(args: &[String]) -> ExitCode {
    let args = match DeanonymizeArgs::parse(args) {
        Ok(args) => args,
        Err(error) => return super::args_error(error),
    };
    let open = match OpenStore::open(args.store) {
        Ok(open) => open,
        Err(message) => return fail(&message),
    };
    let source = args.source.as_ref();
    let all_done = match args.input {
        Input::Text(text) => match deanonymize_text(text, &open.store, source) {
            Ok(Outcome::Done((restored, report))) => {
                println!("{restored}");
                eprint!("{}", report_lines(&report));
                true
            }
            Ok(Outcome::Conflicts(conflicts)) => {
                eprint!("{}", conflict_lines("Texto", &conflicts));
                false
            }
            Err(error) => {
                eprintln!("{error}");
                false
            }
        },
        Input::Files { files, dir } => {
            let mut done = 0;
            for file in &files {
                match deanonymize(Path::new(file), Path::new(dir), &open.store, source) {
                    Ok(Outcome::Done((output, report))) => {
                        done += 1;
                        println!("{file} → {}", output.display());
                        print!("{}", report_lines(&report));
                    }
                    Ok(Outcome::Conflicts(conflicts)) => {
                        eprint!("{}", conflict_lines(file, &conflicts))
                    }
                    Err(error) => eprintln!("{file}: {error}"),
                }
            }
            println!("Desanonimizados: {done} de {}.", files.len());
            done == files.len()
        }
    };
    if let Err(message) = open.save_if_purged() {
        return fail(&message);
    }
    if all_done {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn report_lines(report: &Report) -> String {
    let mut lines = format!("  Restaurados: {}\n", report.restored.len());
    if !report.missing.is_empty() {
        lines.push_str(&format!("  Sin fuente: {}\n", report.missing.join(", ")));
    }
    lines
}

fn conflict_lines(label: &str, conflicts: &[Conflict]) -> String {
    let mut lines = String::new();
    for conflict in conflicts {
        let sources: Vec<String> = conflict
            .sources
            .iter()
            .map(|source| match source {
                MarkerSource::Store => "propio".to_owned(),
                MarkerSource::Dictionary(name) => format!("«{name}»"),
            })
            .collect();
        lines.push_str(&format!(
            "{label}: el marcador {} tiene valores distintos en: {}.\n",
            conflict.marker,
            sources.join(", ")
        ));
    }
    lines.push_str(&format!(
        "{label}: elija la fuente con --fuente propio o --fuente <nombre del diccionario>.\n"
    ));
    lines
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}
