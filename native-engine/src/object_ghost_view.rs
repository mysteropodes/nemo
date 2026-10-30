//! #2358 D23: the onion / Ghost All presentation view of the existing object read (`view.kind` `ghosts`).
//!
//! The host proves that the request names its actual successful presentation (work, generation, context and frame); the
//! ghost frames come from the immutable snapshot's retained ghost query (`tween_ghost_query.rs`), so a result is pinned to the
//! revision it was read at and a changed revision or frame is refused as stale. One page holds the stored vector geometry of
//! ONE ghost frame, as many segments as the response byte limit allows; `next` names the following page. Nothing here writes.
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchErrorCode, GhostFrameFacts, GhostFrames, GhostPathFacts, OpacityRequest,
    ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde_json::{json, Map, Value};
use std::any::Any;
use std::sync::{Arc, Mutex, Weak};

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

/// The drawable paths of the ghost frames a read pages through. Every page is its own request, and the frame geometry
/// (stored paths, combine booleans) is the same for all of them, so it is built once per (revision, selector, frame).
type Drawable = Arc<Vec<GhostPathFacts>>;
struct Memo {
    revision: Weak<dyn Any + Send + Sync>,
    selector: Vec<u8>,
    ghost: usize,
    paths: Drawable,
}
/// Small and recency-ordered: one active read plus a concurrent one. A dead revision is dropped on the next access.
const MEMO_ENTRIES: usize = 4;
static MEMOS: Mutex<Vec<Memo>> = Mutex::new(Vec::new());

impl ObjectSnapshot {
    fn memo_paths(
        &self,
        selector: &[u8],
        ghost: usize,
        build: impl FnOnce() -> Result<Vec<GhostPathFacts>, DispatchErrorCode>,
    ) -> Result<Drawable, DispatchErrorCode> {
        let revision = self.revision_identity();
        let hit = MEMOS.lock().ok().and_then(|mut memos| {
            memos.retain(|m| m.revision.strong_count() > 0);
            let at = memos.iter().position(|m| {
                m.ghost == ghost && m.selector == selector && Weak::ptr_eq(&m.revision, &revision)
            })?;
            let memo = memos.remove(at);
            let paths = Arc::clone(&memo.paths);
            memos.insert(0, memo);
            Some(paths)
        });
        if let Some(paths) = hit {
            return Ok(paths);
        }
        let paths: Drawable = Arc::new(build()?);
        if let Ok(mut memos) = MEMOS.lock() {
            memos.insert(
                0,
                Memo {
                    revision,
                    selector: selector.to_vec(),
                    ghost,
                    paths: Arc::clone(&paths),
                },
            );
            memos.truncate(MEMO_ENTRIES);
        }
        Ok(paths)
    }
}

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
fn segment(s: &[f64; 6]) -> Value {
    if s[2..].iter().all(|v| *v == 0.) {
        json!([round(s[0]), round(s[1])])
    } else {
        json!(s.iter().map(|v| round(*v)).collect::<Vec<_>>())
    }
}
fn descriptor(index: usize, count: usize, f: &GhostFrameFacts) -> Value {
    json!({"index":index,"count":count,"frame":f.frame,"kind":f.kind,"opacity":scaled(f.opacity, 1000.),
        "translation":[round(f.translation[0]),round(f.translation[1])]})
}
/// The pieces of the segment stream `[start, end)` of a ghost frame, one per path they touch.
fn pieces(paths: &[GhostPathFacts], start: u64, end: u64) -> Vec<Value> {
    let (mut out, mut at) = (vec![], 0u64);
    for (i, p) in paths.iter().enumerate() {
        let n = p.segments.len() as u64;
        let (lo, hi) = (at.max(start), (at + n).min(end));
        let base = at;
        at += n;
        if lo >= hi {
            continue;
        }
        let segs: Vec<Value> = p.segments[(lo - base) as usize..(hi - base) as usize]
            .iter()
            .map(segment)
            .collect();
        out.push(json!({"path":i,"from":lo - base,"count":n,"sourceId":p.source_id,"closed":p.closed,
            "width":round(p.width),"fill":p.fill,"stroke":p.stroke,"opacity":scaled(p.opacity, 1000.),"segments":segs}));
    }
    out
}

impl ObjectSnapshot {
    /// `presented` is the host's proof: `(workId, viewGeneration, contextId, frame)` of its actual presentation.
    /// `query` is the application layer's retained ghost query over the validated selector bytes.
    pub fn dispatch_ghost_view_with<'a>(
        &'a self,
        request: OpacityRequest,
        presented: Result<(&str, u64, &str, u32), &str>,
        query: &dyn Fn(&[u8]) -> Result<Box<dyn GhostFrames + 'a>, DispatchErrorCode>,
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
        let read = match query(&bytes) {
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
        let count = read.count();
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
        let frame_info = read.frame(ghost);
        let drawable = match self.memo_paths(&bytes, ghost, || read.paths(ghost)) {
            Ok(paths) => paths,
            Err(code) => return refused(code, "Ghost geometry is unavailable"),
        };
        let total: u64 = drawable.iter().map(|p| p.segments.len() as u64).sum();
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
        let descriptor = descriptor(ghost, count, &frame_info);
        let page = |end: u64| -> Result<(ResponseEnvelope, usize), DispatchErrorCode> {
            let response = envelope(head(
                descriptor.clone(),
                total,
                after(end),
                pieces(&drawable, cursor, end),
            ));
            let size = serde_json::to_vec(&response)
                .map_err(|_| DispatchErrorCode::Internal)?
                .len();
            Ok((response, size))
        };
        // Fill the byte limit: shrink proportionally until the page fits, and grow a page that leaves room (a line
        // segment is a few bytes, a cubic dozens) so a large frame needs few pages. The last page that fits wins.
        let mut end = (cursor + FIRST_TRY as u64).min(total);
        let mut best: Option<ResponseEnvelope> = None;
        for _ in 0..8 {
            let (response, size) = page(end)?;
            let len = end - cursor;
            if size <= MAX_MESSAGE_BYTES {
                let grow = end < total && size * 10 < MAX_MESSAGE_BYTES * 8;
                let fits = end;
                best = Some(response);
                if !grow {
                    break;
                }
                let want = len * (MAX_MESSAGE_BYTES * 9 / 10) as u64 / size as u64;
                end = (cursor + want.max(len + 1)).min(total);
                if end == fits {
                    break;
                }
            } else {
                if best.is_some() {
                    break;
                }
                if len <= 1 {
                    return refused(
                        DispatchErrorCode::Unavailable,
                        "Ghost view exceeds the response byte limit",
                    );
                }
                let want = len * (MAX_MESSAGE_BYTES * 9 / 10) as u64 / size as u64;
                end = cursor + want.clamp(1, len - 1);
            }
        }
        match best {
            Some(response) => Ok(response),
            None => refused(
                DispatchErrorCode::Unavailable,
                "Ghost view exceeds the response byte limit",
            ),
        }
    }
}
