//! Ordinary typed Text -> real glyph contours -> mixed paint and retained-resource witness.
//! Scene preparation is atomic; unsupported blocks never disappear from the ordered frame.
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::object_scene_producer_text::TextSceneItem;
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    SceneResourceEntry, SceneResourceMismatch,
};
use crate::object_render_scene::{
    ObjectCompositionWitness, ObjectSceneError, ObjectSceneResourceSet,
};
use std::sync::Arc;
use vello::{
    kurbo::Affine,
    peniko::{Color, Fill},
    Scene,
};

#[cfg(feature = "font_asset_admission")]
#[path = "object_text_scene_fonts.rs"]
mod fonts;

#[derive(Debug, Clone)]
pub(crate) struct TextSource {
    pub(crate) source_index: usize,
    pub(crate) layer_index: usize,
    pub(crate) item: TextSceneItem,
}
impl TextSource {
    pub(crate) fn draw(&self, scene: &mut Scene) {
        for (contour, [r, g, b, a]) in self.item.painted() {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(r, g, b, a),
                None,
                contour.contour.path(),
            );
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct TextSources {
    pub(crate) entries: Vec<TextSource>,
    pub(crate) resources: Arc<ObjectSceneResourceSet>,
}
fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: index,
        reason,
    }
}
#[cfg(not(feature = "font_asset_admission"))]
pub(super) fn prepare(packet: &ObjectFramePacket) -> Result<TextSources, ObjectSceneError> {
    if let Some(record) = packet.text_records().next() {
        return Err(unavailable(
            record.source_index(),
            "native font parser is unavailable",
        ));
    }
    Ok(TextSources {
        entries: Vec::new(),
        resources: Arc::new(ObjectSceneResourceSet::empty()),
    })
}
#[cfg(feature = "font_asset_admission")]
pub(super) fn prepare(packet: &ObjectFramePacket) -> Result<TextSources, ObjectSceneError> {
    use crate::object_render_scene::object_scene_producer_text::{
        prepare_block, TextBlockTarget, TextFontProvider, TextProducerError,
    };
    use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceKind;
    use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::GeometryAdmission;
    use std::collections::BTreeMap;
    let mut bundled = fonts::BundledInputs::default();
    for record in packet.text_records() {
        let layer = &packet.source_layers()[record.layer_index()];
        // Existing Motion holder/range guard remains mandatory in frame preparation.
        // Parent/effect/special contexts must not paint unmodified text while their
        // actual consumers are absent. No above-raw motion or expression fallback.
        if layer.parent_layer_uid.value().is_some_and(|v| v.is_some())
            || layer.path_fx().is_some_and(|v| !v.is_empty())
            || layer.widget.value().is_some()
            || layer.groups.value().is_some()
            || layer.rig.value().is_some()
            || layer.folder_id.value().is_some_and(|v| v.is_some())
            || layer.blend_mode.value().is_some_and(|v| v != "normal")
            || layer.matte_mode.value().is_some_and(|v| v != "none")
            || layer.matte_source_layer_uid.value().is_some()
            || layer.mattes_more.value().is_some_and(|v| !v.is_empty())
            || layer.is_effect_layer.value() == Some(&true)
            || layer.effects.value().is_some_and(|v| !v.is_empty())
            || layer.effects_from.value().is_some()
        {
            return Err(unavailable(
                record.source_index(),
                "Text parent, group or layer effects are unavailable",
            ));
        }
        bundled.include(&record.entry().block, record.source_index())?;
    }
    let mut resources: BTreeMap<String, Arc<fonts::FontResource>> = BTreeMap::new();
    let mut entries = Vec::new();
    for record in packet.text_records() {
        let source_index = record.source_index();
        let entry = record.entry();
        let font = fonts::prepare(packet.snapshot(), &entry.block, &bundled, source_index)?;
        let version = font.resource.version();
        let holder = record.holder_id();
        let item = prepare_block(
            Some(TextFontProvider {
                catalog: &font.faces,
                outlines: &fonts::Outlines(&font.faces),
                resource_version: &version,
            }),
            &TextBlockTarget {
                source_object_index: source_index,
                holder_id: &holder,
                record: &entry.block,
                glyph_stages: &[],
                chain: &[],
                item_opacity: packet
                    .layer_opacity(record.layer_index())
                    .map_err(|_| unavailable(source_index, "Text layer opacity is unavailable"))?,
                admission: GeometryAdmission::DropCoincidentCurves,
            },
        )
        .map_err(|error| {
            unavailable(
                source_index,
                match error {
                    TextProducerError::Scene(ref error) => error.reason(),
                    TextProducerError::Layout(_) => "native Text layout unavailable",
                    TextProducerError::FontProviderUnavailable => {
                        "native retained font unavailable"
                    }
                    TextProducerError::Color => "native Text paint unavailable",
                },
            )
        })?;
        let id = format!("font:{}", font.key);
        if item.font_resource() != id {
            return Err(unavailable(
                source_index,
                "Text resolved font identity mismatch",
            ));
        }
        if let Some(previous) = resources.get(&id) {
            if !previous.same_allocation_as(&font.resource) {
                return Err(unavailable(
                    source_index,
                    "Text font allocation identity mismatch",
                ));
            }
        } else {
            resources.insert(id, font.resource);
        }
        entries.push(TextSource {
            source_index,
            layer_index: record.layer_index(),
            item,
        });
    }
    let resources = resources
        .into_iter()
        .map(|(id, font)| {
            SceneResourceEntry::new(
                id,
                font.version(),
                SceneResourceKind::Font,
                font.bytes(),
                font,
            )
        })
        .collect();
    Ok(TextSources {
        entries,
        resources: Arc::new(
            ObjectSceneResourceSet::new(resources)
                .map_err(|_| unavailable(0, "native Text resource declarations invalid"))?,
        ),
    })
}

/// Both viewport and export call this after the ordinary raster attachment.
/// Union preserves every original retention token/version/charge; no resource
/// set is overwritten, and duplicate IDs refuse the entire witness.
pub(super) fn attach(
    mut witness: ObjectCompositionWitness,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    let fonts = &witness.plan.text_resources();
    if fonts.is_empty() {
        return Ok(witness);
    }
    let entries = witness
        .resources
        .iter()
        .flat_map(|set| set.entries())
        .chain(fonts.entries())
        .map(|entry| {
            SceneResourceEntry::new(
                entry.resource_id(),
                entry.version(),
                entry.kind(),
                entry.charged_bytes(),
                Arc::clone(entry.retention()),
            )
        })
        .collect();
    witness.resources = Some(Arc::new(ObjectSceneResourceSet::new(entries).map_err(
        |_| unavailable(0, "mixed native scene resource declarations invalid"),
    )?));
    Ok(witness)
}
pub(super) fn verify(
    witness: &ObjectCompositionWitness,
    declared: &std::collections::BTreeMap<String, String>,
) -> Result<(), SceneResourceMismatch> {
    let fonts = declared
        .iter()
        .filter(|(id, _)| id.starts_with("font:"))
        .map(|(id, version)| (id.clone(), version.clone()))
        .collect();
    witness.plan.text_resources().verify_declared(&fonts)
}

#[cfg(all(
    test,
    feature = "font_asset_admission",
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_text_scene_join.rs"]
mod tests;
