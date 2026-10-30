//! #2358 D23: the ghosts view of the object read pages stored ghost geometry, keeps its snapshot after owner edits and
//! refuses a changed revision, presentation or selector. It never writes.
use super::*;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};

const PROOF: (&str, u64, &str, u32) = ("native-work-9", 3, "scene-root", 2);

fn view(ghost: u64, cursor: u64, mode: &str) -> Value {
    json!({"kind":"ghosts","contextId":"scene-root","frame":2,"workId":"native-work-9","viewGeneration":3,"layerUid":"L0",
        "onion":true,"ghostAll":true,"from":0,"to":5,"previousOpacity":30,"nextOpacity":40,"mode":mode,
        "previousTint":"#aabbcc","nextTint":"#112244","shift":{"x":4,"y":-3,"next":true},"ghost":ghost,"cursor":cursor})
}
fn query(snapshot: &ObjectSnapshot, view: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"ghost-read","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"view":view}}))
    .unwrap()
}
fn ask(
    snapshot: &ObjectSnapshot,
    view: Value,
    proof: Result<(&str, u64, &str, u32), &str>,
) -> ResponseEnvelope {
    let response = snapshot
        .dispatch_with_ghost_view(query(snapshot, view), proof)
        .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn code_of(response: ResponseEnvelope) -> DispatchErrorCode {
    assert!(!response.ok && response.result.is_none());
    response.error.unwrap().code()
}
fn frames_of(frames: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("ghost-view", document(frames, true)).unwrap()
}
fn oracle_frames() -> Value {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/tween-baseline/oracle.json")).unwrap();
    oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["operation"] == "ghosts")
        .unwrap()["input"]["frames"]
        .clone()
}
/// Every page of every ghost frame, as `(descriptor, [(path, segment count, segments seen)])`.
fn drain(snapshot: &ObjectSnapshot, mode: &str) -> Vec<(Value, Vec<(u64, u64, u64, Value)>)> {
    let (mut at, mut out, mut guard) = ((0u64, 0u64), vec![], 0);
    loop {
        guard += 1;
        assert!(guard < 500, "paging does not terminate");
        let response = ask(snapshot, view(at.0, at.1, mode), Ok(PROOF));
        assert!(response.ok, "{:?}", response.error);
        let r = response.result.unwrap();
        assert_eq!(r["cursor"], at.1);
        if r["ghost"].is_null() {
            return out;
        }
        if out.len() as u64 <= at.0 {
            out.push((r["ghost"].clone(), vec![]));
        }
        let (descriptor, paths) = out.last_mut().unwrap();
        assert_eq!(*descriptor, r["ghost"]);
        for item in r["items"].as_array().unwrap() {
            let (path, from, count) = (
                item["path"].as_u64().unwrap(),
                item["from"].as_u64().unwrap(),
                item["count"].as_u64().unwrap(),
            );
            let seen = item["segments"].as_array().unwrap().len() as u64;
            match paths.iter_mut().find(|p| p.0 == path) {
                Some(p) => {
                    assert_eq!(p.2, from, "pieces of one path are contiguous");
                    p.2 += seen;
                }
                None => {
                    assert_eq!(from, 0);
                    paths.push((path, count, seen, item.clone()));
                }
            }
        }
        match r["next"].as_object() {
            None => return out,
            Some(next) => {
                at = (
                    next["ghost"].as_u64().unwrap(),
                    next["cursor"].as_u64().unwrap(),
                )
            }
        }
    }
}

#[test]
fn native_ghost_view_returns_the_neighbour_and_key_frames_of_the_original_policy() {
    let owner = frames_of(&oracle_frames());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let read = snapshot
        .query_tween_ghosts_json(&serde_json::to_vec(&json!({"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"contextId":"scene-root",
            "layerUid":"L0","frame":2,"onion":true,"ghostAll":true,"from":0,"to":5,"previousOpacity":30,"nextOpacity":40,"mode":"tinted",
            "previousTint":"#aabbcc","nextTint":"#112244","shift":{"x":4,"y":-3,"next":true}})).unwrap())
        .unwrap();
    let paged = drain(&snapshot, "tinted");
    assert_eq!(paged.len(), read.frames().len());
    assert!(
        paged.iter().any(|(d, _)| d["kind"] == "allKeys")
            && paged.iter().any(|(d, _)| d["kind"] == "previous")
            && paged.iter().any(|(d, _)| d["kind"] == "next")
    );
    for (g, (descriptor, paths)) in paged.iter().enumerate() {
        let want = &read.frames()[g];
        assert_eq!(
            (
                descriptor["frame"].as_u64(),
                descriptor["index"].as_u64(),
                descriptor["count"].as_u64()
            ),
            (
                Some(want.frame as u64),
                Some(g as u64),
                Some(paged.len() as u64)
            )
        );
        assert!((descriptor["opacity"].as_f64().unwrap() - want.opacity).abs() < 1e-3);
        let direct = read.frame_paths(g).unwrap();
        let shown = |p: &&crate::application::TweenGhostPath| {
            p.visual.opacity > 0.
                && (p.fill.is_some() || p.stroke.is_some() || p.visual.stroke.is_some())
        };
        let direct: Vec<_> = direct.iter().filter(shown).cloned().collect();
        assert_eq!(paths.len(), direct.len(), "ghost {g}");
        for ((_, count, seen, first), expected) in paths.iter().zip(direct.iter()) {
            assert_eq!(
                (*count, *seen),
                (expected.geometry.segments.len() as u64, *count),
                "ghost {g}"
            );
            assert_eq!(first["sourceId"], expected.source_id);
        }
    }
    // The brush-texture anchor of frame 0 is invisible (omitted) while a plain stroke is drawn tinted by the previous-frame tint.
    let previous = paged.iter().find(|(d, _)| d["kind"] == "previous").unwrap();
    let plain = previous
        .1
        .iter()
        .find(|p| p.3["sourceId"] == "a")
        .expect("plain stroke ghosted");
    assert!(
        plain.3["stroke"].as_str().unwrap().starts_with("#aabbcc"),
        "{}",
        plain.3["stroke"]
    );
    assert!(previous.1.iter().all(|p| p.3["sourceId"] != "invisible"));
    // Ghost All draws outlines in its own lilac and clears the fill.
    let all_keys = paged.iter().find(|(d, _)| d["kind"] == "allKeys").unwrap();
    assert!(all_keys
        .1
        .iter()
        .all(|p| p.3["fill"].is_null() && p.3["stroke"].as_str().unwrap().starts_with("#ad99ff")));
}

#[test]
fn native_ghost_view_pages_a_long_stroke_across_pieces_within_the_byte_limit() {
    let mut frames = oracle_frames();
    let long: Vec<Value> = (0..120).map(|n| json!({"point":[n as f64 * 1.25,(n % 7) as f64],"handleIn":[0.5,0.25],"handleOut":[-0.5,0.0]})).collect();
    for frame in frames.as_array_mut().unwrap() {
        frame["strokes"][0]["segments"] = json!(long);
    }
    let owner = frames_of(&frames);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let first = ask(&snapshot, view(1, 0, "default"), Ok(PROOF))
        .result
        .unwrap();
    assert!(
        first["next"]["cursor"].as_u64().unwrap() < 120,
        "a 120 segment stroke does not fit one 4096 byte page"
    );
    let paged = drain(&snapshot, "default");
    for (_, paths) in &paged {
        let stroke = paths.iter().find(|p| p.3["sourceId"] == "a").unwrap();
        assert_eq!(
            (stroke.1, stroke.2),
            (120, 120),
            "every segment arrives exactly once"
        );
    }
}

#[test]
fn native_ghost_view_keeps_its_snapshot_after_an_edit_and_refuses_stale_selectors() {
    let mut owner = frames_of(&oracle_frames());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = drain(&snapshot, "default");
    let edit = request(
        &owner,
        "edit",
        "tween.generate",
        json!({"command":"object.tween.generate","layerUid":"L0"}),
    );
    assert_eq!(dispatch(&mut owner, &edit)["ok"], true);
    assert_eq!(owner.content_revision(), 1);
    // The retained snapshot still answers at its own revision, unchanged.
    assert_eq!(before, drain(&snapshot, "default"));
    // A read pinned to the old origin against the new snapshot is stale, not a silent re-read.
    let head = owner.acquire_snapshot(1).unwrap();
    let mut stale = query(&head, view(0, 0, "default"));
    stale.payload["atRevision"] = json!(0);
    assert_eq!(
        code_of(head.dispatch_with_ghost_view(stale, Ok(PROOF)).unwrap()),
        DispatchErrorCode::StaleRevision
    );
    let mut old_id = query(&head, view(0, 0, "default"));
    old_id.payload["documentSnapshotId"] = json!(snapshot.snapshot_id());
    assert_eq!(
        code_of(head.dispatch_with_ghost_view(old_id, Ok(PROOF)).unwrap()),
        DispatchErrorCode::StaleRevision
    );
    // A moved playhead, a new presentation generation or another work is a changed presentation.
    for proof in [
        ("native-work-9", 3, "scene-root", 3),
        ("native-work-9", 4, "scene-root", 2),
        ("native-work-10", 3, "scene-root", 2),
    ] {
        assert_eq!(
            code_of(ask(&snapshot, view(0, 0, "default"), Ok(proof))),
            DispatchErrorCode::StaleRevision
        );
    }
    assert_eq!(
        code_of(ask(
            &snapshot,
            view(0, 0, "default"),
            Err("Ghost presentation unavailable")
        )),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(owner.content_revision(), 1);
}

#[test]
fn native_ghost_view_refuses_open_selectors_and_reports_an_empty_layer_as_no_ghosts() {
    let owner = frames_of(&oracle_frames());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let mut extra = view(0, 0, "default");
    extra["limit"] = json!(4);
    assert_eq!(
        code_of(ask(&snapshot, extra, Ok(PROOF))),
        DispatchErrorCode::InvalidRequest
    );
    let mut bad_kind = view(0, 0, "default");
    bad_kind["kind"] = json!("widget");
    assert_eq!(
        code_of(ask(&snapshot, bad_kind, Ok(PROOF))),
        DispatchErrorCode::InvalidRequest
    );
    for (key, value) in [
        ("mode", json!("outlines")),
        ("from", json!(3)),
        ("to", json!(99)),
        ("previousOpacity", json!(101)),
        ("previousTint", json!("red")),
    ] {
        let mut bad = view(0, 0, "default");
        bad[key] = value;
        assert_eq!(
            code_of(ask(&snapshot, bad, Ok(PROOF))),
            DispatchErrorCode::InvalidRequest,
            "{key}"
        );
    }
    let mut missing = view(0, 0, "default");
    missing["layerUid"] = json!("missing");
    assert_eq!(
        code_of(ask(&snapshot, missing, Ok(PROOF))),
        DispatchErrorCode::NotFound
    );
    for (ghost, cursor) in [(99, 0), (0, 262_144)] {
        assert_eq!(
            code_of(ask(&snapshot, view(ghost, cursor, "default"), Ok(PROOF))),
            DispatchErrorCode::InvalidRequest
        );
    }
    // Onion off and no Ghost All: nothing to draw is one empty page, not an error.
    let mut off = view(0, 0, "default");
    off["onion"] = json!(false);
    off["ghostAll"] = json!(false);
    let empty = ask(&snapshot, off, Ok(PROOF));
    assert!(empty.ok);
    let r = empty.result.unwrap();
    assert!(
        r["ghost"].is_null()
            && r["next"].is_null()
            && r["items"].as_array().unwrap().is_empty()
            && r["total"] == 0
    );
}
