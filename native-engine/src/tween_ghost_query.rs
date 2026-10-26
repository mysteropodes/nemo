//! Immutable onion/ghost selection and native frame inputs. These reads grant no UI epoch or writer.
//! The retained snapshot outlives owner edits; native composition is prepared lazily per selected frame.
use crate::object_document::{object_deserialize, ObjectRecord, Presence};
use crate::object_frame_packet::{
    prepare_object_exposure_frame, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    ObjectBrushAdmission, ObjectCompositionWitness,
};
use crate::request_receipts::DispatchErrorCode as Error;
use crate::revision::ObjectSnapshot;
use serde::{Deserialize, Serialize};

#[path = "tween_ghost_geometry.rs"]
mod geometry;
pub use geometry::TweenGhostPath;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TweenGhostKind {
    Previous,
    Next,
    AllKeys,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Mode {
    Default,
    Tinted,
    Outline,
}
#[derive(Debug)]
struct Selector {
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    onion: bool,
    ghost_all: bool,
    from: u32,
    to: u32,
    previous_opacity: f64,
    next_opacity: f64,
    mode: Mode,
    previous_tint: Presence<String>,
    next_tint: Presence<String>,
    shift: Presence<Shift>,
}
object_deserialize!(Selector { at_revision:u64,document_snapshot_id:String,context_id:String,layer_uid:String,frame:u32,
    onion:bool,ghost_all:bool,from:u32,to:u32,previous_opacity:f64,next_opacity:f64,mode:Mode,
    #[serde(default)]previous_tint:Presence<String>,#[serde(default)]next_tint:Presence<String>,#[serde(default)]shift:Presence<Shift> });
#[derive(Debug, Clone)]
struct Shift {
    x: f64,
    y: f64,
    next: bool,
}
object_deserialize!(Shift {
    x: f64,
    y: f64,
    next: bool
});

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TweenGhostFrame {
    pub frame: u32,
    pub kind: TweenGhostKind,
    pub opacity: f64,
    pub translation: [f64; 2],
    #[serde(skip)]
    mode: Mode,
    #[serde(skip)]
    tint: [f64; 3],
}
/// Paint changes apply to the native rendered item at the ghost's source frame.
/// `None` stroke/width retain the evaluated source paint/width; clear_fill is explicit.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TweenGhostVisual {
    pub opacity: f64,
    pub stroke: Option<[f64; 4]>,
    pub stroke_width: Option<f64>,
    pub clear_fill: bool,
    pub translation: [f64; 2],
    pub omit: bool,
}
impl TweenGhostFrame {
    pub fn raster_visual(&self) -> TweenGhostVisual {
        TweenGhostVisual {
            opacity: self.opacity,
            stroke: None,
            stroke_width: None,
            clear_fill: false,
            translation: [0.; 2],
            omit: self.kind == TweenGhostKind::AllKeys,
        }
    }
    pub fn vector_visual(&self, r: &ObjectRecord) -> TweenGhostVisual {
        let mut out = self.raster_visual();
        out.translation = self.translation;
        out.omit = false;
        let body = r.vector.value();
        let painted = r.solid_fill().is_some() || body.is_some_and(|v| v.solid_stroke().is_some());
        if self.kind == TweenGhostKind::AllKeys {
            out.omit = !painted;
            out.clear_fill = true;
            out.stroke = Some([0.68, 0.6, 1., self.opacity * 1.6]);
            return out;
        }
        let texture = r
            .vector_ext
            .value()
            .and_then(|v| v.texture.value())
            .is_some_and(|t| t.is_copy.value() == Some(&true) || !t.preset.missing());
        if texture {
            out.opacity *= body.map_or(1., |v| v.opacity.as_f64().unwrap());
        }
        let can_tint =
            !texture && (body.is_some_and(|v| v.has_real_stroke) || r.solid_fill().is_some());
        if can_tint && self.mode != Mode::Default {
            let alpha = self.opacity * if self.mode == Mode::Outline { 0.8 } else { 1. };
            out.stroke = Some([self.tint[0], self.tint[1], self.tint[2], alpha]);
            if self.mode == Mode::Outline {
                out.clear_fill = true;
                out.stroke_width = Some(1.);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct TweenGhostRead {
    snapshot: ObjectSnapshot,
    layer_uid: String,
    frames: Vec<TweenGhostFrame>,
}
impl TweenGhostRead {
    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub fn frames(&self) -> &[TweenGhostFrame] {
        &self.frames
    }
    fn selector(&self, frame: u32) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: self.snapshot.instance_id(),
            document_id: self.snapshot.document_id(),
            content_revision: self.snapshot.content_revision(),
            document_snapshot_id: self.snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: crate::object_document::FrameScopeKind::Authored,
            frame,
        }
    }
    /// Ghost All reads authored key geometry. It deliberately does not run normal
    /// frame effects, layer Motion or duplicators. Combining groups use the native
    /// boolean provider and preserve that provider's explicit admission limits.
    pub fn all_keys_paths(&self, index: usize) -> Result<Vec<TweenGhostPath>, Error> {
        let frame = self.frames.get(index).ok_or(Error::InvalidRequest)?;
        if frame.kind != TweenGhostKind::AllKeys {
            return Err(Error::InvalidRequest);
        }
        geometry::paths(&self.snapshot, &self.layer_uid, frame)
    }
    /// A real pinned onion frame for existing native mixed/resource consumers.
    /// Ghost All must use all_keys_paths, never normal frame evaluation.
    pub fn prepare_packet(&self, index: usize) -> Result<ObjectFramePacket, Error> {
        let frame = self.frames.get(index).ok_or(Error::InvalidRequest)?;
        if frame.kind == TweenGhostKind::AllKeys {
            return Err(Error::InvalidRequest);
        }
        prepare_object_exposure_frame(&self.snapshot, &self.selector(frame.frame))
            .map_err(|_| Error::Unavailable)
    }
    pub fn prepare_brush_admission(&self, index: usize) -> Result<ObjectBrushAdmission, Error> {
        let packet = self.prepare_packet(index)?;
        ObjectBrushAdmission::prepare(&packet, &self.selector(packet.frame()))
            .map_err(|_| Error::Unavailable)
    }
    /// The overlay consumer selects this read's layer and applies its visual policy.
    /// Bitmap frames use prepare_brush_admission with the shared ObjectBrushRuntime.
    pub fn prepare_frame(&self, index: usize) -> Result<ObjectCompositionWitness, Error> {
        let packet = self.prepare_packet(index)?;
        let selector = self.selector(packet.frame());
        let scene =
            prepare_object_render_scene(&packet, &selector).map_err(|_| Error::Unavailable)?;
        let plan = prepare_object_paint_plan(&scene, &selector).map_err(|_| Error::Unavailable)?;
        prepare_object_composition(&plan, &selector).map_err(|_| Error::Unavailable)
    }
}
fn tint(value: Option<&String>, default: &str) -> Result<[f64; 3], Error> {
    let rgba = crate::object_document::bitmap_brush::rgba(value.map_or(default, String::as_str))
        .ok_or(Error::InvalidRequest)?;
    Ok([
        rgba[0] as f64 / 255.,
        rgba[1] as f64 / 255.,
        rgba[2] as f64 / 255.,
    ])
}
impl ObjectSnapshot {
    pub fn query_tween_ghosts_json(&self, bytes: &[u8]) -> Result<TweenGhostRead, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let s: Selector = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if s.at_revision != self.content_revision() || s.document_snapshot_id != self.snapshot_id()
        {
            return Err(Error::StaleRevision);
        }
        if s.context_id != "scene-root" {
            return Err(Error::Unavailable);
        }
        let doc = self.document();
        if s.frame >= doc.total_frames
            || s.from > s.frame
            || s.to < s.frame
            || s.to >= doc.total_frames
            || s.to - s.from > 4096
            || [s.previous_opacity, s.next_opacity]
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=100.).contains(v))
            || s.shift
                .value()
                .is_some_and(|s| !s.x.is_finite() || !s.y.is_finite())
        {
            return Err(Error::InvalidRequest);
        }
        let layer = doc
            .layers
            .iter()
            .find(|l| l.layer_uid == s.layer_uid)
            .ok_or(Error::NotFound)?;
        let timing = layer.timing.value();
        let component = doc
            .component_set()
            .is_some_and(|c| c.instance("scene-root", &s.layer_uid).is_some());
        let derived = component
            || doc
                .duplicator_set()
                .is_some_and(|d| d.duplicator("scene-root", &s.layer_uid).is_some());
        if timing.is_none() && !component {
            return Err(Error::Unavailable);
        }
        let prev = tint(s.previous_tint.value(), "#ff4d4d")?;
        let next = tint(s.next_tint.value(), "#4d8cff")?;
        let mut frames = vec![];
        if s.ghost_all && !component {
            for frame in timing
                .unwrap()
                .key_frames()
                .filter(|&f| f != s.frame && super::has_frame_content(doc, &s.layer_uid, f))
            {
                frames.push(TweenGhostFrame {
                    frame,
                    kind: TweenGhostKind::AllKeys,
                    opacity: (0.4 - (frame.abs_diff(s.frame) as f64) * 0.03).max(0.12),
                    translation: [0.; 2],
                    mode: Mode::Outline,
                    tint: [0.68, 0.6, 1.],
                });
            }
        }
        if s.onion {
            for (kind, indices, span, opacity, tint) in [
                (
                    TweenGhostKind::Previous,
                    (s.from..s.frame).rev().collect::<Vec<_>>(),
                    (s.frame - s.from).max(1),
                    s.previous_opacity,
                    prev,
                ),
                (
                    TweenGhostKind::Next,
                    (s.frame + 1..=s.to).collect(),
                    (s.to - s.frame).max(1),
                    s.next_opacity,
                    next,
                ),
            ] {
                for frame in indices {
                    let source = timing.and_then(|t| t.exposure(frame).1);
                    if !derived
                        && source.is_none_or(|source| {
                            !super::has_frame_content(doc, &s.layer_uid, source)
                        })
                    {
                        continue;
                    }
                    let translation = s
                        .shift
                        .value()
                        .filter(|s| kind == TweenGhostKind::Previous || s.next)
                        .map_or([0.; 2], |s| [s.x, s.y]);
                    frames.push(TweenGhostFrame {
                        frame,
                        kind,
                        opacity: opacity / 100.
                            * (1. - frame.abs_diff(s.frame) as f64 / span as f64 * 0.85).max(0.15),
                        translation,
                        mode: s.mode,
                        tint,
                    });
                }
            }
        }
        if frames.len() > 4096 {
            return Err(Error::Unavailable);
        }
        Ok(TweenGhostRead {
            snapshot: self.clone(),
            layer_uid: s.layer_uid,
            frames,
        })
    }
}
