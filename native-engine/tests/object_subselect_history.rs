//! G21 through the real command family, revision owner, history and storage codecs.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};
use support::{bytes, dispatch, document, owner_of, record_at, state, target};
const OP: &str = "command.document.object.subselect.edit";
fn req(owner: &NativeObjectHistory, id: &str, intent: Value) -> Value {
    let mut r = support::request(
        owner,
        id,
        json!({"command":"object.subselect.edit","stableTarget":target("L1",0,"s-a"),"intent":intent}),
    );
    r["operation"] = OP.into();
    r
}
fn doc(version: u32) -> Value {
    document(
        version,
        vec![
            record_at("A", "L1", 0, "s-a"),
            record_at("B", "L2", 0, "s-b"),
        ],
        None,
    )
}
#[test]
fn subselect_unsupported_consumers_refuse_read_preview_and_commit_atomically() {
    for kind in ["3d", "rig", "vtx", "animated-parametric"] {
        let mut input = doc(2);
        match kind {
            "3d" => input["layers"][0]["threeD"] = json!(true),
            "rig" => {
                input["layers"][0]["rig"] = json!({"bones":{},"ikChains":{},"nextId":1,"binds":[]})
            }
            "vtx" => {
                input["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"},"properties":[{"property":"vtx0","static":[1,2]}]}])
            }
            _ => {
                input["objects"][0]["vectorExt"] =
                    json!({"paramShape":{"kind":"rect","tl":0,"tr":0,"br":0,"bl":0}});
                input["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"},"properties":[{"property":"cornerTL","track":{"keys":[{"frame":0,"v":[0],"curvePoints":[]},{"frame":10,"v":[20],"curvePoints":[]}]}}]}]);
            }
        }
        let mut owner = owner_of(&input);
        let before = bytes(&owner);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        for preview in [false, true] {
            let mut page = json!({"cursor":0,"limit":16});
            if preview {
                page["preview"] = json!({"kind":"move","indices":[0],"delta":[1,2]});
            }
            let q = json!({"apiVersion":2,"requestId":"unsupported-read","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":page}});
            let r = serde_json::to_value(
                snapshot
                    .dispatch(serde_json::from_value(q).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(r["error"]["code"], "unavailable", "{kind}: {r}");
        }
        let q = req(
            &owner,
            "unsupported-edit",
            json!({"kind":"move","indices":[0],"delta":[1,2]}),
        );
        let r = dispatch(&mut owner, &q);
        assert_eq!(r["error"]["code"], "unavailable", "{kind}: {r}");
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.history_depths(), (0, 0));
    }
}
#[test]
fn subselect_commit_has_one_undo_replay_redo_and_reopen_in_both_formats() {
    for version in [1, 2] {
        let mut owner = owner_of(&doc(version));
        let before = bytes(&owner);
        let pinned = owner.acquire_snapshot(0).unwrap();
        let request = req(
            &owner,
            "subselect-move",
            json!({"kind":"move","indices":[0],"delta":[5,9]}),
        );
        let response = dispatch(&mut owner, &request);
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
        let after = bytes(&owner);
        let result = state(&owner);
        let initial: Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(result["objects"][1], initial["objects"][1]);
        for (axis, delta) in [("x", 5.0), ("y", 9.0)] {
            assert_eq!(
                result["objects"][0]["geometry"]["segments"][0]["point"][axis]
                    .as_f64()
                    .unwrap(),
                initial["objects"][0]["geometry"]["segments"][0]["point"][axis]
                    .as_f64()
                    .unwrap()
                    + delta
            );
        }
        assert_eq!(dispatch(&mut owner, &request), response);
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(encode_storage_project(pinned.document()).unwrap(), before);
        assert_eq!(owner.undo(1), Ok(2));
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.redo(2), Ok(3));
        assert_eq!(bytes(&owner), after);
        let reopened =
            NativeObjectHistory::new("subselect-reopen", decode_storage_project(&after).unwrap())
                .unwrap();
        assert_eq!(bytes(&reopened), after);
    }
}
#[test]
fn subselect_noop_and_cancelled_or_stale_intents_do_not_write() {
    let mut owner = owner_of(&doc(2));
    let before = bytes(&owner);
    let noop = req(
        &owner,
        "no-op",
        json!({"kind":"move","indices":[0],"delta":[0,0]}),
    );
    let result = dispatch(&mut owner, &noop);
    assert_eq!(
        result["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    let mut cancel = req(
        &owner,
        "cancel",
        json!({"kind":"move","indices":[0],"delta":[6,4]}),
    );
    cancel["cancelledBeforeDispatch"] = true.into();
    assert_eq!(
        dispatch(&mut owner, &cancel)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut stale = req(
        &owner,
        "stale",
        json!({"kind":"move","indices":[0],"delta":[6,4]}),
    );
    stale["expectedRevision"] = 9.into();
    assert_eq!(
        dispatch(&mut owner, &stale)["error"]["code"],
        "stale_revision"
    );
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}
#[test]
fn subselect_refuses_unknown_members_bad_indices_and_replacement_documents() {
    let mut owner = owner_of(&doc(2));
    let before = bytes(&owner);
    for (i, intent) in [
        json!({"kind":"move","indices":[0,0],"delta":[1,2]}),
        json!({"kind":"pointType","indices":[0],"pointType":"unknown"}),
        json!({"kind":"move","indices":[0],"delta":[1,2],"extra":true}),
    ]
    .into_iter()
    .enumerate()
    {
        let r = req(&owner, &format!("bad-{i}"), intent);
        assert_eq!(dispatch(&mut owner, &r)["error"]["code"], "invalid_request");
    }
    let mut r = req(
        &owner,
        "old-owner",
        json!({"kind":"move","indices":[0],"delta":[1,2]}),
    );
    r["documentId"] = "replaced-document".into();
    assert_eq!(dispatch(&mut owner, &r)["error"]["code"], "wrong_document");
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn subselect_preserves_unselected_exact_integers_through_edit_and_reopen() {
    let mut input = doc(2);
    input["objects"][0]["geometry"]["segments"][1]["point"]["x"] = json!(9007199254740993u64);
    let mut owner = owner_of(&input);
    let request = req(
        &owner,
        "exact-integer",
        json!({"kind":"move","indices":[0],"delta":[5,9]}),
    );
    assert_eq!(dispatch(&mut owner, &request)["ok"], true);
    let after = state(&owner);
    assert_eq!(
        after["objects"][0]["geometry"]["segments"][1],
        input["objects"][0]["geometry"]["segments"][1]
    );
    let saved = bytes(&owner);
    let reopened =
        NativeObjectHistory::new("exact-reopen", decode_storage_project(&saved).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), saved);
}

#[test]
fn subselect_identity_transforms_preserve_bytes_revision_and_nonempty_redo() {
    let mut input = doc(2);
    input["objects"][0]["geometry"]["segments"][0]["point"] = json!({"x":1,"y":1});
    let mut owner = owner_of(&input);
    let first = req(
        &owner,
        "seed",
        json!({"kind":"move","indices":[0],"delta":[5,9]}),
    );
    assert_eq!(dispatch(&mut owner, &first)["ok"], true);
    assert_eq!(owner.undo(1), Ok(2));
    let before = bytes(&owner);
    for (i, intent) in [
        json!({"kind":"scale","indices":[0],"scale":[1,1],"anchor":[1e20,1e20]}),
        json!({"kind":"rotate","indices":[0],"degrees":0,"center":[1e20,1e20]}),
    ]
    .into_iter()
    .enumerate()
    {
        let request = req(&owner, &format!("identity-{i}"), intent);
        assert_eq!(
            dispatch(&mut owner, &request)["result"],
            json!({"applied":false,"historyEntriesAdded":0})
        );
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.content_revision(), 2);
        assert_eq!(owner.history_depths(), (0, 1));
    }
}

#[test]
fn subselect_static_parametric_paths_match_original_geometry_and_saved_metadata() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/oracle.json")).unwrap();
    let cases: Vec<_> = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("vectorExt").is_some())
        .collect();
    assert_eq!(cases.len(), 24);
    let segments = |values: &Value| {
        json!(values.as_array().unwrap().iter().map(|s| {
        let pair=|key:&str| json!({"x":s[key][0],"y":s[key][1]});
        json!({"point":pair("point"),"handleIn":pair("handleIn"),"handleOut":pair("handleOut")})
    }).collect::<Vec<_>>())
    };
    for version in [1, 2] {
        for case in &cases {
            let mut record = record_at("A", "L1", 0, "s-a");
            record["geometry"]["closed"] = json!(true);
            record["geometry"]["segments"] = segments(&case["segments"]);
            record["vectorExt"] = case["vectorExt"].clone();
            let mut owner = owner_of(&document(version, vec![record], None));
            let before = bytes(&owner);
            // #2356: Corner is `collapse` against the frozen geometry; where the owner decision replaced the rule (Smooth/Symmetric
            // on a vertex that already has handles) the geometry is owned by the kernel tests and only the saved metadata is checked.
            let source: Vec<crate::object_document::subselect::Anchor> = serde_json::from_value(case["segments"].clone()).unwrap();
            let sent = serde_json::from_value(case["intent"].clone()).unwrap();
            let mapped = crate::object_document::subselect::point_type::tests::baseline_intent(&source, &sent);
            let intent = mapped.as_ref().map_or(case["intent"].clone(), |m| serde_json::to_value(m).unwrap());
            let q = req(&owner, "parametric-subselect", intent);
            let r = dispatch(&mut owner, &q);
            assert_eq!(r["ok"], true, "{case}: {r}");
            let actual = state(&owner);
            // #2356: a point-type edit also records its hint, aligned with the vertices; everything else is the frozen metadata.
            let mut saved = actual["objects"][0]["vectorExt"].clone();
            if let Some(hints) = saved.as_object_mut().and_then(|m| m.remove("pointTypes")) {
                assert_eq!(case["intent"]["kind"], "pointType", "{case}");
                assert_eq!(hints.as_array().unwrap().len(), case["segments"].as_array().unwrap().len());
            }
            assert_eq!(saved, case["expectedVectorExt"]);
            let actual = actual["objects"][0]["geometry"]["segments"]
                .as_array()
                .unwrap();
            let expected = segments(&case["expected"]);
            assert_eq!(actual.len(), expected.as_array().unwrap().len());
            for (a, b) in actual.iter().zip(expected.as_array().unwrap()).filter(|_| mapped.is_some()) {
                for key in ["point", "handleIn", "handleOut"] {
                    for axis in ["x", "y"] {
                        assert!(
                            (a[key][axis].as_f64().unwrap() - b[key][axis].as_f64().unwrap()).abs()
                                < 1e-6,
                            "{case}"
                        );
                    }
                }
            }
            let after = bytes(&owner);
            if r["result"]["applied"] == true {
                assert_eq!(owner.history_depths(), (1, 0));
                assert_eq!(owner.undo(1), Ok(2));
                assert_eq!(bytes(&owner), before);
                assert_eq!(owner.redo(2), Ok(3));
                assert_eq!(bytes(&owner), after);
            } else {
                assert_eq!(owner.history_depths(), (0, 0));
                assert_eq!(after, before);
            }
            let reopened = NativeObjectHistory::new(
                "parametric-reopen",
                decode_storage_project(&after).unwrap(),
            )
            .unwrap();
            assert_eq!(bytes(&reopened), after);
        }
    }
}

#[test]
fn subselect_alt_transition_preserves_both_tangents_through_preview_history_and_reopen() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/modifiers.json")).unwrap();
    for version in [1, 2] {
        for row in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["name"] == "handleOut-011" || r["name"] == "handleIn-101")
        {
            let mut input = doc(version);
            input["objects"][0]["geometry"]["segments"] = Value::Array(
                row["segments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| {
                        let mut segment = json!({});
                        for key in ["point", "handleIn", "handleOut"] {
                            segment[key] = json!({"x":s[key][0],"y":s[key][1]});
                        }
                        segment
                    })
                    .collect(),
            );
            let mut owner = owner_of(&input);
            let before = bytes(&owner);
            let step = &row["steps"][2];
            let snapshot = owner.acquire_snapshot(0).unwrap();
            let q = json!({"apiVersion":2,"requestId":"paired-preview","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":{"cursor":0,"limit":16,"preview":step["intent"]}}});
            let preview = serde_json::to_value(
                snapshot
                    .dispatch(serde_json::from_value(q).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(preview["ok"], true, "{preview}");
            assert_eq!(bytes(&owner), before);
            for (invalid, paired) in [Value::Null, json!([1]), json!(["x", 2])]
                .into_iter()
                .enumerate()
            {
                let mut bad = step["intent"].clone();
                bad["pairedPoint"] = paired;
                let q = req(&owner, &format!("paired-invalid-{invalid}"), bad);
                assert_eq!(dispatch(&mut owner, &q)["ok"], false);
                assert_eq!(bytes(&owner), before);
            }
            let request = req(&owner, "paired-commit", step["intent"].clone());
            let response = dispatch(&mut owner, &request);
            assert_eq!(
                response["result"],
                json!({"applied":true,"historyEntriesAdded":1}),
                "{response}"
            );
            let after = bytes(&owner);
            let actual = state(&owner);
            for (i, expected) in step["expected"].as_array().unwrap().iter().enumerate() {
                for key in ["point", "handleIn", "handleOut"] {
                    for (axis, n) in [("x", 0), ("y", 1)] {
                        let value = expected[key][n].as_f64().unwrap();
                        assert!(
                            (actual["objects"][0]["geometry"]["segments"][i][key][axis]
                                .as_f64()
                                .unwrap()
                                - value)
                                .abs()
                                < 1e-9
                        );
                        assert!(
                            (preview["result"]["subselect"]["segments"][i][key][n]
                                .as_f64()
                                .unwrap()
                                - value)
                                .abs()
                                < 1e-9
                        );
                    }
                }
            }
            assert_eq!(dispatch(&mut owner, &request), response);
            assert_eq!(owner.history_depths(), (1, 0));
            assert_eq!(owner.undo(1), Ok(2));
            assert_eq!(bytes(&owner), before);
            assert_eq!(owner.redo(2), Ok(3));
            assert_eq!(bytes(&owner), after);
            let reopened =
                NativeObjectHistory::new("paired-reopen", decode_storage_project(&after).unwrap())
                    .unwrap();
            assert_eq!(bytes(&reopened), after);
        }
    }
}
