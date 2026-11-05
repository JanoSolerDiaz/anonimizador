//! Motor de anonimización: tokenización, inferencia ONNX, troceado,
//! decodificación BIO, regla de tarjetas, tipos opcionales y marcadores.
//!
//! Solo trabaja con texto; no sabe nada de formatos de archivo. Hito 2.

pub mod decodificacion;
pub mod fusion;
pub mod inferencia;
pub mod marcadores;
pub mod opcionales;
pub mod reglas;
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
    DEFAULT_BATCH_SIZE, DOCUMENT_OVERLAP_TOKENS, DOCUMENT_WINDOW_TOKENS, MAX_WINDOW_TOKENS,
    OVERLAP_TOKENS, Window, batch_windows, owners, plan_windows,
};

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::recursos::{ModelPaths, Resources};
use crate::{Error, Result};

/// Qué ve el modelo alrededor de cada segmento.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContextMode {
    /// Dos lecturas, una de cada modo de abajo, y se tapa lo que encuentre
    /// cualquiera de las dos (ver `fusion`). Es lo que más tapa y lo más lento.
    #[default]
    Combined,
    /// Los segmentos de una llamada se encadenan en orden y el modelo los lee
    /// seguidos, con el contexto de sus vecinos. Las entidades no cruzan de
    /// un segmento a otro.
    Document,
    /// Cada segmento se analiza aislado.
    Paragraph,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    pub decode: DecodeOptions,
    /// Ventanas por pasada del modelo.
    pub batch_size: usize,
    /// Hilos de ONNX Runtime para cada pasada. Solo se usa al cargar.
    pub threads: usize,
    /// Tipos opcionales que el usuario ha decidido no sustituir.
    pub skipped: SkippedTypes,
    pub context: ContextMode,
}

impl Default for EngineOptions {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        Self {
            decode: DecodeOptions::default(),
            batch_size: DEFAULT_BATCH_SIZE,
            threads: (cores / 2).max(1),
            skipped: SkippedTypes::default(),
            context: ContextMode::default(),
        }
    }
}

/// Tiempo de cada fase de una llamada a `Engine::detect_timed`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    pub tokenization: Duration,
    pub inference: Duration,
    pub decoding: Duration,
    /// Tokens de texto analizados, sin contar los especiales de cada ventana.
    pub tokens: usize,
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
        let clock = Instant::now();
        model.check_files()?;
        let tokenizer = TextTokenizer::load(&model.tokenizer_path())?;
        let names = labels_from_config(&std::fs::read_to_string(model.config_path())?)?;
        TypeTable::builtin().check_model_labels(&names)?;
        let labels = Labels::parse(&names)?;
        let inference = Inference::load(resources, model, options.threads)?;
        tracing::info!(
            modelo = model.name(),
            hilos = options.threads,
            carga_ms = ms(clock.elapsed()),
            "modelo cargado"
        );
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

    /// Como `detect`, y llama a `progress` con la parte hecha (de 0 a 1)
    /// después de cada lote del modelo. Con la lectura `ambos`, cada lectura
    /// es la mitad.
    pub fn detect_with_progress<S: AsRef<str>>(
        &self,
        segments: &[S],
        progress: &dyn Fn(f32),
    ) -> Result<Vec<Entity>> {
        self.detect_inner(segments, &self.options, progress)
            .map(|(entities, _)| entities)
    }

    /// Como `detect_with`, y además devuelve el tiempo de cada fase.
    pub fn detect_timed<S: AsRef<str>>(
        &self,
        segments: &[S],
        options: &EngineOptions,
    ) -> Result<(Vec<Entity>, Timings)> {
        self.detect_inner(segments, options, &|_| {})
    }

    fn detect_inner<S: AsRef<str>>(
        &self,
        segments: &[S],
        options: &EngineOptions,
        progress: &dyn Fn(f32),
    ) -> Result<(Vec<Entity>, Timings)> {
        let mut timings = Timings::default();
        let clock = Instant::now();
        let texts: Vec<&str> = segments.iter().map(AsRef::as_ref).collect();
        let tokens = texts
            .iter()
            .map(|text| self.tokenizer.encode(text))
            .collect::<Result<Vec<Vec<Token>>>>()?;
        timings.tokenization = clock.elapsed();
        timings.tokens = tokens.iter().map(Vec::len).sum();

        let (mut entities, windows) = match options.context {
            ContextMode::Document => {
                self.read(&texts, &tokens, true, options, &mut timings, progress)?
            }
            ContextMode::Paragraph => {
                self.read(&texts, &tokens, false, options, &mut timings, progress)?
            }
            ContextMode::Combined => {
                let first = |done: f32| progress(done / 2.0);
                let second = |done: f32| progress(0.5 + done / 2.0);
                let (paragraph, w1) =
                    self.read(&texts, &tokens, false, options, &mut timings, &first)?;
                let (document, w2) =
                    self.read(&texts, &tokens, true, options, &mut timings, &second)?;
                let clock = Instant::now();
                let merged = fusion::merge_passes(&texts, paragraph, document);
                timings.decoding += clock.elapsed();
                (merged, w1 + w2)
            }
        };
        let clock = Instant::now();
        options.skipped.retain(&mut entities);
        timings.decoding += clock.elapsed();
        log_detection(texts.len(), windows, &entities, &timings);
        Ok((entities, timings))
    }

    /// Una lectura del modelo. Con `together`, los segmentos van seguidos en
    /// un solo flujo de tokens; si no, cada uno en el suyo. Suma sus tiempos
    /// de inferencia y decodificación a `timings` y devuelve las entidades y
    /// el número de ventanas.
    fn read(
        &self,
        texts: &[&str],
        tokens: &[Vec<Token>],
        together: bool,
        options: &EngineOptions,
        timings: &mut Timings,
        progress: &dyn Fn(f32),
    ) -> Result<(Vec<Entity>, usize)> {
        // Flujos de tokens que lee el modelo: uno con todos los segmentos
        // seguidos, o uno por segmento. `placement[s]` dice en qué flujo va
        // el segmento `s` y en qué token empieza.
        let mut streams: Vec<Vec<u32>> = Vec::new();
        let mut placement = Vec::with_capacity(tokens.len());
        for segment_tokens in tokens {
            if streams.is_empty() || !together {
                streams.push(Vec::new());
            }
            let stream = streams.len() - 1;
            placement.push((stream, streams[stream].len()));
            streams[stream].extend(segment_tokens.iter().map(|t| t.id));
        }
        let (max_len, overlap) = if together {
            (DOCUMENT_WINDOW_TOKENS, DOCUMENT_OVERLAP_TOKENS)
        } else {
            (MAX_WINDOW_TOKENS, OVERLAP_TOKENS)
        };
        let counts: Vec<usize> = streams.iter().map(Vec::len).collect();
        let windows: Vec<Vec<Window>> = counts
            .iter()
            .map(|&n| plan_windows(n, max_len, overlap))
            .collect();
        let clock = Instant::now();
        let logits = self.window_logits(&streams, &windows, options.batch_size, progress)?;
        timings.inference += clock.elapsed();
        let clock = Instant::now();

        // Probabilidades de cada token de cada flujo, tomadas de su ventana dueña.
        let width = self.labels.len();
        let probs: Vec<Vec<f32>> = (0..streams.len())
            .map(|stream| {
                let n = counts[stream];
                let mut probs = Vec::with_capacity(n * width);
                for (token, &owner) in owners(&windows[stream], n).iter().enumerate() {
                    let local = token - windows[stream][owner].start;
                    probs.extend_from_slice(
                        &logits[stream][owner][local * width..(local + 1) * width],
                    );
                }
                softmax_rows(&mut probs, width);
                probs
            })
            .collect();

        // La decodificación es por segmento: una entidad no cruza de uno a otro.
        let table = TypeTable::builtin();
        let mut entities = Vec::new();
        for (segment, text) in texts.iter().enumerate() {
            let n = tokens[segment].len();
            if n == 0 {
                continue;
            }
            let (stream, first) = placement[segment];
            let segment_probs = &probs[stream][first * width..(first + n) * width];
            let chars: Vec<char> = text.chars().collect();
            for d in decode(
                text,
                &tokens[segment],
                segment_probs,
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
            reglas::apply_card_rule(text, segment, &mut entities);
        }
        timings.decoding += clock.elapsed();
        Ok((entities, windows.iter().map(Vec::len).sum()))
    }

    /// Pasa todas las ventanas por el modelo en lotes y devuelve los logits de
    /// los tokens de texto de cada una, por flujo y ventana.
    fn window_logits(
        &self,
        tokens: &[Vec<u32>],
        windows: &[Vec<Window>],
        batch_size: usize,
        progress: &dyn Fn(f32),
    ) -> Result<Vec<Vec<Vec<f32>>>> {
        let width = self.labels.len();
        let prefix = self.tokenizer.prefix();
        let suffix = self.tokenizer.suffix();
        let mut logits: Vec<Vec<Vec<f32>>> =
            windows.iter().map(|w| vec![Vec::new(); w.len()]).collect();

        let batches = batch_windows(windows, batch_size);
        let total = batches.len();
        for (done, batch) in batches.into_iter().enumerate() {
            // Al empezar cada lote: lo hecho hasta el anterior.
            progress(done as f32 / total as f32);
            let sequences: Vec<Vec<u32>> = batch
                .iter()
                .map(|r| {
                    let text_ids = tokens[r.segment][r.window.start..r.window.end]
                        .iter()
                        .copied();
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
        progress(1.0);
        Ok(logits)
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Registro de una detección: solo recuentos, tipos y tiempos. Nunca texto de
/// los documentos ni valores detectados (lo comprueba `tests/logs.rs`).
fn log_detection(segments: usize, windows: usize, entities: &[Entity], timings: &Timings) {
    tracing::info!(
        segmentos = segments,
        tokens = timings.tokens,
        ventanas = windows,
        entidades = entities.len(),
        tokenizacion_ms = ms(timings.tokenization),
        inferencia_ms = ms(timings.inference),
        decodificacion_ms = ms(timings.decoding),
        "detección terminada"
    );
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for e in entities {
        *by_kind.entry(e.kind.as_str()).or_default() += 1;
    }
    for (kind, n) in by_kind {
        tracing::debug!(tipo = kind, n, "entidades por tipo");
    }
}
