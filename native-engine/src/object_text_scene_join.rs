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

#[path = "object_scene_parent_chain.rs"]
pub(crate) mod parent_chain;

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
    pub(crate) parent_chains: Arc<parent_chain::SceneParentChains>,
}
fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: index,
        reason,
    }
}
struct TextInput<'a> {
    source_index: usize,
    layer_index: usize,
    entry: &'a crate::object_frame_packet::text::TextEntry,
    visible: bool,
}
fn text_inputs<'a>(
    packet: &'a ObjectFramePacket,
    chains: &parent_chain::SceneParentChains,
) -> Result<Vec<TextInput<'a>>, ObjectSceneError> {
    let visible: std::collections::BTreeSet<_> =
        packet.text_records().map(|r| r.source_index()).collect();
    let Some(items) = packet.snapshot().document().item_records() else {
        return Ok(Vec::new());
    };
    let offset = packet.snapshot().document().objects().len() + items.raster_entries().len();
    let mut selected = Vec::new();
    for (index, entry) in items.text_entries().iter().enumerate() {
        if entry.target.context_id() != packet.context_id()
            || entry.target.scope_kind() != packet.scope_kind()
        {
            continue;
        }
        let layer = packet
            .source_layers()
            .iter()
            .position(|l| l.layer_uid() == entry.target.layer_uid())
            .ok_or_else(|| unavailable(offset + index, "Text layer unavailable"))?;
        if packet.source_frame_for_layer(layer) != Some(entry.target.frame()) {
            continue;
        }
        if visible.contains(&(offset + index)) || chains.needs_bounds(layer) {
            selected.push(TextInput {
                source_index: offset + index,
                layer_index: layer,
                entry,
                visible: visible.contains(&(offset + index)),
            });
        }
    }
    Ok(selected)
}
#[cfg(not(feature = "font_asset_admission"))]
pub(super) fn prepare(packet: &ObjectFramePacket) -> Result<TextSources, ObjectSceneError> {
    let chains = parent_chain::SceneParentChains::new(packet)
        .map_err(|e| unavailable(e.source_index, e.reason()))?;
    if let Some(input) = text_inputs(packet, &chains)?.first() {
        return Err(unavailable(
            input.source_index,
            "native font parser is unavailable",
        ));
    }
    Ok(TextSources {
        entries: Vec::new(),
        resources: Arc::new(ObjectSceneResourceSet::empty()),
        parent_chains: Arc::new(
            chains
                .finish()
                .map_err(|e| unavailable(e.source_index, e.reason()))?,
        ),
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
    let mut chains = parent_chain::SceneParentChains::new(packet)
        .map_err(|e| unavailable(e.source_index, e.reason()))?;
    let inputs = text_inputs(packet, &chains)?;
    let mut bundled = fonts::BundledInputs::default();
    for input in &inputs {
        // Parent admission now comes from the actual immutable chain producer.
        // No unrelated Text path-effect, group, rig or special-context waiver.
        if packet.source_layers()[input.layer_index]
            .path_fx()
            .is_some_and(|v| !v.is_empty())
        {
            return Err(unavailable(
                input.source_index,
                "Text path effects are unavailable",
            ));
        }
        bundled.include(&input.entry.block, input.source_index)?;
    }
    let mut resources: BTreeMap<String, Arc<fonts::FontResource>> = BTreeMap::new();
    let mut pending = Vec::new();
    let make = |input: &TextInput<'_>,
                font: &fonts::PreparedFont,
                chain: &[crate::evaluation::motion_parent_chain::ChainEntry]| {
        let version = font.resource.version();
        let holder = crate::object_frame_packet::text::text_holder_id(&input.entry.target);
        prepare_block(
            Some(TextFontProvider {
                catalog: &font.faces,
                outlines: &fonts::Outlines(&font.faces),
                resource_version: &version,
            }),
            &TextBlockTarget {
                source_object_index: input.source_index,
                holder_id: &holder,
                record: &input.entry.block,
                glyph_stages: &[],
                chain,
                item_opacity: packet.layer_opacity(input.layer_index).map_err(|_| {
                    unavailable(input.source_index, "Text layer opacity is unavailable")
                })?,
                admission: GeometryAdmission::DropCoincidentCurves,
            },
        )
        .map_err(|error| {
            unavailable(
                input.source_index,
                match error {
                    TextProducerError::Scene(ref e) => e.reason(),
                    TextProducerError::Layout(_) => "native Text layout unavailable",
                    TextProducerError::FontProviderUnavailable => {
                        "native retained font unavailable"
                    }
                    TextProducerError::Color => "native Text paint unavailable",
                    TextProducerError::Animator => "native Text animator evaluation unavailable",
                },
            )
        })
    };
    for input in inputs {
        let font = fonts::prepare(
            packet.snapshot(),
            &input.entry.block,
            &bundled,
            input.source_index,
        )?;
        let raw = make(&input, &font, &[])?;
        // This includes hidden ancestor Text from its actual exposure, never an
        // advance estimate/anchor box. Its retained font remains in the witness.
        chains
            .include_text(input.layer_index, &raw)
            .map_err(|e| unavailable(input.source_index, e.reason()))?;
        let id = format!("font:{}", font.key);
        if raw.font_resource() != id {
            return Err(unavailable(
                input.source_index,
                "Text resolved font identity mismatch",
            ));
        }
        if let Some(previous) = resources.get(&id) {
            if !previous.same_allocation_as(&font.resource) {
                return Err(unavailable(
                    input.source_index,
                    "Text font allocation identity mismatch",
                ));
            }
        } else {
            resources.insert(id, Arc::clone(&font.resource));
        }
        pending.push((input, raw));
    }
    for (layer_index, layer) in packet.source_layers().iter().enumerate() {
        let Some(records) = layer.text_animators.value().filter(|v| !v.0.is_empty()) else {
            continue;
        };
        let animators = records
            .0
            .iter()
            .map(|r| r.resolve(f64::from(packet.frame())))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| unavailable(0, "native Text selector evaluation unavailable"))?;
        let fps = packet
            .snapshot()
            .project()
            .map_or(24.0, |p| p.project().fps());
        let mut blocks = pending
            .iter_mut()
            .filter(|(input, _)| input.layer_index == layer_index)
            .map(|(_, item)| item)
            .collect::<Vec<_>>();
        crate::object_render_scene::object_scene_producer_text::animator::apply_layer(
            &mut blocks,
            &animators,
            f64::from(packet.frame()),
            fps,
        )
        .map_err(|_| unavailable(0, "native Text animator evaluation unavailable"))?;
    }
    let chains = Arc::new(
        chains
            .finish()
            .map_err(|e| unavailable(e.source_index, e.reason()))?,
    );
    let mut entries = Vec::new();
    for (input, mut raw) in pending {
        if !input.visible {
            continue;
        }
        let chain = chains
            .chain(input.layer_index)
            .map_err(|e| unavailable(input.source_index, e.reason()))?;
        crate::object_render_scene::object_scene_producer_text::animator::apply_chain(
            &mut raw, &chain,
        )
        .map_err(|_| {
            unavailable(
                input.source_index,
                "native Text animator parent chain unavailable",
            )
        })?;
        let item = raw;
        entries.push(TextSource {
            source_index: input.source_index,
            layer_index: input.layer_index,
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
        parent_chains: chains,
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

#[cfg(all(
    test,
    feature = "font_asset_admission",
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_scene_parent_chain.rs"]
mod parent_chain_tests;

#[cfg(all(
    test,
    feature = "font_asset_admission",
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_text_animator.rs"]
mod animator_tests;
