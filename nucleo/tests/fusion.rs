//! Fusión de las dos lecturas del modo «ambos»: lo que encuentre cualquiera
//! de las dos queda tapado. Dos entidades que se pisan en el mismo segmento
//! se juntan en una sola que cubre las dos, con el tipo de la más segura.

use nucleo::motor::Entity;
use nucleo::motor::fusion::merge_passes;

const TEXTS: [&str; 2] = [
    "Mme Marchand vive en Lyon con Paul Girard.",
    "Otro párrafo con Ana Gil.",
];

fn entity(segment: usize, start: usize, end: usize, kind: &str, score: f32) -> Entity {
    let chars: Vec<char> = TEXTS[segment].chars().collect();
    Entity {
        segment,
        start,
        end,
        kind: kind.to_owned(),
        label: format!("L-{kind}"),
        text: chars[start..end].iter().collect(),
        score,
    }
}

fn summary(entities: &[Entity]) -> Vec<(usize, String, String)> {
    entities
        .iter()
        .map(|e| (e.segment, e.text.clone(), e.kind.clone()))
        .collect()
}

fn s(segment: usize, text: &str, kind: &str) -> (usize, String, String) {
    (segment, text.to_owned(), kind.to_owned())
}

#[test]
fn what_only_one_pass_finds_is_kept() {
    let paragraph = vec![entity(0, 4, 12, "NOMBRE_PERSONA", 0.9)];
    let document = vec![entity(0, 21, 25, "LUGAR", 0.8)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    assert_eq!(
        summary(&merged),
        [s(0, "Marchand", "NOMBRE_PERSONA"), s(0, "Lyon", "LUGAR")]
    );
}

#[test]
fn the_same_entity_in_both_passes_appears_once() {
    let a = entity(1, 17, 24, "NOMBRE_PERSONA", 0.9);
    let merged = merge_passes(&TEXTS, vec![a.clone()], vec![a]);
    assert_eq!(summary(&merged), [s(1, "Ana Gil", "NOMBRE_PERSONA")]);
}

#[test]
fn overlapping_entities_become_one_that_covers_both() {
    // «Mme Marchand» (lectura seguida, menos segura) y «Marchand» (párrafo
    // a párrafo, más segura): queda el trozo largo con el tipo del segundo.
    let paragraph = vec![entity(0, 4, 12, "NOMBRE_PERSONA", 0.95)];
    let document = vec![entity(0, 0, 12, "CARGO", 0.6)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    assert_eq!(summary(&merged), [s(0, "Mme Marchand", "NOMBRE_PERSONA")]);
    assert_eq!((merged[0].start, merged[0].end), (0, 12));
    assert_eq!(merged[0].label, "L-NOMBRE_PERSONA");
    assert_eq!(merged[0].score, 0.95);
}

#[test]
fn partial_overlaps_are_joined_in_a_chain() {
    // «Paul Gi» + «Girard.» + «rard» → «Paul Girard».
    let paragraph = vec![
        entity(0, 30, 37, "NOMBRE_PERSONA", 0.7),
        entity(0, 37, 41, "NOMBRE_PERSONA", 0.6),
    ];
    let document = vec![entity(0, 35, 41, "NOMBRE_PERSONA", 0.8)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    assert_eq!(summary(&merged), [s(0, "Paul Girard", "NOMBRE_PERSONA")]);
    assert_eq!(merged[0].score, 0.8);
}

#[test]
fn touching_entities_are_not_joined() {
    // «Lyon» acaba donde empieza « con»: no se pisan, se quedan las dos.
    let paragraph = vec![entity(0, 21, 25, "LUGAR", 0.8)];
    let document = vec![entity(0, 25, 29, "CARGO", 0.5)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    assert_eq!(merged.len(), 2);
}

#[test]
fn entities_of_different_segments_are_never_joined() {
    let paragraph = vec![entity(0, 0, 3, "CARGO", 0.5)];
    let document = vec![entity(1, 0, 4, "LUGAR", 0.5)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    assert_eq!(
        summary(&merged),
        [s(0, "Mme", "CARGO"), s(1, "Otro", "LUGAR")]
    );
}

#[test]
fn the_result_is_sorted_by_segment_and_position() {
    let paragraph = vec![
        entity(1, 17, 24, "NOMBRE_PERSONA", 0.9),
        entity(0, 21, 25, "LUGAR", 0.8),
    ];
    let document = vec![entity(0, 4, 12, "NOMBRE_PERSONA", 0.9)];
    let merged = merge_passes(&TEXTS, paragraph, document);
    let order: Vec<(usize, usize)> = merged.iter().map(|e| (e.segment, e.start)).collect();
    assert_eq!(order, [(0, 4), (0, 21), (1, 17)]);
}
