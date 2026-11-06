//! El almacén abierto (`almacen::sesion`): purga al abrir, guardar, exportar
//! e importar, y empezar uno nuevo cuando el que hay no se puede abrir sin
//! borrar el antiguo (PRD, «Archivo del almacén»).

use std::sync::Arc;

use jiff::civil::date;

use nucleo::Error;
use nucleo::almacen::archivo::{KeyProvider, MemoryKey, StoreKey};
use nucleo::almacen::sesion::{Keys, OpenStore};
use nucleo::almacen::{DocumentRecord, OutputFile, SourceFile};

/// La misma clave en memoria para varias aperturas.
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

fn keys(shared: &SharedKey) -> Keys {
    Box::new(shared.clone())
}

fn record(id: &str, expires: jiff::civil::Date) -> DocumentRecord {
    DocumentRecord {
        id: id.to_owned(),
        created: "2026-11-01T10:00:00+01:00".to_owned(),
        expires,
        app_version: "0.1.0".to_owned(),
        model: "modelo".to_owned(),
        origin: SourceFile {
            name: "Contrato cliente.docx".to_owned(),
            sha256: "ab".repeat(32),
        },
        output: OutputFile {
            name: "contrato-cliente_anonimizado.docx".to_owned(),
            format: "docx".to_owned(),
        },
        markers: vec!["[NOMBRE_PERSONA_0001]".to_owned()],
    }
}

#[test]
fn opening_purges_expired_documents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("almacen.anon");
    let shared = SharedKey(Arc::new(MemoryKey::new()));
    let mut open = OpenStore::open_with(&path, keys(&shared), date(2026, 11, 1)).unwrap();
    open.store
        .assign("NOMBRE_PERSONA", "PERSON_NAME", "Marta Lozano", "viejo");
    open.store
        .record_document(record("viejo", date(2026, 11, 10)));
    open.store
        .record_document(record("nuevo", date(2027, 2, 1)));
    open.save().unwrap();

    let open = OpenStore::open_with(&path, keys(&shared), date(2026, 11, 10)).unwrap();
    assert_eq!(open.purged, 1);
    let ids: Vec<&str> = open
        .store
        .documents()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(ids, ["nuevo"]);
}

#[test]
fn a_store_that_cannot_be_opened_is_set_aside_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("almacen.anon");
    let shared = SharedKey(Arc::new(MemoryKey::new()));
    let mut open = OpenStore::open_with(&path, keys(&shared), date(2026, 11, 5)).unwrap();
    open.store.record_document(record("uno", date(2027, 2, 1)));
    open.save().unwrap();
    let old = std::fs::read(&path).unwrap();

    // Con otra clave no se puede descifrar.
    let other = SharedKey(Arc::new(MemoryKey::new()));
    other.save(&StoreKey::generate().unwrap()).unwrap();
    assert!(matches!(
        OpenStore::open_with(&path, keys(&other), date(2026, 11, 5)),
        Err(Error::StoreUndecryptable)
    ));

    let (fresh, set_aside) =
        OpenStore::start_over(&path, keys(&other), date(2026, 11, 5), None).unwrap();
    assert!(fresh.store.documents().is_empty());
    let set_aside = set_aside.expect("el antiguo queda apartado");
    assert_eq!(
        set_aside.file_name().unwrap(),
        "almacen.anon.no-se-pudo-abrir-2026-11-05"
    );
    assert_eq!(
        std::fs::read(&set_aside).unwrap(),
        old,
        "el antiguo no cambia"
    );
    // El nuevo se abre con la clave con que se empezó.
    let reopened = OpenStore::open_with(&path, keys(&other), date(2026, 11, 5)).unwrap();
    assert!(reopened.store.documents().is_empty());

    // Una segunda vez el mismo día no pisa la primera.
    let (_, again) = OpenStore::start_over(&path, keys(&other), date(2026, 11, 5), None).unwrap();
    assert_eq!(
        again.unwrap().file_name().unwrap(),
        "almacen.anon.no-se-pudo-abrir-2026-11-05_2"
    );
    assert!(set_aside.exists());
}

#[test]
fn starting_over_without_a_store_just_creates_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("datos").join("almacen.anon");
    let shared = SharedKey(Arc::new(MemoryKey::new()));
    let (open, set_aside) =
        OpenStore::start_over(&path, keys(&shared), date(2026, 11, 5), None).unwrap();
    assert!(set_aside.is_none());
    assert!(open.store.documents().is_empty());
    assert!(path.exists());
}

#[test]
fn a_dictionary_goes_out_and_comes_back_in() {
    let dir = tempfile::tempdir().unwrap();
    let shared = SharedKey(Arc::new(MemoryKey::new()));
    let mut open =
        OpenStore::open_with(&dir.path().join("a.anon"), keys(&shared), date(2026, 11, 5)).unwrap();
    open.store
        .assign("NOMBRE_PERSONA", "PERSON_NAME", "Marta Lozano", "uno");
    open.store.record_document(record("uno", date(2027, 2, 1)));
    let exported = open.export("uno", dir.path()).unwrap();
    assert_eq!(
        exported.file_name().unwrap(),
        "contrato-cliente-anonimizado_diccionario.json"
    );
    assert!(matches!(
        open.export("no-existe", dir.path()),
        Err(Error::DocumentNotFound)
    ));

    let mut other =
        OpenStore::open_with(&dir.path().join("b.anon"), keys(&shared), date(2026, 11, 5)).unwrap();
    let name = other.import(&exported).unwrap();
    assert_eq!(name, "contrato-cliente-anonimizado_diccionario.json");
    assert_eq!(other.store.imported().len(), 1);
}

#[test]
fn a_retention_policy_shortens_stored_expiries_but_never_extends_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("almacen.anon");
    let shared = SharedKey(Arc::new(MemoryKey::new()));
    let mut open = OpenStore::open_with(&path, keys(&shared), date(2026, 11, 1)).unwrap();
    // Creado el 2026-11-01 con 90 días; otro que ya caducaba antes.
    open.store
        .record_document(record("largo", date(2027, 1, 30)));
    open.store
        .record_document(record("corto", date(2026, 11, 3)));
    open.save().unwrap();

    let open =
        OpenStore::open_with_policy(&path, keys(&shared), date(2026, 11, 2), Some(7)).unwrap();
    assert_eq!(open.policy(), Some(7));
    assert_eq!(open.store.retention_days(), 7);
    let expires: Vec<_> = open.store.documents().iter().map(|d| d.expires).collect();
    assert_eq!(expires, [date(2026, 11, 8), date(2026, 11, 3)]);
    assert_eq!(
        open.store.expiry_from(date(2026, 11, 2)),
        date(2026, 11, 9),
        "los nuevos caducan a los 7 días"
    );
    open.save_if_purged().unwrap();

    // El plazo acortado queda guardado aunque se abra sin política.
    let again = OpenStore::open_with(&path, keys(&shared), date(2026, 11, 2)).unwrap();
    assert_eq!(again.store.documents()[0].expires, date(2026, 11, 8));
    assert_eq!(again.policy(), None);

    // Una política más larga no alarga nada.
    let longer =
        OpenStore::open_with_policy(&path, keys(&shared), date(2026, 11, 2), Some(365)).unwrap();
    assert_eq!(longer.store.documents()[0].expires, date(2026, 11, 8));

    // Y al llegar el día, se purga.
    let later =
        OpenStore::open_with_policy(&path, keys(&shared), date(2026, 11, 8), Some(7)).unwrap();
    assert_eq!(later.purged, 2);
}
