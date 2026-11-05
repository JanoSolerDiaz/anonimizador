//! El trabajo de la app, sin Tauri: lo que hace cada botón de la interfaz.
//! Los comandos de Tauri solo llaman a esto, así que se prueba con
//! `cargo test` y un detector falso.
//!
//! El almacén se abre al crear el servicio y se guarda entero al terminar
//! cada lote (PRD, «Archivo del almacén»). El modelo llega después, cuando
//! termina de cargarse en segundo plano.

pub mod dto;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use jiff::Zoned;
use jiff::civil::Date;

use nucleo::almacen::sesion::{EXPORT_WARNING, Keys, OpenStore};
use nucleo::formatos::pdf::{DEFAULT_RESOLUTION, MAX_RESOLUTION, MIN_RESOLUTION};
use nucleo::motor::Entity;
use nucleo::motor::opcionales::SkippedTypes;
use nucleo::motor::tipos::TypeTable;
use nucleo::operaciones::anonimizar::{AnonymizeOptions, Stage, finish, prepare};
use nucleo::operaciones::desanonimizar::{Outcome, deanonymize, deanonymize_text};
use nucleo::operaciones::{Detector, inspect};

use dto::{
    AnonymizeJob, ConflictView, DeanonymizeJob, DictionaryView, DocumentView, Event, Exported,
    FileInfo, JobFile, ModelStatus, Phase, Settings, Source, Status, StoreStatus, StoreView,
    TextResult, WarningsView,
};

/// Archivos que se procesan a la vez (PRD, «Rendimiento»).
pub const WORKERS: usize = 2;
/// Como mucho un evento de progreso del análisis cada tanto por archivo.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Lo que analiza los documentos: el motor, o uno falso en las pruebas.
pub type SharedDetector = Arc<dyn Detector + Send + Sync>;

/// Por dónde salen los eventos hacia la interfaz.
pub trait Emitter: Send + Sync {
    fn emit(&self, event: Event);
}

/// De dónde sale la clave del almacén cada vez que se abre.
pub type KeySource = Box<dyn Fn() -> nucleo::Result<Keys> + Send + Sync>;

/// Dónde vive cada cosa y cómo se avisa a la interfaz.
pub struct Config {
    /// Archivo del almacén; `None` si el sistema no tiene carpeta de datos.
    pub store_path: Option<PathBuf>,
    /// Archivo de ajustes; `None` para no guardarlos.
    pub settings_path: Option<PathBuf>,
    pub keys: KeySource,
    pub model_id: String,
    pub emitter: Arc<dyn Emitter>,
}

enum ModelState {
    Loading,
    Ready(SharedDetector),
    Failed(String),
}

enum StoreState {
    Open(Box<OpenStore>),
    Failed(String),
}

pub struct Service {
    model: RwLock<ModelState>,
    model_id: String,
    store: Mutex<StoreState>,
    store_path: Option<PathBuf>,
    keys: KeySource,
    settings: Mutex<Settings>,
    settings_path: Option<PathBuf>,
    emitter: Arc<dyn Emitter>,
    busy: AtomicBool,
    /// Último día en que se purgaron los caducados.
    purged_on: Mutex<Date>,
}

type Answer<T> = Result<T, String>;

fn message(error: nucleo::Error) -> String {
    error.to_string()
}

impl Service {
    /// Abre el almacén y lee los ajustes. Si el almacén no se abre, el
    /// servicio arranca igual y lo dice en [`Service::status`].
    pub fn new(config: Config) -> Service {
        let today = Zoned::now().date();
        let store = match &config.store_path {
            None => StoreState::Failed(
                "No se encuentra la carpeta de datos del usuario para guardar el almacén.".into(),
            ),
            Some(path) => {
                match (config.keys)().and_then(|keys| OpenStore::open_with(path, keys, today)) {
                    Ok(open) => {
                        if let Err(error) = open.save_if_purged() {
                            tracing::warn!(%error, "no se ha podido guardar la purga al abrir");
                        }
                        StoreState::Open(Box::new(open))
                    }
                    Err(error) => StoreState::Failed(message(error)),
                }
            }
        };
        let settings = config
            .settings_path
            .as_deref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Service {
            model: RwLock::new(ModelState::Loading),
            model_id: config.model_id,
            store: Mutex::new(store),
            store_path: config.store_path,
            keys: config.keys,
            settings: Mutex::new(settings),
            settings_path: config.settings_path,
            emitter: config.emitter,
            busy: AtomicBool::new(false),
            purged_on: Mutex::new(today),
        }
    }

    /// El modelo ya cargado, o el error de la carga.
    pub fn set_model(&self, model: Result<SharedDetector, String>) {
        let (state, status) = match model {
            Ok(detector) => (ModelState::Ready(detector), ModelStatus::Ready),
            Err(message) => (
                ModelState::Failed(message.clone()),
                ModelStatus::Failed { message },
            ),
        };
        *self.model.write().unwrap_or_else(|e| e.into_inner()) = state;
        self.emitter.emit(Event::Model { status });
    }

    pub fn status(&self) -> Status {
        let model = match &*self.model.read().unwrap_or_else(|e| e.into_inner()) {
            ModelState::Loading => ModelStatus::Loading,
            ModelState::Ready(_) => ModelStatus::Ready,
            ModelState::Failed(message) => ModelStatus::Failed {
                message: message.clone(),
            },
        };
        let store = match &*self.lock_store() {
            StoreState::Open(_) => StoreStatus::Ready,
            StoreState::Failed(message) => StoreStatus::Failed {
                message: message.clone(),
            },
        };
        Status {
            model,
            store,
            settings: self
                .settings
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            optional_types: TypeTable::builtin()
                .optional_markers()
                .map(str::to_owned)
                .collect(),
            busy: self.busy.load(Ordering::SeqCst),
        }
    }

    /// Tipo y salidas de cada archivo. Solo se miran archivos regulares.
    pub fn inspect(&self, paths: &[String]) -> Vec<FileInfo> {
        paths
            .iter()
            .map(|path| {
                let name = file_name(Path::new(path));
                match readable_file(path) {
                    Ok(file) => FileInfo::new(path.clone(), name, inspect(&file)),
                    Err(reason) => FileInfo {
                        path: path.clone(),
                        name,
                        kind: "No admitido".to_owned(),
                        format: None,
                        outputs: Vec::new(),
                        deanonymizable: false,
                        reason: Some(reason),
                    },
                }
            })
            .collect()
    }

    /// Anonimiza un lote. Comprueba lo que puede fallar del todo (modelo,
    /// almacén, carpeta, opciones) y después procesa cada archivo, de dos en
    /// dos, avisando de su progreso. Un archivo que falla no para el resto.
    /// Bloquea hasta terminar: la capa de Tauri lo llama en otro hilo.
    pub fn anonymize(&self, job: &AnonymizeJob) -> Answer<()> {
        let detector = self.detector()?;
        let dir = existing_dir(&job.dir)?;
        let skipped = SkippedTypes::new(&job.skipped).map_err(message)?;
        let resolution = job.resolution.unwrap_or(DEFAULT_RESOLUTION);
        if !(MIN_RESOLUTION..=MAX_RESOLUTION).contains(&resolution) {
            return Err(message(nucleo::Error::InvalidResolution(resolution)));
        }
        self.ensure_store()?;
        let _busy = self.start_batch()?;
        self.remember(|settings| {
            settings.last_dir = Some(job.dir.clone());
            settings.skipped = job.skipped.clone();
        });

        let filtered = Filtered {
            inner: detector,
            skipped,
        };
        for file in &job.files {
            self.progress(file, Phase::Queued);
        }
        let queue = Mutex::new(job.files.iter().collect::<VecDeque<_>>());
        let failed = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..WORKERS.min(job.files.len()) {
                scope.spawn(|| {
                    loop {
                        let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
                        let Some(file) = next else { break };
                        let phase = self.anonymize_one(file, &dir, &filtered, job, resolution);
                        if matches!(phase, Phase::Failed { .. }) {
                            failed.fetch_add(1, Ordering::SeqCst);
                        }
                        self.progress(file, phase);
                    }
                });
            }
        });
        let failed = failed.into_inner();
        self.finish_batch(job.files.len() - failed, failed);
        Ok(())
    }

    fn anonymize_one(
        &self,
        file: &JobFile,
        dir: &Path,
        detector: &Filtered,
        job: &AnonymizeJob,
        resolution: u32,
    ) -> Phase {
        let failed = |message: String| Phase::Failed { message };
        let path = match readable_file(&file.path) {
            Ok(path) => path,
            Err(reason) => return failed(reason),
        };
        let format = match file.format.as_deref().map(dto::output_format) {
            Some(Some(format)) => format,
            _ => return failed("Falta el formato de salida de este archivo.".into()),
        };
        let options = AnonymizeOptions {
            format,
            model: self.model_id.clone(),
            now: Zoned::now(),
            spreadsheet_amounts: job.spreadsheet_amounts,
            pdf_resolution: resolution,
        };
        let throttle = Throttle::new();
        let report = |stage: Stage| {
            let phase = match stage {
                Stage::Reading => Phase::Reading,
                Stage::Analyzing(fraction) => {
                    if !throttle.ready(fraction >= 1.0) {
                        return;
                    }
                    Phase::Analyzing { fraction }
                }
                Stage::Writing => Phase::Writing,
                Stage::Verifying => Phase::Verifying,
            };
            self.progress(file, phase);
        };
        // Preparar no usa el almacén: dos archivos se preparan a la vez.
        let prepared = match prepare(&path, detector, &options, &report) {
            Ok(prepared) => prepared,
            Err(error) => return failed(message(error)),
        };
        let mut store = self.lock_store();
        let StoreState::Open(open) = &mut *store else {
            return failed("El almacén no está abierto.".into());
        };
        match finish(prepared, dir, &mut open.store, &options, &report) {
            Ok(summary) => Phase::Anonymized {
                output: summary.output.display().to_string(),
                entities: summary.entities,
                warnings: WarningsView {
                    images: summary.warnings.images,
                    objects: summary.warnings.objects,
                    attachments: summary.warnings.attachments,
                    links: summary.warnings.links,
                },
            },
            Err(error) => failed(message(error)),
        }
    }

    /// Desanonimiza un lote. Un archivo con marcadores de valores distintos
    /// en dos fuentes queda en conflicto hasta que se elija una.
    pub fn deanonymize(&self, job: &DeanonymizeJob) -> Answer<()> {
        let dir = existing_dir(&job.dir)?;
        self.ensure_store()?;
        let _busy = self.start_batch()?;
        self.remember(|settings| settings.last_dir = Some(job.dir.clone()));
        for file in &job.files {
            self.progress(file, Phase::Queued);
        }
        let (mut done, mut failed) = (0, 0);
        for file in &job.files {
            self.progress(file, Phase::Reading);
            let phase = match readable_file(&file.path) {
                Err(message) => Phase::Failed { message },
                Ok(path) => {
                    let store = self.lock_store();
                    let StoreState::Open(open) = &*store else {
                        return Err("El almacén no está abierto.".into());
                    };
                    let source = file.source.as_ref().map(Into::into);
                    match deanonymize(&path, &dir, &open.store, source.as_ref()) {
                        Ok(Outcome::Done((output, report))) => Phase::Deanonymized {
                            output: output.display().to_string(),
                            restored: report.restored,
                            missing: report.missing,
                        },
                        Ok(Outcome::Conflicts(conflicts)) => Phase::Conflict {
                            conflicts: conflict_views(&conflicts),
                        },
                        Err(error) => Phase::Failed {
                            message: message(error),
                        },
                    }
                }
            };
            match phase {
                Phase::Failed { .. } => failed += 1,
                Phase::Deanonymized { .. } => done += 1,
                _ => {}
            }
            self.progress(file, phase);
        }
        // Desanonimizar no cambia el almacén: no hace falta guardarlo.
        self.emitter.emit(Event::BatchDone {
            done,
            failed,
            store_error: None,
        });
        Ok(())
    }

    /// Texto pegado, con sus marcadores restaurados.
    pub fn deanonymize_text(&self, text: &str, source: Option<&Source>) -> Answer<TextResult> {
        let store = self.lock_store();
        let StoreState::Open(open) = &*store else {
            return Err("El almacén no está abierto.".into());
        };
        let source = source.map(Into::into);
        match deanonymize_text(text, &open.store, source.as_ref()).map_err(message)? {
            Outcome::Done((text, report)) => Ok(TextResult::Done {
                text,
                restored: report.restored,
                missing: report.missing,
            }),
            Outcome::Conflicts(conflicts) => Ok(TextResult::Conflict {
                conflicts: conflict_views(&conflicts),
            }),
        }
    }

    /// Documentos y diccionarios del almacén, sin sus valores.
    pub fn store_view(&self) -> Answer<StoreView> {
        let store = self.lock_store();
        let StoreState::Open(open) = &*store else {
            return Err("El almacén no está abierto.".into());
        };
        Ok(StoreView {
            documents: open
                .store
                .documents()
                .iter()
                .map(|d| DocumentView {
                    id: d.id.clone(),
                    name: d.origin.name.clone(),
                    output: d.output.name.clone(),
                    created: d.created.clone(),
                    expires: d.expires.to_string(),
                    entries: d.markers.len(),
                })
                .collect(),
            dictionaries: open
                .store
                .imported()
                .iter()
                .map(|d| DictionaryView {
                    name: d.name().to_owned(),
                    document: d.document().output.name.clone(),
                    entries: d.entries().len(),
                })
                .collect(),
        })
    }

    pub fn export(&self, id: &str, dir: &str) -> Answer<Exported> {
        let dir = existing_dir(dir)?;
        let store = self.lock_store();
        let StoreState::Open(open) = &*store else {
            return Err("El almacén no está abierto.".into());
        };
        let path = open.export(id, &dir).map_err(message)?;
        Ok(Exported {
            path: path.display().to_string(),
            warning: EXPORT_WARNING.to_owned(),
        })
    }

    pub fn import(&self, path: &str) -> Answer<String> {
        let file = readable_file(path)?;
        self.change_store(|open| open.import(&file).map_err(message))
    }

    pub fn forget(&self, id: &str) -> Answer<()> {
        self.change_store(|open| {
            if !open.store.documents().iter().any(|d| d.id == id) {
                return Err(message(nucleo::Error::DocumentNotFound));
            }
            open.store.forget(id);
            Ok(())
        })
    }

    pub fn clear(&self) -> Answer<()> {
        self.change_store(|open| {
            open.store.clear();
            Ok(())
        })
    }

    /// Aparta el almacén que no se pudo abrir y empieza uno vacío. Devuelve
    /// dónde quedó el antiguo.
    pub fn start_over(&self) -> Answer<Option<String>> {
        let path = self
            .store_path
            .clone()
            .ok_or("No se encuentra la carpeta de datos del usuario.")?;
        let mut store = self.lock_store();
        if matches!(*store, StoreState::Open(_)) {
            return Err("El almacén ya está abierto.".into());
        }
        let keys = (self.keys)().map_err(message)?;
        let (open, set_aside) =
            OpenStore::start_over(&path, keys, Zoned::now().date()).map_err(message)?;
        *store = StoreState::Open(Box::new(open));
        Ok(set_aside.map(|p| p.display().to_string()))
    }

    /// Borra los caducados si el día ha cambiado desde la última vez, y
    /// guarda. La capa de Tauri lo llama cada hora.
    pub fn purge_if_new_day(&self, today: Date) {
        let mut purged_on = self.purged_on.lock().unwrap_or_else(|e| e.into_inner());
        if *purged_on == today {
            return;
        }
        *purged_on = today;
        drop(purged_on);
        let mut store = self.lock_store();
        if let StoreState::Open(open) = &mut *store
            && open.purge(today) > 0
        {
            if let Err(error) = open.save() {
                tracing::warn!(%error, "no se ha podido guardar la purga diaria");
            }
            drop(store);
            self.emitter.emit(Event::StoreChanged);
        }
    }

    fn change_store<T>(&self, change: impl FnOnce(&mut OpenStore) -> Answer<T>) -> Answer<T> {
        if self.busy.load(Ordering::SeqCst) {
            return Err("Espere a que termine el lote en curso.".into());
        }
        let mut store = self.lock_store();
        let StoreState::Open(open) = &mut *store else {
            return Err("El almacén no está abierto.".into());
        };
        let value = change(open)?;
        open.save().map_err(message)?;
        Ok(value)
    }

    fn detector(&self) -> Answer<SharedDetector> {
        match &*self.model.read().unwrap_or_else(|e| e.into_inner()) {
            ModelState::Ready(detector) => Ok(detector.clone()),
            ModelState::Loading => Err("El modelo aún se está cargando.".into()),
            ModelState::Failed(message) => Err(message.clone()),
        }
    }

    fn ensure_store(&self) -> Answer<()> {
        match &*self.lock_store() {
            StoreState::Open(_) => Ok(()),
            StoreState::Failed(message) => Err(message.clone()),
        }
    }

    fn lock_store(&self) -> MutexGuard<'_, StoreState> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn start_batch(&self) -> Answer<BusyGuard<'_>> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("Ya hay un lote en marcha.".into());
        }
        Ok(BusyGuard(&self.busy))
    }

    fn finish_batch(&self, done: usize, failed: usize) {
        let store_error = match &*self.lock_store() {
            StoreState::Open(open) => open.save().err().map(message),
            StoreState::Failed(message) => Some(message.clone()),
        };
        self.emitter.emit(Event::BatchDone {
            done,
            failed,
            store_error,
        });
    }

    fn remember(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut settings);
        if let Some(path) = &self.settings_path {
            let saved = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| {
                    std::fs::write(
                        path,
                        serde_json::to_vec_pretty(&*settings).unwrap_or_default(),
                    )
                });
            if let Err(error) = saved {
                tracing::warn!(%error, "no se han podido guardar los ajustes");
            }
        }
    }

    fn progress(&self, file: &JobFile, phase: Phase) {
        self.emitter.emit(Event::Progress {
            file: file.id.clone(),
            phase,
        });
    }
}

/// Marca el servicio como ocupado mientras vive.
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// El detector con los tipos que el usuario no quiere sustituir quitados.
struct Filtered {
    inner: SharedDetector,
    skipped: SkippedTypes,
}

impl Detector for Filtered {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        let mut entities = self.inner.detect(segments)?;
        self.skipped.retain(&mut entities);
        Ok(entities)
    }

    fn detect_with_progress(
        &self,
        segments: &[&str],
        progress: &dyn Fn(f32),
    ) -> nucleo::Result<Vec<Entity>> {
        let mut entities = self.inner.detect_with_progress(segments, progress)?;
        self.skipped.retain(&mut entities);
        Ok(entities)
    }
}

/// Deja pasar un evento cada [`PROGRESS_INTERVAL`], y siempre el último.
struct Throttle(Mutex<Option<Instant>>);

impl Throttle {
    fn new() -> Throttle {
        Throttle(Mutex::new(None))
    }

    fn ready(&self, last: bool) -> bool {
        let mut previous = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if last || previous.is_none_or(|p| now.duration_since(p) >= PROGRESS_INTERVAL) {
            *previous = Some(now);
            true
        } else {
            false
        }
    }
}

fn conflict_views(conflicts: &[nucleo::operaciones::desanonimizar::Conflict]) -> Vec<ConflictView> {
    conflicts
        .iter()
        .map(|c| ConflictView {
            marker: c.marker.clone(),
            sources: c.sources.iter().map(Into::into).collect(),
        })
        .collect()
}

/// La ruta, si es un archivo regular que se puede leer (PRD, «Seguridad y
/// privacidad»: Rust valida las rutas que manda la interfaz).
fn readable_file(path: &str) -> Answer<PathBuf> {
    let path = PathBuf::from(path);
    let regular = std::fs::metadata(&path).is_ok_and(|m| m.is_file());
    if regular && std::fs::File::open(&path).is_ok() {
        Ok(path)
    } else {
        Err("No se encuentra el archivo o no se puede leer.".into())
    }
}

fn existing_dir(dir: &str) -> Answer<PathBuf> {
    let path = PathBuf::from(dir);
    if path.is_dir() {
        Ok(path)
    } else {
        Err("La carpeta de salida no existe.".into())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
