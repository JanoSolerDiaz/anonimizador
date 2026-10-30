//! Núcleo del anonimizador local.
//!
//! Todo el procesamiento ocurre aquí. La app de Tauri solo envía rutas y
//! opciones, y recibe eventos de progreso. Este crate no depende de Tauri:
//! se prueba con `cargo test` y con la CLI (`src/bin/cli.rs`).

pub mod almacen;
pub mod error;
pub mod evaluacion;
pub mod formatos;
pub mod motor;
pub mod operaciones;
pub mod recursos;

pub use error::{Error, Result};

/// Versión de la aplicación, tal como se guarda en el almacén.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
