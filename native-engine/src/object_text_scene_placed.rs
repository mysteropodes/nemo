//! A placed component's text blocks (#2244 C4). Native text ports the baseline's vector text
//! (`buildVectorTextGroup`), whose glyphs are paths. Inside a placed component, `getEffectiveStrokes`
//! maps them as any path through `applyMatrixToStrokeData`: points through the camera, then through
//! symMatrix, and handles through each matrix's linear part. A held block is laid out at its inner
//! position with no inner chain. Its contours go through the placement chain, and the block joins
//! the scene's text sources with its retained font, painted at the host slot. Text animators on the
//! inner layer stay unavailable, as inner Motion does.
use crate::object_frame_packet::{ObjectFramePacket, ObjectLayer};
use crate::object_render_scene::text_scene::{unavailable, TextSources};
use crate::object_render_scene::ObjectSceneError;

/// Append the held block of raster-table-relative text entry `entry` to `sources`; its index.
#[allow(clippy::too_many_arguments)]
pub(crate) fn placed(
    packet: &ObjectFramePacket,
    layer: &ObjectLayer,
    entry: usize,
    source_index: usize,
    host: usize,
    chain: &[[f64; 6]],
    item_opacity: f64,
    sources: &mut TextSources,
) -> Result<usize, ObjectSceneError> {
    let text = packet
        .snapshot()
        .document()
        .item_records()
        .and_then(|items| items.text_entries().get(entry))
        .ok_or_else(|| unavailable(source_index, "component Text block is unavailable"))?;
    if layer.layer_uid() != text.target.layer_uid() {
        return Err(unavailable(
            source_index,
            "component Text layer is unavailable",
        ));
    }
    if layer
        .text_animators
        .value()
        .is_some_and(|animators| !animators.0.is_empty())
    {
        return Err(unavailable(
            source_index,
            "component Text animators are unavailable",
        ));
    }
    #[cfg(feature = "font_asset_admission")]
    {
        fonted(
            packet,
            text,
            source_index,
            host,
            chain,
            item_opacity,
            sources,
        )
    }
    #[cfg(not(feature = "font_asset_admission"))]
    {
        let _ = (host, chain, item_opacity, sources);
        Err(unavailable(
            source_index,
            "native font parser is unavailable",
        ))
    }
}

#[cfg(feature = "font_asset_admission")]
fn fonted(
    packet: &ObjectFramePacket,
    text: &crate::object_frame_packet::text::TextEntry,
    source_index: usize,
    host: usize,
    chain: &[[f64; 6]],
    item_opacity: f64,
    sources: &mut TextSources,
) -> Result<usize, ObjectSceneError> {
    use crate::object_render_scene::object_scene_producer_text::{
        prepare_block, TextBlockTarget, TextFontProvider,
    };
    use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
        SceneResourceEntry, SceneResourceKind,
    };
    use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::GeometryAdmission;
    use crate::object_render_scene::text_scene::{fonts, TextSource};
    use crate::object_render_scene::ObjectSceneResourceSet;
    use std::sync::Arc;
    let mut bundled = fonts::BundledInputs::default();
    bundled.include(&text.block, source_index)?;
    let font = fonts::prepare(packet.snapshot(), &text.block, &bundled, source_index)?;
    let version = font.resource.version();
    let holder = crate::object_frame_packet::text::text_holder_id(&text.target);
    let raw = prepare_block(
        Some(TextFontProvider {
            catalog: &font.faces,
            outlines: &fonts::Outlines(&font.faces),
            resource_version: &version,
        }),
        &TextBlockTarget {
            source_object_index: source_index,
            holder_id: &holder,
            record: &text.block,
            glyph_stages: &[],
            chain: &[],
            item_opacity,
            admission: GeometryAdmission::DropCoincidentCurves,
        },
    )
    .map_err(|_| unavailable(source_index, "component Text layout is unavailable"))?;
    let id = format!("font:{}", font.key);
    if raw.font_resource() != id {
        return Err(unavailable(
            source_index,
            "Text resolved font identity mismatch",
        ));
    }
    let item = raw
        .placed_by(chain, GeometryAdmission::DropCoincidentCurves)
        .map_err(|error| unavailable(source_index, error.reason()))?;
    match sources.resources.get(&id) {
        Some(held) if held.version() != version => {
            return Err(unavailable(
                source_index,
                "Text font allocation identity mismatch",
            ))
        }
        Some(_) => {}
        None => {
            let mut entries: Vec<_> = sources
                .resources
                .entries()
                .iter()
                .map(|held| {
                    SceneResourceEntry::new(
                        held.resource_id(),
                        held.version(),
                        held.kind(),
                        held.charged_bytes(),
                        Arc::clone(held.retention()),
                    )
                })
                .collect();
            let retention: Arc<dyn std::any::Any + Send + Sync> = font.resource.clone();
            entries.push(SceneResourceEntry::new(
                &id,
                &version,
                SceneResourceKind::Font,
                font.resource.bytes(),
                retention,
            ));
            sources.resources = Arc::new(
                ObjectSceneResourceSet::new(entries)
                    .map_err(|_| unavailable(0, "native Text resource declarations invalid"))?,
            );
        }
    }
    sources.entries.push(TextSource {
        source_index,
        layer_index: host,
        item,
        placed: true,
    });
    Ok(sources.entries.len() - 1)
}
