//! Capa de Tauri: comandos que llaman a `nucleo` y eventos de progreso.
//! Aquí no se procesa ningún documento: todo está en [`servicio`].

pub mod comandos;
pub mod servicio;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter as _, Manager};

use nucleo::almacen::archivo;
use nucleo::almacen::sesion::default_keys;
use nucleo::motor::{Engine, EngineOptions};
use nucleo::operaciones::model_id;
use nucleo::recursos::{DEFAULT_MODEL, Resources};
use servicio::dto::Event;
use servicio::{Config, Emitter, Service, SharedDetector};

/// Nombre del evento por el que llegan todos los avisos a la interfaz.
pub const EVENT: &str = "anonimizador";

/// Variable con otra ruta para el almacén. Solo la aceptan las compilaciones
/// de depuración, como la clave de prueba: así se prueba la app sin tocar el
/// almacén del usuario.
pub const STORE_PATH_ENV: &str = "ANONIMIZADOR_ALMACEN";

/// Cada cuánto se mira si ha cambiado el día para purgar los caducados.
const PURGE_CHECK: Duration = Duration::from_secs(60 * 60);

struct TauriEmitter(tauri::AppHandle);

impl Emitter for TauriEmitter {
    fn emit(&self, event: Event) {
        if let Err(error) = self.0.emit(EVENT, event) {
            tracing::warn!(%error, "no se ha podido avisar a la interfaz");
        }
    }
}

/// Los recursos del paquete instalado, o si no, los de la carpeta
/// `recursos/` del proyecto (al desarrollar).
fn resources(app: &tauri::AppHandle) -> Result<Resources, String> {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("recursos"));
    match bundled.filter(|dir| dir.is_dir()) {
        Some(dir) => Ok(Resources::new(dir)),
        None => Resources::locate().map_err(|e| e.to_string()),
    }
}

fn load_engine(resources: Result<Resources, String>) -> Result<SharedDetector, String> {
    let resources = resources?;
    let engine = Engine::load(
        &resources,
        &resources.model(DEFAULT_MODEL),
        EngineOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(Arc::new(engine))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Debe ser el primer plugin. Una segunda instancia no abre otra
        // ventana: enfoca la existente. Así nunca hay dos procesos
        // escribiendo el almacén a la vez.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let service = Arc::new(Service::new(Config {
                store_path: store_path(),
                settings_path: app
                    .path()
                    .app_config_dir()
                    .ok()
                    .map(|dir| dir.join("ajustes.json")),
                keys: Box::new(default_keys),
                model_id: model_id(DEFAULT_MODEL),
                emitter: Arc::new(TauriEmitter(handle.clone())),
            }));
            app.manage(service.clone());

            // El modelo se carga en segundo plano: la ventana se usa mientras.
            // PDFium y el motor leen los mismos recursos.
            let resources = resources(&handle);
            if let Ok(resources) = &resources {
                nucleo::formatos::pdf::use_resources(resources.clone());
            }
            let loading = service.clone();
            std::thread::spawn(move || loading.set_model(load_engine(resources)));

            // Purga de caducados una vez al día si la app sigue abierta.
            let purging = service;
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(PURGE_CHECK);
                    purging.purge_if_new_day(jiff::Zoned::now().date());
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            comandos::status,
            comandos::inspect,
            comandos::anonymize,
            comandos::deanonymize,
            comandos::deanonymize_text,
            comandos::store_view,
            comandos::store_export,
            comandos::store_import,
            comandos::store_forget,
            comandos::store_clear,
            comandos::store_start_over,
            comandos::pick_files,
            comandos::choose_folder,
            comandos::open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("no se ha podido iniciar la aplicación");
}

/// El almacén de la carpeta de datos del usuario, u otro en depuración.
fn store_path() -> Option<std::path::PathBuf> {
    if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os(STORE_PATH_ENV).filter(|p| !p.is_empty())
    {
        return Some(path.into());
    }
    archivo::default_path()
}
