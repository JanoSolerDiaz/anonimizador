//! Capa de Tauri: comandos que llaman a `nucleo` y eventos de progreso.
//! Aquí no se procesa ningún documento.

use tauri::Manager;

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
        .run(tauri::generate_context!())
        .expect("no se ha podido iniciar la aplicación");
}
