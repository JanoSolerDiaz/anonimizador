//! Decodificación: de las probabilidades de cada token a entidades con su
//! posición en caracteres.
//!
//! Primero se decide la etiqueta de cada palabra (con una de las dos
//! estrategias), después se reconstruyen las entidades con el esquema BIO
//! palabra a palabra, se recortan los bordes y se aplica el umbral.

use super::texto::split_words;
use super::tokenizacion::{Token, assign_words};
use crate::{Error, Result};

/// Cómo se decide la etiqueta de una palabra partida en varios subtokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordStrategy {
    /// La etiqueta del primer subtoken, como dice el PRD.
    #[default]
    FirstSubtoken,
    /// La media de las probabilidades de todos los subtokens, sumando B- e I-
    /// de cada tipo, como recomienda la tarjeta del modelo v2. B- frente a I-
    /// decide si la palabra empieza una entidad o sigue la anterior.
    WordMean,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodeOptions {
    pub strategy: WordStrategy,
    /// Puntuación media mínima de una entidad.
    pub threshold: f32,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            strategy: WordStrategy::default(),
            threshold: 0.5,
        }
    }
}

/// Entidad detectada por el modelo. `start` y `end` son posiciones en
/// caracteres, con `end` excluido; `label` es la etiqueta del modelo sin
/// prefijo BIO, por ejemplo `PERSON_NAME`.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Outside,
    Begin(usize),
    Inside(usize),
}

/// Etiquetas del modelo en el orden de su salida, separadas en prefijo BIO y
/// tipo.
#[derive(Debug, Clone)]
pub struct Labels {
    tags: Vec<Tag>,
    kinds: Vec<String>,
}

impl Labels {
    pub fn parse(labels: &[String]) -> Result<Self> {
        let mut kinds: Vec<String> = Vec::new();
        let mut kind_index = |kind: &str| match kinds.iter().position(|k| k == kind) {
            Some(index) => index,
            None => {
                kinds.push(kind.to_owned());
                kinds.len() - 1
            }
        };
        let mut tags = Vec::with_capacity(labels.len());
        for label in labels {
            let tag = if label == "O" {
                Tag::Outside
            } else if let Some(kind) = label.strip_prefix("B-") {
                Tag::Begin(kind_index(kind))
            } else if let Some(kind) = label.strip_prefix("I-") {
                Tag::Inside(kind_index(kind))
            } else {
                return Err(Error::InvalidModelConfig(format!(
                    "la etiqueta «{label}» no es O ni empieza por B- o I-"
                )));
            };
            tags.push(tag);
        }
        Ok(Self { tags, kinds })
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

/// Convierte cada fila de `width` logits en probabilidades. Resta el máximo
/// antes de la exponencial para no desbordar.
pub fn softmax_rows(values: &mut [f32], width: usize) {
    for row in values.chunks_mut(width) {
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut sum = 0.0;
        for v in row.iter_mut() {
            *v = (*v - max).exp();
            sum += *v;
        }
        for v in row.iter_mut() {
            *v /= sum;
        }
    }
}

/// Signos que se quitan de los bordes de una entidad, además de los espacios.
/// No incluye los que pueden ser parte del dato, como `+`, `@`, `#`, `_` o `€`.
const EDGE_PUNCTUATION: &str = ".,;:!?¿¡\"'«»“”‘’„()[]{}<>…–—-*/\\|";

fn is_edge_noise(c: char) -> bool {
    c.is_whitespace() || EDGE_PUNCTUATION.contains(c)
}

/// Entidades del texto a partir de las probabilidades de sus tokens, en fila:
/// `probs` tiene `labels.len()` valores por token.
pub fn decode(
    text: &str,
    tokens: &[Token],
    probs: &[f32],
    labels: &Labels,
    options: &DecodeOptions,
) -> Vec<Detection> {
    let width = labels.len();
    assert_eq!(
        probs.len(),
        tokens.len() * width,
        "el número de probabilidades no coincide con tokens × etiquetas"
    );
    let row = |token: usize| &probs[token * width..(token + 1) * width];

    let words = split_words(text);
    let mut tokens_of_word = vec![Vec::new(); words.len()];
    for (token, word) in assign_words(tokens, &words).into_iter().enumerate() {
        if let Some(word) = word {
            tokens_of_word[word].push(token);
        }
    }

    let word_tags: Vec<(Tag, f32)> = tokens_of_word
        .iter()
        .map(|subtokens| match (subtokens.first(), options.strategy) {
            (None, _) => (Tag::Outside, 0.0),
            (Some(&first), WordStrategy::FirstSubtoken) => {
                let (best, &p) = argmax(row(first));
                (labels.tags[best], p)
            }
            (Some(_), WordStrategy::WordMean) => {
                word_mean(subtokens.iter().map(|&t| row(t)), labels)
            }
        })
        .collect();

    // Esquema BIO sobre palabras: (tipo, primera palabra, última, suma de puntuaciones).
    let mut spans: Vec<(usize, usize, usize, f32)> = Vec::new();
    let mut open: Option<(usize, usize, usize, f32)> = None;
    for (index, &(tag, score)) in word_tags.iter().enumerate() {
        match (tag, open.as_mut()) {
            (Tag::Inside(kind), Some(current)) if current.0 == kind => {
                current.2 = index;
                current.3 += score;
            }
            (Tag::Begin(kind) | Tag::Inside(kind), _) => {
                spans.extend(open.take());
                open = Some((kind, index, index, score));
            }
            (Tag::Outside, _) => spans.extend(open.take()),
        }
    }
    spans.extend(open);

    let chars: Vec<char> = text.chars().collect();
    spans
        .into_iter()
        .filter_map(|(kind, first, last, sum)| {
            let score = sum / (last - first + 1) as f32;
            let mut start = words[first].start;
            let mut end = words[last].end;
            while start < end && is_edge_noise(chars[start]) {
                start += 1;
            }
            while end > start && is_edge_noise(chars[end - 1]) {
                end -= 1;
            }
            (start < end && score >= options.threshold).then(|| Detection {
                start,
                end,
                label: labels.kinds[kind].clone(),
                score,
            })
        })
        .collect()
}

fn argmax(values: &[f32]) -> (usize, &f32) {
    values.iter().enumerate().fold(
        (0, &values[0]),
        |best, (i, v)| if *v > *best.1 { (i, v) } else { best },
    )
}

/// Etiqueta de una palabra con la media de sus subtokens: gana `O` o el tipo
/// con más probabilidad (B- más I-), y dentro del tipo, B- o I-.
fn word_mean<'a>(rows: impl Iterator<Item = &'a [f32]>, labels: &Labels) -> (Tag, f32) {
    let mut outside = 0.0;
    let mut begin = vec![0.0; labels.kinds.len()];
    let mut inside = vec![0.0; labels.kinds.len()];
    let mut count = 0.0;
    for row in rows {
        count += 1.0;
        for (&tag, &p) in labels.tags.iter().zip(row) {
            match tag {
                Tag::Outside => outside += p,
                Tag::Begin(kind) => begin[kind] += p,
                Tag::Inside(kind) => inside[kind] += p,
            }
        }
    }
    let mut best = (Tag::Outside, outside / count);
    for kind in 0..labels.kinds.len() {
        let score = (begin[kind] + inside[kind]) / count;
        if score > best.1 {
            let tag = if begin[kind] >= inside[kind] {
                Tag::Begin(kind)
            } else {
                Tag::Inside(kind)
            };
            best = (tag, score);
        }
    }
    best
}
