//! Troceado en ventanas de tokens y agrupación en lotes.
//!
//! El modelo lee como mucho 512 tokens de una vez: 510 del texto más los dos
//! especiales de inicio y fin. Un texto más largo se parte en ventanas que se
//! solapan 128 tokens. Cada token toma la predicción de la ventana donde está
//! más lejos de un corte, porque ahí el modelo ve más contexto alrededor.

use std::collections::BTreeSet;

use nucleo::motor::troceado::{
    DEFAULT_BATCH_SIZE, DOCUMENT_OVERLAP_TOKENS, DOCUMENT_WINDOW_TOKENS, MAX_WINDOW_TOKENS, OVERLAP_TOKENS, Window, WindowRef, owners, plan_batches,
    plan_windows,
};

fn ranges(windows: &[Window]) -> Vec<(usize, usize)> {
    windows.iter().map(|w| (w.start, w.end)).collect()
}

fn plan(n: usize) -> Vec<Window> {
    plan_windows(n, MAX_WINDOW_TOKENS, OVERLAP_TOKENS)
}

#[test]
fn defaults_match_the_model_limits() {
    assert_eq!(MAX_WINDOW_TOKENS, 510);
    assert_eq!(OVERLAP_TOKENS, 128);
    assert_eq!(DEFAULT_BATCH_SIZE, 8);
}

#[test]
fn the_document_reading_uses_shorter_windows_with_less_overlap() {
    // Medido en el corpus: igual protección y la lectura seguida pasa de
    // 5,5 s a 3,9 s en diez páginas (ver DECISIONES.md).
    assert_eq!(DOCUMENT_WINDOW_TOKENS, 256);
    assert_eq!(DOCUMENT_OVERLAP_TOKENS, 32);
    let windows = plan_windows(600, DOCUMENT_WINDOW_TOKENS, DOCUMENT_OVERLAP_TOKENS);
    assert_eq!(ranges(&windows), [(0, 256), (224, 480), (344, 600)]);
}

#[test]
fn boundary_lengths() {
    assert!(plan(0).is_empty());
    assert_eq!(ranges(&plan(1)), [(0, 1)]);
    assert_eq!(ranges(&plan(510)), [(0, 510)]);
    // La última ventana se alinea con el final para que tenga contexto completo.
    assert_eq!(ranges(&plan(511)), [(0, 510), (1, 511)]);
    assert_eq!(ranges(&plan(1000)), [(0, 510), (382, 892), (490, 1000)]);
}

#[test]
fn small_parameters() {
    assert_eq!(
        ranges(&plan_windows(10, 4, 2)),
        [(0, 4), (2, 6), (4, 8), (6, 10)]
    );
}

#[test]
fn windows_cover_every_token_without_gaps_and_overlap_enough() {
    for n in (0..3000).step_by(7).chain([509, 510, 511, 892, 893]) {
        let windows = plan(n);
        if n == 0 {
            continue;
        }
        assert_eq!(windows.first().unwrap().start, 0, "n = {n}");
        assert_eq!(windows.last().unwrap().end, n, "n = {n}");
        for w in &windows {
            assert!(
                w.start < w.end && w.len() <= MAX_WINDOW_TOKENS,
                "n = {n}: {w:?}"
            );
        }
        for pair in windows.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(b.start > a.start, "n = {n}: no avanza");
            assert!(a.end - b.start >= OVERLAP_TOKENS, "n = {n}: solape corto");
        }
    }
}

/// Distancia del token al corte más cercano de la ventana. El principio y el
/// final del texto no son cortes: ahí no hay contexto que perder.
fn distance_to_cut(w: Window, token: usize, n: usize) -> usize {
    let left = if w.start == 0 {
        usize::MAX
    } else {
        token - w.start
    };
    let right = if w.end == n {
        usize::MAX
    } else {
        w.end - 1 - token
    };
    left.min(right)
}

#[test]
fn each_token_has_one_owner_that_contains_it_and_is_farthest_from_a_cut() {
    for n in [1, 300, 510, 511, 1000, 2345] {
        let windows = plan(n);
        let owner = owners(&windows, n);
        assert_eq!(owner.len(), n);
        for (token, &o) in owner.iter().enumerate() {
            let w = windows[o];
            assert!(w.start <= token && token < w.end, "n = {n}, token {token}");
            let best = windows
                .iter()
                .filter(|w| w.start <= token && token < w.end)
                .map(|&w| distance_to_cut(w, token, n))
                .max()
                .unwrap();
            assert_eq!(distance_to_cut(w, token, n), best, "n = {n}, token {token}");
        }
        assert!(
            owner.windows(2).all(|p| p[0] <= p[1]),
            "n = {n}: dueños desordenados"
        );
    }
}

#[test]
fn owners_in_the_overlap_of_1000_tokens() {
    let windows = plan(1000);
    let owner = owners(&windows, 1000);
    // Token 400: a 109 del corte derecho de la primera, a 18 del izquierdo de la segunda.
    assert_eq!(owner[400], 0);
    // Token 450: a 59 en la primera, a 68 en la segunda.
    assert_eq!(owner[450], 1);
    // Token 500: a 118 en la segunda, a 10 en la tercera.
    assert_eq!(owner[500], 1);
    assert_eq!(owner[999], 2);
}

#[test]
#[should_panic(expected = "solape")]
fn overlap_must_be_smaller_than_the_window() {
    plan_windows(1000, 128, 128);
}

#[test]
fn batches_mix_segments_and_contain_every_window_once() {
    // Segmentos de 0, 3, 1000 y 5 tokens: 0 + 1 + 3 + 1 = 5 ventanas.
    let counts = [0, 3, 1000, 5];
    let batches = plan_batches(&counts, MAX_WINDOW_TOKENS, OVERLAP_TOKENS, 2);
    let sizes: Vec<usize> = batches.iter().map(Vec::len).collect();
    assert_eq!(sizes, [2, 2, 1]);

    let all: Vec<WindowRef> = batches.into_iter().flatten().collect();
    let unique: BTreeSet<(usize, usize, usize)> = all
        .iter()
        .map(|r| (r.segment, r.window.start, r.window.end))
        .collect();
    assert_eq!(unique.len(), 5);
    for (segment, &n) in counts.iter().enumerate() {
        let expected: BTreeSet<(usize, usize)> = ranges(&plan(n)).into_iter().collect();
        let found: BTreeSet<(usize, usize)> = all
            .iter()
            .filter(|r| r.segment == segment)
            .map(|r| (r.window.start, r.window.end))
            .collect();
        assert_eq!(found, expected, "segmento {segment}");
    }
}

#[test]
fn batches_group_windows_of_similar_length() {
    // Ordenar por longitud reduce el relleno: en cada lote se rellena hasta la
    // ventana más larga.
    let counts = [5, 510, 7, 500, 6, 505];
    let batches = plan_batches(&counts, MAX_WINDOW_TOKENS, OVERLAP_TOKENS, 3);
    let lengths: Vec<Vec<usize>> = batches
        .iter()
        .map(|b| b.iter().map(|r| r.window.len()).collect())
        .collect();
    assert_eq!(lengths, [vec![510, 505, 500], vec![7, 6, 5]]);
}

#[test]
fn no_tokens_means_no_batches() {
    assert!(plan_batches(&[], MAX_WINDOW_TOKENS, OVERLAP_TOKENS, 8).is_empty());
    assert!(plan_batches(&[0, 0], MAX_WINDOW_TOKENS, OVERLAP_TOKENS, 8).is_empty());
}

#[test]
#[should_panic(expected = "lote")]
fn batch_size_must_be_positive() {
    plan_batches(&[10], MAX_WINDOW_TOKENS, OVERLAP_TOKENS, 0);
}

#[test]
fn ties_go_to_the_earlier_window() {
    // Con solape impar hay empates: (0,5), (2,7), (4,9), (5,10). El token 3
    // está a 1 del corte en la primera ventana y a 1 en la segunda.
    let windows = plan_windows(10, 5, 3);
    assert_eq!(ranges(&windows), [(0, 5), (2, 7), (4, 9), (5, 10)]);
    assert_eq!(owners(&windows, 10)[3], 0);
}
