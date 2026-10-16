//! Existing-owner structural creation, immutable consumers and shared history.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn drawing(version: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"});
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":120,"layers":[{"layerUid":"L0","name":"Original","visible":false},
        {"layerUid":"native-layer:1:0"}],"objects":[record]});
    if version == 2 {
        let layers = json!([{"layerUid":"L0","naturalPosition":1,"order":{"static":[9]}},
            {"layerUid":"native-layer:1:0","naturalPosition":0,"order":{}}]);
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":layers,"scopes":[
                {"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]},
                {"contextId":"unused-symbol","layers":layers,"scopes":[]}]});
    }
    value
}
fn owner(version: u32) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "layer-create",
        decode_storage_project(drawing(version).to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn value(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&encode_storage_project(current(owner).document()).unwrap()).unwrap()
}
fn payload() -> Value {
    json!({"command":"object.layer.create","name":"新しい Layer"})
}
fn request(owner: &NativeObjectHistory, id: &str) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":"command.document.object.layer.create","payload":payload()})
}
fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request.clone()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn expected(mut before: Value, revision: u64) -> Value {
    let id = if revision == 1 {
        "native-layer:1:1".to_owned()
    } else {
        format!("native-layer:{revision}:0")
    };
    before["layers"].as_array_mut().unwrap().push(
        json!({"layerUid":id,"name":"新しい Layer","visible":true,
            "locked":false,"color":"#F0A585","parentLayerUid":null,
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}),
    );
    if let Some(contexts) = before
        .pointer_mut("/compositionOrder/contexts")
        .and_then(Value::as_array_mut)
    {
        for context in contexts {
            context["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":id,"naturalPosition":2,"order":{}}));
        }
    }
    before
}

#[test]
fn create_preserves_v1_v2_content_and_every_context_with_one_history_entry() {
    for version in [1, 2] {
        let mut owner = owner(version);
        let before = current(&owner);
        let original = value(&owner);
        let candidate = owner
            .prepare_layer_create_json(payload().to_string().as_bytes())
            .unwrap();
        assert_eq!(value(&owner), original);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(owner.commit_layer_create(candidate).unwrap(), 1);
        assert_eq!(value(&owner), expected(original.clone(), 1));
        assert_eq!(serde_json::to_value(before.document()).unwrap(), original);
        assert_eq!(owner.history_depths(), (1, 0));
        let bytes = encode_storage_project(current(&owner).document()).unwrap();
        assert_eq!(
            serde_json::to_value(decode_storage_project(&bytes).unwrap()).unwrap(),
            value(&owner)
        );
        owner.undo(1).unwrap();
        assert_eq!(value(&owner), original);
        assert_eq!(owner.history_depths(), (0, 1));
        owner.redo(2).unwrap();
        assert_eq!(value(&owner), expected(original, 1));
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn exact_replay_collision_and_create_after_undo_keep_native_identity_monotonic() {
    let mut owner = owner(2);
    let req = request(&owner, "create");
    let first = dispatch(&mut owner, &req);
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:1"})
    );
    let after = value(&owner);
    assert_eq!(first["result"]["layerUid"], after["layers"][2]["layerUid"]);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let mut altered = req.clone();
    altered["payload"]["name"] = json!("different");
    assert_eq!(dispatch(&mut owner, &altered)["ok"], false);
    assert_eq!(value(&owner), after);
    owner.undo(1).unwrap();
    assert_eq!(
        dispatch(&mut owner, &req),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
    let next = request(&owner, "create-after-undo");
    let next_receipt = dispatch(&mut owner, &next);
    assert_eq!(next_receipt["ok"], true);
    assert_eq!(next_receipt["result"]["layerUid"], "native-layer:3:0");
    assert_ne!(
        next_receipt["result"]["layerUid"],
        first["result"]["layerUid"]
    );
    assert_eq!(value(&owner), expected(drawing(2), 3));
    assert_eq!(owner.history_depths(), (1, 0));
    assert_ne!(
        value(&owner)["layers"][2]["layerUid"],
        after["layers"][2]["layerUid"]
    );
}

#[test]
fn prepared_creation_rejects_foreign_and_stale_owners_without_history_effects() {
    let mut source = owner(2);
    let candidate = source
        .prepare_layer_create_json(payload().to_string().as_bytes())
        .unwrap();
    let mut other = owner(2);
    let before = value(&other);
    assert_eq!(
        other.commit_layer_create(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(value(&other), before);
    assert_eq!(other.history_depths(), (0, 0));
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(drawing(2).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign.commit_layer_create(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    source.commit_layer_create(candidate.clone()).unwrap();
    let after = value(&source);
    assert_eq!(
        source.commit_layer_create(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(value(&source), after);
    assert_eq!(source.history_depths(), (1, 0));
}

#[test]
fn raw_core_payload_refusals_preserve_the_original_owner_and_byte_budget() {
    let owner = owner(2);
    let original = value(&owner);
    for raw in [
        r#"{"command":"object.layer.create","name":"A","name":"B"}"#,
        r#"{"command":"object.layer.create","na\u006de":"A","name":"B"}"#,
        r#"{"command":"object.layer.create","name":"A","extra":true}"#,
        r#"{"command":"object.layer.create","name":null}"#,
        r#"{"command":"object.layer.create"}"#,
        r#"{"command":"object.layer.create","name":0}"#,
        r#"{"command":"other","name":"A"}"#,
        r#"["object.layer.create","A"]"#,
    ] {
        assert_eq!(
            owner.prepare_layer_create_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{raw}"
        );
        assert_eq!(value(&owner), original);
        assert_eq!(owner.history_depths(), (0, 0));
    }
    let mut raw = payload().to_string();
    raw.push_str(&" ".repeat(4096 - raw.len()));
    assert!(owner.prepare_layer_create_json(raw.as_bytes()).is_ok());
    raw.push(' ');
    assert_eq!(
        owner.prepare_layer_create_json(raw.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(value(&owner), original);
    assert_eq!(owner.content_revision(), 0);
}

#[test]
fn typed_dispatch_refusals_do_not_claim_raw_ingress_or_mutate_history() {
    let mut owner = owner(2);
    let original = value(&owner);
    for (id, patch) in [
        ("cancel", json!({"cancelledBeforeDispatch":true})),
        ("stale", json!({"expectedRevision":99})),
        ("foreign-instance", json!({"instanceId":"foreign"})),
        ("foreign-document", json!({"documentId":"foreign"})),
        ("wrong-api", json!({"apiVersion":1})),
        (
            "unknown-payload",
            json!({"payload":{"command":"object.layer.create","name":"A","extra":1}}),
        ),
        (
            "caller-id",
            json!({"payload":{"command":"object.layer.create","name":"A","layerUid":"chosen"}}),
        ),
    ] {
        let mut req = request(&owner, id);
        for (key, val) in patch.as_object().unwrap() {
            req[key] = val.clone();
        }
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], false, "{id}");
        let code = match id {
            "cancel" => "cancelled_before_dispatch",
            "stale" => "stale_revision",
            "foreign-instance" => "wrong_instance",
            "foreign-document" => "wrong_document",
            _ => "invalid_request",
        };
        assert_eq!(reply["error"]["code"], code, "{id}");
        assert_eq!(value(&owner), original);
        assert_eq!(owner.history_depths(), (0, 0));
    }
    let mut req = request(&owner, "missing-revision");
    req.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(dispatch(&mut owner, &req)["ok"], false);
    assert_eq!(value(&owner), original);
    assert_eq!(owner.content_revision(), 0);
}

#[test]
fn creation_and_fill_share_one_undo_redo_sequence_and_retained_snapshots() {
    let mut owner = owner(2);
    let zero = current(&owner);
    let create = request(&owner, "create");
    assert_eq!(dispatch(&mut owner, &create)["ok"], true);
    let one = value(&owner);
    let mut fill = request(&owner, "fill-after-create");
    fill["operation"] = json!("command.document.object.fill.set");
    fill["payload"] = json!({"command":"object.fill.set","stableTarget":one["objects"][0]["target"],
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":0.4}});
    assert_eq!(dispatch(&mut owner, &fill)["ok"], true);
    let two = value(&owner);
    assert_eq!(owner.history_depths(), (2, 0));
    owner.undo(2).unwrap();
    assert_eq!(value(&owner), one);
    owner.undo(3).unwrap();
    assert_eq!(value(&owner), drawing(2));
    owner.redo(4).unwrap();
    assert_eq!(value(&owner), one);
    owner.redo(5).unwrap();
    assert_eq!(value(&owner), two);
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), drawing(2));
    assert_eq!(owner.history_depths(), (2, 0));
}

#[cfg(feature = "project_media_codec")]
#[test]
fn project_reference_and_metadata_survive_create_save_reopen_and_undo() {
    use crate::object_document::{ProjectMetadata, ProjectReferenceState};
    let hex="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
    let png: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    for version in [1, 2] {
        let document = decode_storage_project(drawing(version).to_string().as_bytes()).unwrap();
        let project =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), document)
                .with_reference(
                    ProjectReferenceState::admitted_still(
                        "media-1".into(),
                        "plate.png".into(),
                        false,
                        0.5,
                        -7,
                        png.clone(),
                        1024,
                    )
                    .unwrap(),
                );
        let mut owner =
            NativeObjectHistory::from_admitted_project("project-create", project).unwrap();
        let zero = current(&owner);
        let before: Value =
            serde_json::from_slice(&zero.serialize_project_file().unwrap()).unwrap();
        let req = request(&owner, "create-with-reference");
        assert_eq!(dispatch(&mut owner, &req)["ok"], true);
        let snapshot = current(&owner);
        let bytes = owner
            .serialize_project_file(1, snapshot.snapshot_id())
            .unwrap();
        let after: Value = serde_json::from_slice(&bytes).unwrap();
        let mut want = before.clone();
        want["drawing"] = expected(drawing(version), 1);
        assert_eq!(after, want);
        let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
        let reopened_snapshot = current(&reopened);
        assert_ne!(reopened.document_id(), owner.document_id());
        assert_eq!(
            reopened
                .serialize_project_file(0, reopened_snapshot.snapshot_id())
                .unwrap(),
            bytes
        );
        owner.undo(1).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&current(&owner).serialize_project_file().unwrap())
                .unwrap(),
            before
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&zero.serialize_project_file().unwrap()).unwrap(),
            before
        );
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn actual_frame_packets_retain_old_layers_and_admit_new_empty_layer_metadata() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    for version in [1, 2] {
        let mut source = drawing(version);
        source["layers"][0]["visible"] = json!(true);
        let mut owner = NativeObjectHistory::new(
            "layer-create",
            decode_storage_project(source.to_string().as_bytes()).unwrap(),
        )
        .unwrap();
        let zero = current(&owner);
        let packet = |snapshot: &ObjectSnapshot| {
            prepare_object_frame(
                snapshot,
                &ObjectFrameSelector {
                    instance_id: snapshot.instance_id(),
                    document_id: snapshot.document_id(),
                    content_revision: snapshot.content_revision(),
                    document_snapshot_id: snapshot.snapshot_id(),
                    context_id: "scene-root",
                    scope_kind: FrameScopeKind::Authored,
                    frame: 0,
                },
            )
            .unwrap()
        };
        let before = packet(&zero);
        let req = request(&owner, "create-for-packet");
        assert_eq!(dispatch(&mut owner, &req)["ok"], true);
        let after = packet(&current(&owner));
        assert_eq!(before.source_layers().len(), 2);
        assert_eq!(after.source_layers().len(), 3);
        assert_eq!(after.source_layers()[2].layer_uid(), "native-layer:1:1");
        assert_eq!(after.source_layers()[2].name(), Some("新しい Layer"));
        assert!(after.source_layers()[2].visible());
        let metadata = serde_json::to_value(&after.source_layers()[2]).unwrap();
        assert_eq!(metadata["locked"], false);
        assert_eq!(metadata["color"], "#F0A585");
        assert!(metadata["parentLayerUid"].is_null());
        assert_eq!(before.records().count(), after.records().count());
        assert_eq!(after.records().count(), 1);
        assert_eq!(before.content_revision(), 0);
        assert_eq!(after.content_revision(), 1);
        assert_eq!(packet(&zero).source_layers().len(), 2);
    }
}

#[test]
fn creation_palette_wraps_by_layer_count_and_undo_does_not_consume_a_color() {
    let colors = [
        "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4", "#5FA875", "#B49BE6",
        "#F0A585",
    ];
    for version in [1, 2] {
        let mut owner = owner(version);
        for (i, color) in colors.iter().enumerate() {
            let req = request(&owner, &format!("palette-{i}"));
            assert_eq!(dispatch(&mut owner, &req)["ok"], true);
            let after = value(&owner);
            let layer = after["layers"].as_array().unwrap().last().unwrap();
            assert_eq!(layer["color"], *color);
            assert_eq!(layer["locked"], false);
            assert_eq!(layer.get("parentLayerUid"), Some(&Value::Null));
        }
        let last = value(&owner)["layers"][10].clone();
        owner.undo(owner.content_revision()).unwrap();
        let req = request(&owner, "after-palette-undo");
        assert_eq!(dispatch(&mut owner, &req)["ok"], true);
        let next = value(&owner)["layers"][10].clone();
        assert_eq!(next["color"], last["color"]);
        assert_ne!(next["layerUid"], last["layerUid"]);
    }
}

#[test]
fn layer_defaults_preserve_missing_fields_custom_color_and_original_snapshots() {
    for version in [1, 2] {
        let mut source = drawing(version);
        source["layers"][0]["locked"] = json!(false);
        source["layers"][0]["color"] = json!("#aB12ef");
        source["layers"][0]["parentLayerUid"] = Value::Null;
        let admitted = decode_storage_project(source.to_string().as_bytes()).unwrap();
        let mut owner = NativeObjectHistory::new("metadata", admitted).unwrap();
        let old = current(&owner);
        let req = request(&owner, "preserve-labels");
        assert_eq!(dispatch(&mut owner, &req)["ok"], true);
        let after = value(&owner);
        assert_eq!(after["layers"][0], source["layers"][0]);
        assert_eq!(after["layers"][1], source["layers"][1]);
        let bytes = encode_storage_project(current(&owner).document()).unwrap();
        assert_eq!(
            encode_storage_project(&decode_storage_project(&bytes).unwrap()).unwrap(),
            bytes
        );
        owner.undo(1).unwrap();
        assert_eq!(value(&owner), source);
        owner.redo(2).unwrap();
        assert_eq!(value(&owner), after);
        assert_eq!(serde_json::to_value(old.document()).unwrap(), source);
    }
}

#[test]
fn layer_defaults_refuse_unimplemented_policy_malformed_values_and_duplicate_fields() {
    for version in [1, 2] {
        for (field, invalid) in [
            ("locked", Value::Null),
            ("locked", json!(0)),
            ("parentLayerUid", json!("L0")),
            ("parentLayerUid", json!(false)),
            ("color", Value::Null),
            ("color", json!(12)),
            ("color", json!("red")),
            ("color", json!("#123")),
            ("color", json!("#12345g")),
            ("color", json!("#12345678")),
            ("color", json!("#é2345")),
        ] {
            let mut source = drawing(version);
            source["layers"][0][field] = invalid;
            assert!(
                decode_storage_project(source.to_string().as_bytes()).is_err(),
                "{source}"
            );
        }
        for duplicate in [
            r#""locked":false,"locked":false,"#,
            r##""color":"#123456","color":"#654321","##,
            r#""parentLayerUid":null,"parentLayerUid":null,"#,
        ] {
            let raw = drawing(version).to_string().replacen(
                r#""name":"Original""#,
                &format!("{duplicate}\"name\":\"Original\""),
                1,
            );
            assert!(decode_storage_project(raw.as_bytes()).is_err(), "{raw}");
        }
    }
}
