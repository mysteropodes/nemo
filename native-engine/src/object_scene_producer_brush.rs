//! Bitmap-brush scene producer (L07-J; G17): anchor plus ONE companion bitmap per stroke,
//! reported to the pinned scene registry of L07-C.
//!
//! A brush stroke is one selected record of the admitted `ObjectFramePacket` (the anchor
//! path, identified by its stable native `(layer_uid, stroke_id)` target). For every such
//! record the producer reports ONE `BrushStrokeScene`, a single claim of the record's
//! `source_object_index`:
//! * `anchor`: read from the immutable record only (its fill and its vector body's stroke
//!   and opacity, i.e. what baseline `serP` stored after the camouflage; L07-J anchor
//!   module). The source keeps just the remembered `preTexture*` values: they are never
//!   drawn, and a record that contradicts them is a typed unavailable entry;
//! * `ink`: the companion bitmap, drawn above the anchor (`raster.insertAbove(basePath)`),
//!   leased from the charged `BrushBitmapStore` for the lifetime of the scene payload
//!   (an `Arc` alias pins it against eviction and keeps its quota charged), or the
//!   explicit `Collapsed` state of a trim window that left nothing to draw;
//! * with a trim window the ink comes from the anchor's trim slot (re-bake along the
//!   trimmed centerline, one slot per anchor). The centerline is the PINNED record's own
//!   geometry (`TrimAnchor`, converted like the contour producer reads it; anything but a
//!   finite `document-world` relative cubic path is `anchor-geometry-unrepresentable`); the
//!   provider trims exactly that, and the slot fingerprints the geometry and spec so an
//!   edited anchor never reuses ink baked from another one. Every re-bake is validated
//!   against the L07-E stamp plan before it is published, so a baker that disagrees with
//!   the immutable spec/path cannot publish. The trim slot is resolved BEFORE the untrimmed companion is looked
//!   at, exactly as the baseline trimmed path never touches the untrimmed raster: a
//!   companion that could not be published only matters when no trim window is active.
//! The `(resource id, version)` of every leased bitmap is declared on the output, so a
//! regenerated or re-baked companion changes the scheduler key while a scene built earlier
//! keeps its own immutable pixels.
//!
//! Nothing is dropped silently: a record whose anchor, ink or trim cannot be produced is a
//! typed `ProducerEntry::Unavailable`; a source naming no selected record is listed in
//! `unmatched`. Structural source errors (duplicate target or group, empty ids) refuse the
//! whole production before any lease or bake. The brush producer CLAIMS the anchor
//! records it has sources for: a record-level producer (the contour producer) must not
//! claim them again (`claimed_indices`), the join reports a double claim as
//! `DuplicateClaim`.
//!
//! The brush-only fields (`bitmapBrushSpec`, `brushGroupId`, `preTexture*`, the companion
//! raster) are not part of the native record yet (no provider: G14 carries pressure fields
//! only), so they arrive as typed `BrushStrokeSource`s. Paint (fill, stroke, opacity) is
//! never part of a source: the record is its only authority. The frame
//! lifecycle belongs to the caller: `store.begin_build()` before and `end_build(retire)`
//! after the frame, exactly as the baseline's `beginBuild`/`endBuild`.

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_stamp::{
    plan_stamps, StampPath, StampSpec, WorldRect,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::{
    BrushBitmap, BrushBitmapStore, BrushStoreError, RetirePort,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_anchor::{
    anchor_draw, AnchorDraw, AnchorError, AnchorRestore,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
    anchor_key, BakeRefused, BakedInk, Resolution, SlotError, SlotRequest, TrimAnchor,
    TrimProvider, TrimSlots, TrimWindow, TrimmedPath,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerEntry, ProducerOutput, ScenePin,
};
use crate::object_document::ObjectRecord;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord};
use crate::revision::ImageResourceError;
use std::collections::{BTreeMap, BTreeSet};

/// Producer name in the pinned registry.
pub(crate) const PRODUCER: &str = "brush";

/// The untrimmed companion: the store id it was published under and the world rectangle
/// its pixels cover (the raster's position and size).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InkRef {
    pub(crate) brush_id: String,
    pub(crate) rect: WorldRect,
}

/// Where the untrimmed companion comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InkSource {
    /// Published under `brush_id`; a missing bitmap is a typed unavailable entry.
    Resident(InkRef),
    /// The caller could not publish it (missing asset, over budget, ...): the code is
    /// carried to the unavailable entry so the cause is never lost.
    Refused(&'static str),
}

/// The stroke's `bitmapBrushSpec` and trim state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrimSetup {
    pub(crate) spec: StampSpec,
    /// `data.bitmapPressureProfile`.
    pub(crate) pressure_profile: Option<Vec<[f64; 3]>>,
    /// `SMMotion.trimWindowAt`; `None` is no trim on this stroke.
    pub(crate) window: Option<TrimWindow>,
}

impl TrimSetup {
    /// The exact bits of everything besides the anchor geometry a bake reads: the slot's
    /// fingerprint, so a changed spec or pressure profile never reuses stale ink.
    fn bits(&self) -> Vec<u64> {
        let spec = &self.spec;
        let mut bits: Vec<u64> = [
            spec.size,
            spec.spacing,
            spec.scatter,
            spec.seed,
            spec.opacity,
        ]
        .iter()
        .map(|n| n.to_bits())
        .collect();
        bits.push(u64::from(spec.pressure));
        match &self.pressure_profile {
            None => bits.push(0),
            Some(profile) => {
                bits.extend([1, profile.len() as u64]);
                bits.extend(profile.iter().flatten().map(|n| n.to_bits()));
            }
        }
        bits
    }
}

/// The persisted brush members of one anchor, keyed by its stable native target.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrushStrokeSource {
    pub(crate) layer_uid: String,
    pub(crate) stroke_id: String,
    /// `data.brushGroupId`: binds the anchor to its one companion.
    pub(crate) group_id: String,
    /// `data.preTextureStroke` / `data.preTextureOpacity`: never drawn, checked against the record.
    pub(crate) restore: AnchorRestore,
    pub(crate) ink: InkSource,
    /// The companion raster's `opacity`.
    pub(crate) ink_opacity: f64,
    /// `None`: the stroke has no `bitmapBrushSpec`, so it can never be trimmed.
    pub(crate) trim: Option<TrimSetup>,
}

#[derive(Debug, Clone)]
pub(crate) struct BrushInk {
    pub(crate) bitmap: BrushBitmap,
    pub(crate) rect: WorldRect,
    pub(crate) opacity: f64,
}

#[derive(Debug, Clone)]
pub(crate) enum InkState {
    Ink(BrushInk),
    /// The trim window collapsed: the anchor is drawn, no ink is.
    Collapsed,
}

/// The payload of one claimed record.
#[derive(Debug, Clone)]
pub(crate) struct BrushStrokeScene {
    pub(crate) layer_uid: String,
    pub(crate) stroke_id: String,
    pub(crate) group_id: String,
    pub(crate) anchor: AnchorDraw,
    pub(crate) ink: InkState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrushUnavailable {
    Anchor(AnchorError),
    InkOpacityNotFinite,
    InkRefused(&'static str),
    InkMissing {
        brush_id: String,
    },
    /// The record's centerline is not a finite document-world relative cubic path.
    AnchorGeometry(&'static str),
    Trim(SlotError),
}

impl BrushUnavailable {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Anchor(AnchorError::Fill(_)) => "anchor-fill-unrepresentable",
            Self::Anchor(AnchorError::OpacityNotFinite) => "anchor-opacity-not-finite",
            Self::Anchor(AnchorError::CamouflageDisagrees(_)) => {
                "anchor-camouflage-disagrees-with-record"
            }
            Self::InkOpacityNotFinite => "ink-opacity-not-finite",
            Self::InkRefused(_) => "ink-refused",
            Self::InkMissing { .. } => "ink-missing",
            Self::AnchorGeometry(_) => "anchor-geometry-unrepresentable",
            Self::Trim(SlotError::WindowNotFinite) => "trim-window-not-finite",
            Self::Trim(SlotError::Bake(_)) => "trim-bake-refused",
            Self::Trim(SlotError::Store(BrushStoreError::Budget(ImageResourceError::Capacity))) => {
                "trim-over-budget"
            }
            Self::Trim(SlotError::Store(_)) => "trim-store-refused",
        }
    }

    /// The text carried by the registry's `Unavailable` entry.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Anchor(AnchorError::CamouflageDisagrees(why))
            | Self::InkRefused(why)
            | Self::AnchorGeometry(why) => {
                format!("{}: {why}", self.code())
            }
            Self::InkMissing { brush_id } => format!("{}: '{brush_id}'", self.code()),
            Self::Trim(SlotError::Bake(BakeRefused(cause))) => {
                format!("{}: {cause}", self.code())
            }
            _ => self.code().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrushProduceError {
    /// Empty group id or empty resident brush id.
    InvalidSource {
        index: usize,
    },
    DuplicateTarget {
        layer_uid: String,
        stroke_id: String,
    },
    DuplicateGroup {
        group_id: String,
    },
}

/// The long-lived state one frame build shares: the one charged store, the per-anchor
/// trim slots, the trim/bake collaborators and the optional retire port of the store.
pub(crate) struct BrushContext<'a> {
    pub(crate) store: &'a mut BrushBitmapStore,
    pub(crate) slots: &'a mut TrimSlots,
    pub(crate) provider: &'a mut dyn TrimProvider,
    pub(crate) retire: Option<&'a mut dyn FnMut(&[String]) -> bool>,
}

pub(crate) struct BrushProduction {
    pub(crate) output: ProducerOutput<BrushStrokeScene>,
    /// `(layer_uid, stroke_id)` of sources that name no selected record of the packet
    /// (for instance an anchor on an invisible layer), in source order.
    pub(crate) unmatched: Vec<(String, String)>,
}

impl BrushProduction {
    /// Every `source_object_index` this producer claimed (item or unavailable), in
    /// provenance order: a record-level producer must leave exactly these alone.
    pub(crate) fn claimed_indices(&self) -> Vec<usize> {
        self.output
            .entries
            .iter()
            .map(|entry| match entry {
                ProducerEntry::Item {
                    source_object_index,
                    ..
                }
                | ProducerEntry::Unavailable {
                    source_object_index,
                    ..
                } => *source_object_index,
            })
            .collect()
    }
}

/// Re-bakes are checked against the L07-E plan of the immutable spec and trimmed path.
struct PlanChecked<'a> {
    inner: &'a mut dyn TrimProvider,
    spec: StampSpec,
    pressure_profile: Option<&'a [[f64; 3]]>,
}

impl TrimProvider for PlanChecked<'_> {
    fn trimmed(&mut self, anchor: &TrimAnchor, window: &TrimWindow) -> Option<TrimmedPath> {
        self.inner.trimmed(anchor, window)
    }

    fn bake(&mut self, path: &TrimmedPath) -> Result<Option<BakedInk>, BakeRefused> {
        let baked = self.inner.bake(path)?;
        if let Some(ink) = &baked {
            let plan = plan_stamps(
                &self.spec,
                &StampPath {
                    segments: &path.segments,
                    closed: path.closed,
                    pressure_profile: self.pressure_profile,
                },
            )
            .map_err(|_| BakeRefused("trim-plan-unavailable"))?;
            plan.validate_published(ink.width, ink.height, ink.rect)
                .map_err(|_| BakeRefused("trim-bake-disagrees-with-plan"))?;
        }
        Ok(baked)
    }
}

fn validate(sources: &[BrushStrokeSource]) -> Result<(), BrushProduceError> {
    let mut targets = BTreeSet::new();
    let mut groups = BTreeSet::new();
    for (index, source) in sources.iter().enumerate() {
        let empty_brush =
            matches!(&source.ink, InkSource::Resident(ink) if ink.brush_id.is_empty());
        if source.group_id.is_empty() || empty_brush {
            return Err(BrushProduceError::InvalidSource { index });
        }
        let target = (source.layer_uid.clone(), source.stroke_id.clone());
        if !targets.insert(target) {
            return Err(BrushProduceError::DuplicateTarget {
                layer_uid: source.layer_uid.clone(),
                stroke_id: source.stroke_id.clone(),
            });
        }
        if !groups.insert(source.group_id.clone()) {
            return Err(BrushProduceError::DuplicateGroup {
                group_id: source.group_id.clone(),
            });
        }
    }
    Ok(())
}

/// The untrimmed companion: a lease of the charged bitmap, or the typed cause it is missing.
fn untrimmed(
    source: &BrushStrokeSource,
    ctx: &mut BrushContext<'_>,
) -> Result<InkState, BrushUnavailable> {
    let ink = match &source.ink {
        InkSource::Resident(ink) => ink,
        InkSource::Refused(cause) => return Err(BrushUnavailable::InkRefused(*cause)),
    };
    let bitmap = ctx
        .store
        .lease(&ink.brush_id)
        .ok_or_else(|| BrushUnavailable::InkMissing {
            brush_id: ink.brush_id.clone(),
        })?;
    Ok(InkState::Ink(BrushInk {
        bitmap,
        rect: ink.rect,
        opacity: source.ink_opacity,
    }))
}

/// The trim slot first (the baseline trimmed path never reads the untrimmed raster); the
/// untrimmed companion, with its typed cause, only when no trim window applies. The trimmed
/// centerline is converted from the PINNED record, and only when a window is active.
fn resolve_ink(
    packet: &ObjectFramePacket,
    stored: &ObjectRecord,
    source: &BrushStrokeSource,
    ctx: &mut BrushContext<'_>,
) -> Result<InkState, BrushUnavailable> {
    let Some(setup) = source.trim.as_ref().filter(|setup| setup.window.is_some()) else {
        return untrimmed(source, ctx);
    };
    let geometry = TrimAnchor::from_record(stored).map_err(BrushUnavailable::AnchorGeometry)?;
    let anchor_key = anchor_key(
        packet.instance_id(),
        packet.context_id(),
        &source.layer_uid,
        &source.stroke_id,
    );
    let spec_bits = setup.bits();
    let request = SlotRequest {
        anchor_key: &anchor_key,
        stroke_id: &source.stroke_id,
        has_spec: true,
        window: setup.window,
        anchor: &geometry,
        spec_bits: &spec_bits,
    };
    let mut checked = PlanChecked {
        inner: &mut *ctx.provider,
        spec: setup.spec,
        pressure_profile: setup.pressure_profile.as_deref(),
    };
    // Forward to the frame's retire port (if any) without naming trait-object lifetimes.
    let has_port = ctx.retire.is_some();
    let mut forward = |ids: &[String]| match ctx.retire.as_mut() {
        Some(port) => (**port)(ids),
        None => false,
    };
    let port: RetirePort<'_> = if has_port { Some(&mut forward) } else { None };
    let resolution = ctx
        .slots
        .resolve(&mut *ctx.store, &request, &mut checked, port)
        .map_err(BrushUnavailable::Trim)?;
    match resolution {
        Resolution::NoTrim => untrimmed(source, ctx),
        Resolution::Collapsed { .. } => Ok(InkState::Collapsed),
        Resolution::Ink { bitmap, rect, .. } => Ok(InkState::Ink(BrushInk {
            bitmap,
            rect,
            opacity: source.ink_opacity,
        })),
    }
}

fn build(
    packet: &ObjectFramePacket,
    record: &ObjectFrameRecord<'_>,
    source: &BrushStrokeSource,
    ctx: &mut BrushContext<'_>,
) -> Result<BrushStrokeScene, BrushUnavailable> {
    let stored = record.record();
    let vector =
        stored
            .vector()
            .ok_or(BrushUnavailable::Anchor(AnchorError::CamouflageDisagrees(
                "record-has-no-vector-body",
            )))?;
    let anchor = anchor_draw(
        stored.solid_fill(),
        vector.solid_stroke(),
        &vector.opacity,
        &source.restore,
    )
    .map_err(BrushUnavailable::Anchor)?;
    if !source.ink_opacity.is_finite() {
        return Err(BrushUnavailable::InkOpacityNotFinite);
    }
    let ink = resolve_ink(packet, stored, source, ctx)?;
    Ok(BrushStrokeScene {
        layer_uid: source.layer_uid.clone(),
        stroke_id: source.stroke_id.clone(),
        group_id: source.group_id.clone(),
        anchor,
        ink,
    })
}

/// Produce the brush entries of one pinned packet.
pub(crate) fn produce(
    packet: &ObjectFramePacket,
    sources: &[BrushStrokeSource],
    ctx: &mut BrushContext<'_>,
) -> Result<BrushProduction, BrushProduceError> {
    validate(sources)?;
    let by_target: BTreeMap<(String, String), usize> = sources
        .iter()
        .enumerate()
        .map(|(at, source)| ((source.layer_uid.clone(), source.stroke_id.clone()), at))
        .collect();
    let mut output = ProducerOutput::new(PRODUCER, ScenePin::of_packet(packet));
    let mut matched = BTreeSet::new();
    for record in packet.records() {
        let target = record.record().target();
        let key = (target.layer_uid().to_owned(), target.stroke_id().to_owned());
        let Some(&at) = by_target.get(&key) else {
            continue;
        };
        matched.insert(at);
        let index = record.source_object_index();
        match build(packet, &record, &sources[at], ctx) {
            Ok(scene) => {
                if let InkState::Ink(ink) = &scene.ink {
                    let (id, version) = ink.bitmap.declared_resource();
                    output = output.with_resource(id, version);
                }
                output = output.with_item(index, scene);
            }
            Err(reason) => output = output.with_unavailable(index, reason.reason()),
        }
    }
    let unmatched = sources
        .iter()
        .enumerate()
        .filter(|(at, _)| !matched.contains(at))
        .map(|(_, source)| (source.layer_uid.clone(), source.stroke_id.clone()))
        .collect();
    Ok(BrushProduction { output, unmatched })
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_record_controls.rs"]
mod record_controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_rows.rs"]
pub(crate) mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_brush.rs"]
mod tests;
