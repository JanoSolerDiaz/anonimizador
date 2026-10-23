//! CLI para pruebas y mediciones del núcleo.
//!
//! Uso:
//!   cli version
//!   cli comprobar-recursos [carpeta]

use std::process::ExitCode;

use nucleo::recursos::Resources;

const USAGE: &str = "Uso:
  cli version
  cli comprobar-recursos [carpeta]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("version") => {
            println!("anonimizador {}", nucleo::VERSION);
            ExitCode::SUCCESS
        }
        Some("comprobar-recursos") => check_resources(args.get(1).map(String::as_str)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn check_resources(dir: Option<&str>) -> ExitCode {
    let resources = match dir {
        Some(dir) => Ok(Resources::new(dir)),
        None => Resources::locate(),
    };
    let result = resources.and_then(|r| {
        println!("Recursos: {}", r.root().display());
        r.smoke_test()
    });
    match result {
        Ok(report) => {
            println!("Modelo cargado.");
            println!("  Entradas: {}", report.model_inputs.join(", "));
            println!("  Salidas:  {}", report.model_outputs.join(", "));
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
