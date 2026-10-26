//! CLI para pruebas y mediciones del núcleo.
//!
//! Uso:
//!   cli version
//!   cli comprobar-recursos [carpeta] [--modelo <nombre>]...

use std::process::ExitCode;

use nucleo::recursos::{DEFAULT_MODEL, Resources};

const USAGE: &str = "Uso:
  cli version
  cli comprobar-recursos [carpeta] [--modelo <nombre>]...";

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
