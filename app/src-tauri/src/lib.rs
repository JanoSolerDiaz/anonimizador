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

/// Carga el modelo: primero desde los recursos del paquete instalado, si no
/// desde la carpeta `recursos/` del proyecto.
fn load_engine(app: &tauri::AppHandle) -> Result<SharedDetector, String> {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("recursos"));
    let resources = match bundled.filter(|dir| dir.is_dir()) {
        Some(dir) => Resources::new(dir),
        None => Resources::locate().map_err(|e| e.to_string())?,
    };
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
                store_path: archivo::default_path(),
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
            let loading = service.clone();
            std::thread::spawn(move || loading.set_model(load_engine(&handle)));

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
