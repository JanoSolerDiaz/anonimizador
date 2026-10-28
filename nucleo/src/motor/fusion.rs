//! Fusión de las dos lecturas del modo de contexto «ambos».
//!
//! El motor lee el documento dos veces, párrafo a párrafo y seguido, y tapa
//! todo lo que encuentre cualquiera de las dos lecturas (decisión de Jano del
//! 2026-10-28, ver `DECISIONES.md`). Si dos entidades se pisan en el mismo
//! segmento, se juntan en una sola que cubre las dos, con el tipo, la
//! etiqueta y la puntuación de la más segura.

use super::Entity;

/// Une las entidades de las dos lecturas. `texts` son los segmentos, para
/// recortar el texto de las entidades juntadas. El resultado va ordenado por
/// segmento y posición.
pub fn merge_passes<S: AsRef<str>>(
    texts: &[S],
    first: Vec<Entity>,
    second: Vec<Entity>,
) -> Vec<Entity> {
    let mut all: Vec<Entity> = first.into_iter().chain(second).collect();
    all.sort_by_key(|e| (e.segment, e.start, std::cmp::Reverse(e.end)));

    let mut merged: Vec<Entity> = Vec::with_capacity(all.len());
    // Mejor entidad del grupo abierto, y dónde acaba el grupo.
    let mut group: Option<(Entity, usize, usize)> = None;
    for e in all {
        group = match group {
            Some((best, start, end)) if best.segment == e.segment && e.start < end => {
                let end = end.max(e.end);
                let best = if e.score > best.score { e } else { best };
                Some((best, start, end))
            }
            Some(closed) => {
                merged.push(close(texts, closed));
                let (start, end) = (e.start, e.end);
                Some((e, start, end))
            }
            None => {
                let (start, end) = (e.start, e.end);
                Some((e, start, end))
            }
        };
    }
    if let Some(closed) = group {
        merged.push(close(texts, closed));
    }
    merged
}

/// Entidad que cubre todo el grupo, con los datos de la más segura.
fn close<S: AsRef<str>>(texts: &[S], (best, start, end): (Entity, usize, usize)) -> Entity {
    if (best.start, best.end) == (start, end) {
        return best;
    }
    let text = texts[best.segment]
        .as_ref()
        .chars()
        .skip(start)
        .take(end - start)
        .collect();
    Entity {
        start,
        end,
        text,
        ..best
    }
}
