//! El servicio de la app sin Tauri: lotes, progreso, conflictos, almacén y
//! ajustes, con un detector falso y la clave del almacén en memoria.

use std::path::Path;
use std::sync::{Arc, Mutex};

use jiff::civil::date;

use anonimizador_lib::servicio::dto::{
    AnonymizeJob, DeanonymizeJob, Event, JobFile, ModelStatus, Phase, Source, StoreStatus,
    TextResult,
};
use anonimizador_lib::servicio::{Config, Emitter, Service, SharedDetector};
use nucleo::almacen::archivo::{KeyProvider, MemoryKey, StoreKey};
use nucleo::almacen::sesion::{Keys, OpenStore};
use nucleo::motor::Entity;
use nucleo::operaciones::Detector;

/// Encuentra unos valores fijos y avisa del progreso muchas veces seguidas.
struct Fake(Vec<(&'static str, &'static str)>);

impl Detector for Fake {
    fn detect(&self, segments: &[&str]) -> nucleo::Result<Vec<Entity>> {
        let mut entities = Vec::new();
        for (segment, text) in segments.iter().enumerate() {
            for (value, kind) in &self.0 {
                for (byte, _) in text.match_indices(value) {
                    let start = text[..byte].chars().count();
                    entities.push(Entity {
                        segment,
                        start,
                        end: start + value.chars().count(),
                        kind: (*kind).to_owned(),
                        label: format!("ETIQUETA_{kind}"),
                        text: (*value).to_owned(),
                        score: 0.99,
                    });
                }
            }
        }
        Ok(entities)
    }

    fn detect_with_progress(
        &self,
        segments: &[&str],
        progress: &dyn Fn(f32),
    ) -> nucleo::Result<Vec<Entity>> {
        for step in 0..1000 {
            progress(step as f32 / 1000.0);
        }
        let entities = self.detect(segments)?;
        progress(1.0);
        Ok(entities)
    }
}

fn detector() -> SharedDetector {
    Arc::new(Fake(vec![
        ("Marta Lozano", "NOMBRE_PERSONA"),
        ("Zaragoza", "LUGAR"),
    ]))
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Emitter for Recorder {
    fn emit(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    fn take(&self) -> Vec<Event> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// La misma clave para cada apertura.
#[derive(Clone)]
struct SharedKey(Arc<MemoryKey>);

impl KeyProvider for SharedKey {
    fn load(&self) -> nucleo::Result<Option<StoreKey>> {
        self.0.load()
    }

    fn save(&self, key: &StoreKey) -> nucleo::Result<()> {
        self.0.save(key)
    }
}

struct World {
    dir: tempfile::TempDir,
    key: SharedKey,
    events: Arc<Recorder>,
    service: Service,
}

impl World {
    fn new() -> World {
        let dir = tempfile::tempdir().unwrap();
        let key = SharedKey(Arc::new(MemoryKey::new()));
        World::with(dir, key)
    }

    fn with(dir: tempfile::TempDir, key: SharedKey) -> World {
        let events = Arc::new(Recorder::default());
        let for_service = key.clone();
        let service = Service::new(Config {
            store_path: Some(dir.path().join("datos").join("almacen.anon")),
            settings_path: Some(dir.path().join("config").join("ajustes.json")),
            keys: Box::new(move || Ok(Box::new(for_service.clone()) as Keys)),
            model_id: "modelo-de-prueba".to_owned(),
            emitter: events.clone(),
        });
        World {
            dir,
            key,
            events,
            service,
        }
    }

    fn ready(self) -> World {
        self.service.set_model(Ok(detector()));
        self.events.take();
        self
    }

    fn file(&self, name: &str, text: &str) -> String {
        let path = self.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path.display().to_string()
    }

    fn out(&self) -> String {
        let out = self.dir.path().join("salida");
        std::fs::create_dir_all(&out).unwrap();
        out.display().to_string()
    }

    fn reopen_store(&self) -> OpenStore {
        OpenStore::open_with(
            &self.dir.path().join("datos").join("almacen.anon"),
            Box::new(self.key.clone()),
            date(2026, 11, 5),
        )
        .unwrap()
    }
}

fn job_file(id: &str, path: &str, format: Option<&str>) -> JobFile {
    JobFile {
        id: id.to_owned(),
        path: path.to_owned(),
        format: format.map(str::to_owned),
        source: None,
    }
}

/// Fases de un archivo, en orden.
fn phases<'a>(events: &'a [Event], id: &str) -> Vec<&'a Phase> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Progress { file, phase } if file == id => Some(phase),
            _ => None,
        })
        .collect()
}

#[test]
fn at_start_the_model_is_loading_and_the_store_is_open() {
    let world = World::new();
    let status = world.service.status();
    assert_eq!(status.model, ModelStatus::Loading);
    assert_eq!(status.store, StoreStatus::Ready);
    assert!(!status.busy);
    let mut optional = status.optional_types.clone();
    optional.sort();
    assert_eq!(optional, ["CARGO", "COORDENADAS", "IMPORTE", "LUGAR"]);

    let job = AnonymizeJob {
        files: vec![],
        dir: world.out(),
        skipped: vec![],
        spreadsheet_amounts: false,
        resolution: None,
    };
    assert_eq!(
        world.service.anonymize(&job).unwrap_err(),
        "El modelo aún se está cargando."
    );

    world.service.set_model(Ok(detector()));
    assert_eq!(
        world.events.take(),
        [Event::Model {
            status: ModelStatus::Ready
        }]
    );
    assert_eq!(world.service.status().model, ModelStatus::Ready);
}

#[test]
fn a_model_that_fails_to_load_explains_why() {
    let world = World::new();
    world.service.set_model(Err(
        "No se ha podido cargar el modelo de anonimización: falta".into(),
    ));
    assert!(matches!(
        world.service.status().model,
        ModelStatus::Failed { .. }
    ));
}

#[test]
fn a_batch_reports_each_file_and_saves_the_store_and_settings() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Firma: Marta Lozano, de Zaragoza.\n");
    let md = world.file("acta.md", "# Acta\n\nAsiste Marta Lozano.\n");
    let missing = world.dir.path().join("no-existe.txt").display().to_string();
    let job = AnonymizeJob {
        files: vec![
            job_file("1", &text, Some("txt")),
            job_file("2", &md, Some("md")),
            job_file("3", &missing, Some("txt")),
        ],
        dir: world.out(),
        skipped: vec!["LUGAR".to_owned()],
        spreadsheet_amounts: false,
        resolution: None,
    };
    world.service.anonymize(&job).unwrap();
    let events = world.events.take();

    let first = phases(&events, "1");
    assert_eq!(first[0], &Phase::Queued);
    assert_eq!(first[1], &Phase::Reading);
    let Phase::Anonymized {
        output, entities, ..
    } = first.last().unwrap()
    else {
        panic!("{first:?}");
    };
    assert_eq!(entities["NOMBRE_PERSONA"], 1);
    assert!(!entities.contains_key("LUGAR"), "LUGAR no se sustituye");
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "Firma: [NOMBRE_PERSONA_0001], de Zaragoza.\n"
    );
    assert!(matches!(
        phases(&events, "2").last().unwrap(),
        Phase::Anonymized { .. }
    ));
    assert_eq!(
        phases(&events, "3").last().unwrap(),
        &&Phase::Failed {
            message: "No se encuentra el archivo o no se puede leer.".into()
        }
    );
    assert_eq!(
        events.last().unwrap(),
        &Event::BatchDone {
            done: 2,
            failed: 1,
            store_error: None
        }
    );

    // Como mucho un evento de análisis cada 100 ms, y siempre el último.
    let analyzing: Vec<f32> = first
        .iter()
        .filter_map(|p| match p {
            Phase::Analyzing { fraction } => Some(*fraction),
            _ => None,
        })
        .collect();
    assert!(analyzing.len() < 10, "{} eventos", analyzing.len());
    assert_eq!(analyzing.last(), Some(&1.0));

    assert_eq!(world.reopen_store().store.documents().len(), 2);
    let status = world.service.status();
    assert_eq!(
        status.settings.last_dir.as_deref(),
        Some(world.out().as_str())
    );
    assert_eq!(status.settings.skipped, ["LUGAR"]);
    assert!(!status.busy);

    // Los ajustes se recuerdan en la siguiente sesión.
    let World { dir, key, .. } = world;
    let again = World::with(dir, key);
    assert_eq!(again.service.status().settings.skipped, ["LUGAR"]);
}

#[test]
fn bad_batch_options_are_rejected_before_anything_runs() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Marta Lozano\n");
    let mut job = AnonymizeJob {
        files: vec![job_file("1", &text, Some("txt"))],
        dir: world.dir.path().join("no-hay").display().to_string(),
        skipped: vec![],
        spreadsheet_amounts: false,
        resolution: None,
    };
    assert_eq!(
        world.service.anonymize(&job).unwrap_err(),
        "La carpeta de salida no existe."
    );
    job.dir = world.out();
    job.skipped = vec!["NOMBRE_PERSONA".to_owned()];
    assert!(world.service.anonymize(&job).is_err());
    job.skipped = vec![];
    job.resolution = Some(400);
    assert!(
        world
            .service
            .anonymize(&job)
            .unwrap_err()
            .contains("entre 150 y 300 ppp")
    );
    assert!(world.events.take().is_empty());
}

#[test]
fn deanonymizing_files_and_text_restores_the_values() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Firma: Marta Lozano.\n");
    world
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &text, Some("txt"))],
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    let anonymized = Path::new(&world.out()).join("nota_anonimizado.txt");
    world.events.take();

    world
        .service
        .deanonymize(&DeanonymizeJob {
            files: vec![job_file("a", &anonymized.display().to_string(), None)],
            dir: world.out(),
        })
        .unwrap();
    let events = world.events.take();
    let Phase::Deanonymized {
        output,
        restored,
        missing,
    } = phases(&events, "a").last().unwrap()
    else {
        panic!("{events:?}");
    };
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "Firma: Marta Lozano.\n"
    );
    assert_eq!(restored, &["[NOMBRE_PERSONA_0001]"]);
    assert!(missing.is_empty());

    assert_eq!(
        world
            .service
            .deanonymize_text("Hola, [nombre_persona_1] y [EMAIL_0009].", None)
            .unwrap(),
        TextResult::Done {
            text: "Hola, Marta Lozano y [EMAIL_0009].".into(),
            restored: vec!["[NOMBRE_PERSONA_0001]".into()],
            missing: vec!["[EMAIL_0009]".into()],
        }
    );
}

#[test]
fn a_conflict_waits_for_a_source_and_then_restores() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Firma: Marta Lozano.\n");
    world
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &text, Some("txt"))],
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();

    // Un compañero manda un diccionario con el mismo marcador y otro valor.
    let other = World::new().ready();
    let theirs = other.file("suyo.txt", "Firma: Pablo Ruiz.\n");
    let fake: SharedDetector = Arc::new(Fake(vec![("Pablo Ruiz", "NOMBRE_PERSONA")]));
    other.service.set_model(Ok(fake));
    other
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &theirs, Some("txt"))],
            dir: other.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    let id = other.service.store_view().unwrap().documents[0].id.clone();
    let exported = other.service.export(&id, &other.out()).unwrap();
    assert!(
        exported
            .warning
            .starts_with("Este archivo contiene los datos originales")
    );
    let name = world.service.import(&exported.path).unwrap();
    assert_eq!(world.service.store_view().unwrap().dictionaries.len(), 1);

    let pasted = "Hola, [NOMBRE_PERSONA_0001].";
    let TextResult::Conflict { conflicts } = world.service.deanonymize_text(pasted, None).unwrap()
    else {
        panic!("debe haber conflicto");
    };
    assert_eq!(conflicts[0].marker, "[NOMBRE_PERSONA_0001]");
    assert_eq!(
        conflicts[0].sources,
        [Source::Store, Source::Dictionary { name: name.clone() }]
    );
    let theirs = world
        .service
        .deanonymize_text(pasted, Some(&Source::Dictionary { name }))
        .unwrap();
    assert!(matches!(theirs, TextResult::Done { text, .. } if text == "Hola, Pablo Ruiz."));

    let file = world.file("respuesta.txt", pasted);
    world.events.take();
    let mut job = DeanonymizeJob {
        files: vec![job_file("r", &file, None)],
        dir: world.out(),
    };
    world.service.deanonymize(&job).unwrap();
    assert!(matches!(
        phases(&world.events.take(), "r").last().unwrap(),
        Phase::Conflict { .. }
    ));
    job.files[0].source = Some(Source::Store);
    world.service.deanonymize(&job).unwrap();
    let events = world.events.take();
    let Phase::Deanonymized { output, .. } = phases(&events, "r").last().unwrap() else {
        panic!("{events:?}");
    };
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "Hola, Marta Lozano."
    );
}

#[test]
fn the_store_panel_lists_forgets_and_clears() {
    let world = World::new().ready();
    let text = world.file("Contrato cliente.txt", "Firma: Marta Lozano.\n");
    world
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &text, Some("md"))],
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    let view = world.service.store_view().unwrap();
    assert_eq!(view.documents.len(), 1);
    let document = &view.documents[0];
    assert_eq!(document.name, "Contrato cliente.txt");
    assert_eq!(document.output, "contrato-cliente_anonimizado.md");
    assert_eq!(document.entries, 1);

    assert!(world.service.forget("no-existe").is_err());
    world.service.forget(&document.id).unwrap();
    assert!(world.service.store_view().unwrap().documents.is_empty());
    assert!(
        world.reopen_store().store.documents().is_empty(),
        "se guarda"
    );
    world.service.clear().unwrap();
}

#[test]
fn a_store_that_cannot_be_opened_can_be_started_over() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Marta Lozano\n");
    world
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &text, Some("txt"))],
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    let World { dir, .. } = world;

    // Otra clave: el almacén no se puede descifrar.
    let other = SharedKey(Arc::new(MemoryKey::new()));
    other.save(&StoreKey::generate().unwrap()).unwrap();
    let world = World::with(dir, other).ready();
    let StoreStatus::Failed { message } = world.service.status().store else {
        panic!("no debe abrirse");
    };
    assert!(message.contains("no se puede descifrar"), "{message}");
    assert!(world.service.store_view().is_err());

    let set_aside = world
        .service
        .start_over()
        .unwrap()
        .expect("se aparta el antiguo");
    assert!(Path::new(&set_aside).exists());
    assert_eq!(world.service.status().store, StoreStatus::Ready);
    assert!(world.service.store_view().unwrap().documents.is_empty());
    assert!(world.service.start_over().is_err(), "ya está abierto");
}

#[test]
fn expired_documents_go_once_a_day() {
    let world = World::new().ready();
    let text = world.file("nota.txt", "Marta Lozano\n");
    world
        .service
        .anonymize(&AnonymizeJob {
            files: vec![job_file("1", &text, Some("txt"))],
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    world.events.take();
    let expires = world.service.store_view().unwrap().documents[0]
        .expires
        .clone();
    let expires: jiff::civil::Date = expires.parse().unwrap();

    world.service.purge_if_new_day(expires.yesterday().unwrap());
    assert_eq!(world.service.store_view().unwrap().documents.len(), 1);
    world.service.purge_if_new_day(expires);
    assert!(world.service.store_view().unwrap().documents.is_empty());
    assert_eq!(world.events.take(), [Event::StoreChanged]);
    assert!(
        world.reopen_store().store.documents().is_empty(),
        "se guarda"
    );
}

#[test]
fn inspecting_lists_each_file_with_its_type() {
    let world = World::new();
    let text = world.file("nota.txt", "Hola\n");
    let rtf = world.file("carta.rtf", "{\\rtf1}");
    let folder = world.dir.path().display().to_string();
    let files = world.service.inspect(&[text, rtf, folder]);
    assert_eq!(files[0].kind, "TXT");
    assert_eq!(files[0].outputs, ["txt", "md"]);
    assert!(files[0].deanonymizable);
    assert_eq!(files[1].kind, "No admitido");
    assert!(files[1].reason.as_deref().unwrap().contains("«rtf»"));
    assert_eq!(files[2].kind, "No admitido");
    assert!(files[2].outputs.is_empty());
}

#[test]
fn a_batch_of_twenty_with_one_corrupt_file_processes_the_other_nineteen() {
    let world = World::new().ready();
    let mut files = Vec::new();
    for n in 0..20 {
        let (name, text) = match n % 4 {
            0 => (format!("nota-{n}.txt"), "Firma: Marta Lozano.\n".to_owned()),
            1 => (
                format!("acta-{n}.md"),
                "# Acta\n\nAsiste Marta Lozano.\n".to_owned(),
            ),
            2 => (
                format!("carta-{n}.txt"),
                "Sin datos personales.\n".to_owned(),
            ),
            _ => (format!("informe-{n}.md"), "Vive en Zaragoza.\n".to_owned()),
        };
        files.push(job_file(
            &n.to_string(),
            &world.file(&name, &text),
            Some("txt"),
        ));
    }
    // Un «Word» que no lo es.
    let corrupt = world.file("roto.docx", "esto no es un Word");
    files[7] = job_file("7", &corrupt, Some("docx"));

    world
        .service
        .anonymize(&AnonymizeJob {
            files,
            dir: world.out(),
            skipped: vec![],
            spreadsheet_amounts: false,
            resolution: None,
        })
        .unwrap();
    let events = world.events.take();
    assert!(matches!(
        phases(&events, "7").last().unwrap(),
        Phase::Failed { message } if message.contains("Word")
    ));
    let done = (0..20)
        .filter(|n| {
            matches!(
                phases(&events, &n.to_string()).last(),
                Some(Phase::Anonymized { .. })
            )
        })
        .count();
    assert_eq!(done, 19);
    assert_eq!(
        events.last().unwrap(),
        &Event::BatchDone {
            done: 19,
            failed: 1,
            store_error: None
        }
    );
    assert_eq!(world.reopen_store().store.documents().len(), 19);
}
