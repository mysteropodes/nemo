//! L06-M2 production raster sources and charged resources of the existing immutable scene witness.
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
    resource_id, RasterSceneSource,
};
use crate::object_render_scene::{ObjectPaintPlan, ObjectSceneError};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct RasterSource {
    pub(crate) source_index: usize,
    pub(crate) layer_index: usize,
    pub(crate) source: RasterSceneSource,
}
pub(super) fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: index,
        reason,
    }
}
pub(super) fn sources(packet: &ObjectFramePacket) -> Result<Vec<RasterSource>, ObjectSceneError> {
    packet
        .raster_records()
        .map(|record| {
            let entry = record.entry();
            let item = &entry.item;
            if item.mesh_id().is_some()
                || item.is_bitmap_brush() == Some(true)
                || item.brush_group_id().is_some()
                || item.is_brush_texture_copy() == Some(true)
            {
                return Err(unavailable(
                    record.source_index(),
                    "raster mesh/brush producer is unavailable in the still-image join",
                ));
            }
            let layer = &packet.source_layers()[record.layer_index()];
            if layer.path_fx().is_some_and(|effects| !effects.is_empty()) {
                return Err(unavailable(
                    record.source_index(),
                    "raster path effects are unavailable",
                ));
            }
            Ok(RasterSource {
                source_index: record.source_index(),
                layer_index: record.layer_index(),
                source: RasterSceneSource {
                    asset_id: item.asset_id().as_str().to_owned(),
                    rect: item.display_rect(),
                    opacity: item.opacity(),
                },
            })
        })
        .collect()
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub fn declared_versions(
    plan: &ObjectPaintPlan,
) -> Result<Vec<(String, String)>, ObjectSceneError> {
    let mut resources = BTreeMap::new();
    for source in plan.raster_sources() {
        let id = resource_id(&source.source.asset_id);
        if !resources.contains_key(&id) {
            use crate::object_snapshot::object_raster_asset_resources::{
                AssetPixels, AssetPixelsError,
            };
            let version =
                AssetPixels::version_for(plan.packet().snapshot(), &source.source.asset_id)
                    .map_err(|error| match error {
                        AssetPixelsError::BareDrawing | AssetPixelsError::Missing => {
                            unavailable(0, "native raster asset is absent")
                        }
                        AssetPixelsError::WrongKind | AssetPixelsError::DecoderUnavailable => {
                            unavailable(0, "native raster decoder is unavailable")
                        }
                        AssetPixelsError::Unavailable => {
                            unavailable(0, "native raster linked source is unavailable")
                        }
                        _ => unavailable(0, "native raster pixel resource is unavailable"),
                    })?;
            resources.insert(id, version);
        }
    }
    Ok(resources.into_iter().collect())
}

#[cfg(not(all(feature = "history", feature = "project_media_codec")))]
pub fn declared_versions(
    plan: &ObjectPaintPlan,
) -> Result<Vec<(String, String)>, ObjectSceneError> {
    if plan.raster_sources().is_empty() {
        Ok(Vec::new())
    } else {
        Err(unavailable(
            0,
            "native raster resources require project media support",
        ))
    }
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod retained {
    use super::*;
    use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
        decoded_rgba8_bytes, SceneResourceEntry, SceneResourceKind, SceneResourceMismatch,
    };
    use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
        prepare_raster_scene_image, RasterPixelPort, RasterPixels, RasterSceneImage,
    };
    use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneResourceSet};
    use crate::object_snapshot::object_raster_asset_resources::AssetPixels;
    use std::sync::Arc;
    use vello::{
        kurbo::Affine,
        peniko::{Blob, ImageAlphaType, ImageData, ImageFormat},
    };

    #[derive(Debug, Clone)]
    struct RasterAsset {
        pixels: AssetPixels,
        image: ImageData,
        version: String,
    }
    #[derive(Debug, Clone)]
    pub(crate) struct RasterResources {
        assets: BTreeMap<String, RasterAsset>,
        draws: Vec<RasterSceneImage<AssetPixels>>,
        resources: Arc<ObjectSceneResourceSet>,
    }
    struct Pixels<'a>(&'a BTreeMap<String, RasterAsset>);
    impl RasterPixelPort for Pixels<'_> {
        type Handle = AssetPixels;
        fn pixels(&self, id: &str) -> Option<RasterPixels<AssetPixels>> {
            let asset = self.0.get(id)?;
            let (width, height) = asset.pixels.dimensions();
            Some(RasterPixels {
                handle: asset.pixels.clone(),
                width,
                height,
                version: asset.version.clone(),
            })
        }
    }
    impl RasterResources {
        fn prepare(
            plan: &ObjectPaintPlan,
            previous: Option<&Self>,
        ) -> Result<Self, ObjectSceneError> {
            let mut held: Vec<_> = previous
                .into_iter()
                .flat_map(|old| old.assets.values().map(|asset| asset.pixels.clone()))
                .collect();
            let mut assets = BTreeMap::new();
            let mut draws = Vec::new();
            for source in plan.raster_sources() {
                let id = &source.source.asset_id;
                if !assets.contains_key(id) {
                    let pixels = AssetPixels::retain_for_scene(plan.packet().snapshot(), id, &held)
                        .map_err(|_| {
                            unavailable(
                                source.source_index,
                                "native raster pixel resource is unavailable",
                            )
                        })?;
                    let old = previous
                        .and_then(|old| old.assets.get(id))
                        .filter(|old| old.pixels.same_allocation_as(&pixels));
                    let (width, height) = pixels.dimensions();
                    let image = old
                        .map(|old| old.image.clone())
                        .unwrap_or_else(|| ImageData {
                            data: Blob::new(Arc::new(pixels.clone())),
                            format: ImageFormat::Rgba8,
                            alpha_type: ImageAlphaType::Alpha,
                            width,
                            height,
                        });
                    let version = old
                        .map(|old| old.version.clone())
                        .unwrap_or_else(|| pixels.encoded_version());
                    held.push(pixels.clone());
                    assets.insert(
                        id.clone(),
                        RasterAsset {
                            pixels,
                            image,
                            version,
                        },
                    );
                }
                draws.push(
                    prepare_raster_scene_image(&source.source, &[], &Pixels(&assets)).map_err(
                        |_| {
                            unavailable(
                                source.source_index,
                                "native raster placement is unavailable",
                            )
                        },
                    )?,
                );
            }
            let entries = assets
                .iter()
                .map(|(id, asset)| {
                    let (width, height) = asset.pixels.dimensions();
                    SceneResourceEntry::new(
                        resource_id(id),
                        asset.version.clone(),
                        SceneResourceKind::Raster,
                        decoded_rgba8_bytes(width, height)
                            .expect("admitted bounded image dimensions"),
                        Arc::new(asset.pixels.clone()),
                    )
                })
                .collect();
            let resources = Arc::new(
                ObjectSceneResourceSet::new(entries)
                    .map_err(|_| unavailable(0, "native raster resource declarations invalid"))?,
            );
            Ok(Self {
                assets,
                draws,
                resources,
            })
        }
        pub(crate) fn draw(&self, index: usize) -> Option<(&ImageData, Affine, f32)> {
            let draw = self.draws.get(index)?;
            let asset = self.assets.get(draw.asset_id())?;
            Some((&asset.image, draw.placement(), draw.opacity_f32()))
        }
        pub(crate) fn images(&self) -> impl Iterator<Item = &ImageData> {
            self.assets.values().map(|asset| &asset.image)
        }
        pub(crate) fn matches(&self, plan: &ObjectPaintPlan) -> bool {
            let expected: std::collections::BTreeSet<_> = plan
                .raster_sources()
                .iter()
                .map(|source| source.source.asset_id.as_str())
                .collect();
            expected.len() == self.assets.len()
                && expected.iter().all(|id| {
                    self.assets.get(*id).is_some_and(|asset| {
                        asset.pixels.matches_asset(plan.packet().snapshot(), id)
                    })
                })
        }
        pub(crate) fn verify(
            &self,
            declared: &BTreeMap<String, String>,
        ) -> Result<(), SceneResourceMismatch> {
            let raster = declared
                .iter()
                .filter(|(id, _)| id.starts_with("raster-asset:"))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
            self.resources.verify_declared(&raster)
        }
    }
    pub(super) fn attach(
        mut witness: ObjectCompositionWitness,
        previous: Option<&ObjectCompositionWitness>,
    ) -> Result<ObjectCompositionWitness, ObjectSceneError> {
        if witness.plan().raster_sources().is_empty() {
            return Ok(witness);
        }
        let resources = Arc::new(RasterResources::prepare(
            witness.plan(),
            previous.and_then(|old| old.rasters.as_deref()),
        )?);
        witness.resources = Some(Arc::clone(&resources.resources));
        witness.rasters = Some(resources);
        Ok(witness)
    }
}
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(crate) use retained::RasterResources;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(super) fn attach(
    witness: crate::object_render_scene::ObjectCompositionWitness,
    previous: Option<&crate::object_render_scene::ObjectCompositionWitness>,
) -> Result<crate::object_render_scene::ObjectCompositionWitness, ObjectSceneError> {
    retained::attach(witness, previous)
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
impl crate::object_render_scene::ObjectCompositionWitness {
    pub fn has_scene_images(&self) -> bool {
        self.reference.is_some() || self.rasters.is_some()
    }
    pub(crate) fn has_raster_images(&self) -> bool {
        self.rasters.is_some()
    }
    pub(crate) fn raster_image(
        &self,
        index: usize,
    ) -> Option<(&vello::peniko::ImageData, vello::kurbo::Affine, f32)> {
        self.rasters.as_ref()?.draw(index)
    }
    pub(crate) fn raster_images(&self) -> impl Iterator<Item = &vello::peniko::ImageData> + '_ {
        self.rasters.iter().flat_map(|resources| resources.images())
    }
}
