//! Localización y comprobación de los recursos que no van en git: el modelo,
//! ONNX Runtime y PDFium. Los descarga `scripts/descargar-*.mjs` en
//! `recursos/`; en la app instalada los proporciona Tauri.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Variable de entorno para indicar la carpeta de recursos explícitamente.
pub const RESOURCES_ENV: &str = "ANONIMIZADOR_RECURSOS";

/// Archivos del modelo que deben estar en `recursos/modelo/`.
pub const MODEL_FILES: [&str; 4] = [
    "model_quantized.onnx",
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
];

/// Nombre de la subcarpeta de binarios para la plataforma actual.
pub fn platform_dir() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "win-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "mac-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "mac-x64"
    } else {
        "no-soportada"
    }
}

fn onnxruntime_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.dylib"
    }
}

/// Rutas a los recursos a partir de su carpeta raíz.
#[derive(Debug, Clone)]
pub struct Resources {
    root: PathBuf,
}

/// Resultado de cargar las bibliotecas nativas y el modelo.
#[derive(Debug)]
pub struct SmokeReport {
    pub model_inputs: Vec<String>,
    pub model_outputs: Vec<String>,
    pub pdf_pages: usize,
}

impl Resources {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Busca la carpeta de recursos: primero en la variable de entorno
    /// [`RESOURCES_ENV`], después subiendo desde el directorio actual y
    /// desde el ejecutable hasta encontrar una carpeta `recursos/`.
    pub fn locate() -> Result<Self> {
        if let Some(dir) = std::env::var_os(RESOURCES_ENV) {
            return Ok(Self::new(dir));
        }
        let starts = [std::env::current_dir().ok(), std::env::current_exe().ok()];
        starts
            .into_iter()
            .flatten()
            .flat_map(|start| {
                start
                    .ancestors()
                    .map(|dir| dir.join("recursos"))
                    .collect::<Vec<_>>()
            })
            .find(|dir| dir.is_dir())
            .map(Self::new)
            .ok_or(Error::ResourcesDirNotFound)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn model_dir(&self) -> PathBuf {
        self.root.join("modelo")
    }

    pub fn model_path(&self) -> PathBuf {
        self.model_dir().join("model_quantized.onnx")
    }

    pub fn onnxruntime_path(&self) -> PathBuf {
        self.root
            .join("onnxruntime")
            .join(platform_dir())
            .join(onnxruntime_file_name())
    }

    pub fn pdfium_dir(&self) -> PathBuf {
        self.root.join("pdfium").join(platform_dir())
    }

    pub fn pdfium_path(&self) -> PathBuf {
        pdfium_render::prelude::Pdfium::pdfium_platform_library_name_at_path(&self.pdfium_dir())
    }

    /// Comprueba que existen todos los archivos necesarios. No comprueba
    /// los hashes: eso lo hacen los scripts de descarga.
    pub fn check_files(&self) -> Result<()> {
        let mut expected: Vec<PathBuf> = MODEL_FILES
            .iter()
            .map(|name| self.model_dir().join(name))
            .collect();
        expected.push(self.onnxruntime_path());
        expected.push(self.pdfium_path());

        match expected.into_iter().find(|path| !path.is_file()) {
            Some(missing) => Err(Error::ResourceMissing(missing)),
            None => Ok(()),
        }
    }

    /// Carga ONNX Runtime con el modelo y PDFium con un PDF mínimo creado en
    /// memoria. Sirve para detectar pronto problemas de versión o de enlace.
    ///
    /// Solo se puede llamar una vez por proceso: las dos bibliotecas se
    /// inicializan de forma global.
    pub fn smoke_test(&self) -> Result<SmokeReport> {
        self.check_files()?;
        let (model_inputs, model_outputs) = self.load_model()?;
        let pdf_pages = self.load_pdfium()?;
        Ok(SmokeReport {
            model_inputs,
            model_outputs,
            pdf_pages,
        })
    }

    fn load_model(&self) -> Result<(Vec<String>, Vec<String>)> {
        let model_error = |e: &dyn std::fmt::Display| Error::ModelLoad(e.to_string());

        // ONNX Runtime envía telemetría por defecto en Windows: se desactiva.
        let env = ort::init_from(self.onnxruntime_path())
            .map_err(|e| model_error(&e))?
            .with_name("anonimizador")
            .with_telemetry(false)
            .build()
            .map_err(|e| model_error(&e))?;

        let session = ort::session::Session::builder(&env)
            .and_then(|mut builder| builder.commit_from_file(self.model_path()))
            .map_err(|e| model_error(&e))?;

        let names =
            |outlets: &[ort::value::Outlet]| outlets.iter().map(|o| o.name().to_owned()).collect();
        Ok((names(session.inputs()), names(session.outputs())))
    }

    fn load_pdfium(&self) -> Result<usize> {
        use pdfium_render::prelude::*;

        let pdfium_error = |e: PdfiumError| Error::PdfiumLoad(e.to_string());

        let pdfium =
            Pdfium::new(Pdfium::bind_to_library(self.pdfium_path()).map_err(pdfium_error)?);

        let mut document = pdfium.create_new_pdf().map_err(pdfium_error)?;
        document
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .map_err(pdfium_error)?;
        let bytes = document.save_to_bytes().map_err(pdfium_error)?;

        let reloaded = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(pdfium_error)?;
        Ok(reloaded.pages().len() as usize)
    }
}
