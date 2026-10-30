//! #2358 D23: the onion / Ghost All presentation view of the existing object read (`view.kind` `ghosts`).
//!
//! The host proves that the request names its actual successful presentation (work, generation, context and frame); the
//! ghost frames come from the immutable snapshot's retained ghost query (`tween_ghost_query.rs`), so a result is pinned to the
//! revision it was read at and a changed revision or frame is refused as stale. One page holds the stored vector geometry of
//! ONE ghost frame, as many segments as the response byte limit allows; `next` names the following page. Nothing here writes.
use crate::application::{TweenGhostFrame, TweenGhostPath};
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde_json::{json, Map, Value};

const REQUIRED: [&str; 15] = [
    "kind",
    "contextId",
    "frame",
    "workId",
    "viewGeneration",
    "layerUid",
    "onion",
    "ghostAll",
    "from",
    "to",
    "previousOpacity",
    "nextOpacity",
    "mode",
    "ghost",
    "cursor",
];
const OPTIONAL: [&str; 3] = ["previousTint", "nextTint", "shift"];
/// Segments of one ghost frame addressable by `cursor`.
const MAX_SEGMENTS: u64 = 262_144;
const MAX_GHOSTS: u64 = 4_096;
/// Segments tried for a first page; the page shrinks until the whole response fits the byte limit.
const FIRST_TRY: usize = 160;

fn round(v: f64) -> Value {
    scaled(v, 100.)
}
fn scaled(v: f64, scale: f64) -> Value {
    let r = (v * scale).round() / scale;
    if r.fract() == 0. && r.abs() < 9e15 {
        json!(r as i64)
    } else {
        json!(r)
    }
}
fn segment(s: &crate::object_document::ObjectSegment) -> Value {
    let n =
        |p: &crate::object_document::ObjectPoint| (p.x.as_f64().unwrap(), p.y.as_f64().unwrap());
    let (x, y) = n(&s.point);
    let ((ix, iy), (ox, oy)) = (n(&s.handle_in), n(&s.handle_out));
    if ix == 0. && iy == 0. && ox == 0. && oy == 0. {
        json!([round(x), round(y)])
    } else {
        json!([
            round(x),
            round(y),
            round(ix),
            round(iy),
            round(ox),
            round(oy)
        ])
    }
}
fn hex(rgba: [f64; 4]) -> String {
    let c = |v: f64| (v.clamp(0., 1.) * 255.).round() as u32;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        c(rgba[0]),
        c(rgba[1]),
        c(rgba[2]),
        c(rgba[3])
    )
}
/// The presented paint of one ghost path: an override of the source paint, or the source paint itself.
fn meta(path: &TweenGhostPath) -> (Option<String>, Option<String>, f64) {
    let v = &path.visual;
    let stroke = v.stroke.map(hex).or_else(|| path.stroke.clone());
    let fill = if v.clear_fill {
        None
    } else {
        path.fill.clone()
    };
    (fill, stroke, v.stroke_width.unwrap_or(path.stroke_width))
}
fn descriptor(index: usize, count: usize, f: &TweenGhostFrame) -> Value {
    json!({"index":index,"count":count,"frame":f.frame,"kind":f.kind,"opacity":scaled(f.opacity, 1000.),
        "translation":[round(f.translation[0]),round(f.translation[1])]})
}
/// The pieces of the segment stream `[start, end)` of a ghost frame, one per path they touch.
fn pieces(paths: &[&TweenGhostPath], start: u64, end: u64) -> Vec<Value> {
    let (mut out, mut at) = (vec![], 0u64);
    for (i, p) in paths.iter().enumerate() {
        let n = p.geometry.segments.len() as u64;
        let (lo, hi) = (at.max(start), (at + n).min(end));
        at += n;
        if lo >= hi {
            continue;
        }
        let (fill, stroke, width) = meta(p);
        let segs: Vec<Value> = p.geometry.segments
            [(lo - (at - n)) as usize..(hi - (at - n)) as usize]
            .iter()
            .map(segment)
            .collect();
        out.push(json!({"path":i,"from":lo - (at - n),"count":n,"sourceId":p.source_id,"closed":p.geometry.closed,
            "width":round(width),"fill":fill,"stroke":stroke,"opacity":scaled(p.visual.opacity, 1000.),"segments":segs}));
    }
    out
}

impl ObjectSnapshot {
    /// `presented` is the host's proof: `(workId, viewGeneration, contextId, frame)` of its actual presentation.
    pub fn dispatch_with_ghost_view(
        &self,
        request: OpacityRequest,
        presented: Result<(&str, u64, &str, u32), &str>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let ReadAdmission::Refused(response) = self.read_admission(&request)? {
            return Ok(response);
        }
        let refused = |code, message| Ok(self.read_failure(&request, code, message));
        let invalid = || {
            refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid ghost view selector",
            )
        };
        let (Some(payload), Some(view)) = (
            request.payload.as_object(),
            request.payload.get("view").and_then(Value::as_object),
        ) else {
            return invalid();
        };
        let shape = payload.len() == 3
            && payload.contains_key("atRevision")
            && payload.contains_key("documentSnapshotId")
            && REQUIRED.iter().all(|k| view.contains_key(*k))
            && view
                .keys()
                .all(|k| REQUIRED.contains(&k.as_str()) || OPTIONAL.contains(&k.as_str()));
        let str_of = |k: &str| view.get(k).and_then(Value::as_str);
        let num = |k: &str| view.get(k).and_then(Value::as_u64);
        let (Some(at), Some(snapshot_id)) = (
            payload.get("atRevision").and_then(Value::as_u64),
            payload.get("documentSnapshotId").and_then(Value::as_str),
        ) else {
            return invalid();
        };
        let (Some(work), Some(generation), Some(frame), Some(ghost), Some(cursor)) = (
            str_of("workId"),
            num("viewGeneration"),
            num("frame"),
            num("ghost"),
            num("cursor"),
        ) else {
            return invalid();
        };
        if !shape
            || request.operation != "query.document.object"
            || str_of("kind") != Some("ghosts")
            || !(1..=9_007_199_254_740_991).contains(&generation)
            || frame > u32::MAX as u64
            || ghost >= MAX_GHOSTS
            || cursor > MAX_SEGMENTS
        {
            return invalid();
        }
        if at != self.content_revision() || snapshot_id != self.snapshot_id() {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Ghost view origin changed",
            );
        }
        let (p_work, p_generation, p_context, p_frame) = match presented {
            Ok(proof) => proof,
            Err(reason) => return refused(DispatchErrorCode::Unavailable, reason),
        };
        if p_work != work
            || p_generation != generation
            || p_context != str_of("contextId").unwrap_or_default()
            || p_frame as u64 != frame
        {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Ghost presentation changed",
            );
        }
        // The retained query validates and owns every remaining selector field.
        let mut selector = Map::new();
        selector.insert("atRevision".into(), json!(at));
        selector.insert("documentSnapshotId".into(), json!(snapshot_id));
        for (key, value) in view {
            if !["kind", "workId", "viewGeneration", "ghost", "cursor"].contains(&key.as_str()) {
                selector.insert(key.clone(), value.clone());
            }
        }
        let bytes = serde_json::to_vec(&selector).map_err(|_| DispatchErrorCode::Internal)?;
        let read = match self.query_tween_ghosts_json(&bytes) {
            Ok(read) => read,
            Err(DispatchErrorCode::Unavailable) => {
                return refused(
                    DispatchErrorCode::Unavailable,
                    "Ghost frames are unavailable for this layer",
                )
            }
            Err(DispatchErrorCode::NotFound) => {
                return refused(DispatchErrorCode::NotFound, "Ghost layer was not found")
            }
            Err(DispatchErrorCode::StaleRevision) => {
                return refused(
                    DispatchErrorCode::StaleRevision,
                    "Ghost view origin changed",
                )
            }
            Err(_) => return invalid(),
        };
        let count = read.frames().len();
        let ghost = ghost as usize;
        let envelope = |result: Value| ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(result),
        };
        let head = |descriptor: Value, total: u64, next: Value, items: Vec<Value>| {
            json!({"kind":"ghosts","atRevision":at,"documentSnapshotId":snapshot_id,"contextId":"scene-root",
                "frame":frame,"workId":work,"viewGeneration":generation,"layerUid":read.layer_uid(),"ghost":descriptor,
                "cursor":cursor,"total":total,"next":next,"items":items})
        };
        if count == 0 {
            if ghost != 0 || cursor != 0 {
                return invalid();
            }
            return Ok(envelope(head(Value::Null, 0, Value::Null, vec![])));
        }
        if ghost >= count {
            return invalid();
        }
        let frame_info = &read.frames()[ghost];
        let paths = match read.frame_paths(ghost) {
            Ok(paths) => paths,
            Err(code) => return refused(code, "Ghost geometry is unavailable"),
        };
        // Nothing is shown for a path with no segments, no paint left, or no opacity (a texture anchor hidden by its dabs).
        let drawable: Vec<&TweenGhostPath> = paths
            .iter()
            .filter(|p| {
                let (fill, stroke, _) = meta(p);
                !p.geometry.segments.is_empty()
                    && p.visual.opacity > 0.
                    && (fill.is_some() || stroke.is_some())
            })
            .collect();
        let total: u64 = drawable
            .iter()
            .map(|p| p.geometry.segments.len() as u64)
            .sum();
        if total > MAX_SEGMENTS {
            return refused(
                DispatchErrorCode::Unavailable,
                "Ghost frame exceeds the segment limit",
            );
        }
        if cursor > total {
            return invalid();
        }
        let after = |end: u64| -> Value {
            if end < total {
                json!({"ghost":ghost,"cursor":end})
            } else if ghost + 1 < count {
                json!({"ghost":ghost + 1,"cursor":0})
            } else {
                Value::Null
            }
        };
        let descriptor = descriptor(ghost, count, frame_info);
        let mut end = (cursor + FIRST_TRY as u64).min(total);
        loop {
            let response = envelope(head(
                descriptor.clone(),
                total,
                after(end),
                pieces(&drawable, cursor, end),
            ));
            let size = serde_json::to_vec(&response)
                .map_err(|_| DispatchErrorCode::Internal)?
                .len();
            if size <= MAX_MESSAGE_BYTES {
                return Ok(response);
            }
            if end <= cursor + 1 {
                return refused(
                    DispatchErrorCode::Unavailable,
                    "Ghost view exceeds the response byte limit",
                );
            }
            end = cursor + ((end - cursor) * 3 / 4).max(1);
        }
    }
}
