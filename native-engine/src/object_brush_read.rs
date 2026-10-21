//! Native brush source binding and charged pixel publication from immutable reads.
use crate::evaluation::object_scene_style_kernels::{
    bitmap_brush_resources_stamp::{StampSpec, WorldRect},
    bitmap_brush_resources_store::{BrushBitmapStore, RetirePort},
    object_scene_producer_brush::{BrushStrokeSource, InkRef, InkSource, InkState, TrimSetup},
    object_scene_producer_brush_anchor::{AnchorRestore, Remembered},
    object_scene_producer_brush_slot::{anchor_key, TrimSlots, TrimWindow},
    object_scene_producer_pin::ProducerEntry,
};
use crate::object_codec::ImageAssetLimits;
use crate::object_frame_packet::{
    sample_property_raw, BitmapBrushFields, ObjectFramePacket, Presence, PropertyHolderTarget,
};
use crate::read_queries::{BrushAssetPublisher, BrushAssetRequest};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BrushReadError {
    pub(crate) source_index: usize,
    pub(crate) reason: &'static str,
}
fn unavailable(source_index: usize, reason: &'static str) -> BrushReadError {
    BrushReadError {
        source_index,
        reason,
    }
}
/// This read-admission envelope is validated here and consumed only by this
/// producer's charged PNG publication; rendering never receives codec options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BrushReadLimits(ImageAssetLimits);
impl BrushReadLimits {
    pub(crate) fn new(
        encoded: usize,
        rgba: usize,
        decoder: usize,
        width: u32,
        height: u32,
    ) -> Result<Self, BrushReadError> {
        ImageAssetLimits::new(encoded, rgba, decoder, width, height)
            .map(Self)
            .map_err(|_| unavailable(0, "native brush PNG bounds unavailable"))
    }
    pub(crate) fn operating() -> Result<Self, BrushReadError> {
        Self::new(16 << 20, 256 << 20, 64 << 20, 8192, 8192)
    }
}
/// Constructed once from the existing shared quota/store; no second authority.
#[derive(Debug)]
pub(crate) struct NativeBrushProducer {
    publisher: BrushAssetPublisher,
    slots: TrimSlots,
    guarded: bool,
}
impl NativeBrushProducer {
    pub(crate) fn new(store: BrushBitmapStore) -> Self {
        Self {
            publisher: BrushAssetPublisher::new(store),
            slots: TrimSlots::new(),
            guarded: true,
        }
    }
    #[cfg(test)]
    pub(crate) fn resident_count(&self) -> usize {
        self.publisher.store().stats().count
    }
    #[cfg(test)]
    pub(crate) fn for_controls(store: BrushBitmapStore) -> Self {
        Self {
            guarded: false,
            ..Self::new(store)
        }
    }
}
#[derive(Debug)]
pub(crate) struct ProducedBrushPaint {
    pub(crate) source_index: usize,
    pub(crate) layer_index: usize,
    pub(crate) ink: InkState,
}
fn profile(fields: &BitmapBrushFields) -> Option<Vec<[f64; 3]>> {
    fields.bitmap_pressure_profile.value().map(|rows| {
        rows.iter()
            .map(|p| {
                [
                    p[0].as_f64().unwrap(),
                    p[1].as_f64().unwrap(),
                    p[2].as_f64().unwrap(),
                ]
            })
            .collect()
    })
}
fn window(
    packet: &ObjectFramePacket,
    layer: &str,
    stroke: &str,
) -> Result<Option<TrimWindow>, BrushReadError> {
    let target = PropertyHolderTarget {
        context_id: packet.context_id().into(),
        layer_uid: layer.into(),
        stroke_id: Presence::Present(stroke.into()),
    };
    let holder = packet
        .snapshot()
        .document()
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .find(|h| h.target == target);
    // No authored trim property means no trim, rather than inventing one from
    // the builtin defaults. Expressions must use the real evaluated provider.
    if !holder.is_some_and(|h| {
        h.properties
            .iter()
            .any(|p| ["trimStart", "trimEnd", "trimOffset"].contains(&p.property.as_str()))
    }) {
        return Ok(None);
    }
    if holder.is_some_and(|h| {
        h.properties.iter().any(|p| {
            ["trimStart", "trimEnd", "trimOffset"].contains(&p.property.as_str())
                && p.expression.value().is_some_and(|e| e.enabled)
        })
    }) {
        return Err(unavailable(
            0,
            "native brush trim expression evaluation is unavailable",
        ));
    }
    let sample = |property| {
        sample_property_raw(
            packet.snapshot(),
            &target,
            property,
            f64::from(packet.frame()),
        )
        .ok()
        .filter(|v| v.len() == 1 && v[0].is_finite())
        .map(|v| v[0])
        .ok_or_else(|| unavailable(0, "native brush trim property evaluation is unavailable"))
    };
    Ok(Some(TrimWindow {
        start: sample("trimStart")?,
        end: sample("trimEnd")?,
        offset: sample("trimOffset")?,
    }))
}
fn source(
    fields: &BitmapBrushFields,
    layer: &str,
    stroke: &str,
    id: String,
    rect: WorldRect,
    opacity: f64,
    win: Option<TrimWindow>,
) -> BrushStrokeSource {
    let spec = &fields.bitmap_brush_spec;
    let restore = AnchorRestore {
        pre_stroke: match &fields.pre_texture_stroke {
            Presence::Missing => Remembered::Unset,
            Presence::Present(value) => Remembered::Set(value.clone()),
        },
        pre_opacity: match &fields.pre_texture_opacity {
            Presence::Missing => Remembered::Unset,
            Presence::Present(value) => Remembered::Set(value.as_f64().unwrap()),
        },
    };
    BrushStrokeSource {
        layer_uid: layer.into(),
        stroke_id: stroke.into(),
        group_id: fields.brush_group_id.clone(),
        restore,
        ink: InkSource::Resident(InkRef { brush_id: id, rect }),
        ink_opacity: opacity,
        trim: Some(TrimSetup {
            spec: StampSpec {
                size: spec.size.as_f64().unwrap(),
                spacing: spec.spacing.as_f64().unwrap(),
                scatter: spec.scatter.as_f64().unwrap(),
                seed: spec.seed.as_f64().unwrap(),
                opacity: spec.opacity.as_f64().unwrap(),
                pressure: spec.pressure,
            },
            pressure_profile: profile(fields),
            window: win,
        }),
    }
}
type Binding<'a> = (
    crate::object_frame_packet::ObjectFrameRecord<'a>,
    &'a BitmapBrushFields,
    crate::object_frame_packet::ObjectFrameRasterRecord<'a>,
    Option<TrimWindow>,
);
fn bindings(packet: &ObjectFramePacket) -> Result<Vec<Binding<'_>>, BrushReadError> {
    let rasters: Vec<_> = packet.raster_records().collect();
    let mut bindings = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut groups = BTreeSet::new();
    // Resolve all identities before publication. The RasterItem remains the
    // sole asset/rectangle/opacity authority; a second match is not guessed.
    for record in packet.records() {
        let stored = record.record();
        let Some(fields) = stored
            .vector_ext
            .value()
            .and_then(|v| v.bitmap_brush.value())
        else {
            continue;
        };
        if crate::object_codec::object_vector_ext_codec::bitmap_brush::validate(stored).is_err() {
            return Err(unavailable(
                record.source_object_index(),
                "stored bitmap anchor is invalid",
            ));
        }
        let target = stored.target();
        let key = (
            target.context_id.clone(),
            target.layer_uid.clone(),
            target.frame_scope.clone(),
            fields.brush_group_id.clone(),
        );
        if !groups.insert((key.0.clone(), key.1.clone(), key.2.frame, key.3.clone())) {
            return Err(unavailable(
                record.source_object_index(),
                "multiple bitmap anchors share a companion group",
            ));
        }
        let matches: Vec<_> = rasters
            .iter()
            .filter(|r| {
                let entry = r.entry();
                let t = &entry.target;
                t.context_id == key.0
                    && t.layer_uid == key.1
                    && t.frame_scope == key.2
                    && entry.item.brush_group_id() == Some(key.3.as_str())
            })
            .collect();
        if matches.len() != 1 {
            return Err(unavailable(
                record.source_object_index(),
                "bitmap anchor requires exactly one native companion",
            ));
        }
        let companion = matches[0];
        let item = &companion.entry().item;
        if packet.source_layers()[companion.layer_index()]
            .path_fx()
            .is_some_and(|fx| !fx.is_empty())
        {
            return Err(unavailable(
                companion.source_index(),
                "native bitmap layer path effects are unavailable",
            ));
        }
        if item.is_bitmap_brush() != Some(true)
            || item.is_brush_texture_copy() != Some(true)
            || item.mesh_id().is_some()
            || item.rotation() != 0.
            || !claimed.insert(companion.source_index())
        {
            return Err(unavailable(
                companion.source_index(),
                "bitmap companion flags or placement unavailable",
            ));
        }
        let win = window(packet, &target.layer_uid, &target.stroke_id)?;
        if win.is_none() {
            crate::object_snapshot::object_raster_asset_resources::AssetPixels::version_for(
                packet.snapshot(),
                item.asset_id().as_str(),
            )
            .map_err(|_| {
                unavailable(
                    companion.source_index(),
                    "native bitmap encoded source unavailable",
                )
            })?;
        }
        bindings.push((record, fields, *companion, win));
    }
    if rasters.iter().any(|r| {
        let item = &r.entry().item;
        (item.is_bitmap_brush() == Some(true)
            || item.is_brush_texture_copy() == Some(true)
            || item.brush_group_id().is_some())
            && !claimed.contains(&r.source_index())
    }) {
        return Err(unavailable(
            0,
            "unpaired native bitmap companion is unavailable",
        ));
    }
    Ok(bindings)
}
pub(crate) fn admitted_indices(
    packet: &ObjectFramePacket,
) -> Result<Vec<(usize, usize)>, BrushReadError> {
    Ok(bindings(packet)?
        .iter()
        .map(|(_, _, r, _)| (r.source_index(), r.layer_index()))
        .collect())
}

pub(crate) fn validate_raster_source(
    packet: &ObjectFramePacket,
    asset_id: &str,
) -> Result<(), BrushReadError> {
    crate::object_snapshot::object_raster_asset_resources::AssetPixels::version_for(
        packet.snapshot(),
        asset_id,
    )
    .map(|_| ())
    .map_err(|_| unavailable(0, "native raster/brush encoded source unavailable"))
}
pub(crate) fn produce(
    packet: &ObjectFramePacket,
    runtime: &mut NativeBrushProducer,
    limits: BrushReadLimits,
    mut retire: RetirePort<'_>,
) -> Result<Vec<ProducedBrushPaint>, BrushReadError> {
    let bindings = bindings(packet)?;
    let guarded = runtime.guarded;
    let NativeBrushProducer {
        publisher, slots, ..
    } = runtime;
    publisher.store_mut().begin_build();
    let result = (|| {
        let mut paints = Vec::new();
        // Keep hidden/other-frame live anchors; only deleted authored identities
        // retire their one trim slot. The host owns document/lifecycle retirement.
        let live: BTreeSet<_> = packet
            .snapshot()
            .document()
            .objects()
            .iter()
            .filter(|r| {
                r.vector_ext
                    .value()
                    .and_then(|v| v.bitmap_brush.value())
                    .is_some()
            })
            .map(|r| {
                anchor_key(
                    packet.instance_id(),
                    &r.target.context_id,
                    &r.target.layer_uid,
                    &r.target.stroke_id,
                )
            })
            .collect();
        for (record, fields, companion, win) in bindings {
            let target = record.record().target();
            let item = &companion.entry().item;
            let anchor = anchor_key(
                packet.instance_id(),
                packet.context_id(),
                &target.layer_uid,
                &target.stroke_id,
            );
            let brush_id = format!("{anchor}|resident");
            // A trim path is resolved before resident PNG publication, as in
            // engine-bridge: a missing untrimmed PNG must not block valid trim.
            if win.is_none() {
                if guarded
                    && publisher
                        .published_from(packet.snapshot(), &brush_id, item.asset_id().as_str())
                        .is_none()
                {
                    if !retire
                        .as_mut()
                        .is_some_and(|port| port(std::slice::from_ref(&brush_id)))
                    {
                        return Err(unavailable(
                            companion.source_index(),
                            "native bitmap atlas retirement unavailable",
                        ));
                    }
                }
                let has_retire = retire.is_some();
                let mut forward = |ids: &[String]| retire.as_mut().is_some_and(|p| p(ids));
                let port: RetirePort<'_> = if has_retire { Some(&mut forward) } else { None };
                publisher
                    .publish(
                        packet.snapshot(),
                        BrushAssetRequest {
                            asset_id: item.asset_id().as_str(),
                            brush_id: &brush_id,
                            limits: limits.0,
                            expected_size: None,
                        },
                        port,
                    )
                    .map_err(|_| {
                        unavailable(
                            companion.source_index(),
                            "native bitmap PNG publication unavailable",
                        )
                    })?;
            }
            let rect = item.display_rect();
            let desc = source(
                fields,
                &target.layer_uid,
                &target.stroke_id,
                brush_id,
                WorldRect {
                    min_x: rect.x,
                    min_y: rect.y,
                    w: rect.width,
                    h: rect.height,
                },
                item.opacity()
                    * packet.layer_opacity(companion.layer_index()).map_err(|_| {
                        unavailable(
                            companion.source_index(),
                            "native brush layer opacity unavailable",
                        )
                    })?,
                win,
            );
            let has_retire = retire.is_some();
            let mut forward = |ids: &[String]| retire.as_mut().is_some_and(|p| p(ids));
            let port: RetirePort<'_> = if has_retire { Some(&mut forward) } else { None };
            let mut output = crate::evaluation::brush_trim_bake::produce_native_guarded(
                packet,
                &desc,
                fields,
                publisher.store_mut(),
                slots,
                port,
                guarded,
            )
            .map_err(|_| {
                unavailable(
                    record.source_object_index(),
                    "native bitmap source production unavailable",
                )
            })?
            .output;
            let entry = output.entries.pop().ok_or_else(|| {
                unavailable(
                    record.source_object_index(),
                    "native bitmap source was not selected",
                )
            })?;
            let ProducerEntry::Item { payload, .. } = entry else {
                return Err(unavailable(
                    record.source_object_index(),
                    "native bitmap ink or trim unavailable",
                ));
            };
            paints.push(ProducedBrushPaint {
                source_index: companion.source_index(),
                layer_index: companion.layer_index(),
                ink: payload.ink,
            });
        }
        slots.retain_live(&live);
        Ok(paints)
    })();
    // Both successful and refused preparations close the original build scope.
    publisher.store_mut().end_build(retire);
    result
}
