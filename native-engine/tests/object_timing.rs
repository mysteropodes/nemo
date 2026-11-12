//! Frozen legacy exposure oracle consumed through actual native storage/history.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-layer-timing-v1/cases.json"
    ))
    .unwrap()
}
fn drawing(version: u32) -> Value {
    let oracle = corpus();
    let base: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let objects: Vec<_> = oracle["drawing"]["objects"].as_array().unwrap().iter().map(|entry| {
        let mut record = base["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":entry["frame"]},"layerUid":"L","strokeId":entry["strokeId"]});
        record
    }).collect();
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":8,
        "layers":[{"layerUid":"L","timing":oracle["drawing"]["timing"]}],"objects":objects});
    if version == 2 {
        let scopes: Vec<_> = (0..8).map(|frame| {
            let elements: Vec<_> = objects.iter().filter(|record| record["target"]["frameScope"]["frame"] == frame)
                .enumerate().map(|(i,record)| json!({"layerUid":"L","strokeId":record["target"]["strokeId"],"naturalPosition":i,"order":{}})).collect();
            json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
        }).collect();
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],"scopes":scopes}]});
    }
    value
}
fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "timing",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn selector(snapshot: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    json!({"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame})
}
fn read(snapshot: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    serde_json::to_value(
        snapshot
            .query_layer_timing_json(selector(snapshot, layer, frame).to_string().as_bytes())
            .unwrap(),
    )
    .unwrap()
}
fn project(version: u32) -> Value {
    let mut value = crate::object_codec::encoded_wire_test::wire();
    value["drawing"] = drawing(version);
    value
}
fn head(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    owner
        .serialize_project_file(owner.content_revision(), head(owner).snapshot_id())
        .unwrap()
}

#[test]
fn explicit_sparse_oracle_matches_both_native_storage_versions_and_actual_pins() {
    for version in [1, 2] {
        let doc = drawing(version);
        let decoded = decode_storage_project(doc.to_string().as_bytes()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&encode_storage_project(&decoded).unwrap()).unwrap(),
            doc
        );
        let pin = ObjectSnapshot::new("oracle", decoded).unwrap();
        for expected in corpus()["drawing"]["expected"].as_array().unwrap() {
            let response = read(&pin, "L", expected["frame"].as_u64().unwrap() as u32);
            for field in [
                "frame",
                "cellKind",
                "sourceFrame",
                "hasContent",
                "manualEdit",
            ] {
                assert_eq!(response[field], expected[field], "{field}");
            }
            assert_eq!(response["instanceId"], pin.instance_id());
            assert_eq!(response["documentId"], pin.document_id());
            assert_eq!(response["atRevision"], pin.content_revision());
            assert_eq!(response["documentSnapshotId"], pin.snapshot_id());
            assert_eq!(response["contextId"], "scene-root");
            assert_eq!(response["layerUid"], "L");
        }
        assert_eq!(
            read(&pin, "L", 4)["sourceFrame"],
            1,
            "sample3 is NOT the held source"
        );
        assert_eq!(
            read(&pin, "L", 7)["hasContent"],
            false,
            "manual sample6 does NOT end blank-key hold"
        );
    }
}

#[test]
fn unknown_timing_remains_omitted_while_an_explicit_empty_plan_is_unexposed() {
    for version in [1, 2] {
        let mut doc = drawing(version);
        doc["layers"][0].as_object_mut().unwrap().remove("timing");
        let decoded = decode_storage_project(doc.to_string().as_bytes()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&encode_storage_project(&decoded).unwrap()).unwrap(),
            doc
        );
        let pin = ObjectSnapshot::new("missing", decoded).unwrap();
        assert_eq!(
            pin.query_layer_timing_json(selector(&pin, "L", 1).to_string().as_bytes()),
            Err(DispatchErrorCode::Unavailable)
        );
        doc["layers"][0]["timing"] = corpus()["drawing"]["timing"].clone();
        doc["layers"][0]["timing"]["keys"] = json!([]);
        doc["layers"][0]["timing"]["samples"] = json!([]);
        let known = snapshot(&doc);
        assert_eq!(read(&known, "L", 1)["cellKind"], "unexposed");
        assert_eq!(
            read(&known, "L", 1)["hasContent"],
            false,
            "objects alone never imply a key"
        );
    }
}

#[test]
fn timing_admission_refuses_unknown_null_positional_duplicate_and_invalid_scopes() {
    let timing = corpus()["drawing"]["timing"].clone();
    let mut invalid = vec![Value::Null, json!([]), json!({}), json!("{}")];
    for (path, value) in [
        ("/family", json!("component")),
        ("/contextId", json!("symbol")),
        ("/formatVersion", json!(2)),
        ("/format", json!("other")),
        ("/keys/0/frame", json!(-1)),
        ("/keys/0/frame", json!(0.5)),
        ("/keys/0/frame", json!(4294967296u64)),
        ("/keys/0/frame", json!(8)),
        ("/samples/0/manualEdit", Value::Null),
        ("/samples/0/manualEdit", json!(1)),
    ] {
        let mut candidate = timing.clone();
        *candidate.pointer_mut(path).unwrap() = value;
        invalid.push(candidate);
    }
    for keys in [
        json!([{"frame":1},{"frame":1}]),
        json!([{"frame":5},{"frame":1}]),
        json!([[]]),
        json!([{"frame":1,"extra":true}]),
    ] {
        let mut value = timing.clone();
        value["keys"] = keys;
        invalid.push(value);
    }
    for samples in [
        json!([{"frame":3},{"frame":3}]),
        json!([{"frame":6},{"frame":3}]),
        json!([[]]),
        json!([{"frame":3,"manualEdit":"false"}]),
    ] {
        let mut value = timing.clone();
        value["samples"] = samples;
        invalid.push(value);
    }
    let mut collision = timing.clone();
    collision["samples"][0]["frame"] = json!(1);
    invalid.push(collision);
    let mut extra = timing.clone();
    extra["extra"] = json!(true);
    invalid.push(extra);
    for version in [1, 2] {
        for value in &invalid {
            let mut doc = drawing(version);
            doc["layers"][0]["timing"] = value.clone();
            assert!(decode_storage_project(doc.to_string().as_bytes()).is_err());
        }
        let raw = drawing(version).to_string();
        for bad in [
            raw.replace("\"timing\":", &format!("\"timing\":{},\"timing\":", timing)),
            raw.replace("\"keys\":", "\"ke\\u0079s\":[],\"keys\":"),
            raw.replace("\"manualEdit\":", "\"manualEdit\":false,\"manualEdit\":"),
            raw.replace("\"frame\":1", "\"frame\":1,\"frame\":1"),
        ] {
            assert!(decode_storage_project(bad.as_bytes()).is_err());
        }
    }
}

#[test]
fn long_timelines_and_the_exact_sparse_entry_cap_never_expand_per_frame() {
    let mut doc = drawing(1);
    doc["totalFrames"] = json!(u32::MAX);
    doc["objects"] = json!([]);
    doc["layers"][0]["timing"]["keys"] = json!([{"frame":u32::MAX-2}]);
    doc["layers"][0]["timing"]["samples"] = json!([]);
    let pin = snapshot(&doc);
    assert_eq!(read(&pin, "L", u32::MAX - 1)["sourceFrame"], u32::MAX - 2);
    assert_eq!(read(&pin, "L", 0)["cellKind"], "unexposed");
    doc["layers"][0]["timing"]["keys"] =
        Value::Array((0..2048).map(|i| json!({"frame":i*2})).collect());
    doc["layers"][0]["timing"]["samples"] =
        Value::Array((0..2048).map(|i| json!({"frame":i*2+1})).collect());
    let pin = snapshot(&doc);
    assert_eq!(read(&pin, "L", 4095)["cellKind"], "interpolated");
    assert_eq!(read(&pin, "L", u32::MAX - 1)["sourceFrame"], 4094);
    doc["layers"][0]["timing"]["samples"]
        .as_array_mut()
        .unwrap()
        .push(json!({"frame":4096}));
    assert!(decode_storage_project(doc.to_string().as_bytes()).is_err());
    for field in ["keys", "samples"] {
        doc["layers"][0]["timing"]["keys"] = json!([]);
        doc["layers"][0]["timing"]["samples"] = json!([]);
        doc["layers"][0]["timing"][field] =
            Value::Array((0..4097).map(|i| json!({"frame":i})).collect());
        assert!(
            decode_storage_project(doc.to_string().as_bytes()).is_err(),
            "per-vector bound {field}"
        );
    }
}

#[test]
fn raw_selector_is_closed_named_bounded_and_pins_the_real_immutable_view() {
    let pin = snapshot(&drawing(1));
    let valid = selector(&pin, "L", 4);
    let call = |value: &Value| pin.query_layer_timing_json(value.to_string().as_bytes());
    for (path, value, code) in [
        ("/atRevision", json!(1), DispatchErrorCode::StaleRevision),
        (
            "/documentSnapshotId",
            json!("replacement"),
            DispatchErrorCode::StaleRevision,
        ),
        (
            "/contextId",
            json!("symbol"),
            DispatchErrorCode::Unavailable,
        ),
        ("/layerUid", json!("absent"), DispatchErrorCode::NotFound),
        ("/layerUid", json!(""), DispatchErrorCode::InvalidRequest),
        ("/contextId", json!(""), DispatchErrorCode::InvalidRequest),
        ("/frame", json!(8), DispatchErrorCode::InvalidRequest),
        ("/frame", json!(-1), DispatchErrorCode::InvalidRequest),
        ("/frame", json!(1.5), DispatchErrorCode::InvalidRequest),
        (
            "/atRevision",
            json!(9007199254740992u64),
            DispatchErrorCode::InvalidRequest,
        ),
    ] {
        let mut candidate = valid.clone();
        *candidate.pointer_mut(path).unwrap() = value;
        assert_eq!(call(&candidate), Err(code), "{path}");
    }
    let raw = valid.to_string();
    for bad in [
        raw.replace("\"frame\":4", "\"frame\":4,\"frame\":4"),
        raw.replace(
            "\"atRevision\":0",
            "\"atRev\\u0069sion\":0,\"atRevision\":0",
        ),
        raw.replace("\"contextId\":", "\"extra\":true,\"contextId\":"),
        json!([0, pin.snapshot_id(), "scene-root", "L", 4]).to_string(),
        "null".into(),
        "{}".into(),
    ] {
        assert_eq!(
            pin.query_layer_timing_json(bad.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    for field in [
        "atRevision",
        "documentSnapshotId",
        "contextId",
        "layerUid",
        "frame",
    ] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(call(&missing), Err(DispatchErrorCode::InvalidRequest));
        let mut null = valid.clone();
        null[field] = Value::Null;
        assert_eq!(call(&null), Err(DispatchErrorCode::InvalidRequest));
    }
    let exact = format!("{raw}{}", " ".repeat(4096 - raw.len()));
    assert_eq!(exact.len(), 4096);
    assert!(pin.query_layer_timing_json(exact.as_bytes()).is_ok());
    assert_eq!(
        pin.query_layer_timing_json(format!("{exact} ").as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let long = ObjectSnapshot::new(
        "x".repeat(4096),
        decode_storage_project(drawing(1).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        long.query_layer_timing_json(selector(&long, "L", 4).to_string().as_bytes()),
        Err(DispatchErrorCode::Unavailable),
        "no truncated/fake identity on oversized output"
    );
}

#[test]
fn real_project_reference_fill_undo_redo_and_old_pins_preserve_exact_timing() {
    for version in [1, 2] {
        let bytes = project(version).to_string().into_bytes();
        let mut owner = NativeObjectHistory::from_project_file("history", &bytes).unwrap();
        let initial = head(&owner);
        let before = saved(&owner);
        let initial_read = read(&initial, "L", 4);
        let payload = json!({"command":"object.fill.set","stableTarget":drawing(version)["objects"][0]["target"],
            "fill":{"kind":"solid","r":0.987,"g":0.123,"b":0.456,"a":1.0}});
        let candidate = owner
            .prepare_fill_json(payload.to_string().as_bytes())
            .unwrap();
        owner.commit_fill(candidate).unwrap();
        assert_eq!(owner.content_revision(), 1);
        assert_eq!(read(&head(&owner), "L", 4)["sourceFrame"], 1);
        let after: Value = serde_json::from_slice(&saved(&owner)).unwrap();
        let old: Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(after["drawing"]["layers"], old["drawing"]["layers"]);
        assert_eq!(after["project"], old["project"]);
        assert_eq!(after["viewportReference"], old["viewportReference"]);
        assert_eq!(after["assets"], old["assets"]);
        owner.undo(1).unwrap();
        assert_eq!(saved(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(read(&initial, "L", 4), initial_read);
        assert_eq!(initial.serialize_project_file().unwrap(), before);
        let current = head(&owner);
        assert_eq!(
            current.query_layer_timing_json(selector(&initial, "L", 4).to_string().as_bytes()),
            Err(DispatchErrorCode::StaleRevision)
        );
        let fresh = NativeObjectHistory::from_project_file("reopen", &saved(&owner)).unwrap();
        assert_ne!(fresh.document_id(), owner.document_id());
        assert_eq!(read(&head(&fresh), "L", 7)["hasContent"], false);
        let info = owner.project_file_info().unwrap();
        assert_eq!(
            info["layers"],
            json!([{"layerUid":"L","kind":"draw"}]),
            "existing UI projection excludes new stored timing"
        );
    }
}

#[test]
fn new_create_replacement_and_inverse_history_seed_only_real_authored_intent() {
    let mut owner = NativeObjectHistory::new_blank_project("new-timing", 320, 180, 24.).unwrap();
    let before = head(&owner);
    assert_eq!(read(&before, "layer-1", 0)["cellKind"], "key");
    assert_eq!(read(&before, "layer-1", 1)["cellKind"], "held");
    assert_eq!(read(&before, "layer-1", 1)["hasContent"], false);
    let candidate = owner
        .prepare_layer_create_json(br#"{"command":"object.layer.create","name":"Added"}"#)
        .unwrap();
    owner.commit_layer_create(candidate).unwrap();
    let created = head(&owner);
    let uid = created
        .document()
        .layers
        .last()
        .unwrap()
        .layer_uid()
        .to_owned();
    assert_eq!(read(&created, &uid, 0)["cellKind"], "key");
    assert_eq!(read(&created, &uid, 17)["sourceFrame"], 0);
    owner.undo(1).unwrap();
    assert_eq!(
        head(&owner)
            .query_layer_timing_json(selector(&head(&owner), &uid, 0).to_string().as_bytes()),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(read(&created, &uid, 0)["cellKind"], "key");
    owner.redo(2).unwrap();
    let prepared = owner
        .prepare_project_replacement(
            head(&owner).snapshot_id(),
            project(2).to_string().as_bytes(),
        )
        .unwrap();
    owner.commit_project_replacement(prepared).unwrap();
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(read(&head(&owner), "L", 4)["sourceFrame"], 1);
    assert_eq!(read(&created, &uid, 17)["sourceFrame"], 0);
    let prepared = owner
        .prepare_blank_project_replacement(head(&owner).snapshot_id(), 100, 50, 24.)
        .unwrap();
    owner.commit_project_replacement(prepared).unwrap();
    assert_eq!(read(&head(&owner), "layer-1", 0)["cellKind"], "key");
}

fn shared_timing_request(pin: &ObjectSnapshot, payload: Value) -> crate::commands::OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"timing-public",
        "instanceId":pin.instance_id(),"documentId":pin.document_id(),
        "operation":"query.document.object.layer.timing","payload":payload}))
    .unwrap()
}

#[test]
fn shared_timing_envelope_preserves_all_frozen_oracle_fields_and_snapshot_identity() {
    let pin = snapshot(&drawing(1));
    let before = serde_json::to_value(pin.document()).unwrap();
    for expected in corpus()["drawing"]["expected"].as_array().unwrap() {
        let frame = expected["frame"].as_u64().unwrap() as u32;
        let response = pin
            .dispatch(shared_timing_request(&pin, selector(&pin, "L", frame)))
            .unwrap();
        assert!(response.ok);
        assert_eq!(response.content_revision, 0);
        let result = response.result.as_ref().unwrap();
        assert_eq!(result["instanceId"], pin.instance_id());
        assert_eq!(result["documentId"], pin.document_id());
        assert_eq!(result["documentSnapshotId"], pin.snapshot_id());
        assert_eq!(result["layerUid"], "L");
        assert_eq!(result["contextId"], "scene-root");
        assert_eq!(result["atRevision"], 0);
        assert_eq!(result.as_object().unwrap().len(), 11);
        for key in [
            "frame",
            "cellKind",
            "sourceFrame",
            "hasContent",
            "manualEdit",
        ] {
            assert_eq!(result[key], expected[key]);
        }
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    }
    assert_eq!(serde_json::to_value(pin.document()).unwrap(), before);
    let mut stale = selector(&pin, "L", 2);
    stale["atRevision"] = json!(1);
    let response = pin.dispatch(shared_timing_request(&pin, stale)).unwrap();
    assert_eq!(
        response.error.unwrap().code,
        DispatchErrorCode::StaleRevision
    );
}

#[test]
fn timing_shared_envelope_limit_includes_outer_identity_and_never_truncates() {
    let mut proved = false;
    for length in (3500..3900).step_by(10) {
        let uid = format!("雪{}", "x".repeat(length));
        let mut document = drawing(1);
        document["layers"][0]["layerUid"] = json!(uid);
        for object in document["objects"].as_array_mut().unwrap() {
            object["target"]["layerUid"] = json!(uid);
        }
        let pin = snapshot(&document);
        let payload = selector(&pin, &uid, 2);
        let request = shared_timing_request(&pin, payload.clone());
        if serde_json::to_vec(&request).unwrap().len() > 4096 {
            continue;
        }
        let Ok(inner) = pin.query_layer_timing_json(&serde_json::to_vec(&payload).unwrap()) else {
            continue;
        };
        let full = json!({"apiVersion":2,"requestId":request.request_id,"instanceId":pin.instance_id(),
            "documentId":pin.document_id(),"contentRevision":0,"ok":true,"result":inner});
        if serde_json::to_vec(&full).unwrap().len() <= 4096 {
            continue;
        }
        let response = pin.dispatch(request).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, DispatchErrorCode::Unavailable);
        assert!(response.result.is_none());
        proved = true;
        break;
    }
    assert!(
        proved,
        "a bounded selector must exercise a complete oversized success envelope"
    );
}
