//! Live stack observation must neither consume receipts nor select historical stacks.
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "history-read",
        decode_storage_project(
            json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":20,
        "layers":[{"layerUid":"L"}],"objects":[]})
            .to_string()
            .as_bytes(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(o: &NativeObjectHistory) -> OpacityRequest {
    OpacityRequest::query(
        "same-read-id",
        o.instance_id(),
        o.document_id(),
        "query.document.object.history",
        json!({"atRevision":o.content_revision(),
        "documentSnapshotId":o.acquire_snapshot(o.content_revision()).unwrap().snapshot_id()}),
    )
}
fn read(o: &NativeObjectHistory, r: &OpacityRequest) -> Value {
    serde_json::to_value(o.dispatch_history_read(r).unwrap()).unwrap()
}
fn edit(o: &mut NativeObjectHistory, value: f64) {
    let payload = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[value]}});
    let candidate = o
        .prepare_property_edit_json(payload.to_string().as_bytes())
        .unwrap();
    o.commit_property_edit(candidate).unwrap();
}
#[test]
fn actual_nonzero_stacks_old_snapshots_noop_and_same_request_id_are_read_without_receipts() {
    let mut o = owner();
    let old = request(&o);
    assert_eq!(read(&o, &old)["result"]["undoDepth"], 0);
    edit(&mut o, 42.);
    let after_edit = request(&o);
    assert_eq!(read(&o, &after_edit)["result"]["undoDepth"], 1);
    edit(&mut o, 73.);
    o.undo(2).unwrap();
    assert_eq!(o.history_depths(), (1, 1));
    let pinned = request(&o);
    let before = format!("{o:?}");
    for _ in 0..4 {
        let result = read(&o, &pinned);
        assert_eq!(
            result["result"],
            json!({"atRevision":3,
            "documentSnapshotId":pinned.payload["documentSnapshotId"],"undoDepth":1,"redoDepth":1,
            "offset":0,"entries":[{"index":0,"label":"Edit"},{"index":1,"label":"Edit"}]})
        );
    }
    assert_eq!(
        format!("{o:?}"),
        before,
        "all owner state including receipts stays unchanged"
    );
    for stale in [old, after_edit] {
        assert_eq!(read(&o, &stale)["error"]["code"], "stale_revision");
    }
    edit(&mut o, 42.);
    assert_eq!(o.history_depths(), (1, 1), "semantic no-op retains redo");
    assert_eq!(read(&o, &pinned)["result"]["redoDepth"], 1);
    o.redo(3).unwrap();
    assert_eq!(read(&o, &request(&o))["result"]["undoDepth"], 2);
    assert_eq!(read(&o, &request(&o))["result"]["redoDepth"], 0);
}
#[test]
fn foreign_stale_malformed_cancelled_and_oversize_requests_preserve_all_state() {
    let o = owner();
    let r = request(&o);
    let before = format!("{o:?}");
    let mut cases = vec![];
    for (key, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(0), "invalid_request"),
        ("apiVersion", json!(1), "invalid_request"),
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[key] = value;
        cases.push((serde_json::from_value(bad).unwrap(), code));
    }
    for payload in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"atRevision":0,"documentSnapshotId":"s","extra":1}),
        json!({"atRevision":0,"documentSnapshotId":"s","limit":0}),
        json!({"atRevision":0,"documentSnapshotId":"s","limit":33}),
        json!({"atRevision":0,"documentSnapshotId":"s","limit":1.5}),
        json!({"atRevision":0,"documentSnapshotId":"s","offset":-1}),
        json!({"atRevision":0,"documentSnapshotId":"s","offset":9007199254740992u64}),
        json!({"atRevision":-1,"documentSnapshotId":"s"}),
        json!({"atRevision":0.5,"documentSnapshotId":"s"}),
        json!({"atRevision":9007199254740992u64,"documentSnapshotId":"s"}),
        json!({"atRevision":0,"documentSnapshotId":"x".repeat(5000)}),
    ] {
        let mut bad = r.clone();
        bad.payload = payload;
        cases.push((bad, "invalid_request"));
    }
    for (key, value) in [
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("other-snapshot")),
    ] {
        let mut bad = r.clone();
        bad.payload[key] = value;
        cases.push((bad, "stale_revision"));
    }
    for (bad, code) in cases {
        assert_eq!(read(&o, &bad)["error"]["code"], code);
    }
    assert_eq!(format!("{o:?}"), before);
}

// ---- #2376 G26: labels, cursor and click-to-jump over the actual dispatcher --------------------
use crate::object_codec::encode_storage_project;

fn send(o: &mut NativeObjectHistory, id: &str, op: &str, payload: Value) -> Value {
    let request = json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":format!("command.document.object.{op}"),"payload":payload});
    serde_json::to_value(
        o.try_dispatch_object_command(&serde_json::from_value(request).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn opacity(o: &mut NativeObjectHistory, id: &str, value: f64) -> Value {
    send(
        o,
        id,
        "property.edit",
        json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},
            "property":"opacity","action":{"kind":"setValue","frame":0,"values":[value]}}),
    )
}
fn page(o: &NativeObjectHistory, extra: Value) -> Value {
    let mut r = request(o);
    for (key, value) in extra.as_object().unwrap() {
        r.payload[key] = value.clone();
    }
    read(o, &r)
}
fn labels(o: &NativeObjectHistory) -> Vec<String> {
    page(o, json!({}))["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["label"].as_str().unwrap().to_owned())
        .collect()
}
fn content(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
/// One property edit, a visibility toggle, a rename and a layer creation: four families.
fn four_families() -> NativeObjectHistory {
    let mut o = owner();
    assert_eq!(opacity(&mut o, "e1", 40.)["ok"], true);
    let hide = json!({"command":"object.layer.visibility.set","layerUid":"L","visible":false});
    assert_eq!(send(&mut o, "e2", "layer.visibility.set", hide)["ok"], true);
    let create = json!({"command":"object.layer.create","name":"Second"});
    assert_eq!(send(&mut o, "e3", "layer.create", create)["ok"], true);
    let rename = json!({"command":"object.layer.rename","layerUid":"L","name":"First"});
    assert_eq!(send(&mut o, "e4", "layer.rename", rename)["ok"], true);
    o
}

#[test]
fn entries_carry_their_family_label_and_the_cursor_follows_undo_and_redo() {
    let mut o = four_families();
    let all = [
        "Property: Opacity",
        "Layer Visibility",
        "New Layer",
        "Rename Layer",
    ];
    assert_eq!(labels(&o), all);
    let result = page(&o, json!({}))["result"].clone();
    assert_eq!(
        (&result["undoDepth"], &result["redoDepth"]),
        (&json!(4), &json!(0))
    );
    let revision = o.content_revision();
    assert_eq!(
        send(&mut o, "u1", "undo", json!({"command":"object.undo"}))["ok"],
        true
    );
    assert_eq!(
        send(&mut o, "u2", "undo", json!({"command":"object.undo"}))["ok"],
        true
    );
    // The same entries in the same chronological order; only the cursor (undoDepth) moved.
    assert_eq!(labels(&o), all);
    let result = page(&o, json!({}))["result"].clone();
    assert_eq!(
        (&result["undoDepth"], &result["redoDepth"]),
        (&json!(2), &json!(2))
    );
    assert_eq!(result["entries"][2], json!({"index":2,"label":"New Layer"}));
    assert_eq!(o.content_revision(), revision + 2);
    assert_eq!(
        send(&mut o, "r1", "redo", json!({"command":"object.redo"}))["ok"],
        true
    );
    assert_eq!(page(&o, json!({}))["result"]["undoDepth"], 3);
    // A new edit drops the redo entries and their labels; the new entry is labelled by its family.
    assert_eq!(opacity(&mut o, "e5", 12.)["ok"], true);
    assert_eq!(
        labels(&o),
        [
            "Property: Opacity",
            "Layer Visibility",
            "New Layer",
            "Property: Opacity"
        ]
    );
}

#[test]
fn jump_by_n_steps_is_n_undos_in_one_revision_and_redo_jump_returns() {
    let (mut one_by_one, mut jumped) = (four_families(), four_families());
    for n in 0..3 {
        let id = format!("u{n}");
        assert_eq!(
            send(
                &mut one_by_one,
                &id,
                "undo",
                json!({"command":"object.undo"})
            )["ok"],
            true
        );
    }
    let before = jumped.content_revision();
    let moved = send(
        &mut jumped,
        "j1",
        "undo",
        json!({"command":"object.undo","steps":3}),
    );
    assert_eq!(moved["ok"], true);
    assert_eq!(
        moved["contentRevision"],
        before + 1,
        "one restore, one new revision"
    );
    assert_eq!(content(&jumped), content(&one_by_one));
    assert_eq!(jumped.history_depths(), one_by_one.history_depths());
    assert_eq!(labels(&jumped), labels(&one_by_one));
    for n in 0..3 {
        let id = format!("r{n}");
        assert_eq!(
            send(
                &mut one_by_one,
                &id,
                "redo",
                json!({"command":"object.redo"})
            )["ok"],
            true
        );
    }
    assert_eq!(
        send(
            &mut jumped,
            "j2",
            "redo",
            json!({"command":"object.redo","steps":3})
        )["ok"],
        true
    );
    assert_eq!(content(&jumped), content(&one_by_one));
    assert_eq!(jumped.history_depths(), (4, 0));
    // Fewer entries than steps, a zero count and extra members are refused with nothing moved.
    let snapshot = (
        content(&jumped),
        jumped.content_revision(),
        format!("{:?}", jumped.history_depths()),
    );
    for (id, payload, code) in [
        (
            "j3",
            json!({"command":"object.undo","steps":5}),
            "unavailable",
        ),
        (
            "j4",
            json!({"command":"object.undo","steps":0}),
            "invalid_request",
        ),
        (
            "j5",
            json!({"command":"object.undo","steps":1.5}),
            "invalid_request",
        ),
        (
            "j6",
            json!({"command":"object.undo","steps":"2"}),
            "invalid_request",
        ),
        (
            "j7",
            json!({"command":"object.undo","count":2}),
            "invalid_request",
        ),
        (
            "j8",
            json!({"command":"object.redo","steps":1}),
            "unavailable",
        ),
        ("j9", json!({"command":"object.redo"}), "unavailable"),
    ] {
        let op = if payload["command"] == "object.redo" {
            "redo"
        } else {
            "undo"
        };
        assert_eq!(
            send(&mut jumped, id, op, payload)["error"]["code"],
            code,
            "{id}"
        );
    }
    assert_eq!(
        (
            content(&jumped),
            jumped.content_revision(),
            format!("{:?}", jumped.history_depths())
        ),
        snapshot
    );
}

#[test]
fn a_stale_expected_revision_refuses_the_jump_and_a_stale_pin_refuses_the_read() {
    let mut o = four_families();
    let pinned = request(&o);
    let mut stale = json!({"apiVersion":2,"requestId":"s1","instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision() - 1,
        "operation":"command.document.object.undo","payload":{"command":"object.undo","steps":2}});
    let refused = serde_json::to_value(
        o.try_dispatch_object_command(&serde_json::from_value(stale.take()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refused["error"]["code"], "stale_revision");
    assert_eq!(o.history_depths(), (4, 0));
    assert_eq!(
        send(
            &mut o,
            "j1",
            "undo",
            json!({"command":"object.undo","steps":2})
        )["ok"],
        true
    );
    assert_eq!(read(&o, &pinned)["error"]["code"], "stale_revision");
}

#[test]
fn a_page_is_pinned_bounded_and_windowed_around_the_cursor() {
    let mut o = owner();
    for n in 0..40 {
        assert_eq!(
            opacity(&mut o, &format!("e{n}"), f64::from(n) + 1.)["ok"],
            true
        );
    }
    assert_eq!(
        send(
            &mut o,
            "u",
            "undo",
            json!({"command":"object.undo","steps":5})
        )["ok"],
        true
    );
    // Cursor 35: the default page starts 20 before it and holds at most 32 entries.
    let result = page(&o, json!({}))["result"].clone();
    assert_eq!(
        (
            &result["undoDepth"],
            &result["redoDepth"],
            &result["offset"]
        ),
        (&json!(35), &json!(5), &json!(15))
    );
    let entries = result["entries"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        25,
        "indexes 15..40: the window ends with the last entry"
    );
    assert_eq!(entries[0]["index"], 15);
    assert_eq!(entries[24]["index"], 39);
    let explicit = page(&o, json!({"offset":2,"limit":3}))["result"].clone();
    assert_eq!(explicit["entries"].as_array().unwrap().len(), 3);
    assert_eq!(explicit["entries"][0]["index"], 2);
    assert_eq!(
        page(&o, json!({"offset":40}))["result"]["entries"],
        json!([])
    );
    assert_eq!(
        page(&o, json!({"offset":0,"limit":32}))["result"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
}
