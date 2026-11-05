//! Escritura del PDF anonimizado (PRD, «Escritura»). Cada página con datos
//! se renderiza, se tapan los datos con rectángulos blancos y se rehace como
//! una página nueva con la imagen y los marcadores en texto encima. Las
//! demás se copian tal cual.

use image::{DynamicImage, Rgba, RgbaImage};
use pdfium_render::prelude::*;

use super::{PdfDocument, Rect, find_value, open, pdf_error, pdfium};
use crate::formatos::{Replacement, order_for_writing};
use crate::{Error, Result};

pub const MIN_RESOLUTION: u32 = 150;
pub const MAX_RESOLUTION: u32 = 300;
pub const DEFAULT_RESOLUTION: u32 = 200;

/// Margen alrededor de las cajas de los caracteres, en puntos.
const MARGIN: f32 = 1.0;
/// El marcador ocupa como mucho esta parte del alto de su rectángulo.
const MAX_HEIGHT_SHARE: f32 = 0.7;
/// Por debajo de este tamaño se usa la forma corta.
const MIN_FONT_SIZE: f32 = 5.0;
/// Alto de las mayúsculas de Helvetica, en fracción del cuerpo.
const CAP_HEIGHT: f32 = 0.718;
/// Gris del borde de los rectángulos.
const BORDER: Rgba<u8> = Rgba([150, 150, 150, 255]);
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Resolución de las páginas rasterizadas, en ppp (150 a 300).
    pub resolution: u32,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            resolution: DEFAULT_RESOLUTION,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    /// Páginas convertidas en imagen.
    pub rasterized_pages: usize,
    /// Marcadores escritos en forma corta.
    pub short_forms: usize,
    /// Enlaces de páginas copiadas que se han quitado por llevar un dato
    /// en el destino.
    pub removed_links: usize,
}

pub(super) fn write(
    document: &PdfDocument,
    replacements: &[Replacement],
    options: &WriteOptions,
    short_form: &mut dyn FnMut(&str) -> Option<String>,
) -> Result<PdfOutput> {
    if !(MIN_RESOLUTION..=MAX_RESOLUTION).contains(&options.resolution) {
        return Err(Error::InvalidResolution(options.resolution));
    }
    let lengths: Vec<usize> = document
        .pages
        .iter()
        .map(|p| p.text.chars().count())
        .collect();
    order_for_writing(replacements, &lengths)?;

    let pdfium = pdfium()?;
    let source = open(&pdfium, document.bytes())?;
    // Aplanadas, las páginas que se copian llevan el texto de sus campos y
    // anotaciones en el contenido, y no dependen del formulario del original.
    // Aplanar borra también los enlaces: los enlaces a una URI se leen antes
    // y se vuelven a poner en la copia.
    let mut links = Vec::new();
    for mut page in source.pages().iter() {
        links.push(uri_links(&page));
        page.flatten().map_err(pdf_error)?;
    }

    // Los valores que se tapan: el texto original de cada sustitución.
    let mut values: Vec<String> = Vec::new();
    for r in replacements {
        let value: String = document.pages[r.segment]
            .text
            .chars()
            .skip(r.start)
            .take(r.end - r.start)
            .collect();
        if !values.contains(&value) {
            values.push(value);
        }
    }
    let values: Vec<&str> = values.iter().map(String::as_str).collect();

    let mut output = pdfium.create_new_pdf().map_err(pdf_error)?;
    let font = output.fonts_mut().helvetica();
    let mut rasterized_pages = 0;
    let mut short_forms = 0;
    let mut removed_links = 0;
    for (index, page_text) in document.pages.iter().enumerate() {
        let own: Vec<&Replacement> = replacements.iter().filter(|r| r.segment == index).collect();
        let destination = output.pages().len();
        if own.is_empty() {
            output
                .pages_mut()
                .copy_page_from_document(&source, index as PdfPageIndex, destination)
                .map_err(pdf_error)?;
            // Sus enlaces vuelven, salvo los que llevan un dato en el destino.
            let mut page = output.pages().get(destination).map_err(pdf_error)?;
            for (uri, bounds) in &links[index] {
                if has_value(&percent_decode(uri), &values) {
                    removed_links += 1;
                    continue;
                }
                page.annotations_mut()
                    .create_link_annotation(uri)
                    .and_then(|mut link| link.set_bounds(*bounds))
                    .map_err(pdf_error)?;
            }
            continue;
        }
        rasterized_pages += 1;
        let page = source
            .pages()
            .get(index as PdfPageIndex)
            .map_err(pdf_error)?;
        let geometry = Geometry::of(&page)?;

        // Rectángulos de cada sustitución, ya en el espacio de la página tal
        // como se ve (girada), con el origen arriba a la izquierda.
        let covered: Vec<(Vec<Rect>, &str)> = own
            .iter()
            .map(|r| {
                let boxes = (r.start..r.end).flat_map(|i| page_text.boxes(i).iter().copied());
                let rects = lines_of(boxes)
                    .iter()
                    .map(|rect| geometry.to_display(&rect.grown(MARGIN)))
                    .collect();
                (rects, r.text.as_str())
            })
            .collect();

        let scale = options.resolution as f32 / 72.0;
        let width_px = (geometry.width * scale).round().max(1.0) as i32;
        let height_px = (geometry.height * scale).round().max(1.0) as i32;
        let mut image = page
            .render_with_config(&PdfRenderConfig::new().set_target_size(width_px, height_px))
            .and_then(|bitmap| bitmap.as_image())
            .map_err(pdf_error)?
            .to_rgba8();
        drop(page);
        let border = (options.resolution as f32 / 150.0).round().max(1.0) as u32;
        let px_per_pt_x = image.width() as f32 / geometry.width;
        let px_per_pt_y = image.height() as f32 / geometry.height;
        for (rects, _) in &covered {
            for rect in rects {
                paint(&mut image, rect, px_per_pt_x, px_per_pt_y, border);
            }
        }

        let mut page = output
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(geometry.width),
                PdfPoints::new(geometry.height),
            ))
            .map_err(pdf_error)?;
        page.objects_mut()
            .create_image_object(
                PdfPoints::ZERO,
                PdfPoints::ZERO,
                &DynamicImage::ImageRgba8(image),
                Some(PdfPoints::new(geometry.width)),
                Some(PdfPoints::new(geometry.height)),
            )
            .map_err(pdf_error)?;
        for (rects, marker) in &covered {
            // El marcador va en el rectángulo más ancho; los demás quedan
            // en blanco.
            let Some(rect) = rects.iter().max_by(|a, b| a.width().total_cmp(&b.width())) else {
                continue;
            };
            let (text, size) = match fit(marker, rect) {
                Some(size) => ((*marker).to_owned(), size),
                None => match short_form(marker) {
                    Some(short) => {
                        short_forms += 1;
                        let size = fit(&short, rect).unwrap_or_else(|| largest_size(&short, rect));
                        (short, size)
                    }
                    None => ((*marker).to_owned(), largest_size(marker, rect)),
                },
            };
            let text_width = helvetica_width(&text) * size;
            // De arriba abajo a coordenadas de PDF, centrado. En la página
            // vista, `rect.top` es la distancia del borde de abajo.
            let bottom = geometry.height - rect.top;
            let x = rect.left + (rect.width() - text_width) / 2.0;
            let y = bottom + (rect.height() - size * CAP_HEIGHT) / 2.0;
            page.objects_mut()
                .create_text_object(
                    PdfPoints::new(x),
                    PdfPoints::new(y),
                    &text,
                    font,
                    PdfPoints::new(size),
                )
                .map_err(pdf_error)?;
        }
    }
    let bytes = output.save_to_bytes().map_err(pdf_error)?;
    Ok(PdfOutput {
        bytes,
        rasterized_pages,
        short_forms,
        removed_links,
    })
}

/// Tamaño de la página y paso de las coordenadas del PDF a las de la página
/// tal como se ve.
struct Geometry {
    crop: Rect,
    rotation: PdfPageRenderRotation,
    /// Ancho y alto de la página tal como se ve, en puntos.
    width: f32,
    height: f32,
}

impl Geometry {
    fn of(page: &PdfPage) -> Result<Geometry> {
        let boundaries = page.boundaries();
        let crop = boundaries
            .crop()
            .or_else(|_| boundaries.media())
            .map_err(pdf_error)?;
        let crop = Rect::of(&crop.bounds);
        let rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let (width, height) = match rotation {
            PdfPageRenderRotation::Degrees90 | PdfPageRenderRotation::Degrees270 => {
                (crop.height(), crop.width())
            }
            _ => (crop.width(), crop.height()),
        };
        Ok(Geometry {
            crop,
            rotation,
            width,
            height,
        })
    }

    /// Un punto del PDF en la página vista, desde arriba a la izquierda. Con
    /// `/Rotate 90` la página gira en el sentido de las agujas del reloj.
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let (u, v) = (x - self.crop.left, y - self.crop.bottom);
        let (w, h) = (self.crop.width(), self.crop.height());
        match self.rotation {
            PdfPageRenderRotation::Degrees90 => (v, u),
            PdfPageRenderRotation::Degrees180 => (w - u, v),
            PdfPageRenderRotation::Degrees270 => (h - v, w - u),
            PdfPageRenderRotation::None => (u, h - v),
        }
    }

    /// Un rectángulo del PDF en la página vista. `bottom` y `top` pasan a
    /// ser distancias desde arriba: `bottom` es el borde de arriba.
    fn to_display(&self, rect: &Rect) -> Rect {
        let (x0, y0) = self.point(rect.left, rect.bottom);
        let (x1, y1) = self.point(rect.right, rect.top);
        Rect {
            left: x0.min(x1),
            right: x0.max(x1),
            bottom: y0.min(y1),
            top: y0.max(y1),
        }
    }
}

/// Destino (URI) de cada anotación de la página, en su orden; `None` si no
/// es un enlace a una URI. Va decodificado: `marta%40correo.es` es
/// `marta@correo.es`.
pub(super) fn link_targets(page: &PdfPage) -> Vec<Option<String>> {
    page.annotations()
        .iter()
        .map(|annotation| {
            let link = annotation.as_link_annotation()?.link().ok()?;
            let uri = link.action()?.as_uri_action()?.uri().ok()?;
            Some(percent_decode(&uri))
        })
        .collect()
}

/// Si algún valor aparece en el destino de un enlace, con la misma regla que
/// la verificación.
pub(super) fn has_value(target: &str, values: &[&str]) -> bool {
    values.iter().any(|v| !find_value(target, v).is_empty())
}

/// Enlaces a una URI de la página: el destino tal cual y su rectángulo.
fn uri_links(page: &PdfPage) -> Vec<(String, PdfRect)> {
    page.annotations()
        .iter()
        .filter_map(|annotation| {
            let bounds = annotation.bounds().ok()?;
            let link = annotation.as_link_annotation()?.link().ok()?;
            let uri = link.action()?.as_uri_action()?.uri().ok()?;
            Some((uri, bounds))
        })
        .collect()
}

/// Deshace los `%XX` de una URI. Un `%` sin dos cifras hexadecimales detrás
/// se deja como está.
fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Une las cajas en una por línea: una caja sigue en la línea si se solapa
/// en vertical con ella y no vuelve hacia la izquierda.
fn lines_of(boxes: impl Iterator<Item = Rect>) -> Vec<Rect> {
    let mut lines: Vec<Rect> = Vec::new();
    for b in boxes {
        if let Some(line) = lines.last_mut() {
            let overlap = line.top.min(b.top) - line.bottom.max(b.bottom);
            let same_line =
                overlap > 0.5 * line.height().min(b.height()) && b.left >= line.left - MARGIN;
            if same_line {
                *line = line.union(&b);
                continue;
            }
        }
        lines.push(b);
    }
    lines
}

/// Pinta el rectángulo (en la página vista, en puntos) en blanco opaco con
/// un borde gris fino.
fn paint(image: &mut RgbaImage, rect: &Rect, sx: f32, sy: f32, border: u32) {
    let (width, height) = (image.width(), image.height());
    let clamp = |value: f32, max: u32| (value.max(0.0) as u32).min(max);
    let x0 = clamp((rect.left * sx).floor(), width);
    let x1 = clamp((rect.right * sx).ceil(), width);
    let y0 = clamp((rect.bottom * sy).floor(), height);
    let y1 = clamp((rect.top * sy).ceil(), height);
    for y in y0..y1 {
        for x in x0..x1 {
            let edge = x < x0 + border || x + border >= x1 || y < y0 + border || y + border >= y1;
            image.put_pixel(x, y, if edge { BORDER } else { WHITE });
        }
    }
}

/// El mayor tamaño al que cabe `text` en el rectángulo, hasta el 70 % de su
/// alto, si llega a 5 pt.
fn fit(text: &str, rect: &Rect) -> Option<f32> {
    let size = largest_size(text, rect);
    (size >= MIN_FONT_SIZE).then_some(size)
}

fn largest_size(text: &str, rect: &Rect) -> f32 {
    let by_height = rect.height() * MAX_HEIGHT_SHARE;
    let by_width = rect.width() / helvetica_width(text).max(f32::EPSILON);
    by_height.min(by_width)
}

/// Ancho de `text` en Helvetica a cuerpo 1 (métricas AFM de la fuente
/// estándar). Los marcadores solo llevan mayúsculas, cifras, guiones bajos
/// y corchetes; el resto se cuenta como una mayúscula ancha.
fn helvetica_width(text: &str) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| match c {
            '[' | ']' | 'I' => 278,
            'J' => 500,
            '0'..='9' | '_' | 'L' => 556,
            'F' | 'T' | 'Z' => 611,
            'A' | 'B' | 'E' | 'K' | 'P' | 'S' | 'V' | 'X' | 'Y' => 667,
            'C' | 'D' | 'H' | 'N' | 'R' | 'U' => 722,
            'G' | 'O' | 'Q' => 778,
            'M' => 833,
            'W' => 944,
            _ => 722,
        })
        .sum();
    units as f32 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_width_matches_the_font_metrics() {
        // [NOMBRE_PERSONA_0001] mide 13 171 unidades.
        assert!((helvetica_width("[NOMBRE_PERSONA_0001]") - 13.171).abs() < 1e-3);
    }

    #[test]
    fn boxes_on_two_lines_give_two_rectangles() {
        let b = |left: f32, bottom: f32| Rect {
            left,
            bottom,
            right: left + 5.0,
            top: bottom + 12.0,
        };
        let lines = lines_of([b(200.0, 700.0), b(205.0, 700.0), b(72.0, 685.0)].into_iter());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].left, 200.0);
        assert_eq!(lines[0].right, 210.0);
    }

    #[test]
    fn a_narrow_rectangle_does_not_fit_the_full_marker() {
        let rect = Rect {
            left: 0.0,
            bottom: 0.0,
            right: 40.0,
            top: 14.0,
        };
        assert_eq!(fit("[NOMBRE_PERSONA_0001]", &rect), None);
        let size = fit("[N1]", &rect).unwrap();
        assert!((size - 14.0 * 0.7).abs() < 1e-3);
    }
}
