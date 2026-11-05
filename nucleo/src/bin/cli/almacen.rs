//! Abrir y guardar el almacén, y la orden `cli almacen`. El almacén abierto
//! es el de `nucleo` ([`sesion::OpenStore`]); aquí solo se elige su ruta y
//! los errores pasan a texto para el usuario.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jiff::Zoned;
use nucleo::almacen::archivo;
use nucleo::almacen::sesion::{self, EXPORT_WARNING};

use super::ArgsError;

/// Almacén abierto, con los documentos caducados ya borrados.
pub struct OpenStore(sesion::OpenStore);

impl std::ops::Deref for OpenStore {
    type Target = sesion::OpenStore;

    fn deref(&self) -> &sesion::OpenStore {
        &self.0
    }
}

impl std::ops::DerefMut for OpenStore {
    fn deref_mut(&mut self) -> &mut sesion::OpenStore {
        &mut self.0
    }
}

impl OpenStore {
    /// Abre el almacén de `path`, o el de la carpeta de datos del usuario, y
    /// borra los documentos caducados. El mensaje de error ya es para el
    /// usuario.
    pub fn open(path: Option<&str>) -> Result<OpenStore, String> {
        let path = match path {
            Some(path) => PathBuf::from(path),
            None => archivo::default_path().ok_or_else(|| {
                "No se encuentra la carpeta de datos del usuario; indique el almacén con --almacen."
                    .to_owned()
            })?,
        };
        sesion::OpenStore::open(&path, Zoned::now().date())
            .map(OpenStore)
            .map_err(|e| e.to_string())
    }

    /// Guarda el almacén entero, creando su carpeta si hace falta.
    pub fn save(&self) -> Result<(), String> {
        self.0.save().map_err(|e| e.to_string())
    }

    /// Guarda solo si al abrirlo se borró algo.
    pub fn save_if_purged(&self) -> Result<(), String> {
        self.0.save_if_purged().map_err(|e| e.to_string())
    }
}

enum Action<'a> {
    List,
    Export { id: &'a str, dir: &'a str },
    Import { file: &'a str },
    Forget { id: &'a str },
    Clear { confirmed: bool },
    Purge,
}

struct StoreArgs<'a> {
    action: Action<'a>,
    store: Option<&'a str>,
}

impl<'a> StoreArgs<'a> {
    fn parse(args: &'a [String]) -> Result<Self, ArgsError> {
        let mut args = args.iter().map(String::as_str);
        let command = args.next().ok_or(ArgsError::Usage)?;
        let mut positional = Vec::new();
        let (mut store, mut dir, mut confirmed) = (None, None, false);
        while let Some(arg) = args.next() {
            match arg {
                "--almacen" => store = Some(args.next().ok_or(ArgsError::Usage)?),
                "--salida" => dir = Some(args.next().ok_or(ArgsError::Usage)?),
                "--confirmar" => confirmed = true,
                _ if arg.starts_with("--") => return Err(ArgsError::Usage),
                _ => positional.push(arg),
            }
        }
        let one = |positional: &[&'a str]| match positional {
            [only] => Ok(*only),
            _ => Err(ArgsError::Usage),
        };
        let none = |positional: &[&'a str]| {
            if positional.is_empty() {
                Ok(())
            } else {
                Err(ArgsError::Usage)
            }
        };
        let action = match command {
            "listar" => none(&positional).map(|()| Action::List)?,
            "exportar" => Action::Export {
                id: one(&positional)?,
                dir: dir.ok_or(ArgsError::Usage)?,
            },
            "importar" => Action::Import {
                file: one(&positional)?,
            },
            "olvidar" => Action::Forget {
                id: one(&positional)?,
            },
            "vaciar" => none(&positional).map(|()| Action::Clear { confirmed })?,
            "purgar" => none(&positional).map(|()| Action::Purge)?,
            _ => return Err(ArgsError::Usage),
        };
        Ok(StoreArgs { action, store })
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let args = match StoreArgs::parse(args) {
        Ok(args) => args,
        Err(_) => return super::usage(),
    };
    if let Action::Clear { confirmed: false } = args.action {
        eprintln!(
            "Vaciar borra todos los documentos, entradas y diccionarios del almacén, y no se puede deshacer. Repita la orden con --confirmar."
        );
        return ExitCode::from(2);
    }
    let mut open = match OpenStore::open(args.store) {
        Ok(open) => open,
        Err(message) => return fail(&message),
    };
    let result = match args.action {
        Action::List => {
            list(&open);
            open.save_if_purged()
        }
        Action::Export { id, dir } => {
            export(&open, id, Path::new(dir)).and_then(|()| open.save_if_purged())
        }
        Action::Import { file } => import(&mut open, Path::new(file)).and_then(|()| open.save()),
        Action::Forget { id } => {
            if open.store.documents().iter().any(|d| d.id == id) {
                open.store.forget(id);
                println!("Documento olvidado: {id}.");
                open.save()
            } else {
                Err(nucleo::Error::DocumentNotFound.to_string())
            }
        }
        Action::Clear { .. } => {
            open.store.clear();
            println!("Almacén vaciado.");
            open.save()
        }
        Action::Purge => {
            println!("Documentos caducados borrados: {}.", open.purged);
            open.save_if_purged()
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&message),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// Documentos y diccionarios, sin ningún valor.
fn list(open: &OpenStore) {
    println!("Almacén: {}", open.path.display());
    let store = &open.store;
    if store.documents().is_empty() && store.imported().is_empty() {
        println!("El almacén está vacío.");
        return;
    }
    println!("Documentos: {}", store.documents().len());
    for d in store.documents() {
        println!(
            "  {}  {}  caduca {}  {} → {}  {} marcadores",
            d.id,
            d.created,
            d.expires,
            d.origin.name,
            d.output.name,
            d.markers.len()
        );
    }
    println!("Diccionarios importados: {}", store.imported().len());
    for d in store.imported() {
        println!(
            "  {}  documento {}  {} entradas",
            d.name(),
            d.document().id,
            d.entries().len()
        );
    }
}

fn export(open: &OpenStore, id: &str, dir: &Path) -> Result<(), String> {
    let path = open.export(id, dir).map_err(|e| e.to_string())?;
    println!("Diccionario exportado: {}", path.display());
    eprintln!("{EXPORT_WARNING}");
    Ok(())
}

fn import(open: &mut OpenStore, file: &Path) -> Result<(), String> {
    let name = open.0.import(file).map_err(|e| e.to_string())?;
    println!("Diccionario importado: {name}.");
    Ok(())
}
