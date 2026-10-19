//! Actual sealed cut owner/history, not modeled undo or revision bookkeeping.
use crate::commands::{ObjectFillOwner, PreparedLayerPlayheadCut};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_component_convert_fixture.rs"]
#[allow(dead_code)]
mod build;
use build::{decode, document, layer, wire};

fn fixture() -> Value {
    document(
        &[
            layer("before", "Before", &[(0, "back")]),
            layer("a", "Ink", &[(0, "ink"), (15, "ink")])
                .with(json!({"range":{"inPoint":2,"outPoint":20,
            "markers":[{"frame":12,"name":"α"}]},"locked":true})),
            layer("after", "After", &[(0, "front")]),
        ],
        json!({"propertyHolders":[
            {"target":{"contextId":"scene-root","layerUid":"a"},"controls":[],
             "properties":[{"property":"position","static":[2.5,7],"track":{"keys":[{"frame":3.5,"v":[1,2],"hold":true},{"frame":16.5,"v":[9,8]}]}}]},
            {"target":{"contextId":"scene-root","layerUid":"a","strokeId":"ink"},
             "properties":[{"property":"opacity","static":[82.5]}]}
        ]}),
    )
}
fn payload(frame: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":frame})).unwrap()
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("cut-history", decode(value)).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(
        owner
            .acquire_snapshot(owner.content_revision())
            .unwrap()
            .document(),
    )
    .unwrap()
}
fn prepare(owner: &NativeObjectHistory, frame: u32) -> PreparedLayerPlayheadCut {
    owner
        .prepare_layer_playhead_cut_json(&payload(frame))
        .unwrap()
}

#[test]
fn cut_publishes_once_with_one_undo_entry_and_exact_save_reopen_and_old_snapshots() {
    let input = fixture();
    let mut history = owner(&input);
    let original = bytes(&history);
    let old = history.acquire_snapshot(0).unwrap();
    let candidate = prepare(&history, 10);
    assert!(candidate.changed());
    assert_eq!(candidate.created(), Some("native-layer:1:0"));
    let selection = candidate.selection().unwrap();
    assert_eq!(
        (
            selection.active,
            selection.selected.as_slice(),
            selection.anchor
        ),
        (2, &[2][..], Some(2))
    );
    assert_eq!(bytes(&history), original, "prepare must not publish");
    assert_eq!(history.history_depths(), (0, 0));
    assert_eq!(wire(candidate.document())["layers"][1]["locked"], true);
    assert_eq!(
        wire(candidate.document())["layers"][2]["locked"],
        false,
        "ordinary second half keeps createUserLayer's unlocked default"
    );
    let planned = encode_storage_project(candidate.document()).unwrap();
    assert_eq!(history.commit_layer_playhead_cut(candidate).unwrap(), 1);
    let after = bytes(&history);
    let retained = history.acquire_snapshot(1).unwrap();
    assert_eq!(after, planned);
    assert_eq!(history.history_depths(), (1, 0));
    assert_eq!(encode_storage_project(old.document()).unwrap(), original);
    let reopened = decode_storage_project(&after).unwrap();
    assert_eq!(encode_storage_project(&reopened).unwrap(), after);
    let reopened_owner = NativeObjectHistory::new("reopened-cut", reopened).unwrap();
    assert_eq!(bytes(&reopened_owner), after);
    assert_eq!(history.undo(1).unwrap(), 2);
    assert_eq!(bytes(&history), original);
    assert_eq!(history.history_depths(), (0, 1));
    assert_eq!(encode_storage_project(retained.document()).unwrap(), after);
    assert_eq!(history.redo(2).unwrap(), 3);
    assert_eq!(bytes(&history), after);
    assert_eq!(history.history_depths(), (1, 0));
}

#[test]
fn outside_cut_keeps_revision_selection_presence_bytes_and_redo_exact() {
    let mut history = owner(&fixture());
    let candidate = prepare(&history, 10);
    history.commit_layer_playhead_cut(candidate).unwrap();
    history.undo(1).unwrap();
    let original = bytes(&history);
    for frame in [0, 2, 21, 23] {
        let candidate = prepare(&history, frame);
        assert!(!candidate.changed());
        assert!(candidate.created().is_none());
        assert!(candidate.selection().is_none());
        assert_eq!(
            encode_storage_project(candidate.document()).unwrap(),
            original
        );
        assert_eq!(history.commit_layer_playhead_cut(candidate).unwrap(), 2);
        assert_eq!(history.content_revision(), 2);
        assert_eq!(history.history_depths(), (0, 1));
        assert_eq!(bytes(&history), original);
    }
    history.redo(2).unwrap();
    assert_eq!(history.history_depths(), (1, 0));
}

#[test]
fn foreign_document_instance_and_stale_candidates_cannot_publish_partial_clones() {
    let input = fixture();
    let mut source = owner(&input);
    let candidate = prepare(&source, 10);
    let mut other = owner(&input);
    let original_other = bytes(&other);
    assert_eq!(
        other
            .commit_layer_playhead_cut(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(
        (bytes(&other), other.history_depths()),
        (original_other, (0, 0))
    );
    let mut foreign = NativeObjectHistory::new("foreign-cut", decode(&input)).unwrap();
    let foreign_before = bytes(&foreign);
    assert_eq!(
        foreign
            .commit_layer_playhead_cut(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(
        (bytes(&foreign), foreign.history_depths()),
        (foreign_before, (0, 0))
    );
    source.commit_layer_playhead_cut(candidate.clone()).unwrap();
    let after = bytes(&source);
    assert_eq!(
        source.commit_layer_playhead_cut(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&source), source.history_depths()), (after, (1, 0)));
}

#[test]
fn dropping_prepared_cut_leaves_no_layer_order_holder_or_history_change() {
    let history = owner(&fixture());
    let original = bytes(&history);
    drop(prepare(&history, 10));
    assert_eq!(history.content_revision(), 0);
    assert_eq!(history.history_depths(), (0, 0));
    assert_eq!(bytes(&history), original);
}

#[test]
fn strict_raw_payload_refusals_and_inclusive4096_limit_leave_owner_untouched() {
    let history = owner(&fixture());
    let original = bytes(&history);
    let bad = [
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":10,"frame":11}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":10,"extra":1}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"other","contextId":"scene-root","layerUid":"a","frame":10}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":null}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":-1}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":1.5}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":24}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":4294967296}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"missing","frame":10}"#,
            DispatchErrorCode::NotFound,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"symbol-context","layerUid":"a","frame":10}"#,
            DispatchErrorCode::Unavailable,
        ),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","layerUid":"","frame":10}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (r#"["object.layer.cut"]"#, DispatchErrorCode::InvalidRequest),
        (
            r#"{"command":"object.layer.cut","contextId":"scene-root","frame":10}"#,
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (raw, code) in bad {
        assert_eq!(
            history
                .prepare_layer_playhead_cut_json(raw.as_bytes())
                .unwrap_err(),
            code,
            "{raw}"
        );
        assert_eq!(
            (
                bytes(&history),
                history.content_revision(),
                history.history_depths()
            ),
            (original.clone(), 0, (0, 0))
        );
    }
    let mut limit = payload(10);
    limit.resize(4096, b' ');
    assert!(history.prepare_layer_playhead_cut_json(&limit).is_ok());
    limit.push(b' ');
    assert_eq!(
        history.prepare_layer_playhead_cut_json(&limit).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(bytes(&history), original);
}

#[test]
fn direct_fill_owner_uses_same_real_preparation_and_single_revision_publication() {
    let mut owner = ObjectFillOwner::new("cut-direct", decode(&fixture())).unwrap();
    let before = owner.acquire_snapshot(0).unwrap();
    let candidate = owner.prepare_layer_playhead_cut_json(&payload(10)).unwrap();
    let planned = wire(candidate.document());
    assert_eq!(wire(before.document()), fixture());
    assert_eq!(owner.commit_layer_playhead_cut(candidate).unwrap(), 1);
    assert_eq!(wire(owner.acquire_snapshot(1).unwrap().document()), planned);
    assert_eq!(wire(before.document()), fixture());
}

#[test]
fn staged_cut_uses_existing_receipts_for_replay_after_undo_and_fresh_cancel_stale_refusals() {
    use crate::request_receipts::OpacityRequest;
    fn request(
        history: &NativeObjectHistory,
        id: &str,
        revision: u64,
        frame: u32,
    ) -> OpacityRequest {
        serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":history.instance_id(),
            "documentId":history.document_id(),"expectedRevision":revision,
            "operation":"command.document.object.layer.cut",
            "payload":{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":frame}})).unwrap()
    }
    fn dispatch(history: &mut NativeObjectHistory, request: &OpacityRequest) -> Value {
        serde_json::to_value(
            history
                .try_dispatch_object_command(request)
                .unwrap()
                .expect("Root staged cut operation must be explicitly registered"),
        )
        .unwrap()
    }
    let mut history = owner(&fixture());
    let original = bytes(&history);
    let accepted = request(&history, "cut-replay-main", 0, 10);
    let first = dispatch(&mut history, &accepted);
    assert_eq!(first["ok"], true);
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"})
    );
    let after = bytes(&history);
    assert_eq!(dispatch(&mut history, &accepted), first);
    assert_eq!(
        (history.content_revision(), history.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(bytes(&history), after);
    let changed_body = request(&history, "cut-replay-main", 0, 11);
    let refused = dispatch(&mut history, &changed_body);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(
        (
            bytes(&history),
            history.content_revision(),
            history.history_depths()
        ),
        (after, 1, (1, 0))
    );
    history.undo(1).unwrap();
    assert_eq!(bytes(&history), original);
    assert_eq!(
        dispatch(&mut history, &accepted),
        first,
        "historical replay cannot execute against restored drawing"
    );
    assert_eq!(
        (history.content_revision(), history.history_depths()),
        (2, (0, 1))
    );
    assert_eq!(bytes(&history), original);
    // These IDs have never been dispatched: cancellation/staleness must reach
    // their guards, rather than accidentally replaying the earlier success.
    let cancelled = request(&history, "cut-cancel-fresh", 2, 10).cancelled();
    let stale = request(&history, "cut-stale-fresh", 1, 10);
    for (req, code) in [
        (cancelled, "cancelled_before_dispatch"),
        (stale, "stale_revision"),
    ] {
        let reply = dispatch(&mut history, &req);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"]["code"], code);
        assert_eq!(
            (
                bytes(&history),
                history.content_revision(),
                history.history_depths()
            ),
            (original.clone(), 2, (0, 1))
        );
    }
    let noop = request(&history, "cut-outside-fresh", 2, 2);
    let reply = dispatch(&mut history, &noop);
    assert_eq!(reply["ok"], true);
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        (
            bytes(&history),
            history.content_revision(),
            history.history_depths()
        ),
        (original, 2, (0, 1))
    );
    assert_eq!(
        history.redo(2).unwrap(),
        3,
        "cancel/stale/noop/replay preserve the real redo entry"
    );
    assert_eq!(history.history_depths(), (1, 0));
}

#[cfg(feature = "project_media_codec")]
#[test]
fn linked_video_project_placement_refuses_inside_source_cut_but_preserves_noop_redo_and_unrelated_assets(
) {
    fn saved_project(history: &NativeObjectHistory) -> Vec<u8> {
        let pin = history
            .acquire_snapshot(history.content_revision())
            .unwrap();
        history
            .serialize_project_file(history.content_revision(), pin.snapshot_id())
            .unwrap()
    }
    fn cut(
        history: &NativeObjectHistory,
        uid: &str,
        frame: u32,
    ) -> Result<PreparedLayerPlayheadCut, DispatchErrorCode> {
        let raw = serde_json::to_vec(&json!({"command":"object.layer.cut","contextId":"scene-root","layerUid":uid,"frame":frame})).unwrap();
        history.prepare_layer_playhead_cut_json(&raw)
    }
    for placement_layer in ["a", "before"] {
        // Actual linked video asset/placement schema from the existing media
        // owner tests; no decoder, font provider or rendered video is invented.
        let project = json!({"format":"nemo.native-project-document","formatVersion":2,
            "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":fixture(),
            "viewportReference":null,
            "assets":[{"id":"v1","name":"take α","kind":"video",
                "source":{"kind":"linked","locator":"clips/take.mov"}}],
            "assetInventory":{"assetIds":["v1"],"placements":[{"id":"video-placement","assetId":"v1",
                "target":{"kind":"drawing","contextId":"scene-root","layerUid":placement_layer},"offsetFrames":-3}]}});
        let mut history = NativeObjectHistory::from_project_file(
            "cut-video-project",
            project.to_string().as_bytes(),
        )
        .unwrap();
        let original = saved_project(&history);
        let old = history.acquire_snapshot(0).unwrap();
        if placement_layer == "a" {
            assert_eq!(
                cut(&history, "a", 10).unwrap_err(),
                DispatchErrorCode::Unavailable
            );
            assert_eq!(
                (
                    saved_project(&history),
                    history.content_revision(),
                    history.history_depths()
                ),
                (original.clone(), 0, (0, 0))
            );
            let noop = cut(&history, "a", 2).unwrap();
            assert!(!noop.changed());
            assert_eq!(history.commit_layer_playhead_cut(noop).unwrap(), 0);
            assert_eq!(saved_project(&history), original);
            // Build REAL redo through an allowed unrelated layer, then prove
            // source refusal and outside no-op cannot erase that history.
            let unrelated = cut(&history, "before", 10).unwrap();
            assert!(unrelated.changed());
            history.commit_layer_playhead_cut(unrelated).unwrap();
            assert_eq!(history.history_depths(), (1, 0));
            history.undo(1).unwrap();
            assert_eq!(saved_project(&history), original);
            assert_eq!(
                cut(&history, "a", 10).unwrap_err(),
                DispatchErrorCode::Unavailable
            );
            let noop = cut(&history, "a", 2).unwrap();
            assert!(!noop.changed());
            assert_eq!(history.commit_layer_playhead_cut(noop).unwrap(), 2);
            assert_eq!(
                (saved_project(&history), history.history_depths()),
                (original.clone(), (0, 1))
            );
            assert_eq!(
                history
                    .serialize_project_file(0, old.snapshot_id())
                    .unwrap(),
                original
            );
            assert_eq!(history.redo(2).unwrap(), 3);
        } else {
            let candidate = cut(&history, "a", 10).unwrap();
            assert!(candidate.changed());
            assert_eq!(candidate.created(), Some("native-layer:1:0"));
            assert_eq!(history.commit_layer_playhead_cut(candidate).unwrap(), 1);
            assert_eq!(history.history_depths(), (1, 0));
            let after = saved_project(&history);
            let before_value: Value = serde_json::from_slice(&original).unwrap();
            let after_value: Value = serde_json::from_slice(&after).unwrap();
            for key in ["project", "assets", "assetInventory", "viewportReference"] {
                assert_eq!(
                    serde_json::to_vec(&after_value[key]).unwrap(),
                    serde_json::to_vec(&before_value[key]).unwrap(),
                    "unchanged {key}"
                );
            }
            assert_eq!(
                after_value["drawing"]["layers"].as_array().unwrap().len(),
                4
            );
            assert_eq!(
                after_value["drawing"]["layers"][2]["layerUid"],
                "native-layer:1:0"
            );
            assert_eq!(
                after_value["assetInventory"]["placements"]
                    .as_array()
                    .unwrap()
                    .len(),
                1,
                "unrelated placement remains on its original layer, never fabricated on the clone"
            );
            assert_eq!(
                after_value["assetInventory"]["placements"][0]["target"]["layerUid"],
                "before"
            );
            assert_eq!(
                history
                    .serialize_project_file(0, old.snapshot_id())
                    .unwrap(),
                original
            );
            let reopened =
                NativeObjectHistory::from_project_file("reopened-cut-video", &after).unwrap();
            assert_eq!(saved_project(&reopened), after);
            history.undo(1).unwrap();
            assert_eq!(saved_project(&history), original);
            history.redo(2).unwrap();
            assert_eq!(saved_project(&history), after);
        }
    }
}
