//! Localización y comprobación de los recursos que no van en git: el modelo,
//! ONNX Runtime y PDFium. Los descarga `scripts/descargar-*.mjs` en
//! `recursos/`; en la app instalada los proporciona Tauri.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Variable de entorno para indicar la carpeta de recursos explícitamente.
pub const RESOURCES_ENV: &str = "ANONIMIZADOR_RECURSOS";

/// Carpeta del modelo que se usa si no se indica otro.
pub const DEFAULT_MODEL: &str = "modelo";

/// Archivos que debe tener la carpeta de cada modelo.
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

/// Rutas a los archivos de un modelo, que vive en su propia carpeta dentro de
/// los recursos (`recursos/modelo`, `recursos/modelo-v2`…). El motor no sabe
/// qué modelo es: solo lee estos archivos.
#[derive(Debug, Clone)]
pub struct ModelPaths {
    name: String,
    dir: PathBuf,
}

impl ModelPaths {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn onnx_path(&self) -> PathBuf {
        self.dir.join("model_quantized.onnx")
    }

    pub fn tokenizer_path(&self) -> PathBuf {
        self.dir.join("tokenizer.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    fn expected_files(&self) -> impl Iterator<Item = PathBuf> + '_ {
        MODEL_FILES.iter().map(|name| self.dir.join(name))
    }

    /// Comprueba que la carpeta del modelo tiene todos sus archivos.
    pub fn check_files(&self) -> Result<()> {
        first_missing(self.expected_files())
    }
}

/// Resultado de cargar un modelo en la prueba de humo.
#[derive(Debug)]
pub struct ModelReport {
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Resultado de cargar las bibliotecas nativas y los modelos.
#[derive(Debug)]
pub struct SmokeReport {
    pub models: Vec<ModelReport>,
    pub pdf_pages: usize,
}

fn first_missing(expected: impl IntoIterator<Item = PathBuf>) -> Result<()> {
    match expected.into_iter().find(|path| !path.is_file()) {
        Some(missing) => Err(Error::ResourceMissing(missing)),
        None => Ok(()),
    }
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
        Self::locate_from(starts.into_iter().flatten())
    }

    /// Busca una carpeta `recursos/` subiendo desde cada punto de partida,
    /// en orden. Devuelve la primera que encuentre.
    pub fn locate_from(starts: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        starts
            .into_iter()
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

    /// Rutas del modelo que está en la carpeta `name` de los recursos.
    pub fn model(&self, name: &str) -> ModelPaths {
        ModelPaths {
            name: name.to_owned(),
            dir: self.root.join(name),
        }
    }

    /// Carpeta del modelo por defecto.
    pub fn model_dir(&self) -> PathBuf {
        self.model(DEFAULT_MODEL).dir
    }

    /// Archivo ONNX del modelo por defecto.
    pub fn model_path(&self) -> PathBuf {
        self.model(DEFAULT_MODEL).onnx_path()
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

    /// Comprueba que existen los archivos del modelo por defecto y de las
    /// bibliotecas nativas. No comprueba los hashes: eso lo hacen los
    /// scripts de descarga.
    pub fn check_files(&self) -> Result<()> {
        self.model(DEFAULT_MODEL).check_files()?;
        self.check_native_libraries()
    }

    fn check_native_libraries(&self) -> Result<()> {
        first_missing([self.onnxruntime_path(), self.pdfium_path()])
    }

    /// Prueba de humo con el modelo por defecto.
    pub fn smoke_test(&self) -> Result<SmokeReport> {
        self.smoke_test_models(&[self.model(DEFAULT_MODEL)])
    }

    /// Carga ONNX Runtime con cada modelo y PDFium con un PDF mínimo creado
    /// en memoria. Sirve para detectar pronto problemas de versión o de
    /// enlace.
    ///
    /// Solo se puede llamar una vez por proceso: las dos bibliotecas se
    /// inicializan de forma global.
    pub fn smoke_test_models(&self, models: &[ModelPaths]) -> Result<SmokeReport> {
        for model in models {
            model.check_files()?;
        }
        self.check_native_libraries()?;

        let env = self.onnx_environment()?;
        let models = models
            .iter()
            .map(|model| load_model(&env, model))
            .collect::<Result<Vec<_>>>()?;
        let pdf_pages = self.load_pdfium()?;
        Ok(SmokeReport { models, pdf_pages })
    }

    fn onnx_environment(&self) -> Result<ort::environment::Environment> {
        // ONNX Runtime envía telemetría por defecto en Windows: se desactiva.
        ort::init_from(self.onnxruntime_path())
            .map_err(|e| Error::ModelLoad(e.to_string()))?
            .with_name("anonimizador")
            .with_telemetry(false)
            .build()
            .map_err(|e| Error::ModelLoad(e.to_string()))
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

fn load_model(env: &ort::environment::Environment, model: &ModelPaths) -> Result<ModelReport> {
    let session = ort::session::Session::builder(env)
        .and_then(|mut builder| builder.commit_from_file(model.onnx_path()))
        .map_err(|e| Error::ModelLoad(e.to_string()))?;

    let names =
        |outlets: &[ort::value::Outlet]| outlets.iter().map(|o| o.name().to_owned()).collect();
    Ok(ModelReport {
        name: model.name().to_owned(),
        inputs: names(session.inputs()),
        outputs: names(session.outputs()),
    })
}
