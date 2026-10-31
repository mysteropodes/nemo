//! #2358 GHOST-UNDER: onion skin / Ghost All composed by the native viewport renderer, UNDER the live frame.
//!
//! Baseline order (engine-bridge.js `buildSceneJson`): the onion and Ghost All layers are spliced in below the live
//! items. Here the viewport request names the onion settings, the ghost frames come from the SAME retained ghost query
//! the ghosts read uses (`tween_ghost_query.rs`: opacity falloff, tint, shift and mode decided once), and the compositor
//! draws them into the artwork scene before the live paint sequence, so live pixels win where they overlap.
//! The underlay exists only on a viewport witness (editor overlay, `viewport_scene_overlay.rs`): an export or readback
//! witness refuses it and never draws it. Vector ghosts are the stored key geometry; text and raster ghosts are the
//! evaluated ghost frame filtered to the ghosted layer. Nothing here writes, selects or touches history.
use crate::application::{TweenGhostKind, TweenGhostRead};
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use crate::request_receipts::GhostFrames;
use crate::revision::ObjectSnapshot;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, Weak};
use vello::kurbo::{Affine, BezPath, Cap, Join, Stroke};
use vello::peniko::{Color, Fill, ImageBrush, ImageData};

/// Ghost frames drawn at most per preview, and drawable pieces / path segments in all of them.
const MAX_PIECES: usize = 20_000;
const MAX_SEGMENTS: usize = 2_000_000;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GhostMode {
    Default,
    Tinted,
    Outline,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GhostShift {
    pub x: f64,
    pub y: f64,
    pub next: bool,
}
/// What the viewport request asks to be composed under the live frame (the baseline's onion state).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GhostUnderlayRequest {
    pub layer_uid: String,
    pub onion: bool,
    pub ghost_all: bool,
    pub from: u32,
    pub to: u32,
    pub previous_opacity: f64,
    pub next_opacity: f64,
    pub mode: GhostMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_tint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_tint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shift: Option<GhostShift>,
}
impl GhostUnderlayRequest {
    /// Bounded and well-formed, or the reason it is not. A refused request is a typed host error, never a blank ghost.
    pub fn validate(&self) -> Result<(), &'static str> {
        let opacity = |v: f64| v.is_finite() && (0. ..=100.).contains(&v);
        if self.layer_uid.is_empty() || self.layer_uid.len() > 128 {
            return Err("onion layer uid is invalid");
        }
        if !opacity(self.previous_opacity) || !opacity(self.next_opacity) {
            return Err("onion opacity is out of range");
        }
        if self.shift.is_some_and(|s| !s.x.is_finite() || !s.y.is_finite()) {
            return Err("onion shift is not finite");
        }
        if !self.onion && !self.ghost_all {
            return Err("onion request selects nothing");
        }
        Ok(())
    }
    /// Part of the viewport's canonical cache identity: a different onion setting is a different picture.
    pub fn quality_suffix(&self) -> String {
        format!("+ghost:{}", serde_json::to_string(self).unwrap_or_default())
    }
    /// `from` / `to` are the user's onion range, independent of the frame; the query's range always holds the frame.
    fn query(&self, snapshot: &ObjectSnapshot, frame: u32) -> Vec<u8> {
        let last = snapshot.document().total_frames.saturating_sub(1);
        let (from, to) = (self.from.min(frame), self.to.max(frame).min(last));
        let mut q = serde_json::json!({"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
            "contextId":"scene-root","layerUid":self.layer_uid,"frame":frame,"onion":self.onion,"ghostAll":self.ghost_all,
            "from":from,"to":to,"previousOpacity":self.previous_opacity,"nextOpacity":self.next_opacity,"mode":self.mode});
        if let Some(t) = &self.previous_tint {
            q["previousTint"] = t.clone().into();
        }
        if let Some(t) = &self.next_tint {
            q["nextTint"] = t.clone().into();
        }
        if let Some(s) = &self.shift {
            q["shift"] = serde_json::json!({"x":s.x,"y":s.y,"next":s.next});
        }
        serde_json::to_vec(&q).unwrap_or_default()
    }
}

/// What the receipt tells the UI about the composition it asked for.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GhostReceipt {
    pub selected: GhostUnderlayRequest,
    /// `composited`, `empty` (nothing to ghost at this frame) or `unavailable` (see `reason`).
    pub status: &'static str,
    pub frames: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

enum Piece {
    Shape { path: BezPath, fill: Option<Color>, stroke: Option<(Color, f64)>, at: Affine },
    Image { image: ImageData, transform: Affine, alpha: f32 },
}
/// The prepared ghost pieces of one preview, in baseline order, plus whatever keeps their decoded resources alive.
pub struct GhostUnderlay {
    request: GhostUnderlayRequest,
    snapshot_id: String,
    frame: u32,
    pieces: Vec<Piece>,
    frames: usize,
    refusal: Option<&'static str>,
    _keep: Vec<ObjectCompositionWitness>,
}
impl std::fmt::Debug for GhostUnderlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GhostUnderlay")
            .field("frame", &self.frame)
            .field("pieces", &self.pieces.len())
            .field("refusal", &self.refusal)
            .finish()
    }
}
fn color(hex: &str, opacity: f64) -> Option<Color> {
    let [r, g, b, a] = crate::object_document::bitmap_brush::rgba(hex)?;
    let alpha = (f64::from(a) * opacity.clamp(0., 1.)).round().clamp(0., 255.) as u8;
    Some(Color::from_rgba8(r, g, b, alpha))
}
fn bez(segments: &[[f64; 6]], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let Some(first) = segments.first() else { return path };
    path.move_to((first[0], first[1]));
    let edge = |path: &mut BezPath, a: &[f64; 6], b: &[f64; 6]| {
        path.curve_to((a[0] + a[4], a[1] + a[5]), (b[0] + b[2], b[1] + b[3]), (b[0], b[1]));
    };
    for pair in segments.windows(2) {
        edge(&mut path, &pair[0], &pair[1]);
    }
    if closed && segments.len() > 1 {
        edge(&mut path, &segments[segments.len() - 1], first);
        path.close_path();
    }
    path
}
fn layer_has_items(snapshot: &ObjectSnapshot, layer_uid: &str) -> bool {
    snapshot.document().item_records().is_some_and(|items| {
        items.raster_entries().iter().any(|e| e.target.layer_uid() == layer_uid)
            || items.text_entries().iter().any(|e| e.target.layer_uid() == layer_uid)
    })
}
impl GhostUnderlay {
    pub fn request(&self) -> &GhostUnderlayRequest {
        &self.request
    }
    pub(crate) fn is_for(&self, snapshot_id: &str, frame: u32) -> bool {
        self.snapshot_id == snapshot_id && self.frame == frame
    }
    pub fn receipt(&self) -> GhostReceipt {
        let status = match (self.refusal, self.pieces.is_empty()) {
            (Some(_), _) => "unavailable",
            (None, true) => "empty",
            (None, false) => "composited",
        };
        GhostReceipt { selected: self.request.clone(), status, frames: self.frames, reason: self.refusal }
    }
    /// Images this underlay draws that the live artwork does not: they stay resident in the renderer's atlas.
    pub(crate) fn draws_foreign_images(&self, live: &ObjectCompositionWitness) -> bool {
        let known: Vec<_> = live.raster_images().map(|i| i.data.id()).collect();
        self.pieces.iter().any(|p| matches!(p, Piece::Image { image, .. } if !known.contains(&image.data.id())))
    }
    /// Drawn `(shapes, images)`; shapes are stored vector paths and glyph contours.
    pub(crate) fn counts(&self) -> (usize, usize) {
        let images = self.images().count();
        (self.pieces.len() - images, images)
    }
    pub(crate) fn images(&self) -> impl Iterator<Item = &ImageData> + '_ {
        self.pieces.iter().filter_map(|p| match p {
            Piece::Image { image, .. } => Some(image),
            Piece::Shape { .. } => None,
        })
    }
    /// Into the artwork scene BEFORE the live paint sequence: the live frame is drawn over it.
    pub(crate) fn draw(&self, scene: &mut vello::Scene) {
        for piece in &self.pieces {
            match piece {
                Piece::Shape { path, fill, stroke, at } => {
                    if let Some(fill) = fill {
                        scene.fill(Fill::NonZero, *at, *fill, None, path);
                    }
                    if let Some((paint, width)) = stroke {
                        let style = Stroke::new(*width).with_caps(Cap::Round).with_join(Join::Round);
                        scene.stroke(&style, *at, *paint, None, path);
                    }
                }
                Piece::Image { image, transform, alpha } => {
                    scene.draw_image(&ImageBrush::new(image.clone()).multiply_alpha(*alpha), *transform);
                }
            }
        }
    }
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
pub fn prepare_ghost_underlay(snapshot: &ObjectSnapshot, request: &GhostUnderlayRequest, frame: u32) -> Arc<GhostUnderlay> {
    let key = request.query(snapshot, frame);
    let revision = snapshot.revision_identity();
    let hit = MEMO.lock().ok().and_then(|mut memo| {
        memo.retain(|m| m.revision.strong_count() > 0);
        let at = memo.iter().position(|m| m.key == key && Weak::ptr_eq(&m.revision, &revision))?;
        let entry = memo.remove(at);
        let underlay = Arc::clone(&entry.underlay);
        memo.insert(0, entry);
        Some(underlay)
    });
    if let Some(hit) = hit {
        return hit;
    }
    let underlay = Arc::new(build(snapshot, request, frame, &key));
    if !underlay.pieces.iter().any(|p| matches!(p, Piece::Image { .. })) {
        if let Ok(mut memo) = MEMO.lock() {
            memo.insert(0, Memo { revision, key, underlay: Arc::clone(&underlay) });
            memo.truncate(MEMO_ENTRIES);
        }
    }
    underlay
}
fn refused(snapshot: &ObjectSnapshot, request: &GhostUnderlayRequest, frame: u32, reason: &'static str) -> GhostUnderlay {
    GhostUnderlay {
        request: request.clone(),
        snapshot_id: snapshot.snapshot_id().into(),
        frame,
        pieces: Vec::new(),
        frames: 0,
        refusal: Some(reason),
        _keep: Vec::new(),
    }
}
fn build(snapshot: &ObjectSnapshot, request: &GhostUnderlayRequest, frame: u32, query: &[u8]) -> GhostUnderlay {
    if let Err(reason) = request.validate() {
        return refused(snapshot, request, frame, reason);
    }
    let read = match snapshot.query_tween_ghosts_json(query) {
        Ok(read) => read,
        Err(error) => {
            use crate::request_receipts::DispatchErrorCode as Code;
            let reason = match error {
                Code::StaleRevision => "stale_revision",
                Code::InvalidRequest => "invalid_request",
                Code::NotFound => "layer_not_found",
                _ => "ghost_view_unavailable",
            };
            return refused(snapshot, request, frame, reason);
        }
    };
    let mut out = GhostUnderlay {
        request: request.clone(),
        snapshot_id: snapshot.snapshot_id().into(),
        frame,
        pieces: Vec::new(),
        frames: read.frames().len(),
        refusal: None,
        _keep: Vec::new(),
    };
    let items = layer_has_items(snapshot, &request.layer_uid);
    let mut segments = 0usize;
    for index in 0..read.frames().len() {
        let at = &read.frames()[index];
        let shift = Affine::translate((at.translation[0], at.translation[1]));
        // Items first (they sit below the strokes drawn over them), then the stored vector geometry.
        if items && at.kind != TweenGhostKind::AllKeys {
            if let Ok(witness) = read.prepare_frame(index) {
                collect_items(&mut out, &read, index, &witness);
                out._keep.push(witness);
            }
        }
        // A derived layer (component, duplicator) has no stored key geometry: its text and images above are what shows.
        if let Ok(paths) = read.paths(index) {
            for p in paths {
                segments += p.segments.len();
                let fill = p.fill.as_deref().and_then(|c| color(c, p.opacity));
                let stroke = p.stroke.as_deref().and_then(|c| color(c, p.opacity)).map(|c| (c, p.width.max(0.)));
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                out.pieces.push(Piece::Shape { path: bez(&p.segments, p.closed), fill, stroke, at: shift });
            }
        }
        if out.pieces.len() > MAX_PIECES || segments > MAX_SEGMENTS {
            return refused(snapshot, request, frame, "ghost_budget");
        }
    }
    out
}
/// The text glyphs and images of one evaluated ghost frame that belong to the ghosted layer.
fn collect_items(out: &mut GhostUnderlay, read: &TweenGhostRead, index: usize, witness: &ObjectCompositionWitness) {
    let at = &read.frames()[index];
    let plan = witness.plan();
    let layers = plan.packet().source_layers();
    let ours = |layer_index: usize| layers.get(layer_index).is_some_and(|l| l.layer_uid() == read.layer_uid());
    let text = at.text_visual();
    let raster = at.raster_visual();
    let shift = Affine::translate((text.translation[0], text.translation[1]));
    for entry in ordered_entries(plan) {
        match entry {
            PaintEntry::Text(source) if ours(source.layer_index) => {
                for (contour, [r, g, b, a]) in source.item.painted() {
                    let base = Color::from_rgba8(r, g, b, a);
                    let fill = (!text.clear_fill).then(|| base.with_alpha(base.components[3] * text.opacity as f32));
                    let stroke = text.stroke.map(|[tr, tg, tb, ta]| {
                        (Color::new([tr as f32, tg as f32, tb as f32, (ta * text.opacity).clamp(0., 1.) as f32]), text.stroke_width.unwrap_or(1.))
                    });
                    out.pieces.push(Piece::Shape { path: contour.contour.path().clone(), fill, stroke, at: shift });
                }
            }
            PaintEntry::Raster(i) if plan.raster_sources().get(i).is_some_and(|s| ours(s.layer_index)) => {
                if let Some((image, transform, opacity)) = witness.raster_image(i) {
                    out.pieces.push(Piece::Image { image: image.clone(), transform, alpha: opacity * raster.opacity as f32 });
                }
            }
            _ => {}
        }
    }
}

#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/object_ghost_underlay.rs"]
mod tests;
