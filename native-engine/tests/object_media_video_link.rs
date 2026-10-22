//! T12 linked-video import/replace through the original history/replay owner: one revision
//! and one undo entry on success, nothing published on any refusal, receipts replay first.
use crate::commands::LinkedVideoFacts;
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

fn facts(locator: &str) -> LinkedVideoFacts {
    LinkedVideoFacts {
        locator: locator.into(),
        width: 1920,
        height: 1080,
        frame_count: 48,
    }
}
fn request(owner: &NativeObjectHistory, id: &str, intent: Value) -> OpacityRequest {
    asset_request(
        owner,
        id,
        "video.link",
        json!({"documentSnapshotId":current_snapshot_id(owner),"lifecycleGeneration":1,
            "source":{"kind":"file","path":"/clips/take.mov"},"intent":intent}),
    )
}
fn result(response: &ResponseEnvelope) -> Value {
    serde_json::to_value(response).unwrap()["result"].clone()
}
fn code(response: &ResponseEnvelope) -> DispatchErrorCode {
    response.error().unwrap().code()
}
fn saved(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&saved_bytes(owner)).unwrap()
}
/// (records, layers, placements)
fn counts(owner: &NativeObjectHistory) -> (usize, usize, usize) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let inventory = snapshot.project().unwrap().asset_inventory();
    (
        inventory.map_or(0, |i| i.records().len()),
        snapshot.document().layers().len(),
        inventory.map_or(0, |i| i.placements().len()),
    )
}
fn timed_project() -> NativeObjectHistory {
    timed_project_in("scene-root")
}
/// The same timed placement, targeting `context` of an explicit two-context order plan
/// when it is not the scene root (a placement inside a component).
fn timed_project_in(context: &str) -> NativeObjectHistory {
    let linear = json!([{"x":0,"y":0},{"x":1,"y":1}]);
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":120,"layers":[{"layerUid":"L"}],"objects":[]});
    if context != "scene-root" {
        let layers = json!([{"layerUid":"L","naturalPosition":0,"order":{}}]);
        drawing["formatVersion"] = json!(2);
        drawing["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required",
            "referencePolicy":"unresolved","contexts":[
                {"contextId":"scene-root","layers":layers,"scopes":[]},
                {"contextId":context,"layers":layers,"scopes":[]}]});
    }
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":drawing,
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"/clips/old.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}}],
        "assetInventory":{"assetIds":["v1","a1"],"placements":[
            {"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":context,"layerUid":"L"},
             "offsetFrames":5,"interpMode":"blend","frameBlend":true,"timeRemap":{"keys":[
                {"frame":0,"v":[0],"curvePoints":linear,"hIn":[0,0],"hOut":[0,0]},
                {"frame":10,"v":[19],"curvePoints":linear,"hIn":[0,0],"hOut":[0,0]}]}}]}});
    NativeObjectHistory::from_project_file("video-replace", wire.to_string().as_bytes()).unwrap()
}

#[test]
fn a_new_video_layer_publishes_once_undo_redo_and_save_reload_together() {
    let mut owner = open_history();
    let input = request(&owner, "video-one", json!({"kind":"newLayer"}));
    assert_eq!(owner.video_link_status(&input).unwrap(), None);
    let reply = owner
        .dispatch_video_link(&input, &facts("/clips/take.mov"))
        .unwrap();
    assert!(reply.is_ok());
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(counts(&owner), (1, 2, 1));
    let receipt = result(&reply);
    let layer_uid = owner.acquire_snapshot(1).unwrap().document().layers()[1]
        .layer_uid()
        .to_owned();
    assert_eq!(
        receipt,
        json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-video-one",
            "placementId":"placement-video-one",
            "target":{"contextId":"scene-root","layerUid":layer_uid},"newLayer":true,
            "sourceWidth":1920,"sourceHeight":1080,"sourceFrameCount":48})
    );
    assert_eq!(
        owner.acquire_snapshot(1).unwrap().document().layers()[1].name(),
        Some("take")
    );
    let file = saved(&owner);
    assert_eq!(
        file["assets"][0],
        json!({"id":"asset-video-one","name":"take","kind":"video",
            "source":{"kind":"linked","locator":"/clips/take.mov"}})
    );
    assert_eq!(file["assetInventory"]["placements"][0]["offsetFrames"], 0);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved_bytes(&owner)).unwrap();
    assert_eq!(counts(&reopened), (1, 2, 1));
    assert_eq!(saved(&reopened), file);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(counts(&owner), (0, 1, 0));
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(counts(&owner), (1, 2, 1));
    assert_eq!(saved(&owner), file);
}

#[test]
fn exact_retry_replays_after_undo_before_any_probe_but_a_changed_body_refuses() {
    let mut owner = open_history();
    let input = request(&owner, "retry", json!({"kind":"newLayer"}));
    let first = owner
        .dispatch_video_link(&input, &facts("/clips/take.mov"))
        .unwrap();
    assert!(first.is_ok());
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(
        owner.video_link_status(&input).unwrap(),
        Some(first.clone())
    );
    // Different (even invalid) facts never reach preparation for a retained request id.
    let zero = LinkedVideoFacts {
        frame_count: 0,
        ..facts("/elsewhere.mov")
    };
    assert_eq!(owner.dispatch_video_link(&input, &zero).unwrap(), first);
    assert_eq!(counts(&owner), (0, 1, 0));
    assert_eq!(owner.history_depths(), (0, 1));
    let mut changed = serde_json::to_value(&input).unwrap();
    changed["payload"]["source"]["path"] = json!("/clips/other.mov");
    let changed: OpacityRequest = serde_json::from_value(changed).unwrap();
    let refused = owner.video_link_status(&changed).unwrap().unwrap();
    assert_eq!(code(&refused), DispatchErrorCode::InvalidRequest);
    assert_eq!(owner.video_link_status(&input).unwrap(), Some(first));
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn cancel_invalid_facts_and_stale_snapshot_publish_nothing() {
    for mode in [
        "cancel",
        "flag",
        "zero-frames",
        "stale-snapshot",
        "stale-revision",
    ] {
        let mut owner = open_history();
        let mut input = request(&owner, mode, json!({"kind":"newLayer"}));
        let mut video = facts("/clips/take.mov");
        match mode {
            "flag" => input.cancelled_before_dispatch = true,
            "zero-frames" => video.frame_count = 0,
            "stale-snapshot" => input.payload["documentSnapshotId"] = json!("snapshot-elsewhere"),
            "stale-revision" => input.expected_revision = Some(7),
            _ => {}
        }
        let reply = owner
            .dispatch_video_link_cancellable(&input, &video, &|| mode == "cancel")
            .unwrap();
        let expected = match mode {
            "cancel" | "flag" => DispatchErrorCode::CancelledBeforeDispatch,
            "zero-frames" => DispatchErrorCode::Unavailable,
            _ => DispatchErrorCode::StaleRevision,
        };
        assert_eq!(code(&reply), expected, "{mode}");
        assert_eq!(owner.content_revision(), 0, "{mode}");
        assert_eq!(owner.history_depths(), (0, 0), "{mode}");
        assert_eq!(counts(&owner), (0, 1, 0), "{mode}");
    }
}

#[test]
fn stale_prepared_candidate_cannot_commit_after_another_owner_edit() {
    use crate::commands::VideoLink;
    let mut owner = crate::commands::ObjectFillOwner::from_project_file(
        "desktop".into(),
        &blank_project_file(),
    )
    .unwrap();
    let before = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let payload = VideoLink::parse(
        &serde_json::to_vec(&json!({"documentSnapshotId":before.snapshot_id(),
        "lifecycleGeneration":1,"source":{"kind":"file","path":"/clips/take.mov"},
        "intent":{"kind":"newLayer"}}))
        .unwrap(),
    )
    .unwrap();
    let prepared = owner
        .prepare_video_link("stale-video", &payload, &facts("/clips/take.mov"))
        .unwrap();
    let other = owner
        .prepare_layer_create_json(br#"{"command":"object.layer.create","name":"Other"}"#)
        .unwrap();
    owner.commit_layer_create(other).unwrap();
    assert_eq!(
        owner.commit_video_link(prepared),
        Err(DispatchErrorCode::StaleRevision)
    );
    let now = owner.acquire_snapshot(owner.content_revision()).unwrap();
    assert!(now
        .project()
        .unwrap()
        .asset_inventory()
        .is_none_or(|i| i.records().is_empty()));
}

#[test]
fn replace_keeps_placement_identity_offset_and_timing_and_undo_restores_the_old_asset() {
    let mut owner = timed_project();
    let before = saved(&owner)["assetInventory"]["placements"][0].clone();
    assert_eq!(before["timeRemap"]["keys"][1]["v"], json!([19]));
    let input = request(&owner, "swap", json!({"kind":"replace","placementId":"p1"}));
    assert_eq!(owner.video_link_status(&input).unwrap(), None);
    let reply = owner
        .dispatch_video_link(&input, &facts("/clips/new.mov"))
        .unwrap();
    assert!(reply.is_ok());
    let receipt = result(&reply);
    assert_eq!(receipt["assetId"], "asset-swap");
    assert_eq!(receipt["placementId"], "p1");
    assert_eq!(receipt["newLayer"], false);
    assert_eq!(
        receipt["target"],
        json!({"contextId":"scene-root","layerUid":"L"})
    );
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(counts(&owner), (3, 1, 1));
    let file = saved(&owner);
    let after = file["assetInventory"]["placements"][0].clone();
    let mut expected = before.clone();
    expected["assetId"] = json!("asset-swap");
    assert_eq!(after, expected);
    // The old record is kept; the new one is linked by its canonical path.
    assert_eq!(file["assets"][0]["id"], "v1");
    assert_eq!(
        file["assets"][2]["source"],
        json!({"kind":"linked","locator":"/clips/new.mov"})
    );
    let reopened = NativeObjectHistory::from_project_file("reopen", &saved_bytes(&owner)).unwrap();
    assert_eq!(saved(&reopened), file);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(saved(&owner)["assetInventory"]["placements"][0], before);
    assert_eq!(counts(&owner), (2, 1, 1));
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(saved(&owner), file);
}

#[test]
fn replace_of_an_unknown_or_non_video_placement_and_same_file_refuse_without_history() {
    let mut owner = timed_project();
    let file = saved(&owner);
    let unknown = request(
        &owner,
        "unknown",
        json!({"kind":"replace","placementId":"nope"}),
    );
    let refused = owner.video_link_status(&unknown).unwrap().unwrap();
    assert_eq!(code(&refused), DispatchErrorCode::NotFound);
    let same = request(&owner, "same", json!({"kind":"replace","placementId":"p1"}));
    let refused = owner
        .dispatch_video_link(&same, &facts("/clips/old.mov"))
        .unwrap();
    assert_eq!(code(&refused), DispatchErrorCode::InvalidRequest);
    let mut fresh = open_history();
    let image = request(
        &fresh,
        "no-catalog",
        json!({"kind":"replace","placementId":"p1"}),
    );
    let refused = fresh
        .dispatch_video_link(&image, &facts("/clips/new.mov"))
        .unwrap();
    assert_eq!(code(&refused), DispatchErrorCode::NotFound);
    assert_eq!(owner.history_depths(), (0, 0));
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(saved(&owner), file);
}

#[test]
fn the_same_canonical_locator_reuses_its_linked_record() {
    let mut owner = open_history();
    let first = request(&owner, "first", json!({"kind":"newLayer"}));
    let first = owner
        .dispatch_video_link(&first, &facts("/clips/take.mov"))
        .unwrap();
    let second = request(&owner, "second", json!({"kind":"newLayer"}));
    let second = owner
        .dispatch_video_link(&second, &facts("/clips/take.mov"))
        .unwrap();
    assert_eq!(result(&first)["assetId"], "asset-first");
    assert_eq!(result(&second)["assetId"], "asset-first");
    assert_ne!(
        result(&first)["placementId"],
        result(&second)["placementId"]
    );
    assert_ne!(result(&first)["target"], result(&second)["target"]);
    assert_eq!(counts(&owner), (1, 3, 2));
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn wrong_owner_refusals_do_not_poison_a_genuine_request_id() {
    let mut owner = open_history();
    let genuine = request(&owner, "shared-id", json!({"kind":"newLayer"}));
    for (field, expected) in [
        ("instanceId", DispatchErrorCode::WrongInstance),
        ("documentId", DispatchErrorCode::WrongDocument),
    ] {
        let mut altered = serde_json::to_value(&genuine).unwrap();
        altered[field] = json!("foreign");
        let altered: OpacityRequest = serde_json::from_value(altered).unwrap();
        let reply = owner.video_link_status(&altered).unwrap().unwrap();
        assert_eq!(code(&reply), expected);
        assert_eq!(
            serde_json::to_value(&reply).unwrap()["instanceId"],
            owner.instance_id()
        );
    }
    for id in ["x".repeat(129), String::new(), "bad id".into()] {
        let input = request(&owner, &id, json!({"kind":"newLayer"}));
        assert_eq!(
            owner.video_link_status(&input),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    assert!(owner.video_link_status(&genuine).unwrap().is_none());
    assert!(owner
        .dispatch_video_link(&genuine, &facts("/clips/take.mov"))
        .unwrap()
        .is_ok());
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn wire_rejects_unknown_duplicate_positional_and_unbounded_members() {
    use crate::commands::VideoLink;
    let valid = json!({"documentSnapshotId":"snapshot","lifecycleGeneration":1,
        "source":{"kind":"file","path":"/clips/take.mov"},"intent":{"kind":"newLayer"}});
    assert!(VideoLink::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    let replace = json!({"documentSnapshotId":"snapshot","lifecycleGeneration":1,
        "source":{"kind":"file","path":"/clips/take.mov"},"intent":{"kind":"replace","placementId":"p1"}});
    assert!(VideoLink::parse(&serde_json::to_vec(&replace).unwrap()).is_ok());
    let mut bad = Vec::new();
    for (field, value) in [
        (
            "source",
            json!({"kind":"file","path":"/clips/take.mov","bytes":"x"}),
        ),
        ("source", json!({"kind":"file","path":"a\0b"})),
        ("source", json!({"kind":"file","path":""})),
        ("source", json!(["file", "/clips/take.mov"])),
        ("intent", json!({"kind":"newLayer","invented":true})),
        ("intent", json!({"kind":"replace","placementId":""})),
        ("intent", json!({"kind":"attach"})),
        ("lifecycleGeneration", json!(0)),
        ("name", json!("take")),
    ] {
        let mut value_of = valid.clone();
        value_of[field] = value;
        bad.push(serde_json::to_vec(&value_of).unwrap());
    }
    let duplicate = serde_json::to_string(&valid).unwrap().replace(
        "\"lifecycleGeneration\":1",
        "\"lifecycleGeneration\":1,\"lifecycleGeneration\":1",
    );
    bad.push(duplicate.into_bytes());
    bad.push(serde_json::to_vec(&json!(["snapshot", 1, {}, {}])).unwrap());
    bad.push(b"null".to_vec());
    for bytes in bad {
        assert!(
            VideoLink::parse(&bytes).is_err(),
            "{}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn replace_inside_a_component_context_publishes_and_the_receipt_names_that_context() {
    let mut owner = timed_project_in("sym_comp");
    let input = request(
        &owner,
        "in-comp",
        json!({"kind":"replace","placementId":"p1"}),
    );
    assert_eq!(owner.video_link_status(&input).unwrap(), None);
    let reply = owner
        .dispatch_video_link(&input, &facts("/clips/new.mov"))
        .unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    let receipt = result(&reply);
    assert_eq!(
        receipt["target"],
        json!({"contextId":"sym_comp","layerUid":"L"})
    );
    assert_eq!(receipt["newLayer"], false);
    assert_eq!(owner.history_depths(), (1, 0));
    let file = saved(&owner);
    assert_eq!(
        file["assetInventory"]["placements"][0]["target"],
        json!({"kind":"drawing","contextId":"sym_comp","layerUid":"L"})
    );
    assert_eq!(
        file["assetInventory"]["placements"][0]["assetId"],
        "asset-in-comp"
    );
    // A new layer is always created in the scene root, whatever context holds others.
    let fresh = request(&owner, "comp-new", json!({"kind":"newLayer"}));
    let reply = owner
        .dispatch_video_link(&fresh, &facts("/clips/other.mov"))
        .unwrap();
    assert_eq!(result(&reply)["target"]["contextId"], "scene-root");
}

#[test]
fn the_advertised_path_bound_fits_the_request_budget_with_every_other_field_at_its_maximum() {
    use crate::commands::VideoLink;
    let id = |n: usize| "a".repeat(n);
    let at = |path_bytes: usize| {
        json!({"apiVersion":2,"requestId":id(128),"instanceId":id(128),"documentId":id(128),
            "expectedRevision":9_007_199_254_740_991_u64,
            "operation":"command.document.object.asset.video.link",
            "payload":{"documentSnapshotId":id(128),"lifecycleGeneration":9_007_199_254_740_991_u64,
                "source":{"kind":"file","path":format!("/{}", id(path_bytes - 1))},
                "intent":{"kind":"replace","placementId":id(255)}},
            "cancelledBeforeDispatch":true})
    };
    let limit: OpacityRequest = serde_json::from_value(at(2048)).unwrap();
    assert!(serde_json::to_vec(&limit).unwrap().len() <= 4096);
    assert!(VideoLink::parse(&serde_json::to_vec(&limit.payload).unwrap()).is_ok());
    let over: OpacityRequest = serde_json::from_value(at(2049)).unwrap();
    assert!(VideoLink::parse(&serde_json::to_vec(&over.payload).unwrap()).is_err());
    // Through the owner: the bound passes admission, one byte more is refused.
    let mut owner = open_history();
    let mut input = request(&owner, &id(128), json!({"kind":"newLayer"}));
    input.payload["source"]["path"] = json!(format!("/{}", id(2047)));
    assert_eq!(owner.video_link_status(&input).unwrap(), None);
    assert!(owner
        .dispatch_video_link(&input, &facts("/clips/take.mov"))
        .unwrap()
        .is_ok());
    let mut longer = request(&owner, "longer", json!({"kind":"newLayer"}));
    longer.payload["source"]["path"] = json!(format!("/{}", id(2048)));
    let refused = owner.video_link_status(&longer).unwrap().unwrap();
    assert_eq!(code(&refused), DispatchErrorCode::InvalidRequest);
    assert_eq!(owner.history_depths(), (1, 0));
}
