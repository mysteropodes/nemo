//! Independent whole-content/history/replay controls for staged object commands.
use crate::{commands::OpacityRequest, history::NativeObjectHistory, object_codec};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]})
}
fn after_document() -> Value {
    let mut after = document();
    after["objects"][0] = cases()["command"]["afterRecord"].clone();
    after
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "instance-history",
        object_codec::decode_project(&document().to_string().into_bytes()).unwrap(),
    )
    .unwrap()
}
fn request(owner: &NativeObjectHistory, id: &str, revision: u64, action: &str) -> Value {
    let payload = if action == "fill.set" {
        cases()["command"]["request"]["payload"].clone()
    } else {
        json!({"command":format!("object.{action}")})
    };
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":revision,
        "operation":format!("command.document.object.{action}"),"payload":payload})
}
fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(
                &serde_json::from_value::<OpacityRequest>(request.clone()).unwrap(),
            )
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn visibility_request(
    owner: &NativeObjectHistory,
    id: &str,
    revision: u64,
    visible: bool,
) -> Value {
    let mut value = request(owner, id, revision, "layer.visibility.set");
    value["payload"] = json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":visible});
    value
}

#[test]
fn layer_visibility_uses_actual_owner_history_noop_replay_and_retained_snapshots() {
    let mut owner = owner();
    let zero = owner.acquire_snapshot(0).unwrap();
    let noop = visibility_request(&owner, "visible-noop", 0, true);
    assert_eq!(
        dispatch(&mut owner, &noop)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), document());
    let hide = visibility_request(&owner, "hide", 0, false);
    let hidden = dispatch(&mut owner, &hide);
    assert_eq!(hidden["ok"], true);
    assert_eq!(
        hidden["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert!(!owner.acquire_snapshot(1).unwrap().document().layers()[0].visible());
    assert_eq!(owner.history_depths(), (1, 0));
    let undo = request(&owner, "undo-visible", 1, "undo");
    assert_eq!(dispatch(&mut owner, &undo)["ok"], true);
    let noop = visibility_request(&owner, "visible-redo-noop", 2, true);
    assert_eq!(dispatch(&mut owner, &noop)["result"]["applied"], false);
    assert_eq!(owner.history_depths(), (0, 1));
    let redo = request(&owner, "redo-visible", 2, "redo");
    assert_eq!(dispatch(&mut owner, &redo)["ok"], true);
    let before = state(&owner);
    assert_eq!(dispatch(&mut owner, &hide), hidden);
    assert_eq!(state(&owner), before);
    let mut changed = hide.clone();
    changed["payload"]["visible"] = json!(true);
    assert_eq!(dispatch(&mut owner, &changed)["ok"], false);
    assert_eq!(state(&owner), before);
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), document());
    let visible = visibility_request(&owner, "show", 3, true);
    assert_eq!(dispatch(&mut owner, &visible)["ok"], true);
    assert!(owner.acquire_snapshot(4).unwrap().document().layers()[0].visible());
}

#[test]
fn layer_visibility_refusals_and_sealed_origins_never_publish_or_change_history() {
    let mut owner = owner();
    let before = state(&owner);
    for (id, patch) in [
        ("cancel-visible", json!({"cancelledBeforeDispatch":true})),
        (
            "wrong-visible-document",
            json!({"documentId":"other-document"}),
        ),
    ] {
        let mut value = visibility_request(&owner, id, 0, false);
        for (key, item) in patch.as_object().unwrap() {
            value[key] = item.clone();
        }
        assert_eq!(dispatch(&mut owner, &value)["ok"], false);
        assert_eq!(state(&owner), before);
    }
    let mut missing = visibility_request(&owner, "missing-visible-revision", 0, false);
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(dispatch(&mut owner, &missing)["ok"], false);
    assert_eq!(state(&owner), before);
    for (index, payload) in [
        json!({"command":"object.layer.visibility.set","layerUid":"missing","visible":false}),
        json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":null}),
        json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":"false"}),
        json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":false,"extra":1}),
    ].into_iter().enumerate() {
        let mut value = visibility_request(&owner, &format!("bad-{index}"), 0, false);
        value["payload"] = payload;
        assert_eq!(dispatch(&mut owner, &value)["ok"], false);
        assert_eq!(state(&owner), before);
    }
    let mut stale = visibility_request(&owner, "stale-visible", 9, false);
    assert_eq!(dispatch(&mut owner, &stale)["ok"], false);
    assert_eq!(state(&owner), before);
    stale["requestId"] = json!("wrong-instance");
    stale["expectedRevision"] = json!(0);
    stale["instanceId"] = json!("other");
    assert_eq!(dispatch(&mut owner, &stale)["ok"], false);
    assert_eq!(state(&owner), before);
    let bytes =
        serde_json::to_vec(&visibility_request(&owner, "prepared", 0, false)["payload"]).unwrap();
    let prepared = owner.prepare_layer_visibility_json(&bytes).unwrap();
    let mut other = self::owner();
    assert!(other.commit_layer_visibility(prepared.clone()).is_err());
    assert_eq!(other.content_revision(), 0);
    let changed = visibility_request(&owner, "changed-visible", 0, false);
    assert_eq!(dispatch(&mut owner, &changed)["ok"], true);
    let after = state(&owner);
    assert!(owner.commit_layer_visibility(prepared).is_err());
    assert_eq!(state(&owner), after);
    let duplicate = br#"{"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":false,"visible":true}"#;
    assert!(owner.prepare_layer_visibility_json(duplicate).is_err());
    assert_eq!(state(&owner), after);
}

#[cfg(feature = "compositor")]
#[test]
fn actual_visibility_dispatch_changes_frame_paint_and_undo_restores_original_indices() {
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
    let source = crate::object_render_scene::paint_test_document(&[7]);
    let mut owner = NativeObjectHistory::new(
        "visible-paint",
        object_codec::decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap();
    let render = |owner: &NativeObjectHistory| {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let pins = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: crate::object_document::FrameScopeKind::Authored,
            frame: 7,
        };
        let packet = prepare_object_frame(&snapshot, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        prepare_object_paint_plan(&scene, &pins)
            .unwrap()
            .commands()
            .map(|c| c.source().source_layer_index())
            .collect::<Vec<_>>()
    };
    assert_eq!(render(&owner), vec![1, 1, 1, 2, 0]);
    let mut hide = visibility_request(&owner, "paint-hide", 0, false);
    hide["payload"]["layerUid"] = json!("L1");
    assert_eq!(dispatch(&mut owner, &hide)["ok"], true);
    assert_eq!(render(&owner), vec![1, 1, 1, 0]);
    let undo = request(&owner, "paint-undo", 1, "undo");
    assert_eq!(dispatch(&mut owner, &undo)["ok"], true);
    assert_eq!(render(&owner), vec![1, 1, 1, 2, 0]);
    let redo = request(&owner, "paint-redo", 2, "redo");
    assert_eq!(dispatch(&mut owner, &redo)["ok"], true);
    assert_eq!(render(&owner), vec![1, 1, 1, 0]);
}
fn state(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    json!({"revision":owner.content_revision(),"depths":owner.history_depths(),
        "document":snapshot.document(),"bytes":object_codec::encode_project(snapshot.document()).unwrap()})
}
fn run(owner: &mut NativeObjectHistory, id: &str, revision: u64, action: &str) -> Value {
    let r = request(owner, id, revision, action);
    dispatch(owner, &r)
}

#[test]
fn undo_redo_restore_complete_golden_content_and_preserve_immutable_pins() {
    let mut owner = owner();
    let zero = owner.acquire_snapshot(0).unwrap();
    assert_eq!(run(&mut owner, "fill", 0, "fill.set")["ok"], true);
    let one = owner.acquire_snapshot(1).unwrap();
    for (id, revision, action, expected, depths) in [
        ("undo", 1, "undo", document(), json!([0, 1])),
        ("redo", 2, "redo", after_document(), json!([1, 0])),
    ] {
        let response = run(&mut owner, id, revision, action);
        assert_eq!(response["contentRevision"], revision + 1);
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":0})
        );
        let s = state(&owner);
        assert_eq!(s["document"], expected);
        assert_eq!(s["depths"], depths);
        let bytes =
            object_codec::encode_project(owner.acquire_snapshot(revision + 1).unwrap().document())
                .unwrap();
        assert_eq!(
            serde_json::to_value(object_codec::decode_project(&bytes).unwrap()).unwrap(),
            expected
        );
    }
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), document());
    assert_eq!(
        serde_json::to_value(one.document()).unwrap(),
        after_document()
    );
}

#[test]
fn retries_replay_selected_old_revision_before_stale_check_without_history_effect() {
    let mut owner = owner();
    let fill = request(&owner, "fill", 0, "fill.set");
    let fill_reply = dispatch(&mut owner, &fill);
    let undo = request(&owner, "undo", 1, "undo");
    let undo_reply = dispatch(&mut owner, &undo);
    let redo = request(&owner, "redo", 2, "redo");
    let redo_reply = dispatch(&mut owner, &redo);
    assert_eq!(
        run(&mut owner, "later-undo", 3, "undo")["contentRevision"],
        4
    );
    let before = state(&owner);
    for (r, reply) in [
        (&fill, &fill_reply),
        (&undo, &undo_reply),
        (&redo, &redo_reply),
    ] {
        assert_eq!(&dispatch(&mut owner, r), reply);
        assert_eq!(state(&owner), before);
    }
    let changed = request(&owner, "undo", 4, "redo");
    assert_eq!(
        dispatch(&mut owner, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(dispatch(&mut owner, &undo), undo_reply);
    assert_eq!(state(&owner), before);
    assert_eq!(
        run(&mut owner, "fresh-redo", 4, "redo")["contentRevision"],
        5
    );
    assert_eq!(state(&owner)["document"], after_document());
}

#[test]
fn empty_history_refusal_is_retained_even_after_later_commands_enable_the_move() {
    let mut owner = owner();
    let empty = request(&owner, "empty-undo", 0, "undo");
    let reply = dispatch(&mut owner, &empty);
    assert_eq!(reply["error"]["code"], "unavailable");
    assert_eq!(owner.history_depths(), (0, 0));
    run(&mut owner, "fill", 0, "fill.set");
    let before = state(&owner);
    assert_eq!(dispatch(&mut owner, &empty), reply);
    let corrected = request(&owner, "empty-undo", 1, "undo");
    assert_eq!(
        dispatch(&mut owner, &corrected)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&owner), before);
    run(&mut owner, "fresh-undo", 1, "undo");
    run(&mut owner, "fresh-redo", 2, "redo");
    let empty_redo = request(&owner, "empty-redo", 3, "redo");
    let reply = dispatch(&mut owner, &empty_redo);
    assert_eq!(reply["error"]["code"], "unavailable");
    run(&mut owner, "later-undo", 3, "undo");
    let before = state(&owner);
    assert_eq!(dispatch(&mut owner, &empty_redo), reply);
    assert_eq!(state(&owner), before);
}

#[test]
fn semantic_noop_keeps_redo_but_a_fresh_changed_fill_invalidates_only_redo() {
    let mut owner = owner();
    run(&mut owner, "fill", 0, "fill.set");
    run(&mut owner, "undo", 1, "undo");
    let before = state(&owner);
    let mut noop = request(&owner, "noop", 2, "fill.set");
    noop["payload"]["fill"] = document()["objects"][0]["fill"].clone();
    assert_eq!(
        dispatch(&mut owner, &noop)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), before);
    let changed = run(&mut owner, "changed-fill", 2, "fill.set");
    assert_eq!(
        changed["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        run(&mut owner, "redo-denied", 3, "redo")["error"]["code"],
        "unavailable"
    );
    assert_eq!(owner.content_revision(), 3);
}

#[test]
fn closed_operation_payloads_refuse_without_touching_any_content_or_history() {
    for action in ["undo", "redo"] {
        for payload in [
            json!({}),
            json!({"command":"object.fill.set"}),
            json!({"command":"object.undo","extra":true}),
            json!(["object.undo"]),
            json!({"command":null}),
            json!({"command":"object.undo","padding":"λ".repeat(2200)}),
        ] {
            let mut owner = owner();
            run(&mut owner, "fill", 0, "fill.set");
            let mut r = request(&owner, "invalid", 1, action);
            r["payload"] = payload;
            let before = state(&owner);
            let response = dispatch(&mut owner, &r);
            assert_eq!(response["error"]["code"], "invalid_request");
            assert_eq!(dispatch(&mut owner, &r), response);
            assert_eq!(state(&owner), before);
        }
        let mut owner = owner();
        run(&mut owner, "fill", 0, "fill.set");
        let mut wrong = request(&owner, "wrong-action", 1, action);
        wrong["payload"]["command"] = json!(if action == "undo" {
            "object.redo"
        } else {
            "object.undo"
        });
        assert_eq!(
            dispatch(&mut owner, &wrong)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(owner.content_revision(), 1);
    }
}

#[test]
fn admitted_missing_stale_and_cancelled_failures_are_stable_across_later_history() {
    for action in ["undo", "redo"] {
        for (mode, code) in [
            ("missing", "invalid_request"),
            ("stale", "stale_revision"),
            ("cancelled", "cancelled_before_dispatch"),
        ] {
            let mut owner = owner();
            run(&mut owner, "fill", 0, "fill.set");
            let mut r = request(&owner, "retained-error", 1, action);
            match mode {
                "missing" => {
                    r.as_object_mut().unwrap().remove("expectedRevision");
                }
                "stale" => r["expectedRevision"] = json!(0),
                _ => r["cancelledBeforeDispatch"] = json!(true),
            }
            let before = state(&owner);
            let response = dispatch(&mut owner, &r);
            assert_eq!(response["error"]["code"], code);
            assert_eq!(state(&owner), before);
            run(&mut owner, "later-undo", 1, "undo");
            let later = state(&owner);
            assert_eq!(dispatch(&mut owner, &r), response);
            r["expectedRevision"] = json!(2);
            r["cancelledBeforeDispatch"] = json!(false);
            assert_eq!(dispatch(&mut owner, &r)["error"]["code"], "invalid_request");
            assert_eq!(state(&owner), later);
        }
    }
}

#[test]
fn preflight_identity_bounds_and_unregistered_aliases_cannot_move_history() {
    let mut owner = owner();
    run(&mut owner, "fill", 0, "fill.set");
    let before = state(&owner);
    for (field, value, code) in [
        ("apiVersion", json!(3), "invalid_request"),
        ("instanceId", json!("other-instance"), "wrong_instance"),
        ("documentId", json!("other-document"), "wrong_document"),
        (
            "expectedRevision",
            json!(9_007_199_254_740_992_u64),
            "invalid_request",
        ),
    ] {
        let mut r = request(&owner, "preflight", 1, "undo");
        r[field] = value;
        assert_eq!(dispatch(&mut owner, &r)["error"]["code"], code);
        assert_eq!(state(&owner), before);
    }
    let mut invalid_fill = request(&owner, "fill-message", 1, "fill.set");
    invalid_fill["apiVersion"] = json!(3);
    assert_eq!(
        dispatch(&mut owner, &invalid_fill)["error"]["message"],
        "Invalid bounded object fill request."
    );
    assert_eq!(state(&owner), before);
    for operation in [
        "command.document.undo",
        "command.document.redo",
        "command.document.apply",
    ] {
        let mut r = request(&owner, "alias", 1, "undo");
        r["operation"] = json!(operation);
        let typed = serde_json::from_value::<OpacityRequest>(r).unwrap();
        assert!(owner.try_dispatch_object_command(&typed).unwrap().is_none());
        assert_eq!(state(&owner), before);
    }
    let typed =
        serde_json::from_value::<OpacityRequest>(request(&owner, "compat", 1, "undo")).unwrap();
    assert!(owner.try_dispatch_object_fill(&typed).unwrap().is_none());
    assert_eq!(state(&owner), before);
}
