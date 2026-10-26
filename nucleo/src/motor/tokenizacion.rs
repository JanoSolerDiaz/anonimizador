//! Envoltorio del tokenizador del modelo: tokens con su posición en
//! caracteres, sin truncado y sin tokens especiales.
//!
//! Los tokens especiales de inicio y fin (`<s>` y `</s>` en v1, `<bos>` y
//! `<eos>` en v2) no se añaden aquí, sino en cada ventana del troceado.

use std::path::Path;

use tokenizers::Tokenizer;

use super::texto::Word;
use crate::{Error, Result};

/// Token del texto. `start` y `end` son posiciones en caracteres, con `end`
/// excluido. Según el tokenizador, pueden incluir el espacio anterior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub id: u32,
    pub start: usize,
    pub end: usize,
}

pub struct TextTokenizer {
    inner: Tokenizer,
    prefix: Vec<u32>,
    suffix: Vec<u32>,
}

impl TextTokenizer {
    /// Carga `tokenizer.json` y desactiva el truncado y el relleno que pueda
    /// traer configurados (el de v1 trunca a 512 tokens).
    pub fn load(path: &Path) -> Result<Self> {
        let load_error =
            |e: tokenizers::Error| Error::ModelLoad(format!("tokenizador {}: {e}", path.display()));
        let mut inner = Tokenizer::from_file(path).map_err(load_error)?;
        inner.with_truncation(None).map_err(load_error)?;
        inner.with_padding(None);

        // Los tokens especiales se averiguan codificando un texto mínimo con
        // ellos: los que quedan antes del texto son el prefijo y los de
        // después, el sufijo.
        let sample = inner.encode_char_offsets("a", true).map_err(load_error)?;
        let mut prefix = Vec::new();
        let mut suffix = Vec::new();
        let mut seen_text = false;
        for (&id, &special) in sample
            .get_ids()
            .iter()
            .zip(sample.get_special_tokens_mask())
        {
            match (special == 1, seen_text) {
                (true, false) => prefix.push(id),
                (true, true) => suffix.push(id),
                (false, _) => seen_text = true,
            }
        }
        Ok(Self {
            inner,
            prefix,
            suffix,
        })
    }

    /// Tokens que van al principio de cada ventana.
    pub fn prefix(&self) -> &[u32] {
        &self.prefix
    }

    /// Tokens que van al final de cada ventana.
    pub fn suffix(&self) -> &[u32] {
        &self.suffix
    }

    pub fn encode(&self, text: &str) -> Result<Vec<Token>> {
        let encoding = self
            .inner
            .encode_char_offsets(text, false)
            .map_err(|e| Error::Tokenizer(e.to_string()))?;
        Ok(encoding
            .get_ids()
            .iter()
            .zip(encoding.get_offsets())
            .map(|(&id, &(start, end))| Token { id, start, end })
            .collect())
    }
}

/// Palabra a la que pertenece cada token: la primera con la que comparte algún
/// carácter, o ninguna si el token solo tiene espacios o delimitadores. Tokens
/// y palabras deben venir ordenados por posición.
pub fn assign_words(tokens: &[Token], words: &[Word]) -> Vec<Option<usize>> {
    tokens
        .iter()
        .map(|t| {
            // Primera palabra que termina después de que empiece el token.
            let candidate = words.partition_point(|w| w.end <= t.start);
            words
                .get(candidate)
                .filter(|w| w.start < t.end)
                .map(|_| candidate)
        })
        .collect()
}
