//! Raster scene join (G01 / L06-K): one raster item becomes the image draw the
//! native scene path consumes, positioned exactly the way the baseline engine
//! places an `ImageRef`, with its pixels held by an immutable resource handle.
//!
//! Inputs are the raster item's display rect and opacity (L06-C
//! `RasterItem::display_rect` / `opacity`, resolved by the caller: this file sits
//! in the render-scene layer and does not import the document model), the Motion
//! stages the D30 provider resolved, and a [`RasterPixelPort`] that hands out the
//! decoded pixels of an asset (L06-E `AssetPixels` implements it in the join that
//! owns it). Per item the order is the baseline's (engine-bridge.js at fbe4b8a3,
//! lines 1434-1437 and 1471): element stage, layer stage, then each parent, every
//! step `transformImageRect` and `imgOp *= mat.op`, in that sequence. The
//! opacity is that sequential product (an aggregated `apply_chain_to_image_rect`
//! multiplier rounds differently), `rotation || 0` is applied to the final rect,
//! and the placement affine is the one `paint_layer_items` builds in
//! geometry-wasm/src/engine.rs: `T(x, y) * S(w / pixelW, h / pixelH)`, preceded
//! for a non-zero rotation by `T(c) * R(rad) * T(-c)` about the display centre.
//! Coordinates are not rounded (the baseline JSON rounding was transport size).
//!
//! A missing handle is the typed [`RasterSceneError::PixelsUnavailable`]: the
//! block is refused, never drawn without its image. [`prepare_raster_scene`] is
//! all or nothing, so a failed frame holds no handle. The retained image keeps its
//! handle (and the version it declared) until it is dropped, which is how the
//! pixels outlive the frame that draws them. Mesh deformation and brush tags are
//! other lanes' layers; the item's reserved tags are not read here.

use crate::evaluation::motion_parent_chain::{transform_image_rect, ChainEntry, ImageRect};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::object_render_scene::ObjectSceneError;
use vello::kurbo::Affine;

/// Producer name in the pinned scene registry.
pub(crate) const PRODUCER: &str = "raster-image";

/// The raster item as the scene needs it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RasterSceneSource {
    pub(crate) asset_id: String,
    /// Top-left display rect with the item's own rotation in degrees.
    pub(crate) rect: ImageRect,
    /// `c.opacity !== undefined ? c.opacity : 1`.
    pub(crate) opacity: f64,
}

/// Decoded pixels of one asset at the pinned snapshot.
#[derive(Clone, Debug)]
pub(crate) struct RasterPixels<H> {
    pub(crate) handle: H,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Non-empty content version, declared to the scene as the resource version.
    pub(crate) version: String,
}

pub(crate) trait RasterPixelPort {
    /// The retention token (L06-E `AssetPixels` in production).
    type Handle: Clone;
    /// `None` when the asset has no decoded pixels (missing, linked, over budget).
    fn pixels(&self, asset_id: &str) -> Option<RasterPixels<Self::Handle>>;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RasterSceneImage<H> {
    asset_id: String,
    handle: H,
    version: String,
    pixel_size: (u32, u32),
    rect: ImageRect,
    opacity: f64,
    placement: Affine,
}

impl<H> RasterSceneImage<H> {
    pub(crate) fn asset_id(&self) -> &str {
        &self.asset_id
    }
    /// The retention token: the pixels live as long as this image (or a clone).
    pub(crate) fn handle(&self) -> &H {
        &self.handle
    }
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
    pub(crate) fn pixel_size(&self) -> (u32, u32) {
        self.pixel_size
    }
    /// The final rect of the baseline `imgItem` (rotation already `|| 0`).
    pub(crate) fn rect(&self) -> ImageRect {
        self.rect
    }
    pub(crate) fn opacity(&self) -> f64 {
        self.opacity
    }
    /// What `multiply_alpha` receives: the engine reads the JSON number as f32.
    pub(crate) fn opacity_f32(&self) -> f32 {
        self.opacity as f32
    }
    /// Image-pixel space to document world.
    pub(crate) fn placement(&self) -> Affine {
        self.placement
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RasterSceneError {
    InvalidAssetId,
    NonFiniteSource,
    PixelsUnavailable {
        asset_id: String,
    },
    EmptyPixels {
        asset_id: String,
    },
    EmptyVersion {
        asset_id: String,
    },
    /// A zero-size or non-finite final rect: no invertible placement exists.
    DegenerateRect {
        asset_id: String,
    },
}

impl RasterSceneError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            RasterSceneError::InvalidAssetId => "raster scene asset id must be non-empty",
            RasterSceneError::NonFiniteSource => "raster scene rect and opacity must be finite",
            RasterSceneError::PixelsUnavailable { .. } => "raster asset pixels are unavailable",
            RasterSceneError::EmptyPixels { .. } => "raster asset pixels have no extent",
            RasterSceneError::EmptyVersion { .. } => "raster asset pixels declare no version",
            RasterSceneError::DegenerateRect { .. } => {
                "raster display rect has no invertible placement"
            }
        }
    }
}

/// A failed frame: the first item that cannot be drawn, by position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RasterSceneFailure {
    pub(crate) index: usize,
    pub(crate) error: RasterSceneError,
}

/// JavaScript `x || 0` on a number: NaN, 0 and -0 read as 0.
fn or_zero(value: f64) -> f64 {
    if value.is_nan() || value == 0.0 {
        0.0
    } else {
        value
    }
}

/// `paint_layer_items` placement for an image of `pixel` size drawn into `rect`.
pub(crate) fn image_placement(rect: &ImageRect, pixel: (u32, u32)) -> Affine {
    let sx = rect.width / f64::from(pixel.0);
    let sy = rect.height / f64::from(pixel.1);
    let mut place = Affine::translate((rect.x, rect.y)) * Affine::scale_non_uniform(sx, sy);
    if rect.rotation != 0.0 {
        let cx = rect.x + rect.width / 2.0;
        let cy = rect.y + rect.height / 2.0;
        place = Affine::translate((cx, cy))
            * Affine::rotate(rect.rotation.to_radians())
            * Affine::translate((-cx, -cy))
            * place;
    }
    place
}

fn finite_rect(rect: &ImageRect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|v| v.is_finite())
}

pub(crate) fn prepare_raster_scene_image<P: RasterPixelPort + ?Sized>(
    source: &RasterSceneSource,
    stages: &[ChainEntry],
    port: &P,
) -> Result<RasterSceneImage<P::Handle>, RasterSceneError> {
    if source.asset_id.is_empty() {
        return Err(RasterSceneError::InvalidAssetId);
    }
    if !finite_rect(&source.rect) || !source.opacity.is_finite() {
        return Err(RasterSceneError::NonFiniteSource);
    }
    let asset_id = source.asset_id.clone();
    let pixels =
        port.pixels(&source.asset_id)
            .ok_or_else(|| RasterSceneError::PixelsUnavailable {
                asset_id: asset_id.clone(),
            })?;
    if pixels.width == 0 || pixels.height == 0 {
        return Err(RasterSceneError::EmptyPixels { asset_id });
    }
    if pixels.version.is_empty() {
        return Err(RasterSceneError::EmptyVersion { asset_id });
    }
    let mut rect = source.rect;
    let mut opacity = source.opacity;
    for stage in stages {
        rect = transform_image_rect(&rect, stage.pivot, &stage.mat);
        opacity *= stage.mat.op;
    }
    rect.rotation = or_zero(rect.rotation);
    if !finite_rect(&rect)
        || !rect.rotation.is_finite()
        || !opacity.is_finite()
        || rect.width == 0.0
        || rect.height == 0.0
    {
        return Err(RasterSceneError::DegenerateRect { asset_id });
    }
    let pixel_size = (pixels.width, pixels.height);
    let placement = image_placement(&rect, pixel_size);
    if !placement.as_coeffs().iter().all(|v| v.is_finite()) || placement.determinant() == 0.0 {
        return Err(RasterSceneError::DegenerateRect { asset_id });
    }
    Ok(RasterSceneImage {
        asset_id,
        handle: pixels.handle,
        version: pixels.version,
        pixel_size,
        rect,
        opacity,
        placement,
    })
}

/// Admit every item or none: the first failure is returned with its position
/// and every handle taken for earlier items is dropped before returning.
pub(crate) fn prepare_raster_scene<P: RasterPixelPort + ?Sized>(
    items: &[(RasterSceneSource, Vec<ChainEntry>)],
    port: &P,
) -> Result<Vec<RasterSceneImage<P::Handle>>, RasterSceneFailure> {
    let mut images = Vec::with_capacity(items.len());
    for (index, (source, stages)) in items.iter().enumerate() {
        images.push(
            prepare_raster_scene_image(source, stages, port)
                .map_err(|error| RasterSceneFailure { index, error })?,
        );
    }
    Ok(images)
}

/// The resource id a prepared image declares to the scene (`EvaluationKey`).
pub(crate) fn resource_id(asset_id: &str) -> String {
    format!("raster-asset:{}", asset_id)
}

/// Report one raster item to the pinned scene registry: an item plus its
/// declared pixel resource on success, a typed unavailable entry otherwise.
pub(crate) fn raster_producer_output<H>(
    pin: ScenePin,
    source_object_index: usize,
    result: Result<RasterSceneImage<H>, RasterSceneError>,
) -> ProducerOutput<RasterSceneImage<H>> {
    let output = ProducerOutput::new(PRODUCER, pin);
    match result {
        Ok(image) => {
            let id = resource_id(&image.asset_id);
            let version = image.version.clone();
            output
                .with_item(source_object_index, image)
                .with_resource(id, version)
        }
        Err(error) => output.with_unavailable(source_object_index, error.reason()),
    }
}

/// The frame refusal for a failed item: the existing atomic scene error.
pub(crate) fn scene_refusal(
    error: &RasterSceneError,
    source_object_index: usize,
) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index,
        reason: error.reason(),
    }
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_scene_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_raster_asset_scene_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/object_raster_asset_scene.rs"]
mod tests;
