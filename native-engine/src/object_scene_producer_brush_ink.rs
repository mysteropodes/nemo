//! Draws a leased bitmap-brush ink into a Vello scene (L07-J; G17): the scene consumer of
//! the brush producer's `BrushInk`.
//!
//! Placement is the baseline image item (`paint_layer_items` in
//! `geometry-wasm/src/engine.rs` at fbe4b8a3): the image is placed top-left at the world
//! rectangle with `translate(x, y) * scale_non_uniform(width / image_w, height / image_h)`
//! under the view transform, with the item opacity multiplied into the brush. The pixels
//! are the leased `BrushBitmap` itself: the Vello `Blob` holds an `Arc` alias of it, so
//! the charge stays held (and the image stays pinned against eviction) for as long as
//! Vello may retain the blob, and no RGBA copy exists outside the charged allocation. This
//! is the same retention contract the reference still uses (`ReferencePixels`).
//!
//! A rectangle or opacity that cannot be drawn is a typed error, never a skipped item.
//! The caller owns the Vello image-atlas rules (retire the old image before decoding a
//! replacement, `mark_override_image_dirty` when an image becomes active again, see
//! `Compositor::compose_object`); `ink_image` returns the `ImageData` for that purpose.

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmap;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush::BrushInk;
use std::sync::Arc;
use vello::kurbo::Affine;
use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use vello::Scene;

impl AsRef<[u8]> for BrushBitmap {
    fn as_ref(&self) -> &[u8] {
        self.rgba8()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InkDrawError {
    /// The world rectangle is NaN, infinite, or has a non-positive side.
    Placement,
    /// The opacity is NaN or infinite.
    Opacity,
}

/// The Vello image of a leased bitmap: straight RGBA8 over the charged allocation.
pub(crate) fn ink_image(bitmap: &BrushBitmap) -> ImageData {
    let (width, height) = bitmap.dimensions();
    ImageData {
        data: Blob::new(Arc::new(bitmap.clone())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

/// `translate(x, y) * scale_non_uniform(width / image_w, height / image_h)`.
pub(crate) fn ink_placement(ink: &BrushInk) -> Result<Affine, InkDrawError> {
    let (pixel_w, pixel_h) = ink.bitmap.dimensions();
    let rect = ink.rect;
    let finite = [rect.min_x, rect.min_y, rect.w, rect.h]
        .iter()
        .all(|value| value.is_finite());
    if !finite || rect.w <= 0.0 || rect.h <= 0.0 || pixel_w == 0 || pixel_h == 0 {
        return Err(InkDrawError::Placement);
    }
    let sx = rect.w / f64::from(pixel_w);
    let sy = rect.h / f64::from(pixel_h);
    Ok(Affine::translate((rect.min_x, rect.min_y)) * Affine::scale_non_uniform(sx, sy))
}

/// Draw one ink under `view`; returns the image that was drawn.
pub(crate) fn draw_ink(
    scene: &mut Scene,
    ink: &BrushInk,
    view: Affine,
) -> Result<ImageData, InkDrawError> {
    if !ink.opacity.is_finite() {
        return Err(InkDrawError::Opacity);
    }
    let place = ink_placement(ink)?;
    let image = ink_image(&ink.bitmap);
    let brush = ImageBrush::new(image.clone()).multiply_alpha(ink.opacity as f32);
    scene.draw_image(&brush, view * place);
    Ok(image)
}
