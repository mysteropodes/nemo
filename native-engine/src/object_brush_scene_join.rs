//! Immutable brush paint resources and placement in the admitted render scene.
use crate::evaluation::object_scene_style_kernels::{
    bitmap_brush_resources_store::{BrushBitmapStore, RetirePort},
    object_scene_producer_brush::InkState,
};
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::object_scene_producer_brush_ink::{ink_image, ink_placement};
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    SceneResourceEntry, SceneResourceKind,
};
use crate::object_render_scene::{ObjectSceneError, ObjectSceneResourceSet};
use crate::read_queries::brush_read::{self, BrushReadError, BrushReadLimits, NativeBrushProducer};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vello::{kurbo::Affine, peniko::ImageData};

/// The retained image cache is a render-scene responsibility. Pixel publication
/// remains inside the concrete read producer over the same charged store.
#[derive(Debug)]
pub(crate) struct BrushRuntime {
    producer: NativeBrushProducer,
    pub(super) previous: Option<BrushSources>,
}
impl BrushRuntime {
    pub(crate) fn new(store: BrushBitmapStore) -> Self {
        Self {
            producer: NativeBrushProducer::new(store),
            previous: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn resident_count(&self) -> usize {
        self.producer.resident_count()
    }
    #[cfg(test)]
    pub(crate) fn for_controls(store: BrushBitmapStore) -> Self {
        Self {
            producer: NativeBrushProducer::for_controls(store),
            previous: None,
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct BrushPaint {
    pub(crate) source_index: usize,
    pub(crate) layer_index: usize,
    pub(crate) ink: InkState,
    image: Option<ImageData>,
    transform: Option<Affine>,
}
impl BrushPaint {
    pub(super) fn apply_chain(
        &mut self,
        chain: &[crate::evaluation::motion_parent_chain::ChainEntry],
    ) -> Result<(), ObjectSceneError> {
        use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmap;
        use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
            prepare_raster_scene_image, RasterPixelPort, RasterPixels, RasterSceneSource,
        };
        struct Pixels<'a>(&'a BrushBitmap);
        impl RasterPixelPort for Pixels<'_> {
            type Handle = BrushBitmap;
            fn pixels(&self, _: &str) -> Option<RasterPixels<BrushBitmap>> {
                let (width, height) = self.0.dimensions();
                Some(RasterPixels {
                    handle: self.0.clone(),
                    width,
                    height,
                    version: self.0.declared_resource().1,
                })
            }
        }
        let InkState::Ink(ink) = &mut self.ink else {
            return Ok(());
        };
        let source = RasterSceneSource {
            asset_id: ink.bitmap.declared_resource().0,
            rect: crate::evaluation::motion_parent_chain::ImageRect {
                x: ink.rect.min_x,
                y: ink.rect.min_y,
                width: ink.rect.w,
                height: ink.rect.h,
                rotation: 0.0,
            },
            opacity: ink.opacity,
        };
        let draw =
            prepare_raster_scene_image(&source, chain, &Pixels(&ink.bitmap)).map_err(|_| {
                unavailable(
                    self.source_index,
                    "native bitmap parent placement unavailable",
                )
            })?;
        self.transform = Some(draw.placement());
        ink.opacity = draw.opacity();
        Ok(())
    }
    /// A collapsed trim occupies its original order slot but emits no ink.
    pub(crate) fn image(&self) -> Option<(&ImageData, Affine, f32)> {
        let InkState::Ink(ink) = &self.ink else {
            return None;
        };
        Some((self.image.as_ref()?, self.transform?, ink.opacity as f32))
    }
}
#[derive(Debug, Clone)]
pub(crate) struct BrushSources {
    pub(crate) paints: Vec<BrushPaint>,
    resources: Arc<ObjectSceneResourceSet>,
}
impl BrushSources {
    pub(crate) fn resources(&self) -> &Arc<ObjectSceneResourceSet> {
        &self.resources
    }
    pub(crate) fn claimed_rasters(&self) -> BTreeSet<usize> {
        self.paints.iter().map(|p| p.source_index).collect()
    }
    pub(crate) fn images(&self) -> impl Iterator<Item = &ImageData> {
        self.paints.iter().filter_map(|p| p.image.as_ref())
    }
}
fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: index,
        reason,
    }
}
fn read_error(error: BrushReadError) -> ObjectSceneError {
    unavailable(error.source_index, error.reason)
}
/// Resource versions and their charged leases exist before a scheduler key.
pub(crate) fn prepare(
    packet: &ObjectFramePacket,
    runtime: &mut BrushRuntime,
    previous: Option<&BrushSources>,
    limits: BrushReadLimits,
    retire: RetirePort<'_>,
) -> Result<BrushSources, ObjectSceneError> {
    let produced =
        brush_read::produce(packet, &mut runtime.producer, limits, retire).map_err(read_error)?;
    let mut paints = Vec::new();
    let mut resources = BTreeMap::new();
    for payload in produced {
        let (image, transform) = match &payload.ink {
            InkState::Collapsed => (None, None),
            InkState::Ink(ink) => {
                let (id, version) = ink.bitmap.declared_resource();
                resources.entry(id.clone()).or_insert_with(|| {
                    SceneResourceEntry::new(
                        id,
                        version,
                        SceneResourceKind::BitmapBrush,
                        ink.bitmap.cost() as u64,
                        Arc::new(ink.bitmap.clone()),
                    )
                });
                let prior = previous
                    .and_then(|old| {
                        old.paints
                            .iter()
                            .find(|p| p.source_index == payload.source_index)
                    })
                    .and_then(|p| match &p.ink {
                        InkState::Ink(old) if old.bitmap.same_allocation_as(&ink.bitmap) => {
                            p.image.clone()
                        }
                        _ => None,
                    });
                (
                    Some(prior.unwrap_or_else(|| ink_image(&ink.bitmap))),
                    Some(ink_placement(ink).map_err(|_| {
                        unavailable(payload.source_index, "native bitmap placement unavailable")
                    })?),
                )
            }
        };
        paints.push(BrushPaint {
            source_index: payload.source_index,
            layer_index: payload.layer_index,
            ink: payload.ink,
            image,
            transform,
        });
    }
    Ok(BrushSources {
        paints,
        resources: Arc::new(
            ObjectSceneResourceSet::new(resources.into_values().collect())
                .map_err(|_| unavailable(0, "native bitmap resource declaration unavailable"))?,
        ),
    })
}

/// The normal producer funnel, including real contours, Text, raster and
/// component sources through the pinned registry and qualified raster exclusion.
/// This crate-private preparation port also supports focused producer controls.
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(crate) fn prepare_object_render_scene_with_brush(
    packet: &ObjectFramePacket,
    expected: &crate::object_frame_packet::ObjectFrameSelector<'_>,
    runtime: &mut BrushRuntime,
    previous: Option<&BrushSources>,
    limits: BrushReadLimits,
    retire: RetirePort<'_>,
) -> Result<crate::object_render_scene::ObjectRenderScene, ObjectSceneError> {
    use crate::object_render_scene::{
        component_scene, duplicator_scene, media_scene, object_scene_producer_contour,
        raster_scene, text_scene,
    };
    crate::object_render_scene::validate_pins(packet, expected)?;
    let brushes = prepare(packet, runtime, previous, limits, retire)?;
    let claimed = brushes.claimed_rasters();
    let contours = object_scene_producer_contour::produce(packet).into_admitted()?;
    let duplicators = Arc::new(duplicator_scene::prepare(packet)?);
    let mut rasters = raster_scene::sources_excluding(packet, &claimed)?;
    let mut texts = text_scene::prepare(packet)?;
    let components = component_scene::prepare(packet, expected, &mut rasters, &mut texts)?;
    Ok(crate::object_render_scene::ObjectRenderScene {
        packet: packet.clone(),
        duplicators,
        contours: contours.into(),
        rasters: rasters.into(),
        texts,
        components,
        videos: media_scene::slots(packet)?.into(),
        brushes: Some(Arc::new(brushes)),
    })
}
#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_brush_scene_join.rs"]
mod tests;
