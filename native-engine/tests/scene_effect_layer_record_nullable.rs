//! Nullable authored modes use the existing codec, immutable owner and panel history.
//! No panel command authors null; imported null remains distinct until an actual panel edit replaces it.
use super::scene_effect_layer_record_edit::{
    apply, bytes, dispatch, fixture, head, layer_json, owner, payload, prepare, request, state,
    EDIT,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn nullable_source(version: u32) -> Value {
    let mut source = fixture(version);
    source["layers"][1]["blendMode"] = Value::Null;
    source["layers"][1]["matteMode"] = Value::Null;
    source["layers"][2]["blendMode"] = json!("normal");
    source["layers"][2]["matteMode"] = json!("none");
    source["layers"][3]["blendMode"] = json!("multiply");
    source["layers"][3]["matteMode"] = json!("alpha");
    source
}

#[test]
fn absent_null_default_and_named_modes_roundtrip_both_storage_versions() {
    for version in [1, 2] {
        let source = nullable_source(version);
        let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
        let encoded = encode_storage_project(&document).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), source);
        assert_eq!(decode_storage_project(&encoded).unwrap(), document);
        let layers = document.layers();
        assert!(layers[0].blend_mode.missing() && layers[0].matte_mode.missing());
        assert_eq!(layers[1].blend_mode.value(), Some(&None));
        assert_eq!(layers[1].matte_mode.value(), Some(&None));
        assert_eq!(
            (layers[1].blend_mode(), layers[1].matte_mode()),
            (None, None)
        );
        assert!(!layers[1].matte_on());
        assert_eq!(layers[2].blend_mode(), Some("normal"));
        assert_eq!(layers[2].matte_mode(), Some("none"));
        assert!(!layers[2].matte_on());
        assert_eq!(layers[3].blend_mode(), Some("multiply"));
        assert_eq!(layers[3].matte_mode(), Some("alpha"));
        assert!(layers[3].matte_on());
    }
}

#[test]
fn pinned_public_serialization_reads_keep_null_members_and_original_omissions() {
    let source = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":1,"layers":[{"layerUid":"a"},
            {"layerUid":"b","blendMode":null,"matteMode":null},
            {"layerUid":"c","blendMode":"normal","matteMode":"none"}],"objects":[]});
    let o = owner("nullable-read", &source);
    let snapshot = head(&o);
    let mut read = request(
        &o,
        "read-null",
        "query.document.object.serialize",
        json!({"atRevision":0}),
    );
    read.expected_revision = None;
    let response = snapshot.dispatch(read).unwrap();
    assert!(response.ok);
    let result = response.result.unwrap();
    assert_eq!(result["documentSnapshotId"], snapshot.snapshot_id());
    let stored: Value = serde_json::from_str(result["documentJson"].as_str().unwrap()).unwrap();
    assert_eq!(stored, source);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn null_modes_survive_unrelated_edits_and_original_panel_history_restores_their_presence() {
    for version in [1, 2] {
        let source = nullable_source(version);
        let mut o = owner("nullable-history", &source);
        let original = head(&o);
        let original_bytes = bytes(&o);
        apply(&mut o, "b", "effects", json!({"effects":[]}));
        let untouched = layer_json(&o, "b");
        assert_eq!(untouched.get("blendMode"), Some(&Value::Null));
        assert_eq!(untouched.get("matteMode"), Some(&Value::Null));
        let after_effect = bytes(&o);
        let no_op = prepare(&o, "b", "effects", json!({"effects":[]}));
        assert!(!no_op.changed());
        assert_eq!(o.commit_layer_compositing(no_op), Ok(1));
        assert_eq!(o.history_depths(), (1, 0));
        // Original Normal clears a member, including stored null; selecting a matte source defaults alpha.
        apply(&mut o, "b", "blend", json!({"mode":"normal"}));
        assert!(layer_json(&o, "b").get("blendMode").is_none());
        assert_eq!(layer_json(&o, "b").get("matteMode"), Some(&Value::Null));
        apply(&mut o, "b", "matteSource", json!({"source":"c"}));
        assert_eq!(layer_json(&o, "b")["matteMode"], "alpha");
        assert_eq!(o.history_depths(), (3, 0));
        let changed = bytes(&o);
        for _ in 0..2 {
            o.undo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), after_effect);
        let before_noop = state(&o);
        let no_op = prepare(&o, "b", "effects", json!({"effects":[]}));
        assert!(!no_op.changed());
        o.commit_layer_compositing(no_op).unwrap();
        assert_eq!(state(&o), before_noop, "null-preserving no-op retains redo");
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), original_bytes);
        for _ in 0..3 {
            o.redo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), changed);
        assert_eq!(serde_json::to_value(original.document()).unwrap(), source);
    }
}

#[test]
fn nullable_modes_do_not_weaken_the_existing_owner_or_staged_admission() {
    let source = nullable_source(2);
    let mut o = owner("nullable-seal", &source);
    let foreign = prepare(&o, "b", "blend", json!({"mode":"screen"}));
    let mut other = owner("foreign", &source);
    let before_other = state(&other);
    assert_eq!(
        other.commit_layer_compositing(foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(state(&other), before_other);
    let stale = prepare(&o, "b", "blend", json!({"mode":"screen"}));
    apply(&mut o, "b", "effects", json!({"effects":[]}));
    let before = state(&o);
    assert_eq!(
        o.commit_layer_compositing(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), before);
    let mut cancelled = request(
        &o,
        "cancel-null",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(state(&o), before);
    // Stored null admission is not a new nullable panel command or a waiver for other JSON types.
    let invalid = request(
        &o,
        "null-command",
        EDIT,
        payload("b", "blend", json!({"mode":null})),
    );
    assert_eq!(
        dispatch(&mut o, &invalid)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
    for key in ["blendMode", "matteMode"] {
        for bad in [json!(false), json!(4), json!([]), json!({})] {
            let mut invalid = source.clone();
            invalid["layers"][1][key] = bad;
            assert!(decode_storage_project(invalid.to_string().as_bytes()).is_err());
        }
    }
}

#[cfg(feature = "project_media_codec")]
#[test]
fn nullable_modes_survive_real_project_save_reopen_and_old_snapshot_pins() {
    use crate::history::NativeObjectHistory;
    use crate::object_document::{ProjectEnvelope, ProjectMetadata};
    let source = nullable_source(2);
    let drawing = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.0).unwrap(), drawing);
    let mut o = NativeObjectHistory::from_admitted_project("nullable-project", project).unwrap();
    let original = head(&o);
    let saved = o.serialize_project_file(0, original.snapshot_id()).unwrap();
    let reopened = NativeObjectHistory::from_project_file("nullable-open", &saved).unwrap();
    assert_eq!(
        serde_json::to_value(head(&reopened).document()).unwrap(),
        source
    );
    assert_eq!(
        reopened
            .serialize_project_file(0, head(&reopened).snapshot_id())
            .unwrap(),
        saved
    );
    apply(&mut o, "b", "blend", json!({"mode":"screen"}));
    let current = head(&o);
    let changed = o.serialize_project_file(1, current.snapshot_id()).unwrap();
    let changed_open =
        NativeObjectHistory::from_project_file("nullable-changed", &changed).unwrap();
    assert_eq!(layer_json(&changed_open, "b")["blendMode"], "screen");
    assert_eq!(
        layer_json(&changed_open, "b").get("matteMode"),
        Some(&Value::Null)
    );
    assert_eq!(
        o.serialize_project_file(0, original.snapshot_id()).unwrap(),
        saved
    );
    assert_eq!(serde_json::to_value(original.document()).unwrap(), source);
    o.undo(1).unwrap();
    assert_eq!(
        bytes(&o),
        encode_storage_project(original.document()).unwrap()
    );
    o.redo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), bytes(&changed_open));
}
