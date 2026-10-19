//! Real key.move dispatch consumes stored Arcs in the same sealed document/history.
//! Literal baseline rekey/delete/overwrite order; no test-side replacement owner.
use super::build::{document_json, Build};
use super::rows::parse_layers;
use super::{applied, bytes, dispatch, head, layer_text, mv, owner, request, state, SMALL};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

const OLD: &str = r#"{"L0:0-2-0":{"out":[3,4],"in":[-5,6]},"L1:0-3-0":{"cx":17},"opaque":null}"#;
const MOVED: &str = r#"{"L1:0-3-0":{"cx":17},"opaque":null,"L0:1-3-0":{"out":[3,4],"in":[-5,6]}}"#;
const LONG: &str = "K=a.1.10;.;K=b.2.20;.;.;.;.;.;.;.;.;.;.|K;.;.;K;.;.;.;.;.;.;.;.;.";

fn with_arcs(text: &str, form: Build, arcs: Option<&str>, uid: Option<&str>) -> ObjectDocument {
    let mut raw = document_json(&parse_layers(text), form).to_string();
    if let Some(uid) = uid {
        // Fixture-only exact UID substitution; every layer/record/order reference agrees.
        raw = raw.replace("\"L0\"", &serde_json::to_string(uid).unwrap());
    }
    if let Some(arcs) = arcs {
        assert_eq!(raw.pop(), Some('}'));
        raw.push_str(",\"motionArcs\":");
        raw.push_str(arcs);
        raw.push('}');
    }
    decode_storage_project(raw.as_bytes()).unwrap()
}
fn assert_arcs(o: &NativeObjectHistory, expected: &str) {
    let raw = String::from_utf8(bytes(o)).unwrap();
    assert!(
        raw.contains(&format!("\"motionArcs\":{expected}")),
        "Arc values/presence/order differ"
    );
    assert_eq!(
        encode_storage_project(&decode_storage_project(raw.as_bytes()).unwrap()).unwrap(),
        raw.as_bytes()
    );
}
fn active_move(layer: &str, from: u32, to: u32, active: &str) -> Value {
    let mut payload = mv(layer, from, to);
    payload["activeLayerUid"] = json!(active);
    payload
}
fn refuse_without_publication(mut o: NativeObjectHistory, layer: &str) {
    let before = state(&o);
    let payload = active_move(layer, 0, 9, layer);
    assert_eq!(
        o.prepare_exposure_retime_json(payload.to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(
        state(&o),
        before,
        "preparation cannot publish content before Arc admission"
    );
    let req = request(&o, "arc-overflow", payload);
    assert!(
        req.to_string().len() <= 4096,
        "the request fits its unchanged bound"
    );
    let reply = dispatch(&mut o, &req);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!(
        state(&o),
        before,
        "content, revision, snapshot and history unchanged"
    );
    assert_eq!(dispatch(&mut o, &req), reply, "refusal is retained");
    assert_eq!(state(&o), before);
    let noop = request(&o, "still-valid", active_move(layer, 0, 0, layer));
    assert_eq!(
        applied(&dispatch(&mut o, &noop)),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&o), before, "the admitted source remains usable");
}

#[test]
fn actual_key_move_rekeys_arcs_once_with_content_undo_redo_reopen_and_retained_snapshot() {
    for form in Build::ALL {
        let mut o = owner(with_arcs(SMALL, form, Some(OLD), None));
        let before = bytes(&o);
        let retained = head(&o);
        let req = request(&o, "arc-move", active_move("L0", 0, 1, "L0"));
        let reply = dispatch(&mut o, &req);
        assert_eq!(reply["ok"], true, "{form:?}: {reply}");
        assert_eq!(reply["contentRevision"], 1);
        assert_eq!(
            applied(&reply),
            json!({"applied":true,"historyEntriesAdded":1})
        );
        assert_eq!(layer_text(&o, 0), ".;K=a.1.10;.;K=b.2.20;.");
        assert_eq!(layer_text(&o, 1), "K;.;.;K;.");
        assert_arcs(&o, MOVED);
        let now = head(&o);
        let moved = now
            .document()
            .motion_arc_handles("L0", 1, 3, 0, [0., 0.], [30., 0.])
            .unwrap();
        assert_eq!((moved.out, moved.incoming), ([3., 4.], [25., 6.]));
        let missing = now
            .document()
            .motion_arc_handles("L0", 0, 2, 0, [0., 0.], [30., 0.])
            .unwrap();
        assert_eq!((missing.out, missing.incoming), ([10., 0.], [20., 0.]));
        let old = retained
            .document()
            .motion_arc_handles("L0", 0, 2, 0, [0., 0.], [30., 0.])
            .unwrap();
        assert_eq!((old.out, old.incoming), ([3., 4.], [25., 6.]));
        assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (1, 0));
        let changed = bytes(&o);
        let fresh = owner(decode_storage_project(&changed).unwrap());
        assert_eq!(bytes(&fresh), changed);
        assert_eq!(fresh.history_depths(), (0, 0));
        assert_ne!(fresh.document_id(), o.document_id());
        assert_eq!(o.undo(1), Ok(2));
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (0, 1));
        assert_eq!(o.redo(2), Ok(3));
        assert_eq!(bytes(&o), changed);
        assert_eq!(o.history_depths(), (1, 0));
        assert_arcs(&o, MOVED);
    }
}

#[test]
fn captured_active_layer_rekeys_its_arcs_while_target_layer_content_moves() {
    let old = r#"{"L0:0-2-0":{"out":[1,2]},"L1:0-2-0":{"out":[3,4]},"opaque":null}"#;
    let expected = r#"{"L0:0-2-0":{"out":[1,2]},"opaque":null,"L1:1-3-0":{"out":[3,4]}}"#;
    for form in Build::ALL {
        let mut o = owner(with_arcs(SMALL, form, Some(old), None));
        let before = bytes(&o);
        // Baseline moveKeyframe omitted li: rekey uses active L1, not target L0.
        let req = request(&o, "active-not-target", active_move("L0", 0, 1, "L1"));
        assert_eq!(dispatch(&mut o, &req)["ok"], true);
        assert_eq!(layer_text(&o, 0), ".;K=a.1.10;.;K=b.2.20;.");
        assert_eq!(layer_text(&o, 1), "K;.;.;K;.");
        assert_arcs(&o, expected);
        assert_eq!(o.history_depths(), (1, 0));
        o.undo(1).unwrap();
        assert_eq!(bytes(&o), before);
    }
}

#[test]
fn changed_nonempty_arc_store_requires_captured_valid_active_context_but_noop_keeps_redo() {
    let mut o = owner(with_arcs(SMALL, Build::V2, Some(OLD), None));
    for (i, active, code) in [
        (0, None, "unavailable"),
        (1, Some(json!(null)), "invalid_request"),
        (2, Some(json!(3)), "invalid_request"),
        (3, Some(json!("missing-layer")), "not_found"),
    ] {
        let before = state(&o);
        let mut payload = mv("L0", 0, 1);
        if let Some(active) = active {
            payload["activeLayerUid"] = active;
        }
        let expected = match code {
            "unavailable" => DispatchErrorCode::Unavailable,
            "not_found" => DispatchErrorCode::NotFound,
            _ => DispatchErrorCode::InvalidRequest,
        };
        assert_eq!(
            o.prepare_exposure_retime_json(payload.to_string().as_bytes())
                .unwrap_err(),
            expected
        );
        assert_eq!(state(&o), before);
        let req = request(&o, &format!("bad-active-{i}"), payload);
        let reply = dispatch(&mut o, &req);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"]["code"], code);
        assert_eq!(state(&o), before);
        assert_eq!(dispatch(&mut o, &req), reply);
        assert_eq!(state(&o), before);
    }
    let req = request(&o, "valid-active", active_move("L0", 0, 1, "L0"));
    assert_eq!(dispatch(&mut o, &req)["ok"], true);
    o.undo(1).unwrap();
    let before = state(&o);
    let req = request(&o, "missing-active-noop", mv("L0", 0, 0));
    assert_eq!(
        applied(&dispatch(&mut o, &req)),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    o.redo(2).unwrap();
    assert_arcs(&o, MOVED);
    let mut clamped = owner(with_arcs("K;.;.;.;K|K;.;.;K;.", Build::V2, Some(OLD), None));
    let req = request(&clamped, "seed-redo", active_move("L1", 0, 1, "L1"));
    assert_eq!(dispatch(&mut clamped, &req)["ok"], true);
    clamped.undo(1).unwrap();
    let before = state(&clamped);
    let req = request(&clamped, "missing-active-clamped", mv("L0", 0, 1));
    assert_eq!(
        applied(&dispatch(&mut clamped, &req)),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&clamped), before);
    assert_eq!(clamped.history_depths(), (0, 1));
}

#[test]
fn consecutive_pair_dispatch_preserves_delete_overwrite_append_order_and_opaque_other_layer_keys() {
    let text = "K=a.1.10;.;K=b.2.20;.;K=c.3.30;.;.;.|K;.;.;K;.;.;.;.";
    let old = r#"{"":null,"L0:0-2-0":{"out":[1,2]},"L0:1-3-0":{"out":[9,9]},"L0:2-4-0":{"in":[-1,-2]},"L0:3-5-0":{"out":[8,8]},"L0:0-2-tail":null,"L1:0-2-0":{},"L00:0-2-0":null,"opaque":{"cx":4}}"#;
    let expected = r#"{"":null,"L0:1-3-0":{"out":[1,2]},"L0:3-5-0":{"in":[-1,-2]},"L1:0-2-0":{},"L00:0-2-0":null,"opaque":{"cx":4},"L0:1-3-tail":null}"#;
    for form in Build::ALL {
        let mut o = owner(with_arcs(text, form, Some(old), None));
        let before = bytes(&o);
        let prepared = o
            .prepare_exposure_retime_json(active_move("L0", 0, 1, "L0").to_string().as_bytes())
            .unwrap();
        let pairs: Vec<_> = prepared
            .pairs()
            .iter()
            .map(|p| [p.from_a, p.from_b, p.to_a, p.to_b])
            .collect();
        assert_eq!(
            pairs,
            [[0, 2, 1, 3], [2, 4, 3, 5]],
            "literal baseline pairs"
        );
        assert_eq!(bytes(&o), before);
        let req = request(&o, "two-pairs", active_move("L0", 0, 1, "L0"));
        assert_eq!(dispatch(&mut o, &req)["ok"], true);
        assert_arcs(&o, expected);
        assert_eq!(layer_text(&o, 0), ".;K=a.1.10;.;K=b.2.20;.;K=c.3.30;.;.");
        assert_eq!(layer_text(&o, 1), "K;.;.;K;.;.;.;.");
        assert_eq!(o.history_depths(), (1, 0));
        o.undo(1).unwrap();
        assert_eq!(bytes(&o), before);
    }
}

#[test]
fn absent_null_and_empty_arcs_survive_changed_move_and_noop_preserves_redo() {
    for form in Build::ALL {
        for arcs in [None, Some("null"), Some("{}")] {
            let mut o = owner(with_arcs(SMALL, form, arcs, None));
            let original = bytes(&o);
            let first = request(&o, "changed", mv("L0", 0, 1));
            assert_eq!(dispatch(&mut o, &first)["ok"], true);
            let current: Value = serde_json::from_slice(&bytes(&o)).unwrap();
            match arcs {
                None => assert!(current.get("motionArcs").is_none()),
                Some(raw) => assert_arcs(&o, raw),
            }
            o.undo(1).unwrap();
            assert_eq!(bytes(&o), original);
            assert_eq!(o.history_depths(), (0, 1));
            let before = state(&o);
            let req = request(&o, "noop", mv("L0", 2, 2));
            assert_eq!(
                applied(&dispatch(&mut o, &req)),
                json!({"applied":false,"historyEntriesAdded":0})
            );
            assert_eq!(state(&o), before);
            o.redo(2).unwrap();
            assert_eq!(o.history_depths(), (1, 0));
            let reopened = owner(decode_storage_project(&bytes(&o)).unwrap());
            assert_eq!(bytes(&reopened), bytes(&o));
        }
    }
}

#[test]
fn arc_replay_after_undo_returns_original_receipt_and_changed_body_cannot_reexecute() {
    let mut o = owner(with_arcs(SMALL, Build::V2Extras, Some(OLD), None));
    let original = bytes(&o);
    let req = request(&o, "arc-replay", active_move("L0", 0, 1, "L0"));
    let first = dispatch(&mut o, &req);
    assert_eq!(first["ok"], true);
    assert_arcs(&o, MOVED);
    let changed = state(&o);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(state(&o), changed);
    o.undo(1).unwrap();
    assert_eq!(bytes(&o), original);
    let undone = state(&o);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(state(&o), undone);
    let mut different_active = req.clone();
    different_active["payload"]["activeLayerUid"] = json!("L1");
    assert_eq!(
        dispatch(&mut o, &different_active)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), undone);
    let mut different = req;
    different["payload"]["to"] = json!(2);
    assert_eq!(
        dispatch(&mut o, &different)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), undone);
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn actual_key_rename_exceeding_utf8_key_limit_refuses_before_any_content_or_history_change() {
    let uid = "L".repeat(1018);
    let old_key = format!("{uid}:0-2-0");
    let new_key = format!("{uid}:9-11-0");
    assert_eq!(old_key.len(), 1024);
    assert_eq!(new_key.len(), 1025);
    let arcs = format!(
        "{{{}:{{\"out\":[1,2]}}}}",
        serde_json::to_string(&old_key).unwrap()
    );
    let doc = with_arcs(LONG, Build::V2, Some(&arcs), Some(&uid));
    refuse_without_publication(owner(doc), &uid);
}

fn exact_budget_keys() -> Vec<String> {
    let mut keys: Vec<_> = (0..4096)
        .map(|i| {
            if i == 0 {
                "L0:0-2-0".into()
            } else {
                format!("{i:04}{}", "k".repeat(1012))
            }
        })
        .collect();
    let initial = 1 + keys.iter().map(|key| key.len() + 8).sum::<usize>();
    let mut remaining = 4 * 1024 * 1024 - initial;
    for key in keys.iter_mut().skip(1) {
        let padding = remaining.min(1024 - key.len());
        key.push_str(&"x".repeat(padding));
        remaining -= padding;
    }
    assert_eq!(remaining, 0);
    assert!(keys.iter().all(|k| k.len() <= 1024));
    keys
}
fn null_entries(keys: &[String]) -> String {
    format!(
        "{{{}}}",
        keys.iter()
            .map(|key| format!("{}:null", serde_json::to_string(key).unwrap()))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[test]
fn actual_one_byte_store_growth_refuses_atomically_but_exact_remaining_byte_can_publish() {
    let keys = exact_budget_keys();
    let full = null_entries(&keys);
    assert_eq!(full.len(), 4 * 1024 * 1024);
    refuse_without_publication(owner(with_arcs(LONG, Build::V2, Some(&full), None)), "L0");
    // One available byte proves the limit does not refuse every real Arc move.
    let mut keys = keys;
    assert_eq!(keys[1].pop(), Some('x'));
    let almost = null_entries(&keys);
    assert_eq!(almost.len(), 4 * 1024 * 1024 - 1);
    let mut o = owner(with_arcs(LONG, Build::V2, Some(&almost), None));
    let before = bytes(&o);
    let req = request(&o, "fits-exactly", active_move("L0", 0, 9, "L0"));
    let reply = dispatch(&mut o, &req);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        applied(&reply),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let value: Value = serde_json::from_slice(&bytes(&o)).unwrap();
    let arcs = value["motionArcs"].as_object().unwrap();
    assert!(!arcs.contains_key("L0:0-2-0") && arcs.contains_key("L0:9-11-0"));
    assert_eq!(value["motionArcs"].to_string().len(), 4 * 1024 * 1024);
    assert_eq!(o.history_depths(), (1, 0));
    o.undo(1).unwrap();
    assert_eq!(bytes(&o), before);
}
