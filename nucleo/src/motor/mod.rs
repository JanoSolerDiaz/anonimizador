//! Motor de anonimización: tokenización, inferencia ONNX, troceado,
//! decodificación BIO, tipos opcionales y marcadores.
//!
//! Solo trabaja con texto; no sabe nada de formatos de archivo. Hito 2.

pub mod decodificacion;
pub mod inferencia;
pub mod marcadores;
pub mod opcionales;
pub mod texto;
pub mod tipos;
pub mod tokenizacion;
pub mod troceado;

use decodificacion::{DecodeOptions, Labels, decode, softmax_rows};
use inferencia::Inference;
use opcionales::SkippedTypes;
use tipos::{TypeTable, labels_from_config};
use tokenizacion::{TextTokenizer, Token};
use troceado::{
    DEFAULT_BATCH_SIZE, MAX_WINDOW_TOKENS, OVERLAP_TOKENS, Window, owners, plan_batches,
    plan_windows,
};

use std::time::{Duration, Instant};

use crate::recursos::{ModelPaths, Resources};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    pub decode: DecodeOptions,
    /// Ventanas por pasada del modelo.
    pub batch_size: usize,
    /// Hilos de ONNX Runtime para cada pasada. Solo se usa al cargar.
    pub threads: usize,
    /// Tipos opcionales que el usuario ha decidido no sustituir.
    pub skipped: SkippedTypes,
}

impl Default for EngineOptions {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        Self {
            decode: DecodeOptions::default(),
            batch_size: DEFAULT_BATCH_SIZE,
            threads: (cores / 2).max(1),
            skipped: SkippedTypes::default(),
        }
    }
}

/// Tiempo de cada fase de una llamada a `Engine::detect_timed`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    pub tokenization: Duration,
    pub inference: Duration,
    pub decoding: Duration,
}

/// Dato personal detectado. `start` y `end` son posiciones en caracteres
/// dentro del segmento, con `end` excluido.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub segment: usize,
    pub start: usize,
    pub end: usize,
    /// Marcador de la tabla de tipos, por ejemplo `NOMBRE_PERSONA`.
    pub kind: String,
    /// Etiqueta del modelo sin prefijo BIO, por ejemplo `PERSON_NAME`.
    pub label: String,
    pub text: String,
    pub score: f32,
}

pub struct Engine {
    tokenizer: TextTokenizer,
    labels: Labels,
    inference: Inference,
    options: EngineOptions,
}

impl Engine {
    /// Carga el tokenizador, las etiquetas de `config.json` y el modelo ONNX.
    /// Comprueba que la tabla de tipos cubre todas las etiquetas del modelo.
    pub fn load(resources: &Resources, model: &ModelPaths, options: EngineOptions) -> Result<Self> {
        model.check_files()?;
        let tokenizer = TextTokenizer::load(&model.tokenizer_path())?;
        let names = labels_from_config(&std::fs::read_to_string(model.config_path())?)?;
        TypeTable::builtin().check_model_labels(&names)?;
        let labels = Labels::parse(&names)?;
        let inference = Inference::load(resources, model, options.threads)?;
        Ok(Self {
            tokenizer,
            labels,
            inference,
            options,
        })
    }

    /// Entidades de cada segmento con las opciones de carga.
    pub fn detect<S: AsRef<str>>(&self, segments: &[S]) -> Result<Vec<Entity>> {
        self.detect_with(segments, &self.options)
    }

    /// Entidades de cada segmento con otras opciones de decodificación, de
    /// lote y de tipos opcionales. `options.threads` no se usa: los hilos se
    /// fijan al cargar.
    pub fn detect_with<S: AsRef<str>>(
        &self,
        segments: &[S],
        options: &EngineOptions,
    ) -> Result<Vec<Entity>> {
        self.detect_timed(segments, options)
            .map(|(entities, _)| entities)
    }

    /// Como `detect_with`, y además devuelve el tiempo de cada fase.
    pub fn detect_timed<S: AsRef<str>>(
        &self,
        segments: &[S],
        options: &EngineOptions,
    ) -> Result<(Vec<Entity>, Timings)> {
        let mut timings = Timings::default();
        let clock = Instant::now();
        let texts: Vec<&str> = segments.iter().map(AsRef::as_ref).collect();
        let tokens = texts
            .iter()
            .map(|text| self.tokenizer.encode(text))
            .collect::<Result<Vec<Vec<Token>>>>()?;
        timings.tokenization = clock.elapsed();
        let counts: Vec<usize> = tokens.iter().map(Vec::len).collect();
        let windows: Vec<Vec<Window>> = counts
            .iter()
            .map(|&n| plan_windows(n, MAX_WINDOW_TOKENS, OVERLAP_TOKENS))
            .collect();
        let clock = Instant::now();
        let logits = self.window_logits(&tokens, &counts, &windows, options.batch_size)?;
        timings.inference = clock.elapsed();
        let clock = Instant::now();

        let width = self.labels.len();
        let table = TypeTable::builtin();
        let mut entities = Vec::new();
        for (segment, text) in texts.iter().enumerate() {
            let n = counts[segment];
            if n == 0 {
                continue;
            }
            // Cada token toma los logits de su ventana dueña.
            let mut probs = Vec::with_capacity(n * width);
            for (token, &owner) in owners(&windows[segment], n).iter().enumerate() {
                let local = token - windows[segment][owner].start;
                probs
                    .extend_from_slice(&logits[segment][owner][local * width..(local + 1) * width]);
            }
            softmax_rows(&mut probs, width);

            let chars: Vec<char> = text.chars().collect();
            for d in decode(
                text,
                &tokens[segment],
                &probs,
                &self.labels,
                &options.decode,
            ) {
                let kind = table
                    .marker_for_label(&d.label)
                    .expect("la tabla cubre las etiquetas del modelo: se comprueba al cargar");
                entities.push(Entity {
                    segment,
                    start: d.start,
                    end: d.end,
                    kind: kind.to_owned(),
                    text: chars[d.start..d.end].iter().collect(),
                    label: d.label,
                    score: d.score,
                });
            }
        }
        options.skipped.retain(&mut entities);
        timings.decoding = clock.elapsed();
        Ok((entities, timings))
    }

    /// Pasa todas las ventanas por el modelo en lotes y devuelve los logits de
    /// los tokens de texto de cada una, por segmento y ventana.
    fn window_logits(
        &self,
        tokens: &[Vec<Token>],
        counts: &[usize],
        windows: &[Vec<Window>],
        batch_size: usize,
    ) -> Result<Vec<Vec<Vec<f32>>>> {
        let width = self.labels.len();
        let prefix = self.tokenizer.prefix();
        let suffix = self.tokenizer.suffix();
        let mut logits: Vec<Vec<Vec<f32>>> =
            windows.iter().map(|w| vec![Vec::new(); w.len()]).collect();

        for batch in plan_batches(counts, MAX_WINDOW_TOKENS, OVERLAP_TOKENS, batch_size) {
            let sequences: Vec<Vec<u32>> = batch
                .iter()
                .map(|r| {
                    let text_ids = tokens[r.segment][r.window.start..r.window.end]
                        .iter()
                        .map(|t| t.id);
                    prefix
                        .iter()
                        .copied()
                        .chain(text_ids)
                        .chain(suffix.iter().copied())
                        .collect()
                })
                .collect();
            let output = self.inference.run(&sequences, self.tokenizer.pad_id())?;
            if output.labels != width {
                return Err(Error::InvalidModelConfig(format!(
                    "el modelo devuelve {} etiquetas y config.json declara {width}",
                    output.labels
                )));
            }
            for (sequence, r) in batch.iter().enumerate() {
                let index = windows[r.segment]
                    .iter()
                    .position(|w| *w == r.window)
                    .expect("cada lote solo tiene ventanas planificadas");
                let mut rows = Vec::with_capacity(r.window.len() * width);
                for token in 0..r.window.len() {
                    rows.extend_from_slice(output.row(sequence, prefix.len() + token));
                }
                logits[r.segment][index] = rows;
            }
        }
        Ok(logits)
    }
}
