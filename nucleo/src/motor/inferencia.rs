//! Sesión de ONNX Runtime: recibe lotes de secuencias de tokens y devuelve los
//! logits de cada token.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::TensorRef;

use crate::recursos::{ModelPaths, Resources};
use crate::{Error, Result};

/// Sesión única del modelo. `Session::run` necesita acceso exclusivo, por eso
/// va dentro de un `Mutex`; ONNX Runtime ya reparte cada pasada entre varios
/// hilos.
pub struct Inference {
    session: Mutex<Session>,
}

/// Logits de un lote: `[secuencias, longitud, etiquetas]`, en fila.
pub struct BatchLogits {
    pub values: Vec<f32>,
    pub length: usize,
    pub labels: usize,
}

impl BatchLogits {
    /// Fila de logits de un token de una secuencia del lote.
    pub fn row(&self, sequence: usize, token: usize) -> &[f32] {
        let start = (sequence * self.length + token) * self.labels;
        &self.values[start..start + self.labels]
    }
}

impl Inference {
    pub fn load(resources: &Resources, model: &ModelPaths, threads: usize) -> Result<Self> {
        let env = resources.onnx_environment()?;
        let load_error = |e: String| Error::ModelLoad(e);
        let mut builder = Session::builder(&env)
            .map_err(|e| load_error(e.to_string()))?
            .with_intra_threads(threads)
            .map_err(|e| load_error(e.to_string()))?;
        let session = builder
            .commit_from_file(model.onnx_path())
            .map_err(|e| load_error(e.to_string()))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    /// Pasa un lote por el modelo. Las secuencias más cortas se rellenan con
    /// `pad_id` hasta la más larga y la máscara de atención marca el relleno
    /// para que el modelo lo ignore.
    pub fn run(&self, sequences: &[Vec<u32>], pad_id: u32) -> Result<BatchLogits> {
        let length = sequences.iter().map(Vec::len).max().unwrap_or(0);
        let mut ids = Vec::with_capacity(sequences.len() * length);
        let mut mask = Vec::with_capacity(sequences.len() * length);
        for sequence in sequences {
            ids.extend(sequence.iter().map(|&id| i64::from(id)));
            ids.extend(std::iter::repeat_n(
                i64::from(pad_id),
                length - sequence.len(),
            ));
            mask.extend(std::iter::repeat_n(1i64, sequence.len()));
            mask.extend(std::iter::repeat_n(0i64, length - sequence.len()));
        }

        let inference_error = |e: ort::Error| Error::Inference(e.to_string());
        let shape = [sequences.len(), length];
        let ids = TensorRef::from_array_view((shape, ids.as_slice())).map_err(inference_error)?;
        let mask = TensorRef::from_array_view((shape, mask.as_slice())).map_err(inference_error)?;

        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let outputs = session
            .run(ort::inputs!["input_ids" => ids, "attention_mask" => mask])
            .map_err(inference_error)?;
        let (out_shape, values) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        let dims: Vec<i64> = out_shape.iter().copied().collect();
        match dims[..] {
            [b, l, n] if b as usize == sequences.len() && l as usize == length => Ok(BatchLogits {
                values: values.to_vec(),
                length,
                labels: n as usize,
            }),
            _ => Err(Error::Inference(format!(
                "el modelo devolvió logits con forma {dims:?}"
            ))),
        }
    }
}
