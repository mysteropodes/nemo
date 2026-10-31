//! #2358 GHOST-UNDER: onion skin / Ghost All composed by the native viewport renderer, UNDER the live frame.
//!
//! Baseline order (engine-bridge.js `buildSceneJson`): the onion and Ghost All layers are spliced in below the live
//! items. Here the viewport request names the onion settings, the ghost frames come from the SAME retained ghost query
//! the ghosts read uses (`tween_ghost_query.rs`: opacity falloff, tint, shift and mode decided once), and the compositor
//! draws them into the artwork scene before the live paint sequence, so live pixels win where they overlap.
//! The underlay exists only on a viewport witness (editor overlay, `viewport_scene_overlay.rs`): an export or readback
//! witness refuses it and never draws it. Vector ghosts are the stored key geometry; text and raster ghosts are the
//! evaluated ghost frame filtered to the ghosted layer. Nothing here writes, selects or touches history.
//! This render-scene file knows only plain facts: the application layer owns the retained ghost query and hands them over
//! through [`GhostSource`] (`tween_ghost_underlay.rs`).
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::object_scene_style_emit::PreparedItem;
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use serde::{Deserialize, Serialize};
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
        if self
            .shift
            .is_some_and(|s| !s.x.is_finite() || !s.y.is_finite())
        {
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
    /// The retained ghost query's selector for `frame` of a snapshot. `from` / `to` are the user's onion range,
    /// independent of the frame; the query's range always holds the frame and the last frame.
    pub fn query_bytes(
        &self,
        at_revision: u64,
        snapshot_id: &str,
        total_frames: u32,
        frame: u32,
    ) -> Vec<u8> {
        let (from, to) = (
            self.from.min(frame),
            self.to.max(frame).min(total_frames.saturating_sub(1)),
        );
        let mut q = serde_json::json!({"atRevision":at_revision,"documentSnapshotId":snapshot_id,
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
    Shape {
        path: BezPath,
        fill: Option<Color>,
        stroke: Option<(Color, f64)>,
        at: Affine,
    },
    Image {
        image: ImageData,
        transform: Affine,
        alpha: f32,
    },
    /// A produced brush dab or an evaluated component / duplicator item: the live paint kept as the baseline's
    /// ghost, faded to the ghost opacity (and tinted like any painted path in Tinted / Outline mode).
    Prepared {
        item: PreparedItem,
        path: BezPath,
        at: Affine,
        alpha: f32,
        tint: Option<(Color, Option<f64>)>,
        clear_fill: bool,
    },
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
fn color([r, g, b, a]: [u8; 4], opacity: f64) -> Color {
    let alpha = (f64::from(a) * opacity.clamp(0., 1.))
        .round()
        .clamp(0., 255.) as u8;
    Color::from_rgba8(r, g, b, alpha)
}
fn bez(segments: &[[f64; 6]], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let Some(first) = segments.first() else {
        return path;
    };
    path.move_to((first[0], first[1]));
    let edge = |path: &mut BezPath, a: &[f64; 6], b: &[f64; 6]| {
        path.curve_to(
            (a[0] + a[4], a[1] + a[5]),
            (b[0] + b[2], b[1] + b[3]),
            (b[0], b[1]),
        );
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
        GhostReceipt {
            selected: self.request.clone(),
            status,
            frames: self.frames,
            reason: self.refusal,
        }
    }
    /// Drawn `(shapes, images)`; shapes are stored vector paths and glyph contours.
    pub(crate) fn counts(&self) -> (usize, usize) {
        let images = self.images().count();
        (self.pieces.len() - images, images)
    }
    /// Drawn dab / component / duplicator pieces, which `counts` also includes among its shapes.
    #[cfg(test)]
    pub(crate) fn prepared_count(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| matches!(p, Piece::Prepared { .. }))
            .count()
    }
    pub fn has_images(&self) -> bool {
        self.images().next().is_some()
    }
    pub(crate) fn images(&self) -> impl Iterator<Item = &ImageData> + '_ {
        self.pieces.iter().filter_map(|p| match p {
            Piece::Image { image, .. } => Some(image),
            Piece::Shape { .. } | Piece::Prepared { .. } => None,
        })
    }
    /// Ids of the images this underlay draws that the live artwork does not: they stay resident in the renderer's atlas.
    pub(crate) fn foreign_image_ids(&self, live: &ObjectCompositionWitness) -> Vec<u64> {
        let known: Vec<_> = live.raster_images().map(|i| i.data.id()).collect();
        let mut ids: Vec<u64> = self
            .images()
            .map(|i| i.data.id())
            .filter(|id| !known.contains(id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    /// Decoded bytes the underlay keeps alive (each distinct image once): what a memo of it is charged.
    pub(crate) fn image_bytes(&self) -> usize {
        let mut seen = Vec::new();
        self.images()
            .filter(|i| {
                let fresh = !seen.contains(&i.data.id());
                seen.push(i.data.id());
                fresh
            })
            .map(|i| i.data.data().len())
            .sum()
    }
    /// Into the artwork scene BEFORE the live paint sequence: the live frame is drawn over it.
    pub(crate) fn draw(&self, scene: &mut vello::Scene) {
        for piece in &self.pieces {
            match piece {
                Piece::Shape {
                    path,
                    fill,
                    stroke,
                    at,
                } => {
                    if let Some(fill) = fill {
                        scene.fill(Fill::NonZero, *at, *fill, None, path);
                    }
                    if let Some((paint, width)) = stroke {
                        let style = Stroke::new(*width)
                            .with_caps(Cap::Round)
                            .with_join(Join::Round);
                        scene.stroke(&style, *at, *paint, None, path);
                    }
                }
                Piece::Image {
                    image,
                    transform,
                    alpha,
                } => {
                    scene.draw_image(
                        &ImageBrush::new(image.clone()).multiply_alpha(*alpha),
                        *transform,
                    );
                }
                Piece::Prepared {
                    item,
                    path,
                    at,
                    alpha,
                    tint,
                    clear_fill,
                } => item.draw_ghost(scene, *at, path, *alpha, *tint, *clear_fill),
            }
        }
    }
}

/// One drawable ghost path with its presented paint (sRGB bytes, `[r, g, b, a]`). Segments are `[x, y, inX, inY, outX, outY]`,
/// handles relative to the point.
#[derive(Debug, Clone, PartialEq)]
pub struct GhostPath {
    pub closed: bool,
    pub width: f64,
    pub fill: Option<[u8; 4]>,
    pub stroke: Option<[u8; 4]>,
    pub opacity: f64,
    pub segments: Vec<[f64; 6]>,
}
/// How one ghost frame is painted, as the retained ghost query decided it (kind, falloff, shift, mode and tint).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostFrameSpec {
    pub translation: [f64; 2],
    /// A Ghost All key frame: stored vector geometry only (its text and rasters are not ghosted).
    pub all_keys: bool,
    /// Glyph fill opacity, and the Outline-mode stroke `([r, g, b, a], width)` that replaces the fill.
    pub text_opacity: f64,
    pub text_stroke: Option<([f64; 4], f64)>,
    pub text_clear_fill: bool,
    pub raster_opacity: f64,
}
/// How an evaluated component / duplicator item is painted as a ghost, as the retained ghost query decided it for
/// that record: the opacity, and the tint stroke `([r, g, b, a], width)` that replaces its stroke (width `None` keeps
/// the item's own), with the fill cleared in Outline mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostVisual {
    pub opacity: f64,
    pub stroke: Option<([f64; 4], Option<f64>)>,
    pub clear_fill: bool,
}
/// The application layer's retained ghost query seen as the plain facts the underlay is built from.
pub trait GhostSource {
    fn layer_uid(&self) -> &str;
    fn frame_count(&self) -> usize;
    fn frame(&self, index: usize) -> GhostFrameSpec;
    /// Stored vector geometry of the frame with the mode's visual policy applied; `Err` when the layer has none (derived).
    fn paths(&self, index: usize) -> Result<Vec<GhostPath>, ()>;
    /// The evaluated ghost frame (text glyphs and images), pinned to the same snapshot.
    fn witness(&self, index: usize) -> Result<ObjectCompositionWitness, ()>;
    /// Whether the layer has any text or raster item at all, so a vector-only layer never evaluates ghost frames.
    fn has_items(&self) -> bool;
    /// The paint policy of one evaluated component / duplicator record of the ghost frame (baseline `onionLayerItems`).
    fn record_visual(&self, index: usize, record: &ObjectRecord) -> GhostVisual;
}
impl GhostUnderlay {
    /// A request the engine could not honour: no pieces, the reason kept, the identity stable.
    pub fn refused(
        request: &GhostUnderlayRequest,
        snapshot_id: &str,
        frame: u32,
        reason: &'static str,
    ) -> Self {
        Self {
            request: request.clone(),
            snapshot_id: snapshot_id.into(),
            frame,
            pieces: Vec::new(),
            frames: 0,
            refusal: Some(reason),
            _keep: Vec::new(),
        }
    }
    pub fn build(
        source: &dyn GhostSource,
        request: &GhostUnderlayRequest,
        snapshot_id: &str,
        frame: u32,
    ) -> Self {
        if let Err(reason) = request.validate() {
            return Self::refused(request, snapshot_id, frame, reason);
        }
        let mut out = Self {
            request: request.clone(),
            snapshot_id: snapshot_id.into(),
            frame,
            pieces: Vec::new(),
            frames: source.frame_count(),
            refusal: None,
            _keep: Vec::new(),
        };
        let items = source.has_items();
        let mut segments = 0usize;
        for index in 0..source.frame_count() {
            let at = source.frame(index);
            let shift = Affine::translate((at.translation[0], at.translation[1]));
            // Items first (they sit below the strokes drawn over them), then the stored vector geometry.
            if items && !at.all_keys {
                if let Ok(witness) = source.witness(index) {
                    out.collect_items(source, index, &at, &witness);
                    out._keep.push(witness);
                }
            }
            // A derived layer (component, duplicator) has no stored key geometry: its text and images above are what shows.
            if let Ok(paths) = source.paths(index) {
                for p in paths {
                    segments += p.segments.len();
                    let fill = p.fill.map(|c| color(c, p.opacity));
                    let stroke = p.stroke.map(|c| (color(c, p.opacity), p.width.max(0.)));
                    if fill.is_none() && stroke.is_none() {
                        continue;
                    }
                    out.pieces.push(Piece::Shape {
                        path: bez(&p.segments, p.closed),
                        fill,
                        stroke,
                        at: shift,
                    });
                }
            }
            if out.pieces.len() > MAX_PIECES || segments > MAX_SEGMENTS {
                return Self::refused(request, snapshot_id, frame, "ghost_budget");
            }
        }
        out
    }
    /// The text glyphs, images, brush dabs and evaluated component / duplicator items of one evaluated ghost frame that
    /// belong to the ghosted layer (a derived layer owns no stored key geometry, so these ARE its ghost).
    fn collect_items(
        &mut self,
        source: &dyn GhostSource,
        index: usize,
        at: &GhostFrameSpec,
        witness: &ObjectCompositionWitness,
    ) {
        let layer_uid = source.layer_uid();
        let plan = witness.plan();
        let layers = plan.packet().source_layers();
        let ours = |layer_index: usize| {
            layers
                .get(layer_index)
                .is_some_and(|l| l.layer_uid() == layer_uid)
        };
        let shift = Affine::translate((at.translation[0], at.translation[1]));
        let derived = |this: &mut Self, item: &PreparedItem, path: &BezPath, record: &ObjectRecord| {
            let visual = source.record_visual(index, record);
            this.pieces.push(Piece::Prepared {
                item: item.clone(),
                path: path.clone(),
                at: shift,
                alpha: visual.opacity as f32,
                tint: visual.stroke.map(|([r, g, b, a], width)| {
                    (Color::new([r as f32, g as f32, b as f32, a as f32]), width)
                }),
                clear_fill: visual.clear_fill,
            });
        };
        for entry in ordered_entries(plan) {
            match entry {
                PaintEntry::Text(source) if ours(source.layer_index) => {
                    for (contour, [r, g, b, a]) in source.item.painted() {
                        let base = Color::from_rgba8(r, g, b, a);
                        let fill = (!at.text_clear_fill)
                            .then(|| base.with_alpha(base.components[3] * at.text_opacity as f32));
                        let stroke = at.text_stroke.map(|([tr, tg, tb, ta], width)| {
                            (
                                Color::new([
                                    tr as f32,
                                    tg as f32,
                                    tb as f32,
                                    (ta * at.text_opacity).clamp(0., 1.) as f32,
                                ]),
                                width,
                            )
                        });
                        self.pieces.push(Piece::Shape {
                            path: contour.contour.path().clone(),
                            fill,
                            stroke,
                            at: shift,
                        });
                    }
                }
                PaintEntry::Raster(i)
                    if plan
                        .raster_sources()
                        .get(i)
                        .is_some_and(|s| ours(s.layer_index)) =>
                {
                    if let Some((image, transform, opacity)) = witness.raster_image(i) {
                        self.pieces.push(Piece::Image {
                            image: image.clone(),
                            transform,
                            alpha: opacity * at.raster_opacity as f32,
                        });
                    }
                }
                // Brush-texture dabs: the baseline draws them in their own look at `op * dab opacity` (the dab's
                // colour alpha already is its opacity), never tinted. The anchor itself is the invisible stored path.
                PaintEntry::Dab(dab) if ours(dab.layer_index) => self.pieces.push(Piece::Prepared {
                    item: dab.prepared.clone(),
                    path: dab.path.clone(),
                    at: shift,
                    alpha: at.raster_opacity as f32,
                    tint: None,
                    clear_fill: false,
                }),
                PaintEntry::Brush(i)
                    if plan
                        .brush_sources()
                        .and_then(|b| b.paints.get(i))
                        .is_some_and(|b| ours(b.layer_index)) =>
                {
                    if let Ok(Some((image, transform, opacity))) = witness.brush_image(i) {
                        self.pieces.push(Piece::Image {
                            image: image.clone(),
                            transform,
                            alpha: opacity * at.raster_opacity as f32,
                        });
                    }
                }
                PaintEntry::Component(c) if ours(c.host_layer_index) => {
                    derived(self, &c.prepared, &c.path, &c.record)
                }
                PaintEntry::Duplicator(d) if ours(d.layer_index) => {
                    derived(self, &d.prepared, &d.path, &d.record)
                }
                _ => {}
            }
        }
    }
}
