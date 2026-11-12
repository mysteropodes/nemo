//! #2358 GHOST-UNDER: the retained ghost query seen as the source of the viewport's onion / Ghost All underlay.
//! The ghost frames (opacity falloff, tint, shift, mode) are decided by `tween_ghost_query.rs` once; this file only hands
//! them to the render-scene underlay (`object_ghost_underlay.rs`) and remembers the vector pieces of the last renders.
use crate::application::{TweenGhostKind, TweenGhostRead};
use crate::object_document::bitmap_brush::rgba;
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::{
    ordered_entries, GhostFrameSpec, GhostPath, GhostSource, GhostUnderlay, GhostUnderlayRequest,
    GhostVisual, ObjectCompositionWitness, PaintEntry,
};
use crate::request_receipts::{DispatchErrorCode as Code, GhostFrames};
use crate::revision::ObjectSnapshot;
use std::sync::{Arc, Mutex, Weak};

struct Source<'a> {
    read: &'a TweenGhostRead,
    items: bool,
    brush_items: bool,
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
            key_outline: frame.key_outline(),
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
        let witness = if self.read.frames()[index].kind == TweenGhostKind::AllKeys {
            self.read.prepare_key_frame(index)
        } else {
            self.read.prepare_frame(index)
        }
        .map_err(|_| ())?;
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
    fn has_brush_items(&self) -> bool {
        self.brush_items
    }
}
/// Whether the ghosted layer has produced brush paint that only an evaluated frame shows: a seeded brush-texture
/// anchor (its dabs are stamped at evaluation, never stored) or a bitmap brush stroke.
fn layer_has_brush_items(snapshot: &ObjectSnapshot, layer_uid: &str) -> bool {
    snapshot.document().objects().iter().any(|r| {
        r.target.layer_uid() == layer_uid
            && (crate::evaluation::brush_dab::paint::is_seeded_anchor(r)
                || r.vector_ext
                    .value()
                    .is_some_and(|ext| ext.bitmap_brush.value().is_some()))
    })
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
        || layer_has_brush_items(snapshot, layer_uid)
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

/// Bytes one `PathEl` of a retained plan costs, and the fixed cost of one paint entry (record, style, order slot).
const PATH_EL_BYTES: usize = 56;
const ENTRY_BYTES: usize = 256;
/// What an evaluated frame keeps alive: its decoded images (each distinct one once) plus an estimate of the plan's
/// own vector data (every contour / glyph / dab / component path and paint entry), so a heavy vector document is
/// charged although it decodes no image.
fn witness_bytes(witness: &ObjectCompositionWitness) -> usize {
    let mut seen = Vec::new();
    let images: usize = witness
        .raster_images()
        .chain(witness.brush_images())
        .filter(|i| {
            let fresh = !seen.contains(&i.data.id());
            seen.push(i.data.id());
            fresh
        })
        .map(|i| i.data.data().len())
        .sum();
    let path = |p: &vello::kurbo::BezPath| p.elements().len() * PATH_EL_BYTES;
    let plan: usize = ordered_entries(witness.plan())
        .map(|entry| {
            ENTRY_BYTES
                + match entry {
                    // The closed contour and the drawn path both stay resident.
                    PaintEntry::Contour(c) => path(c.path()) * 2,
                    PaintEntry::Text(t) => t
                        .item
                        .painted()
                        .map(|(c, _)| path(c.contour.path()))
                        .sum(),
                    PaintEntry::Component(c) => path(&c.path),
                    PaintEntry::Duplicator(d) => path(&d.path),
                    PaintEntry::Dab(d) => path(&d.path),
                    _ => 0,
                }
        })
        .sum();
    images + plan
}
/// The document an evaluated frame belongs to.
fn document_of(witness: &ObjectCompositionWitness) -> &str {
    witness.plan().packet().snapshot().document_id()
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
/// Admit one evaluated frame to `memo`: entries of another revision of the SAME document are dropped now (a ghost
/// frame of a superseded revision is never asked for again, and each entry holds its whole snapshot alive), then the
/// set is trimmed to `budget` bytes (images and estimated plan data) and `WITNESS_ENTRIES` entries, newest first.
fn admit_witness(memo: &mut Vec<Evaluated>, entry: Evaluated, budget: usize) {
    if entry.bytes > budget {
        return;
    }
    let document = document_of(&entry.witness).to_owned();
    memo.retain(|m| {
        m.revision.strong_count() > 0
            && (Weak::ptr_eq(&m.revision, &entry.revision) || document_of(&m.witness) != document)
    });
    memo.insert(0, entry);
    let (mut total, mut kept) = (0usize, 0usize);
    memo.retain(|m| {
        total += m.bytes;
        kept += 1;
        total <= budget && kept <= WITNESS_ENTRIES
    });
}
fn witness_keep(revision: &Revision, frame: u32, witness: &ObjectCompositionWitness) {
    let entry = Evaluated {
        revision: revision.clone(),
        frame,
        witness: witness.clone(),
        bytes: witness_bytes(witness),
    };
    if let Ok(mut memo) = EVALUATED.lock() {
        admit_witness(&mut memo, entry, MEMO_IMAGE_BYTES);
    }
}
/// The document was closed or replaced: nothing the ghost caches hold can be asked for again, and every evaluated
/// frame keeps its whole snapshot alive, so both memos are emptied now instead of waiting for the next ghost request.
pub fn release_ghost_caches() {
    if let Ok(mut memo) = EVALUATED.lock() {
        memo.clear();
    }
    if let Ok(mut memo) = MEMO.lock() {
        memo.clear();
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
            let brush_items = layer_has_brush_items(snapshot, &request.layer_uid);
            GhostUnderlay::build(
                &Source {
                    read: &read,
                    items,
                    brush_items,
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
