//! Decodificación: de las probabilidades de cada token a entidades con su
//! posición en caracteres.
//!
//! Hay dos estrategias. `FirstSubtoken` sigue el PRD: cada palabra toma la
//! etiqueta de su primer subtoken y las entidades se reconstruyen con el
//! esquema BIO palabra a palabra. `WordMean` reproduce el decodificador de
//! referencia de la tarjeta de v2 (`pii_detector.py`, revisión 5e61416). En
//! las dos se recortan después los bordes y se aplica el umbral.

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
    /// de cada tipo, como el decodificador de referencia de v2. Una palabra se
    /// une a la entidad anterior del mismo tipo si entre las dos solo hay
    /// espacios, comas, guiones, barras, puntos o paréntesis, y su primer
    /// subtoken tiene menos de 0,5 de probabilidad de B-.
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

/// Tramo de una entidad antes de recortar bordes y aplicar el umbral.
struct Span {
    kind: usize,
    start: usize,
    end: usize,
    score_sum: f32,
    words: usize,
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

    let chars: Vec<char> = text.chars().collect();
    let spans = match options.strategy {
        WordStrategy::FirstSubtoken => first_subtoken_spans(text, tokens, &row, labels),
        WordStrategy::WordMean => word_mean_spans(&chars, text, tokens, &row, labels),
    };

    spans
        .into_iter()
        .filter_map(|span| {
            let score = span.score_sum / span.words as f32;
            let (mut start, mut end) = (span.start, span.end);
            while start < end && is_edge_noise(chars[start]) {
                start += 1;
            }
            while end > start && is_edge_noise(chars[end - 1]) {
                end -= 1;
            }
            (start < end && score >= options.threshold).then(|| Detection {
                start,
                end,
                label: labels.kinds[span.kind].clone(),
                score,
            })
        })
        .collect()
}

/// Estrategia del PRD: etiqueta del primer subtoken y esquema BIO.
fn first_subtoken_spans<'a>(
    text: &str,
    tokens: &[Token],
    row: &impl Fn(usize) -> &'a [f32],
    labels: &Labels,
) -> Vec<Span> {
    let words = split_words(text);
    let mut first_token = vec![None; words.len()];
    for (token, word) in assign_words(tokens, &words).into_iter().enumerate() {
        if let Some(word) = word {
            first_token[word].get_or_insert(token);
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut open: Option<Span> = None;
    for (word, first) in words.iter().zip(first_token) {
        let (tag, score) = match first {
            Some(token) => {
                let (best, &p) = argmax(row(token));
                (labels.tags[best], p)
            }
            None => (Tag::Outside, 0.0),
        };
        match (tag, open.as_mut()) {
            (Tag::Inside(kind), Some(current)) if current.kind == kind => {
                current.end = word.end;
                current.score_sum += score;
                current.words += 1;
            }
            (Tag::Begin(kind) | Tag::Inside(kind), _) => {
                spans.extend(open.take());
                open = Some(Span {
                    kind,
                    start: word.start,
                    end: word.end,
                    score_sum: score,
                    words: 1,
                });
            }
            (Tag::Outside, _) => spans.extend(open.take()),
        }
    }
    spans.extend(open);
    spans
}

/// Signos que el decodificador de referencia quita del principio y del final
/// de cada palabra antes de calcular su media.
const WORD_LEADING: &str = "([{<\"'«„";
const WORD_TRAILING: &str = ")]}>\"'».,;:!?";
/// Lo único que puede haber entre dos palabras para unirlas en una entidad.
const MERGE_GAP: &str = " ,-/.()";

/// Estrategia de la tarjeta de v2. Cada token cuenta en la palabra donde está
/// su primer carácter que no es espacio (v2 incluye el espacio anterior en
/// sus posiciones).
fn word_mean_spans<'a>(
    chars: &[char],
    text: &str,
    tokens: &[Token],
    row: &impl Fn(usize) -> &'a [f32],
    labels: &Labels,
) -> Vec<Span> {
    let anchors: Vec<(usize, usize)> = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            (t.start..t.end)
                .find(|&c| !chars[c].is_whitespace())
                .map(|anchor| (anchor, index))
        })
        .collect();

    let n_kinds = labels.kinds.len();
    let mut spans: Vec<Span> = Vec::new();
    for word in split_words(text) {
        let (mut ws, mut we) = (word.start, word.end);
        while ws < we && WORD_LEADING.contains(chars[ws]) {
            ws += 1;
        }
        while we > ws && WORD_TRAILING.contains(chars[we - 1]) {
            we -= 1;
        }
        let from = anchors.partition_point(|&(anchor, _)| anchor < ws);
        let to = anchors.partition_point(|&(anchor, _)| anchor < we);
        let members = &anchors[from..to];
        let Some(&(first_anchor, _)) = members.first() else {
            continue;
        };

        let mut outside = 0.0;
        let mut by_kind = vec![0.0; n_kinds];
        let (mut first_begin, mut first_count) = (0.0, 0.0);
        for &(anchor, token) in members {
            let mut begin = 0.0;
            for (&tag, &p) in labels.tags.iter().zip(row(token)) {
                match tag {
                    Tag::Outside => outside += p,
                    Tag::Begin(kind) => {
                        by_kind[kind] += p;
                        begin += p;
                    }
                    Tag::Inside(kind) => by_kind[kind] += p,
                }
            }
            if anchor == first_anchor {
                first_begin += begin;
                first_count += 1.0;
            }
        }
        let count = members.len() as f32;
        let Some((kind, score)) = by_kind
            .iter()
            .map(|p| p / count)
            .enumerate()
            .filter(|&(_, p)| p > outside / count)
            .max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            continue;
        };

        let starts_new = first_begin / first_count >= 0.5;
        match spans.last_mut() {
            Some(last)
                if last.kind == kind
                    && !starts_new
                    && chars[last.end..ws].iter().all(|&c| MERGE_GAP.contains(c)) =>
            {
                last.end = we;
                last.score_sum += score;
                last.words += 1;
            }
            _ => spans.push(Span {
                kind,
                start: ws,
                end: we,
                score_sum: score,
                words: 1,
            }),
        }
    }
    spans
}

fn argmax(values: &[f32]) -> (usize, &f32) {
    values.iter().enumerate().fold(
        (0, &values[0]),
        |best, (i, v)| if *v > *best.1 { (i, v) } else { best },
    )
}
