//! Troceado del texto tokenizado en ventanas que caben en el modelo, y
//! agrupación de las ventanas de todos los segmentos en lotes.
//!
//! Las ventanas se solapan para que ninguna palabra quede sin contexto por
//! caer justo en un corte. Cuando un token está en dos ventanas, se queda con
//! la predicción de la ventana donde está más lejos de un corte.

/// Tokens de texto por ventana: 512 menos los dos especiales de inicio y fin.
pub const MAX_WINDOW_TOKENS: usize = 510;
/// Tokens que comparte cada ventana con la siguiente.
pub const OVERLAP_TOKENS: usize = 128;
/// Ventanas por pasada del modelo.
pub const DEFAULT_BATCH_SIZE: usize = 8;

/// Tramo de tokens `[start, end)` de un segmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub end: usize,
}

impl Window {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Ventanas de como mucho `max_len` tokens que avanzan `max_len - overlap`.
/// La última se alinea con el final del texto para que tenga contexto completo.
pub fn plan_windows(n_tokens: usize, max_len: usize, overlap: usize) -> Vec<Window> {
    assert!(
        overlap < max_len,
        "el solape ({overlap}) debe ser menor que la ventana ({max_len})"
    );
    let mut windows = Vec::new();
    if n_tokens == 0 {
        return windows;
    }
    let stride = max_len - overlap;
    let mut start = 0;
    loop {
        let end = (start + max_len).min(n_tokens);
        windows.push(Window { start, end });
        if end == n_tokens {
            return windows;
        }
        start = (start + stride).min(n_tokens - max_len);
    }
}

/// Índice de la ventana dueña de cada token: la que contiene el token más
/// lejos de un corte. En caso de empate gana la primera.
///
/// El principio y el final del texto no son cortes, pero no hace falta
/// tratarlos aparte: cualquier otra ventana que contenga el token empieza
/// después que la primera (o acaba antes que la última), así que en ella el
/// token siempre está más cerca de un corte.
pub fn owners(windows: &[Window], n_tokens: usize) -> Vec<usize> {
    let distance = |w: &Window, token: usize| (token - w.start).min(w.end - 1 - token);
    (0..n_tokens)
        .map(|token| {
            let first = windows.partition_point(|w| w.end <= token);
            let mut best = first;
            for (index, w) in windows.iter().enumerate().skip(first + 1) {
                if w.start > token {
                    break;
                }
                if distance(w, token) > distance(&windows[best], token) {
                    best = index;
                }
            }
            best
        })
        .collect()
}

/// Ventana de un segmento concreto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRef {
    pub segment: usize,
    pub window: Window,
}

/// Ventanas de todos los segmentos, agrupadas en lotes de `batch_size`. Se
/// ordenan de más larga a más corta para que las de cada lote tengan una
/// longitud parecida y haga falta poco relleno.
pub fn plan_batches(
    token_counts: &[usize],
    max_len: usize,
    overlap: usize,
    batch_size: usize,
) -> Vec<Vec<WindowRef>> {
    assert!(batch_size > 0, "el tamaño de lote debe ser mayor que cero");
    let mut all: Vec<WindowRef> = token_counts
        .iter()
        .enumerate()
        .flat_map(|(segment, &n)| {
            plan_windows(n, max_len, overlap)
                .into_iter()
                .map(move |window| WindowRef { segment, window })
        })
        .collect();
    all.sort_by_key(|r| std::cmp::Reverse(r.window.len()));
    all.chunks(batch_size).map(<[WindowRef]>::to_vec).collect()
}
