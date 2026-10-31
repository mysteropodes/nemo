//! #2358 GHOST-UNDER: the retained ghost query seen as the source of the viewport's onion / Ghost All underlay.
//! The ghost frames (opacity falloff, tint, shift, mode) are decided by `tween_ghost_query.rs` once; this file only hands
//! them to the render-scene underlay (`object_ghost_underlay.rs`) and remembers the vector pieces of the last renders.
use crate::application::{TweenGhostKind, TweenGhostRead};
use crate::object_document::bitmap_brush::rgba;
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::{
    GhostFrameSpec, GhostPath, GhostSource, GhostUnderlay, GhostUnderlayRequest, GhostVisual,
    ObjectCompositionWitness,
};
use crate::request_receipts::{DispatchErrorCode as Code, GhostFrames};
use crate::revision::ObjectSnapshot;
use std::sync::{Arc, Mutex, Weak};

struct Source<'a> {
    read: &'a TweenGhostRead,
    items: bool,
    revision: Weak<dyn std::any::Any + Send + Sync>,
}
impl GhostSource for Source<'_> {
    fn layer_uid(&self) -> &str {
        self.read.layer_uid()
    }
    fn frame_count(&self) -> usize {
        self.read.frames().len()
    }
    fn frame(&self, index: usize) -> GhostFrameSpec {
        let frame = &self.read.frames()[index];
        let text = frame.text_visual();
        GhostFrameSpec {
            translation: frame.translation,
            all_keys: frame.kind == TweenGhostKind::AllKeys,
            text_opacity: text.opacity,
            text_stroke: text.stroke.map(|s| (s, text.stroke_width.unwrap_or(1.))),
            text_clear_fill: text.clear_fill,
            raster_opacity: frame.raster_visual().opacity,
        }
    }
    fn paths(&self, index: usize) -> Result<Vec<GhostPath>, ()> {
        let facts = GhostFrames::paths(self.read, index).map_err(|_| ())?;
        Ok(facts
            .into_iter()
            .map(|p| GhostPath {
                closed: p.closed,
                width: p.width,
                fill: p.fill.as_deref().and_then(rgba),
                stroke: p.stroke.as_deref().and_then(rgba),
                opacity: p.opacity,
                segments: p.segments,
            })
            .collect())
    }
    fn witness(&self, index: usize) -> Result<ObjectCompositionWitness, ()> {
        let frame = self.read.frames()[index].frame;
        if let Some(hit) = witness_hit(&self.revision, frame) {
            return Ok(hit);
        }
        #[cfg(test)]
        PREPARED_FRAMES.with(|n| n.set(n.get() + 1));
        let witness = self.read.prepare_frame(index).map_err(|_| ())?;
        witness_keep(&self.revision, frame, &witness);
        Ok(witness)
    }
    fn record_visual(&self, index: usize, record: &ObjectRecord) -> GhostVisual {
        let v = self.read.frames()[index].vector_visual(record);
        GhostVisual {
            opacity: v.opacity,
            stroke: v.stroke.map(|s| (s, v.stroke_width)),
            clear_fill: v.clear_fill,
        }
    }
    fn has_items(&self) -> bool {
        self.items
    }
}
/// Whether the ghosted layer has anything the evaluated ghost frame draws beyond stored key geometry: text or raster
/// items, seeded brush-texture anchors (their dabs are produced, never stored) or a derived layer (component or
/// duplicator instance, which has no stored geometry at all). A plain vector layer never evaluates ghost frames.
fn layer_has_items(snapshot: &ObjectSnapshot, layer_uid: &str) -> bool {
    let document = snapshot.document();
    document.item_records().is_some_and(|items| {
        items
            .raster_entries()
            .iter()
            .any(|e| e.target.layer_uid() == layer_uid)
            || items
                .text_entries()
                .iter()
                .any(|e| e.target.layer_uid() == layer_uid)
    }) || document
        .component_set()
        .is_some_and(|c| c.instance("scene-root", layer_uid).is_some())
        || document
            .duplicator_set()
            .is_some_and(|d| d.duplicator("scene-root", layer_uid).is_some())
        || document.objects().iter().any(|r| {
            r.target.layer_uid() == layer_uid
                && crate::evaluation::brush_dab::paint::is_seeded_anchor(r)
        })
}

type Revision = Weak<dyn std::any::Any + Send + Sync>;
/// Decoded pixels the memos below may keep alive (each); a ghost set over the budget is rebuilt rather than retained.
const MEMO_IMAGE_BYTES: usize = 48 << 20;
const MEMO_ENTRIES: usize = 4;
const WITNESS_ENTRIES: usize = 32;
#[cfg(test)]
thread_local! {
    /// Ghost frames evaluated (`prepare_frame`) on this thread: a probe for "no re-preparation on an unchanged revision".
    pub(crate) static PREPARED_FRAMES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Ghost frames evaluated on this thread so far (test probe).
#[cfg(test)]
pub(crate) fn prepared_frames() -> usize {
    PREPARED_FRAMES.with(|n| n.get())
}

struct Memo {
    revision: Revision,
    key: Vec<u8>,
    underlay: Arc<GhostUnderlay>,
    bytes: usize,
}
/// Pan and zoom re-render the same frame of the same revision: the prepared pieces are reused, decoded ghost images
/// included (byte-bounded), so the compositor sees the SAME image ids and neither re-decodes nor resets its cache.
static MEMO: Mutex<Vec<Memo>> = Mutex::new(Vec::new());

struct Evaluated {
    revision: Revision,
    frame: u32,
    witness: ObjectCompositionWitness,
    bytes: usize,
}
/// The evaluated frame (text, rasters, dabs, components) of one revision, shared by every request that ghosts it: a
/// scrub step re-uses the frames the previous step already evaluated and decoded.
static EVALUATED: Mutex<Vec<Evaluated>> = Mutex::new(Vec::new());

fn witness_bytes(witness: &ObjectCompositionWitness) -> usize {
    let mut seen = Vec::new();
    witness
        .raster_images()
        .chain(witness.brush_images())
        .filter(|i| {
            let fresh = !seen.contains(&i.data.id());
            seen.push(i.data.id());
            fresh
        })
        .map(|i| i.data.data().len())
        .sum()
}
fn witness_hit(revision: &Revision, frame: u32) -> Option<ObjectCompositionWitness> {
    let mut memo = EVALUATED.lock().ok()?;
    memo.retain(|m| m.revision.strong_count() > 0);
    let at = memo
        .iter()
        .position(|m| m.frame == frame && Weak::ptr_eq(&m.revision, revision))?;
    let entry = memo.remove(at);
    let witness = entry.witness.clone();
    memo.insert(0, entry);
    Some(witness)
}
fn witness_keep(revision: &Revision, frame: u32, witness: &ObjectCompositionWitness) {
    let bytes = witness_bytes(witness);
    if bytes > MEMO_IMAGE_BYTES {
        return;
    }
    if let Ok(mut memo) = EVALUATED.lock() {
        memo.insert(
            0,
            Evaluated {
                revision: revision.clone(),
                frame,
                witness: witness.clone(),
                bytes,
            },
        );
        let (mut total, mut kept) = (0usize, 0usize);
        memo.retain(|m| {
            total += m.bytes;
            kept += 1;
            total <= MEMO_IMAGE_BYTES && kept <= WITNESS_ENTRIES
        });
    }
}

/// The ghost pieces of `request` at `frame` of `snapshot`. Never fails: a refusal is recorded in the underlay so the
/// preview identity stays stable and the UI is told why nothing was composed.
pub fn prepare_ghost_underlay(
    snapshot: &ObjectSnapshot,
    request: &GhostUnderlayRequest,
    frame: u32,
) -> Arc<GhostUnderlay> {
    let key = request.query_bytes(
        snapshot.content_revision(),
        snapshot.snapshot_id(),
        snapshot.document().total_frames,
        frame,
    );
    let revision = snapshot.revision_identity();
    let hit = MEMO.lock().ok().and_then(|mut memo| {
        memo.retain(|m| m.revision.strong_count() > 0);
        let at = memo
            .iter()
            .position(|m| m.key == key && Weak::ptr_eq(&m.revision, &revision))?;
        let entry = memo.remove(at);
        let underlay = Arc::clone(&entry.underlay);
        memo.insert(0, entry);
        Some(underlay)
    });
    if let Some(hit) = hit {
        return hit;
    }
    let underlay = Arc::new(match snapshot.query_tween_ghosts_json(&key) {
        Ok(read) => {
            let items = layer_has_items(snapshot, &request.layer_uid);
            GhostUnderlay::build(
                &Source {
                    read: &read,
                    items,
                    revision: revision.clone(),
                },
                request,
                snapshot.snapshot_id(),
                frame,
            )
        }
        Err(error) => {
            let reason = match error {
                Code::StaleRevision => "stale_revision",
                Code::InvalidRequest => "invalid_request",
                Code::NotFound => "layer_not_found",
                // A derived layer is ghosted from its evaluated frames; only a genuine refusal reaches here.
                _ => "ghost_view_unavailable",
            };
            GhostUnderlay::refused(request, snapshot.snapshot_id(), frame, reason)
        }
    });
    let bytes = underlay.image_bytes();
    if bytes <= MEMO_IMAGE_BYTES {
        if let Ok(mut memo) = MEMO.lock() {
            memo.insert(
                0,
                Memo {
                    revision,
                    key,
                    underlay: Arc::clone(&underlay),
                    bytes,
                },
            );
            let (mut total, mut kept) = (0usize, 0usize);
            memo.retain(|m| {
                total += m.bytes;
                kept += 1;
                total <= MEMO_IMAGE_BYTES && kept <= MEMO_ENTRIES
            });
        }
    }
    underlay
}

#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/object_ghost_underlay.rs"]
mod tests;
