//! Public sole-owner copy/cut dispatch oracles. These source tests do not establish installed UI acceptance.
use super::object_selection_edit_tests::{
    apply, base, bytes, current, delete, dispatch, document, frozen, layer, owner_of, record_at,
    selection, shape, strokes_at, value,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn request(
    owner: &NativeObjectHistory,
    id: &str,
    action: &str,
    layer: &str,
    frame: u32,
    mode: &str,
    ids: &[&str],
) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":format!("command.document.object.clipboard.{action}"),
        "payload":selection(&format!("object.clipboard.{action}"),layer,frame,mode,ids)})
}
fn clipboard(
    owner: &NativeObjectHistory,
    layer: &str,
    playhead: u32,
    authored: u32,
    ids: &[&str],
) -> Value {
    let source = value(owner);
    json!({"contextId":"scene-root","layerUid":layer,"frame":playhead,
        "records":ids.iter().map(|id|record_at(&source,layer,authored,id)).collect::<Vec<_>>()})
}
fn receipt(request: &Value, revision: u64, applied: bool, clipboard: Value) -> Value {
    json!({"apiVersion":2,"requestId":request["requestId"],"instanceId":request["instanceId"],
        "documentId":request["documentId"],"contentRevision":revision,"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":u64::from(applied),"clipboard":clipboard}})
}

#[test]
fn public_copy_and_cut_return_exact_source_records_and_cut_undo_redo_restore_exact_bytes() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let before = frozen(&owner);
        let snapshot = current(&owner);
        let expected = clipboard(&owner, "L0", 0, 0, &["c", "a"]);
        let copy = request(
            &owner,
            "copy-source",
            "copy",
            "L0",
            0,
            "animation2d",
            &["c", "a"],
        );
        assert_eq!(
            dispatch(&mut owner, &copy),
            receipt(&copy, 0, false, expected.clone())
        );
        assert_eq!(
            frozen(&owner),
            before,
            "copy changes no content, revision, or stacks"
        );
        let cut = request(&owner, "cut-source", "cut", "L0", 0, "motion", &["c", "a"]);
        assert_eq!(dispatch(&mut owner, &cut), receipt(&cut, 1, true, expected));
        let after = bytes(&owner);
        assert_eq!(strokes_at(&value(&owner), "L0", 0), ["b"]);
        assert_eq!(strokes_at(&value(&owner), "L0", 3), ["a", "d"]);
        assert_eq!(strokes_at(&value(&owner), "L1", 0), ["x"]);
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
        assert_eq!(
            crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
            before.0,
            "the source snapshot and copied record identities remain immutable"
        );
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before.0);
        assert_eq!(owner.history_depths(), (0, 1));
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn group_references_and_authored_identities_travel_without_remapping() {
    for action in ["copy", "cut"] {
        let source = super::object_selection_edit_clipboard::two_tables(2);
        let mut owner = owner_of(&source);
        let mut expected = clipboard(&owner, "L0", 0, 0, &["b", "a"]);
        expected["groups"] = source["layers"][0]["groups"].clone();
        let r = request(
            &owner,
            "group-reference",
            action,
            "L0",
            0,
            "motion",
            &["b", "a"],
        );
        let response = dispatch(&mut owner, &r);
        assert_eq!(response["result"]["clipboard"], expected);
        assert_eq!(
            response["result"]["clipboard"]["groups"]["members"],
            source["layers"][0]["groups"]["members"]
        );
        assert_eq!(strokes_at(&value(&owner), "L1", 0), ["x", "y"]);
    }
}

#[test]
fn replay_after_later_edits_and_undo_returns_original_clip_and_never_executes_again() {
    for action in ["copy", "cut"] {
        let mut owner = owner_of(&base(2));
        let r = request(
            &owner,
            "retained-clipboard",
            action,
            "L0",
            0,
            "motion",
            &["b"],
        );
        let first = dispatch(&mut owner, &r);
        assert_eq!(first["ok"], true);
        apply(&mut owner, "delete", &delete("L1", 0, "motion", &["x"])).unwrap();
        let later = frozen(&owner);
        assert_eq!(dispatch(&mut owner, &r), first);
        assert_eq!(frozen(&owner), later);
        owner.undo(owner.content_revision()).unwrap();
        let undone = frozen(&owner);
        assert_eq!(dispatch(&mut owner, &r), first);
        assert_eq!(frozen(&owner), undone);
        let mut changed = r.clone();
        changed["payload"]["strokeIds"] = json!(["a"]);
        assert_eq!(
            dispatch(&mut owner, &changed)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(frozen(&owner), undone);
    }
}

#[test]
fn stale_cancelled_foreign_and_malformed_public_requests_preserve_content_and_history() {
    for action in ["copy", "cut"] {
        let mut owner = owner_of(&base(2));
        let r = request(&owner, "refusal", action, "L0", 0, "motion", &["b"]);
        let before = frozen(&owner);
        for (index, (key, field, code)) in [
            ("expectedRevision", json!(1), "stale_revision"),
            ("instanceId", json!("foreign"), "wrong_instance"),
            ("documentId", json!("foreign"), "wrong_document"),
            (
                "cancelledBeforeDispatch",
                json!(true),
                "cancelled_before_dispatch",
            ),
            ("apiVersion", json!(1), "invalid_request"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut bad = r.clone();
            bad["requestId"] = json!(format!("refusal-{index}"));
            bad[key] = field;
            assert_eq!(dispatch(&mut owner, &bad)["error"]["code"], code);
            assert_eq!(frozen(&owner), before);
        }
        let mut missing = r.clone();
        missing.as_object_mut().unwrap().remove("expectedRevision");
        missing["requestId"] = json!("missing-revision");
        assert_eq!(
            dispatch(&mut owner, &missing)["error"]["code"],
            "invalid_request"
        );
        for (index, (key, field)) in [
            ("mode", json!("drawing")),
            ("strokeIds", json!(["a", "a"])),
            ("frame", json!(-1)),
            ("frame", json!(0.5)),
            ("extra", json!(true)),
            ("command", json!("object.selection.delete")),
        ]
        .into_iter()
        .enumerate()
        {
            let mut bad = r.clone();
            bad["requestId"] = json!(format!("payload-{index}"));
            bad["payload"][key] = field;
            assert_eq!(
                dispatch(&mut owner, &bad)["error"]["code"],
                "invalid_request"
            );
            assert_eq!(frozen(&owner), before);
        }
        let mut absent = r.clone();
        absent["requestId"] = json!("missing-stroke");
        absent["payload"]["strokeIds"] = json!(["absent"]);
        assert_eq!(dispatch(&mut owner, &absent)["error"]["code"], "not_found");
        assert_eq!(frozen(&owner), before);
    }
}

#[test]
fn copy_on_locked_layer_is_read_only_and_cut_refuses_before_publication() {
    let mut source = base(2);
    source["layers"][0]["locked"] = json!(true);
    let mut owner = owner_of(&source);
    let before = frozen(&owner);
    let copy = request(&owner, "locked-copy", "copy", "L0", 0, "motion", &["a"]);
    assert_eq!(dispatch(&mut owner, &copy)["ok"], true);
    assert_eq!(frozen(&owner), before);
    let cut = request(&owner, "locked-cut", "cut", "L0", 0, "motion", &["a"]);
    assert_eq!(dispatch(&mut owner, &cut)["error"]["code"], "unavailable");
    assert_eq!(frozen(&owner), before);
}

#[test]
fn empty_cut_and_copy_keep_redo_and_noop_receipts_return_explicit_null() {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "delete", &delete("L0", 0, "motion", &["b"])).unwrap();
    owner.undo(1).unwrap();
    let before = frozen(&owner);
    assert_eq!(owner.history_depths(), (0, 1));
    for action in ["copy", "cut"] {
        let r = request(
            &owner,
            &format!("empty-{action}"),
            action,
            "L0",
            5,
            "animation2d",
            &[],
        );
        let response = dispatch(&mut owner, &r);
        assert_eq!(response, receipt(&r, 2, false, Value::Null));
        assert_eq!(dispatch(&mut owner, &r), response);
        assert_eq!(frozen(&owner), before);
    }
    let copy = request(&owner, "copy-with-redo", "copy", "L0", 5, "motion", &["d"]);
    let expected = clipboard(&owner, "L0", 5, 3, &["d"]);
    assert_eq!(
        dispatch(&mut owner, &copy),
        receipt(&copy, 2, false, expected)
    );
    assert_eq!(frozen(&owner), before);
    owner.redo(2).unwrap();
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn held_copy_reports_playhead_but_keeps_authored_source_and_cut_obeys_edit_mode() {
    for version in [1, 2] {
        for mode in ["animation2d", "motion"] {
            let mut owner = owner_of(&base(version));
            let before = bytes(&owner);
            let copy = request(&owner, "held-copy", "copy", "L0", 5, mode, &["d"]);
            let expected = clipboard(&owner, "L0", 5, 3, &["d"]);
            assert_eq!(
                dispatch(&mut owner, &copy),
                receipt(&copy, 0, false, expected.clone())
            );
            let cut = request(&owner, "held-cut", "cut", "L0", 5, mode, &["d"]);
            assert_eq!(dispatch(&mut owner, &cut), receipt(&cut, 1, true, expected));
            let after = value(&owner);
            let after_bytes = bytes(&owner);
            assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c"]);
            if mode == "animation2d" {
                assert_eq!(strokes_at(&after, "L0", 3), ["a", "d"]);
                assert_eq!(strokes_at(&after, "L0", 5), ["a"]);
                assert_eq!(
                    after["layers"][0]["timing"]["keys"],
                    json!([{"frame":0},{"frame":3},{"frame":5}])
                );
            } else {
                assert_eq!(strokes_at(&after, "L0", 3), ["a"]);
                assert!(strokes_at(&after, "L0", 5).is_empty());
                assert_eq!(
                    after["layers"][0]["timing"]["keys"],
                    json!([{"frame":0},{"frame":3}])
                );
            }
            owner.undo(1).unwrap();
            assert_eq!(bytes(&owner), before);
            owner.redo(2).unwrap();
            assert_eq!(bytes(&owner), after_bytes);
        }
    }
}

// Independently construct the complete expected response from canonical source records, then
// tune only the legal request-id length. This covers the envelope, not merely the clip body.
fn bounded_fixture(action: &str, target_bytes: usize) -> (NativeObjectHistory, Value, Value) {
    let uid = "L".repeat(128);
    let stroke = "s".repeat(128);
    for count in 2..=64 {
        let mut record = shape(&uid, &stroke, 0, 0);
        let point = record["geometry"]["segments"][0].clone();
        record["geometry"]["segments"] = json!(vec![point; count]);
        let source = document(1, vec![layer(&uid, "Long identity")], vec![record]);
        let owner = NativeObjectHistory::new(
            "I".repeat(128),
            decode_storage_project(source.to_string().as_bytes()).unwrap(),
        )
        .unwrap();
        let mut r = request(&owner, "r", action, &uid, 0, "motion", &[&stroke]);
        let clip = clipboard(&owner, &uid, 0, 0, &[&stroke]);
        let mut expected = receipt(&r, u64::from(action == "cut"), action == "cut", clip);
        let size = serde_json::to_vec(&expected).unwrap().len();
        if size <= target_bytes && target_bytes - size <= 127 {
            let id = "r".repeat(1 + target_bytes - size);
            r["requestId"] = json!(id);
            expected["requestId"] = r["requestId"].clone();
            assert_eq!(serde_json::to_vec(&expected).unwrap().len(), target_bytes);
            // No calls to the dispatcher or copy kernel were used to choose this oracle.
            assert_eq!(owner.content_revision(), 0);
            return (owner, r, expected);
        }
    }
    panic!("could not construct the exact complete-envelope boundary")
}

#[test]
fn complete4096_byte_reply_succeeds_and4097_refuses_before_cut_or_history_mutation() {
    for action in ["copy", "cut"] {
        let (mut owner, r, expected) = bounded_fixture(action, 4096);
        let response = dispatch(&mut owner, &r);
        assert_eq!(response, expected);
        assert_eq!(serde_json::to_vec(&response).unwrap().len(), 4096);
        assert_eq!(owner.content_revision(), u64::from(action == "cut"));
        let (mut owner, r, expected) = bounded_fixture(action, 4097);
        assert_eq!(serde_json::to_vec(&expected).unwrap().len(), 4097);
        let before = frozen(&owner);
        let snapshot = current(&owner).snapshot_id().to_string();
        let response = dispatch(&mut owner, &r);
        assert_eq!(response["error"]["code"], "invalid_request");
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
        assert_eq!(frozen(&owner), before);
        assert_eq!(current(&owner).snapshot_id(), snapshot);
        assert_eq!(
            dispatch(&mut owner, &r),
            response,
            "oversize refusal is retained exactly"
        );
        assert_eq!(frozen(&owner), before);
    }
}

#[cfg(feature = "project_media_codec")]
#[test]
fn clipboard_dispatch_preserves_project_reference_identity_and_metadata() {
    use crate::object_document::{ProjectEnvelope, ProjectMetadata, ProjectReferenceState};
    let hex="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
    let png: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    let drawing = decode_storage_project(base(2).to_string().as_bytes()).unwrap();
    let project =
        ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), drawing)
            .with_reference(
                ProjectReferenceState::admitted_still(
                    "source-ref".into(),
                    "plate.png".into(),
                    false,
                    0.5,
                    -7,
                    png,
                    1024,
                )
                .unwrap(),
            );
    let mut owner =
        NativeObjectHistory::from_admitted_project("clipboard-reference", project).unwrap();
    let saved = |owner: &NativeObjectHistory| -> Value {
        serde_json::from_slice(&current(owner).serialize_project_file().unwrap()).unwrap()
    };
    let before = saved(&owner);
    for action in ["copy", "cut"] {
        let r = request(&owner, action, action, "L0", 0, "motion", &["b"]);
        assert_eq!(dispatch(&mut owner, &r)["ok"], true);
        let mut after = saved(&owner);
        after["drawing"] = before["drawing"].clone();
        assert_eq!(
            after, before,
            "{action} must preserve exact reference bytes, identity and metadata"
        );
    }
    owner.undo(1).unwrap();
    assert_eq!(saved(&owner), before);
}

#[test]
fn cut_then_paste_preserves_authored_effect_presence_after_the_source_is_gone() {
    for effects in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([{"type":"blur","enabled":false,"p1":null,"p2":0,"keys":null}])),
    ] {
        let mut source = base(2);
        if let Some(effects) = effects {
            source["objects"][1]["effects"] = effects;
        }
        let mut owner = owner_of(&source);
        let before = bytes(&owner);
        let expected = record_at(&value(&owner), "L0", 0, "b");
        let cut = request(&owner, "effect-cut", "cut", "L0", 0, "motion", &["b"]);
        let response = dispatch(&mut owner, &cut);
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["clipboard"]["records"][0], expected);
        let paste = json!({"apiVersion":2,"requestId":"effect-paste","instanceId":owner.instance_id(),"documentId":owner.document_id(),
            "expectedRevision":1,"operation":"command.document.object.clipboard.paste",
            "payload":{"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L1","frame":0,"mode":"motion","clip":response["result"]["clipboard"]}});
        assert_eq!(dispatch(&mut owner, &paste)["ok"], true);
        let restored = record_at(&value(&owner), "L1", 0, "native-stroke:2:0");
        assert_eq!(restored.get("effects"), expected.get("effects"));
        owner.undo(2).unwrap();
        owner.undo(3).unwrap();
        assert_eq!(bytes(&owner), before);
    }
}
