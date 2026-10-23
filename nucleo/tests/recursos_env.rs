//! La variable de entorno es global al proceso: esta prueba vive sola en su
//! propio binario de pruebas para no interferir con las demás.

use nucleo::recursos::{RESOURCES_ENV, Resources};

#[test]
fn environment_variable_takes_precedence_over_search() {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: es la única prueba de este binario; ningún otro hilo lee el entorno.
    unsafe { std::env::set_var(RESOURCES_ENV, dir.path()) };

    let found = Resources::locate().unwrap();

    assert_eq!(found.root(), dir.path());
}
