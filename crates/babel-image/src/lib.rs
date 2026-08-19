//! Deterministic, format-neutral image derivation primitives.
//!
//! This crate never writes to the input image. A render request must provide
//! the expected source hash, so a stale or substituted object is rejected
//! before any pixels are changed.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialPolygon {
    pub points: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderStyle {
    pub font_bytes: Vec<u8>,
    pub font_size_px: f32,
    pub text_color: [u8; 4],
    pub padding_px: u32,
    pub background_color: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedImage {
    pub bytes: Vec<u8>,
    pub source_hash: [u8; 32],
    pub output_hash: [u8; 32],
    pub width: u32,
    pub height: u32,
}

/// Persisted rendering choices refer to immutable font objects by hash. Font
/// bytes are resolved only at render time and are never duplicated in SQLite.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionRenderParameters {
    pub schema_version: u32,
    pub font_object_hash: [u8; 32],
    pub font_size_millipx: u32,
    pub text_color: [u8; 4],
    pub padding_px: u32,
    pub background_color: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrCandidate {
    pub schema_version: u32,
    pub engine_id: String,
    pub model_hash: [u8; 32],
    pub text: String,
    pub confidence_millionths: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("source image hash does not match the expected object hash")]
    SourceHashMismatch,
    #[error("source image cannot be decoded: {0}")]
    Decode(String),
    #[error("spatial polygon must contain at least three finite points")]
    InvalidPolygon,
    #[error("spatial polygon is outside the source image")]
    PolygonOutsideImage,
    #[error("font bytes are required for text embedding")]
    MissingFont,
    #[error("font cannot be decoded: {0}")]
    InvalidFont(String),
    #[error("font size must be finite and positive")]
    InvalidFontSize,
    #[error("translation does not fit inside the image region")]
    TextOverflow,
    #[error("rendered image cannot be encoded: {0}")]
    Encode(String),
}

/// Fill the region with a sampled flat background, then draw the supplied
/// human-authored translation. Complex masks and inpainting are intentionally
/// outside this basic deterministic renderer.
pub fn render_png(
    source_bytes: &[u8],
    expected_source_hash: [u8; 32],
    polygon: &SpatialPolygon,
    translation: &str,
    style: &RenderStyle,
) -> Result<RenderedImage, RenderError> {
    let source_hash: [u8; 32] = Sha256::digest(source_bytes).into();
    if source_hash != expected_source_hash {
        return Err(RenderError::SourceHashMismatch);
    }
    if polygon.points.len() < 3
        || polygon
            .points
            .iter()
            .any(|[x, y]| !x.is_finite() || !y.is_finite())
    {
        return Err(RenderError::InvalidPolygon);
    }
    if !style.font_size_px.is_finite() || style.font_size_px <= 0.0 {
        return Err(RenderError::InvalidFontSize);
    }
    if style.font_bytes.is_empty() {
        return Err(RenderError::MissingFont);
    }
    let mut image = image::load_from_memory(source_bytes)
        .map_err(|error| RenderError::Decode(error.to_string()))?
        .to_rgba8();
    let bounds = bounds(polygon, image.width(), image.height())?;
    let background = style
        .background_color
        .map(Rgba)
        .unwrap_or_else(|| sample_background(&image, bounds));
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            image.put_pixel(x, y, background);
        }
    }

    let font = FontArc::try_from_vec(style.font_bytes.clone())
        .map_err(|_| RenderError::InvalidFont("invalid OpenType font bytes".to_owned()))?;
    draw_text(&mut image, &font, bounds, style, translation)?;

    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|error| RenderError::Encode(error.to_string()))?;
    let output_hash: [u8; 32] = Sha256::digest(&bytes).into();
    Ok(RenderedImage {
        bytes,
        source_hash,
        output_hash,
        width: image.width(),
        height: image.height(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

fn bounds(polygon: &SpatialPolygon, width: u32, height: u32) -> Result<Bounds, RenderError> {
    let min_x = polygon
        .points
        .iter()
        .map(|[x, _]| *x)
        .fold(f32::INFINITY, f32::min);
    let min_y = polygon
        .points
        .iter()
        .map(|[_, y]| *y)
        .fold(f32::INFINITY, f32::min);
    let max_x = polygon
        .points
        .iter()
        .map(|[x, _]| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = polygon
        .points
        .iter()
        .map(|[_, y]| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    if min_x < 0.0 || min_y < 0.0 || max_x > width as f32 || max_y > height as f32 {
        return Err(RenderError::PolygonOutsideImage);
    }
    let left = min_x.floor() as u32;
    let top = min_y.floor() as u32;
    let right = max_x.ceil().min(width as f32) as u32;
    let bottom = max_y.ceil().min(height as f32) as u32;
    if left >= right || top >= bottom {
        return Err(RenderError::InvalidPolygon);
    }
    Ok(Bounds {
        left,
        top,
        right,
        bottom,
    })
}

fn sample_background(image: &RgbaImage, bounds: Bounds) -> Rgba<u8> {
    let points = [
        (bounds.left, bounds.top),
        (bounds.right.saturating_sub(1), bounds.top),
        (bounds.left, bounds.bottom.saturating_sub(1)),
        (
            bounds.right.saturating_sub(1),
            bounds.bottom.saturating_sub(1),
        ),
    ];
    let mut sum = [0_u32; 4];
    for (x, y) in points {
        let pixel = image.get_pixel(x, y).0;
        for (index, value) in pixel.into_iter().enumerate() {
            sum[index] += u32::from(value);
        }
    }
    Rgba(sum.map(|value| (value / 4) as u8))
}

fn draw_text(
    image: &mut RgbaImage,
    font: &FontArc,
    bounds: Bounds,
    style: &RenderStyle,
    text: &str,
) -> Result<(), RenderError> {
    let left = bounds.left.saturating_add(style.padding_px) as f32;
    let top = bounds.top.saturating_add(style.padding_px) as f32;
    let right = bounds.right.saturating_sub(style.padding_px) as f32;
    let bottom = bounds.bottom.saturating_sub(style.padding_px) as f32;
    if left >= right || top >= bottom {
        return Err(RenderError::TextOverflow);
    }

    let scale = PxScale::from(style.font_size_px);
    let scaled = font.as_scaled(scale);
    let line_height = scaled.ascent() - scaled.descent() + scaled.line_gap();
    let mut caret = point(left, top + scaled.ascent());

    for character in text.chars() {
        if character == '\n' {
            caret.x = left;
            caret.y += line_height;
            continue;
        }
        let glyph_id = scaled.glyph_id(character);
        let advance = scaled.h_advance(glyph_id);
        if caret.x > left && caret.x + advance > right {
            caret.x = left;
            caret.y += line_height;
        }
        if caret.y - scaled.descent() > bottom {
            return Err(RenderError::TextOverflow);
        }
        let glyph = glyph_id.with_scale_and_position(scale, caret);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let pixel_bounds = outlined.px_bounds();
            if pixel_bounds.min.x < left
                || pixel_bounds.min.y < top
                || pixel_bounds.max.x > right
                || pixel_bounds.max.y > bottom
            {
                return Err(RenderError::TextOverflow);
            }
            let color = style.text_color;
            outlined.draw(|x, y, coverage| {
                let target_x = pixel_bounds.min.x as u32 + x;
                let target_y = pixel_bounds.min.y as u32 + y;
                let destination = image.get_pixel_mut(target_x, target_y);
                blend(destination, color, coverage);
            });
        }
        caret.x += advance;
    }
    Ok(())
}

fn blend(destination: &mut Rgba<u8>, source: [u8; 4], coverage: f32) {
    let alpha = coverage * (f32::from(source[3]) / 255.0);
    let inverse = 1.0 - alpha;
    for (channel, source_value) in source.iter().take(3).enumerate() {
        destination.0[channel] = (f32::from(*source_value) * alpha
            + f32::from(destination.0[channel]) * inverse)
            .round() as u8;
    }
    destination.0[3] =
        ((alpha + f32::from(destination.0[3]) / 255.0 * inverse) * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn source() -> (Vec<u8>, [u8; 32]) {
        let image = ImageBuffer::from_pixel(12, 8, Rgba([240, 240, 240, 255]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let hash = Sha256::digest(&bytes).into();
        (bytes, hash)
    }

    #[test]
    fn stale_source_is_rejected_before_decode_or_render() {
        let (bytes, _) = source();
        let error = render_png(
            &bytes,
            [0; 32],
            &SpatialPolygon {
                points: vec![[1.0, 1.0], [10.0, 1.0], [10.0, 6.0]],
            },
            "译文",
            &RenderStyle {
                font_bytes: vec![1],
                font_size_px: 12.0,
                text_color: [0, 0, 0, 255],
                padding_px: 1,
                background_color: None,
            },
        )
        .unwrap_err();
        assert_eq!(error, RenderError::SourceHashMismatch);
    }

    #[test]
    fn invalid_regions_are_rejected_without_clipping_silently() {
        let (bytes, hash) = source();
        let error = render_png(
            &bytes,
            hash,
            &SpatialPolygon {
                points: vec![[-1.0, 1.0], [10.0, 1.0], [10.0, 6.0]],
            },
            "translation",
            &RenderStyle {
                font_bytes: vec![1],
                font_size_px: 12.0,
                text_color: [0, 0, 0, 255],
                padding_px: 1,
                background_color: None,
            },
        )
        .unwrap_err();
        assert_eq!(error, RenderError::PolygonOutsideImage);
    }
}
