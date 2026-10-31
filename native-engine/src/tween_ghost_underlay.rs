//! #2358 GHOST-UNDER: the retained ghost query seen as the source of the viewport's onion / Ghost All underlay.
//! The ghost frames (opacity falloff, tint, shift, mode) are decided by `tween_ghost_query.rs` once; this file only hands
//! them to the render-scene underlay (`object_ghost_underlay.rs`) and remembers the vector pieces of the last renders.
use crate::application::{TweenGhostKind, TweenGhostRead};
use crate::object_document::bitmap_brush::rgba;
use crate::object_render_scene::{
    GhostFrameSpec, GhostPath, GhostSource, GhostUnderlay, GhostUnderlayRequest,
    ObjectCompositionWitness,
};
use crate::request_receipts::{DispatchErrorCode as Code, GhostFrames};
use crate::revision::ObjectSnapshot;
use std::sync::{Arc, Mutex, Weak};

struct Source<'a> {
    read: &'a TweenGhostRead,
    items: bool,
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
        self.read.prepare_frame(index).map_err(|_| ())
    }
    fn has_items(&self) -> bool {
        self.items
    }
}
fn layer_has_items(snapshot: &ObjectSnapshot, layer_uid: &str) -> bool {
    snapshot.document().item_records().is_some_and(|items| {
        items
            .raster_entries()
            .iter()
            .any(|e| e.target.layer_uid() == layer_uid)
            || items
                .text_entries()
                .iter()
                .any(|e| e.target.layer_uid() == layer_uid)
    })
}

struct Memo {
    revision: Weak<dyn std::any::Any + Send + Sync>,
    key: Vec<u8>,
    underlay: Arc<GhostUnderlay>,
}
/// Pan and zoom re-render the same frame of the same revision: the prepared vector pieces are reused (no raster
/// ghost is cached, its decoded pixels are charged and must not outlive the preview that drew them).
static MEMO: Mutex<Vec<Memo>> = Mutex::new(Vec::new());
const MEMO_ENTRIES: usize = 4;

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
                &Source { read: &read, items },
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
                _ => "ghost_view_unavailable",
            };
            GhostUnderlay::refused(request, snapshot.snapshot_id(), frame, reason)
        }
    });
    if !underlay.has_images() {
        if let Ok(mut memo) = MEMO.lock() {
            memo.insert(
                0,
                Memo {
                    revision,
                    key,
                    underlay: Arc::clone(&underlay),
                },
            );
            memo.truncate(MEMO_ENTRIES);
        }
    }
    underlay
}

#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/object_ghost_underlay.rs"]
mod tests;
