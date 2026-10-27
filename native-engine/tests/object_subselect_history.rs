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
    for kind in ["3d", "rig", "vtx"] {
        let mut input = doc(2);
        match kind {
            "3d" => input["layers"][0]["threeD"] = json!(true),
            "rig" => {
                input["layers"][0]["rig"] = json!({"bones":{},"ikChains":{},"nextId":1,"binds":[]})
            }
            _ => {
                input["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-a"},"properties":[{"property":"vtx0","static":[1,2]}]}])
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
