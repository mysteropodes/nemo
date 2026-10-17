//! Bitmap-brush trim re-bake slot (L07-J; G17).
//!
//! Baseline `bitmapTrimmedImage` (`src/js/engine-bridge.js` at fbe4b8a3): a trimmed
//! bitmap-brush stroke cannot be trimmed per dab, so the ANCHOR's centerline is trimmed
//! and the texture is re-baked along just that window. A bake is a full stamp pass, so the
//! result is cached ONE entry per anchor, keyed by
//! `anchorStrokeId|round(start*10)|round(end*10)|round((offset||0)*10)` (windows are
//! rounded to 0.1%). The rules mirrored here, each pinned by the verbatim-function oracle:
//! * no `bitmapBrushSpec`, or no trim window, is `undefined` (`NoTrim`): the caller keeps the
//!   normal untrimmed ink, and the slot is left untouched (a later identical window hits);
//! * a window with the cached key reuses the cached value without trimming again, `null`
//!   (a collapsed window) included; a hit that has ink counts as a use of its image;
//! * a miss trims the anchor, bakes only when the trimmed path has at least two segments,
//!   and caches `null` when it does not or when the bake is `null`;
//! * the cache entry is replaced only after the bake: one slot per anchor, never growing.
//!
//! Native tightening of the cache key (the baseline hole: editing the anchor path or the spec
//! under an unchanged window reuses the stale ink). The trim input is the PINNED record's
//! centerline (`TrimAnchor`, handed to the provider as `SMMotion.applyTrimFor(segments,
//! closed)` receives the live anchor), and each slot also stores a fingerprint of everything
//! the pixels were baked from: the anchor's segment and closed bits plus the caller's
//! `spec_bits` (spec and pressure profile). The baseline window key is unchanged (the oracle
//! rows pin it), but a fingerprint mismatch is a miss, so a scene pinned to one snapshot
//! never reuses ink baked from another geometry or spec.
//!
//! Native additions, because the ink is a charged bitmap and not an engine upload:
//! * the slot's image is ONE store id per anchor (`bmbtrim:<anchor key>`); the replacement is
//!   published first, atomically, through the store's replacement fence, so the previous
//!   version stays alive and charged for every scene still leasing it and is released by
//!   the last such holder. A collapse publishes nothing and leaves the previous version
//!   resident but unleased (evictable), exactly as the baseline leaves the old upload;
//! * a failed bake or publication (provider refusal, quota) changes nothing: the slot keeps
//!   its previous key, so the next resolve retries, and the error is typed;
//! * a hit whose image was evicted since is a miss, never an undrawn stale reference;
//! * a non-finite window is a typed refusal (the baseline would build a `NaN` key).

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_stamp::WorldRect;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::{
    BrushBitmap, BrushBitmapStore, BrushStoreError, RetirePort,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use crate::object_document::{ObjectPoint, ObjectRecord};
use serde_json::Number;
use std::collections::{BTreeMap, BTreeSet};

/// `SMMotion.trimWindowAt`: percentages as the baseline reads them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrimWindow {
    pub(crate) start: f64,
    pub(crate) end: f64,
    /// `win.offset || 0`: NaN reads as 0.
    pub(crate) offset: f64,
}

/// The anchor's centerline as the pinned record carries it (`basePath.segments` / `closed`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrimAnchor {
    pub(crate) segments: Vec<SegmentIn>,
    pub(crate) closed: bool,
}

impl TrimAnchor {
    /// The centerline of a pinned record, converted exactly like the contour producer reads
    /// it: a `cubic-path` in `document-world` space with `relative` handles and finite f64
    /// coordinates (open or closed: a brush stroke is usually open). Anything else is the
    /// cause text of `anchor-geometry-unrepresentable`; no coordinate is repaired or defaulted.
    pub(crate) fn from_record(record: &ObjectRecord) -> Result<Self, &'static str> {
        let geometry = &record.geometry;
        if geometry.kind != "cubic-path"
            || geometry.coordinate_space != "document-world"
            || geometry.handle_space != "relative"
        {
            return Err("not-a-document-world-relative-cubic-path");
        }
        let number = |value: &Number| {
            value
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("coordinate-not-finite")
        };
        let pair = |point: &ObjectPoint| -> Result<[f64; 2], &'static str> {
            Ok([number(&point.x)?, number(&point.y)?])
        };
        let segments = geometry
            .segments
            .iter()
            .map(|segment| {
                Ok(SegmentIn {
                    point: pair(&segment.point)?,
                    handle_in: Some(pair(&segment.handle_in)?),
                    handle_out: Some(pair(&segment.handle_out)?),
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(Self {
            segments,
            closed: geometry.closed,
        })
    }

    /// Every bit the trimmed path and its pixels depend on, as a comparable fingerprint.
    fn bits(&self) -> Vec<u64> {
        let handle = |h: Option<[f64; 2]>| match h {
            Some([x, y]) => [1, x.to_bits(), y.to_bits()],
            None => [0, 0, 0],
        };
        let mut bits = vec![self.segments.len() as u64, u64::from(self.closed)];
        for segment in &self.segments {
            bits.extend([segment.point[0].to_bits(), segment.point[1].to_bits()]);
            bits.extend(handle(segment.handle_in));
            bits.extend(handle(segment.handle_out));
        }
        bits
    }
}

/// `SMMotion.applyTrimFor`: the anchor's centerline under a window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrimmedPath {
    pub(crate) segments: Vec<SegmentIn>,
    pub(crate) closed: bool,
}

/// `SMBitmapBrush.bakeTrimmed`: straight RGBA8 pixels and the world rectangle they cover.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BakedInk {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
    pub(crate) rect: WorldRect,
}

/// A bake the provider or the plan check refused (typed reason, never a silent skip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BakeRefused(pub(crate) &'static str);

/// The two external collaborators of a re-bake: the trim math and the pixel baker.
pub(crate) trait TrimProvider {
    /// `None` is "no geometry" (the baseline `trimmed` is null or has no segments). The
    /// path is derived from the received anchor, never from provider-held geometry.
    fn trimmed(&mut self, anchor: &TrimAnchor, window: &TrimWindow) -> Option<TrimmedPath>;
    /// `Ok(None)` is a `null` bake.
    fn bake(&mut self, path: &TrimmedPath) -> Result<Option<BakedInk>, BakeRefused>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlotError {
    /// Start or end is NaN or infinite, or the rounded key does not fit a double exactly.
    WindowNotFinite,
    Bake(BakeRefused),
    Store(BrushStoreError),
}

/// What the caller draws for one anchor.
#[derive(Debug)]
pub(crate) enum Resolution {
    /// `undefined`: take the normal untrimmed ink.
    NoTrim,
    /// `null`: the window collapsed, draw no ink.
    Collapsed { reused: bool },
    Ink {
        bitmap: BrushBitmap,
        rect: WorldRect,
        reused: bool,
    },
}

#[derive(Debug, Clone)]
struct SlotInk {
    brush_id: String,
    rect: WorldRect,
}

#[derive(Debug, Clone)]
struct Slot {
    key: String,
    /// `TrimAnchor::bits` followed by the caller's `spec_bits`.
    fingerprint: Vec<u64>,
    ink: Option<SlotInk>,
}

/// The baseline switches, so the controls can mutate each one and prove a named test
/// notices; `BASELINE` is the only value production callers use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRules {
    /// `Math.round` (ties toward +infinity) rather than half-away-from-zero.
    pub(crate) js_rounding: bool,
    /// `trimmed.segments.length >= min_segments` before a bake is attempted.
    pub(crate) min_segments: usize,
    /// A window with the cached key reuses the cached value.
    pub(crate) cache_hits: bool,
    /// `win.offset || 0`: a NaN offset reads as 0.
    pub(crate) nan_offset_is_zero: bool,
}

impl SlotRules {
    pub(crate) const BASELINE: SlotRules = SlotRules {
        js_rounding: true,
        min_segments: 2,
        cache_hits: true,
        nan_offset_is_zero: true,
    };

    fn round(&self, x: f64) -> f64 {
        if self.js_rounding {
            js_round(x)
        } else {
            x.round()
        }
    }

    fn key_part(&self, value: f64) -> Result<i64, SlotError> {
        let rounded = self.round(value * 10.0);
        if !rounded.is_finite() || rounded.abs() > KEY_LIMIT {
            return Err(SlotError::WindowNotFinite);
        }
        Ok(rounded as i64)
    }

    /// `anchorStrokeId|round(start*10)|round(end*10)|round((offset||0)*10)`.
    pub(crate) fn window_key(
        &self,
        stroke_id: &str,
        window: &TrimWindow,
    ) -> Result<String, SlotError> {
        let offset = if self.nan_offset_is_zero && window.offset.is_nan() {
            0.0
        } else {
            window.offset
        };
        Ok(format!(
            "{stroke_id}|{}|{}|{}",
            self.key_part(window.start)?,
            self.key_part(window.end)?,
            self.key_part(offset)?
        ))
    }
}

/// One entry per anchor, living across frames (like `_bmbTrimCache`).
#[derive(Debug)]
pub(crate) struct TrimSlots {
    rules: SlotRules,
    slots: BTreeMap<String, Slot>,
}

/// Which anchor is being resolved and under which window.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlotRequest<'a> {
    /// Stable native identity of the anchor (context, layer, stroke); the slot and its
    /// store id derive from it.
    pub(crate) anchor_key: &'a str,
    /// The `anchorStrokeId` component of the baseline cache key.
    pub(crate) stroke_id: &'a str,
    /// The stroke carries a `bitmapBrushSpec`.
    pub(crate) has_spec: bool,
    /// The pinned record's centerline: what the provider trims and what the slot fingerprints.
    pub(crate) anchor: &'a TrimAnchor,
    /// The remaining bake inputs (spec and pressure profile) as exact bits; part of the
    /// fingerprint, never of the baseline window key.
    pub(crate) spec_bits: &'a [u64],
    /// `SMMotion.trimWindowAt`; `None` is no trim on this stroke.
    pub(crate) window: Option<TrimWindow>,
}

/// Largest rounded magnitude that still prints as the same integer in JS.
const KEY_LIMIT: f64 = 9_007_199_254_740_992.0;

/// `Math.round`: nearest integer, ties toward +infinity.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// The baseline window key.
pub(crate) fn window_key(stroke_id: &str, window: &TrimWindow) -> Result<String, SlotError> {
    SlotRules::BASELINE.window_key(stroke_id, window)
}

/// The slot key of one anchor: collision-free, because ids may contain `/` (e.g. a stroke id
/// `ink/path_A`). Every component is length-prefixed, so the key parses back uniquely and
/// `(layer "x", stroke "y/z")` never shares a slot or a store id with `(layer "x/y", stroke "z")`.
/// The snapshot instance leads the key: `BrushBitmapStore::shared()` is process-wide, so two open
/// snapshots with the same context, layer and stroke ids never share a slot or a store id.
pub(crate) fn anchor_key(
    instance_id: &str,
    context_id: &str,
    layer_uid: &str,
    stroke_id: &str,
) -> String {
    format!(
        "{}:{instance_id}/{}:{context_id}/{}:{layer_uid}/{}:{stroke_id}",
        instance_id.len(),
        context_id.len(),
        layer_uid.len(),
        stroke_id.len()
    )
}

/// The store id of an anchor's slot image.
pub(crate) fn slot_brush_id(anchor_key: &str) -> String {
    format!("bmbtrim:{anchor_key}")
}

impl TrimSlots {
    pub(crate) fn new() -> Self {
        Self::with_rules(SlotRules::BASELINE)
    }

    pub(crate) fn with_rules(rules: SlotRules) -> Self {
        Self {
            rules,
            slots: BTreeMap::new(),
        }
    }

    /// The cached window key of an anchor, if it has a slot.
    pub(crate) fn key_of(&self, anchor_key: &str) -> Option<&str> {
        self.slots.get(anchor_key).map(|slot| slot.key.as_str())
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// Forget the slots of anchors that no longer exist (the caller's set of live anchor keys).
    /// Their store images stay resident but unleased, so the store's LRU evicts them.
    pub(crate) fn retain_live(&mut self, live: &BTreeSet<String>) {
        self.slots.retain(|key, _| live.contains(key));
    }

    /// `bitmapTrimmedImage` for one anchor and one frame.
    pub(crate) fn resolve(
        &mut self,
        store: &mut BrushBitmapStore,
        request: &SlotRequest<'_>,
        provider: &mut dyn TrimProvider,
        retire: RetirePort<'_>,
    ) -> Result<Resolution, SlotError> {
        if !request.has_spec {
            return Ok(Resolution::NoTrim);
        }
        let Some(window) = request.window else {
            return Ok(Resolution::NoTrim);
        };
        let key = self.rules.window_key(request.stroke_id, &window)?;
        let mut fingerprint = request.anchor.bits();
        fingerprint.extend_from_slice(request.spec_bits);
        if self.rules.cache_hits {
            if let Some(hit) = self.reuse(store, request.anchor_key, &key, &fingerprint) {
                return Ok(hit);
            }
        }
        let trimmed = provider.trimmed(request.anchor, &window);
        let baked = match trimmed.filter(|path| path.segments.len() >= self.rules.min_segments) {
            Some(path) => provider.bake(&path).map_err(SlotError::Bake)?,
            None => None,
        };
        let Some(baked) = baked else {
            let empty = Slot {
                key,
                fingerprint,
                ink: None,
            };
            self.slots.insert(request.anchor_key.to_owned(), empty);
            return Ok(Resolution::Collapsed { reused: false });
        };
        let brush_id = slot_brush_id(request.anchor_key);
        let bitmap = store
            .publish(&brush_id, baked.width, baked.height, &baked.rgba, retire)
            .map_err(SlotError::Store)?;
        self.slots.insert(
            request.anchor_key.to_owned(),
            Slot {
                key,
                fingerprint,
                ink: Some(SlotInk {
                    brush_id,
                    rect: baked.rect,
                }),
            },
        );
        Ok(Resolution::Ink {
            bitmap,
            rect: baked.rect,
            reused: false,
        })
    }

    /// The cached value for `key` baked from `fingerprint`, or `None` on a miss (including an
    /// evicted image and a slot baked from another geometry or spec).
    fn reuse(
        &self,
        store: &mut BrushBitmapStore,
        anchor_key: &str,
        key: &str,
        fingerprint: &[u64],
    ) -> Option<Resolution> {
        let slot = self
            .slots
            .get(anchor_key)
            .filter(|slot| slot.key == key && slot.fingerprint == fingerprint)?;
        match &slot.ink {
            None => Some(Resolution::Collapsed { reused: true }),
            Some(ink) => {
                let bitmap = store.lease(&ink.brush_id)?;
                Some(Resolution::Ink {
                    bitmap,
                    rect: ink.rect,
                    reused: true,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_slot_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_slot.rs"]
mod tests;
