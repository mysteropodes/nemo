//! Shared builders for the brush producer tests (L07-J; G17): real v2 documents holding
//! `editable-vector-path` anchors, real packets, a real charged store, and the two
//! external collaborators of a trim re-bake (the only providers that do not exist natively
//! yet): a scripted trim/bake provider and a baker that paints the L07-E stamp plan.
//!
//! Imports are absolute (`crate::...`) so the file can be included from more than one
//! module tree (the producer tests and the Metal consumer test).

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_stamp::{
    plan_stamps, StampPath, StampSpec, WorldRect,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::{
    BrushBitmap, BrushBitmapStore,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush::{
    produce, BrushContext, BrushInk, BrushProduceError, BrushProduction, BrushStrokeScene,
    BrushStrokeSource, InkRef, InkSource, InkState, TrimSetup,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_anchor::{
    camouflage, AnchorRestore, AnchorState, Remembered,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
    BakeRefused, BakedInk, TrimAnchor, TrimProvider, TrimSlots, TrimWindow, TrimmedPath,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinnedScene, SceneJoinError, SceneOutcome, ScenePin,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use serde_json::{json, Value};

pub(crate) const FRAME: u32 = 7;

/// One anchor record of a test document. Every anchor is the OPEN centerline
/// `(10, 10) -> (end_x, 10)` (a brush stroke is an open vector path; `end_x` is 110 unless a
/// test edits it): the trim math of the tests derives its sub-path from exactly these
/// segments, so a re-bake always lies on the pinned record's geometry.
///
/// The record carries what baseline `serP` stores after `applyBitmapBrushTexture`: a filled
/// anchor has NO stroke and opacity 1; an unfilled one keeps its red stroke at opacity 0.
/// `vector_stroke` / `vector_opacity` can be overwritten to build a record that contradicts it.
#[derive(Clone, Copy)]
pub(crate) struct Anchor {
    pub(crate) layer: &'static str,
    pub(crate) stroke: &'static str,
    /// `r, g, b, a` of the record fill; `None` is an explicit null fill.
    pub(crate) fill: Option<[f64; 4]>,
    pub(crate) visible: bool,
    /// The x of the centerline's second point (the first is `(10, 10)`).
    pub(crate) end_x: f64,
    /// The record's vector-body stroke; `None` is an explicit null (and `hasRealStroke` false).
    pub(crate) vector_stroke: Option<[f64; 4]>,
    /// The record's vector-body opacity.
    pub(crate) vector_opacity: f64,
}

pub(crate) const RED: [f64; 4] = [1.0, 0.0, 0.0, 1.0];

pub(crate) fn anchor(layer: &'static str, stroke: &'static str, fill: Option<[f64; 4]>) -> Anchor {
    Anchor {
        layer,
        stroke,
        fill,
        visible: true,
        end_x: 110.0,
        vector_stroke: fill.is_none().then_some(RED),
        vector_opacity: if fill.is_some() { 1.0 } else { 0.0 },
    }
}

fn record(frame: u32, anchor: &Anchor) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let paint = |color: Option<[f64; 4]>| {
        color
            .map(|[r, g, b, a]| json!({"kind":"solid","r":r,"g":g,"b":b,"a":a}))
            .unwrap_or(Value::Null)
    };
    let fill = paint(anchor.fill);
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
            "layerUid":anchor.layer,"strokeId":anchor.stroke},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(10.0,10.0),corner(anchor.end_x,10.0)]},
        "fill":fill,
        "vector":{"stroke":paint(anchor.vector_stroke),"hasRealStroke":anchor.vector_stroke.is_some(),
            "strokeWidth":4,"strokeCap":"round","strokeJoin":"round",
            "miterLimit":10,"dashOffset":0,"opacity":anchor.vector_opacity}})
}

/// Layers appear once, in first-use order; objects and plan elements are stored reversed so
/// array position never equals paint order.
pub(crate) fn document(frame: u32, anchors: &[Anchor]) -> Value {
    let mut layer_uids: Vec<&str> = Vec::new();
    for anchor in anchors {
        if !layer_uids.contains(&anchor.layer) {
            layer_uids.push(anchor.layer);
        }
    }
    let mut objects = Vec::new();
    let mut elements = Vec::new();
    for uid in &layer_uids {
        for (k, anchor) in anchors.iter().filter(|a| a.layer == *uid).enumerate() {
            objects.push(record(frame, anchor));
            elements.push(json!({"layerUid":anchor.layer,"strokeId":anchor.stroke,"naturalPosition":k,"order":{}}));
        }
    }
    objects.reverse();
    elements.reverse();
    let layers: Vec<Value> = layer_uids
        .iter()
        .rev()
        .map(|uid| {
            let hidden = anchors.iter().any(|a| a.layer == *uid && !a.visible);
            let mut layer = json!({ "layerUid": uid });
            if hidden {
                layer["visible"] = json!(false);
            }
            layer
        })
        .collect();
    let plan_layers: Vec<Value> = layer_uids
        .iter()
        .enumerate()
        .map(|(j, uid)| json!({"layerUid":uid,"naturalPosition":j,"order":{}}))
        .collect();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":layers,"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":plan_layers,
        "scopes":[{"frameScope":{"kind":"authored","frame":frame},"elements":elements}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}

pub(crate) fn snapshot(value: &Value, instance: &str) -> ObjectSnapshot {
    ObjectSnapshot::new(
        instance,
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn packet(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        },
    )
    .unwrap()
}

/// A real packet over `anchors` at `frame`, with its snapshot.
pub(crate) fn world(frame: u32, anchors: &[Anchor]) -> (ObjectSnapshot, ObjectFramePacket) {
    let snap = snapshot(&document(frame, anchors), "brush-scene");
    let pk = packet(&snap, frame);
    (snap, pk)
}

/// A real store over an isolated budget of `limit` bytes.
pub(crate) fn store(limit: usize) -> BrushBitmapStore {
    BrushBitmapStore::with_budget(ImageResourceBudget::isolated(limit).unwrap())
}

pub(crate) fn used(store: &BrushBitmapStore) -> usize {
    store.budget().used().unwrap()
}

/// `w * h` identical RGBA8 pixels.
pub(crate) fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    (0..w * h).flat_map(|_| rgba).collect()
}

pub(crate) fn spec() -> StampSpec {
    StampSpec {
        size: 6.0,
        spacing: 100.0,
        scatter: 0.0,
        seed: 1.0,
        opacity: 1.0,
        pressure: false,
    }
}

pub(crate) fn rect(min_x: f64, min_y: f64, w: f64, h: f64) -> WorldRect {
    WorldRect { min_x, min_y, w, h }
}

pub(crate) fn window(start: f64, end: f64) -> TrimWindow {
    TrimWindow {
        start,
        end,
        offset: 0.0,
    }
}

/// The remembered members of an anchor after `applyBitmapBrushTexture` for an authored red
/// stroke (the edit-side kernel; the drawn paint comes from the record, see `Anchor`).
pub(crate) fn stored_anchor(has_fill: bool) -> AnchorRestore {
    camouflage(
        has_fill,
        &AnchorState {
            stroke: Some("#ff0000".to_owned()),
            opacity: Some(1.0),
            pre_stroke: Remembered::Unset,
            pre_opacity: Remembered::Unset,
        },
    )
    .restore()
}

/// A brush source for `layer/stroke` whose untrimmed ink is `brush_id` over `ink_rect`.
pub(crate) fn source(
    layer: &str,
    stroke: &str,
    group: &str,
    has_fill: bool,
    brush_id: &str,
    ink_rect: WorldRect,
) -> BrushStrokeSource {
    BrushStrokeSource {
        layer_uid: layer.to_owned(),
        stroke_id: stroke.to_owned(),
        group_id: group.to_owned(),
        restore: stored_anchor(has_fill),
        ink: InkSource::Resident(InkRef {
            brush_id: brush_id.to_owned(),
            rect: ink_rect,
        }),
        ink_opacity: 1.0,
        trim: None,
    }
}

pub(crate) fn with_trim(
    mut source: BrushStrokeSource,
    window: Option<TrimWindow>,
) -> BrushStrokeSource {
    source.trim = Some(TrimSetup {
        spec: spec(),
        pressure_profile: None,
        window,
    });
    source
}

fn seg(x: f64, y: f64) -> SegmentIn {
    SegmentIn {
        point: [x, y],
        handle_in: None,
        handle_out: None,
    }
}

/// The point `d` along the polyline `pts` (segment lengths `lens`).
fn point_at(pts: &[[f64; 2]], lens: &[f64], d: f64) -> [f64; 2] {
    let mut walked = 0.0;
    for (at, len) in lens.iter().enumerate() {
        if d <= walked + len || at + 1 == lens.len() {
            let (a, b, t) = (pts[at], pts[at + 1], d - walked);
            return [
                a[0] + (b[0] - a[0]) * t / len,
                a[1] + (b[1] - a[1]) * t / len,
            ];
        }
        walked += len;
    }
    pts[0]
}

/// Trim math derived from the RECEIVED anchor (`SMMotion.applyTrimFor` stand-in): the window
/// is a percentage of the anchor's straight-segment arc length (a closed anchor includes its
/// closing segment); a window with `end <= start` leaves no geometry. For the default anchor
/// `(10, 10) -> (110, 10)` the window `(s, e)` yields the line `(10 + s, 10)..(10 + e, 10)`.
pub(crate) fn trimmed_line(anchor: &TrimAnchor, window: &TrimWindow) -> Option<TrimmedPath> {
    let mut pts: Vec<[f64; 2]> = anchor.segments.iter().map(|s| s.point).collect();
    if anchor.closed {
        pts.push(pts[0]);
    }
    let lens: Vec<f64> = pts
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .collect();
    let total: f64 = lens.iter().sum();
    let (from, to) = (window.start * total / 100.0, window.end * total / 100.0);
    if !(to > from) {
        return None;
    }
    let (from, to) = (from.max(0.0), to.min(total));
    let mut cut = vec![point_at(&pts, &lens, from)];
    let mut walked = 0.0;
    for (at, len) in lens.iter().enumerate() {
        walked += len;
        if walked > from && walked < to && at + 1 < lens.len() {
            cut.push(pts[at + 1]);
        }
    }
    cut.push(point_at(&pts, &lens, to));
    Some(TrimmedPath {
        segments: cut.into_iter().map(|[x, y]| seg(x, y)).collect(),
        closed: false,
    })
}

/// A baker that paints the L07-E plan of the trimmed path: opaque squares in `color` at
/// each planned stamp, on the planned canvas, over the planned world rectangle. `skew`
/// makes it publish a canvas one pixel too wide (a baker that disagrees with the plan).
pub(crate) struct PlanBaker {
    pub(crate) color: [u8; 4],
    pub(crate) skew: bool,
    pub(crate) refuse: Option<&'static str>,
    pub(crate) trims: usize,
    pub(crate) bakes: usize,
    /// The anchor each `trimmed` call received.
    pub(crate) anchors: Vec<TrimAnchor>,
}

impl PlanBaker {
    pub(crate) fn new(color: [u8; 4]) -> Self {
        Self {
            color,
            skew: false,
            refuse: None,
            trims: 0,
            bakes: 0,
            anchors: Vec::new(),
        }
    }
}

impl TrimProvider for PlanBaker {
    fn trimmed(&mut self, anchor: &TrimAnchor, window: &TrimWindow) -> Option<TrimmedPath> {
        self.trims += 1;
        self.anchors.push(anchor.clone());
        trimmed_line(anchor, window)
    }

    fn bake(&mut self, path: &TrimmedPath) -> Result<Option<BakedInk>, BakeRefused> {
        self.bakes += 1;
        if let Some(cause) = self.refuse {
            return Err(BakeRefused(cause));
        }
        let plan = plan_stamps(
            &spec(),
            &StampPath {
                segments: &path.segments,
                closed: path.closed,
                pressure_profile: None,
            },
        )
        .expect("plan of a two-point line");
        let (w, h) = (plan.texture.pixel_width, plan.texture.pixel_height);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for stamp in &plan.stamps {
            let x0 = stamp.left().round().max(0.0) as u32;
            let y0 = stamp.top().round().max(0.0) as u32;
            let x1 = ((stamp.left() + stamp.ps).round().max(0.0) as u32).min(w);
            let y1 = ((stamp.top() + stamp.ps).round().max(0.0) as u32).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let at = ((y * w + x) * 4) as usize;
                    rgba[at..at + 4].copy_from_slice(&self.color);
                }
            }
        }
        let width = if self.skew { w + 1 } else { w };
        if self.skew {
            rgba = vec![0u8; (width * h * 4) as usize];
        }
        Ok(Some(BakedInk {
            width,
            height: h,
            rgba,
            rect: plan.texture.rect,
        }))
    }
}

/// Produce with a fresh frame build around it, no retire port.
pub(crate) fn run(
    pk: &ObjectFramePacket,
    sources: &[BrushStrokeSource],
    store: &mut BrushBitmapStore,
    slots: &mut TrimSlots,
    provider: &mut dyn TrimProvider,
) -> Result<BrushProduction, BrushProduceError> {
    store.begin_build();
    let mut ctx = BrushContext {
        store,
        slots,
        provider,
        retire: None,
    };
    produce(pk, sources, &mut ctx)
}

/// Publish `w x h` identical pixels under `id` (a new version when the id exists).
pub(crate) fn publish(
    store: &mut BrushBitmapStore,
    id: &str,
    w: u32,
    h: u32,
    rgba: [u8; 4],
) -> BrushBitmap {
    store
        .publish(id, w, h, &solid(w, h, rgba), None)
        .expect("publish a test bitmap")
}

/// Join the brush production alone onto its own packet.
pub(crate) fn join(
    pk: &ObjectFramePacket,
    production: BrushProduction,
) -> Result<PinnedScene<BrushStrokeScene>, SceneJoinError> {
    join_scene(pk, &ScenePin::of_packet(pk), vec![production.output])
}

/// The brush payloads of a scene in plan order; panics on an unavailable entry.
pub(crate) fn payloads(scene: &PinnedScene<BrushStrokeScene>) -> Vec<&BrushStrokeScene> {
    scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item { payload, .. } => payload,
            SceneOutcome::Unavailable(cause) => panic!("unavailable: {cause:?}"),
        })
        .collect()
}

/// The drawn ink of a payload; panics on a collapsed one.
pub(crate) fn ink_of(payload: &BrushStrokeScene) -> &BrushInk {
    match &payload.ink {
        InkState::Ink(ink) => ink,
        InkState::Collapsed => panic!("collapsed ink"),
    }
}
