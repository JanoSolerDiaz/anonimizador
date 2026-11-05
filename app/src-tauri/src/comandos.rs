//! Comandos de Tauri: lo único que puede pedir la interfaz. Cada uno llama al
//! servicio; el trabajo largo va en otro hilo para no bloquear la ventana.
//! Los diálogos de archivos se abren desde aquí, así que la interfaz no
//! tiene permisos del plugin de diálogo.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::servicio::Service;
use crate::servicio::dto::{
    AnonymizeJob, DeanonymizeJob, Exported, FileInfo, Source, Status, StoreView, TextResult,
};

type Answer<T> = Result<T, String>;
type Shared<'a> = State<'a, Arc<Service>>;

/// Ejecuta `work` en un hilo aparte y espera su resultado sin bloquear.
async fn in_background<T: Send + 'static>(
    work: impl FnOnce() -> Answer<T> + Send + 'static,
) -> Answer<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "La operación se ha interrumpido.".to_owned())?
}

#[tauri::command]
pub fn status(service: Shared) -> Status {
    service.status()
}

#[tauri::command]
pub async fn inspect(service: Shared<'_>, paths: Vec<String>) -> Answer<Vec<FileInfo>> {
    let service = service.inner().clone();
    in_background(move || Ok(service.inspect(&paths))).await
}

/// Termina cuando acaba el lote; el progreso llega por eventos.
#[tauri::command]
pub async fn anonymize(service: Shared<'_>, job: AnonymizeJob) -> Answer<()> {
    let service = service.inner().clone();
    in_background(move || service.anonymize(&job)).await
}

#[tauri::command]
pub async fn deanonymize(service: Shared<'_>, job: DeanonymizeJob) -> Answer<()> {
    let service = service.inner().clone();
    in_background(move || service.deanonymize(&job)).await
}

#[tauri::command]
pub async fn deanonymize_text(
    service: Shared<'_>,
    text: String,
    source: Option<Source>,
) -> Answer<TextResult> {
    let service = service.inner().clone();
    in_background(move || service.deanonymize_text(&text, source.as_ref())).await
}

#[tauri::command]
pub fn store_view(service: Shared) -> Answer<StoreView> {
    service.store_view()
}

/// Pide la carpeta y exporta. `None` si el usuario cancela.
#[tauri::command]
pub async fn store_export(
    app: AppHandle,
    service: Shared<'_>,
    id: String,
) -> Answer<Option<Exported>> {
    let service = service.inner().clone();
    in_background(move || {
        let start = service.status().settings.last_dir;
        match pick_folder(&app, start) {
            Some(dir) => service.export(&id, &dir).map(Some),
            None => Ok(None),
        }
    })
    .await
}

/// Pide el diccionario y lo importa. `None` si el usuario cancela.
#[tauri::command]
pub async fn store_import(app: AppHandle, service: Shared<'_>) -> Answer<Option<String>> {
    let service = service.inner().clone();
    in_background(move || {
        let file = app
            .dialog()
            .file()
            .set_title("Importar diccionario")
            .add_filter("Diccionario", &["json"])
            .blocking_pick_file()
            .and_then(|f| f.into_path().ok());
        match file {
            Some(file) => service.import(&file.display().to_string()).map(Some),
            None => Ok(None),
        }
    })
    .await
}

#[tauri::command]
pub fn store_forget(service: Shared, id: String) -> Answer<()> {
    service.forget(&id)
}

#[tauri::command]
pub fn store_clear(service: Shared) -> Answer<()> {
    service.clear()
}

#[tauri::command]
pub fn store_start_over(service: Shared) -> Answer<Option<String>> {
    service.start_over()
}

/// Archivos elegidos con el diálogo del sistema; vacío si cancela.
#[tauri::command]
pub async fn pick_files(app: AppHandle, deanonymizing: bool) -> Answer<Vec<String>> {
    in_background(move || {
        let extensions: &[&str] = if deanonymizing {
            &["docx", "pptx", "xlsx", "md", "markdown", "txt"]
        } else {
            &["pdf", "docx", "pptx", "xlsx", "md", "markdown", "txt"]
        };
        Ok(app
            .dialog()
            .file()
            .set_title("Elegir documentos")
            .add_filter("Documentos", extensions)
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|f| f.into_path().ok())
            .map(|p| p.display().to_string())
            .collect())
    })
    .await
}

/// Carpeta elegida; `None` si cancela.
#[tauri::command]
pub async fn choose_folder(app: AppHandle, start: Option<String>) -> Answer<Option<String>> {
    in_background(move || Ok(pick_folder(&app, start))).await
}

fn pick_folder(app: &AppHandle, start: Option<String>) -> Option<String> {
    let mut dialog = app.dialog().file().set_title("Elegir carpeta de salida");
    if let Some(start) = start.map(PathBuf::from).filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(start);
    }
    dialog
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Abre la carpeta en el explorador del sistema.
#[tauri::command]
pub fn open_folder(path: String) -> Answer<()> {
    let path = PathBuf::from(path);
    let folder = if path.is_dir() {
        path
    } else {
        path.parent()
            .filter(|p| p.is_dir())
            .map(PathBuf::from)
            .ok_or("La carpeta ya no existe.")?
    };
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&folder)
        .spawn()
        .map(|_| ())
        .map_err(|_| "No se ha podido abrir la carpeta.".to_owned())
}
