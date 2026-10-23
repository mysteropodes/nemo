//! Literal organization/storage oracles, exercising both real native codecs.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

fn fixture(version: u32, painted: bool) -> Value {
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":21,"layers":[
            {"layerUid":"L0","shy":false,"folderId":"g-one","locked":false,"color":"#5FA875","parentLayerUid":null},
            {"layerUid":"L1","shy":true,"folderId":"g-one"}],
        "layerFolders":{"g-one":{"name":"同じ / Folder","collapsed":false},"unused":{}},"objects":[]});
    if painted {
        value["objects"] = json!([{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":2,"y":1}},
                {"point":{"x":8,"y":4},"handleIn":{"x":-1,"y":-1},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}}]);
    }
    if version == 2 {
        let elements = if painted {
            json!([{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}])
        } else {
            json!([])
        };
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":1,"order":{}},
                {"layerUid":"L1","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]});
    }
    value
}
fn admitted(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn encoded(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&encode_storage_project(document).unwrap()).unwrap()
}

#[test]
fn literal_v1_v2_preserve_folder_entries_membership_and_shy_without_reordering() {
    for version in [1, 2] {
        let source = fixture(version, false);
        let document = admitted(&source);
        assert_eq!(encoded(&document), source);
        assert_eq!(document.layers()[0].layer_uid(), "L0");
        assert!(!document.layers()[0].shy());
        assert!(document.layers()[1].shy());
        assert_eq!(document.layers()[0].folder_id(), Some("g-one"));
        let folder = document.layer_folders().unwrap().get("g-one").unwrap();
        assert_eq!(folder.name(), Some("同じ / Folder"));
        assert!(!folder.collapsed());
        assert_eq!(
            document
                .layer_folders()
                .unwrap()
                .get("unused")
                .unwrap()
                .name(),
            None
        );
        assert_eq!(admitted(&encoded(&document)), document);
    }
}

#[test]
fn absent_null_empty_and_false_are_not_materialized_as_authored_defaults() {
    for version in [1, 2] {
        for table in [None, Some(Value::Null), Some(json!({}))] {
            for membership in [None, Some(Value::Null), Some(json!(""))] {
                let mut source = fixture(version, false);
                let object = source.as_object_mut().unwrap();
                object.remove("layerFolders");
                if let Some(value) = table.clone() {
                    object.insert("layerFolders".into(), value);
                }
                for layer in source["layers"].as_array_mut().unwrap() {
                    layer.as_object_mut().unwrap().remove("folderId");
                    layer.as_object_mut().unwrap().remove("shy");
                    if let Some(value) = membership.clone() {
                        layer["folderId"] = value;
                    }
                }
                let document = admitted(&source);
                assert_eq!(encoded(&document), source);
                assert_eq!(document.layers()[0].folder_id(), None);
                assert!(!document.layers()[0].shy());
            }
        }
    }
}

#[test]
fn folder_ids_have_their_own_namespace_and_need_not_reorder_or_form_a_tree() {
    let mut source = fixture(1, false);
    source["layerFolders"] = json!({"L0":{"name":"Same as a layer UID","collapsed":true}});
    source["layers"][0]["folderId"] = json!("L0");
    source["layers"][1]["folderId"] = json!("L0");
    let document = admitted(&source);
    assert_eq!(encoded(&document), source);
    assert!(document
        .layer_folders()
        .unwrap()
        .get("L0")
        .unwrap()
        .collapsed());
}

#[test]
fn unresolved_membership_invalid_shape_null_boolean_and_unknown_metadata_refuse() {
    for version in [1, 2] {
        let source = fixture(version, false);
        let mut bad = vec![];
        for value in [json!("missing"), json!(true), json!([])] {
            let mut v = source.clone();
            v["layers"][0]["folderId"] = value;
            bad.push(v);
        }
        for value in [Value::Null, json!(1), json!("false")] {
            let mut v = source.clone();
            v["layers"][0]["shy"] = value;
            bad.push(v);
        }
        for value in [
            json!([]),
            json!({"":{"name":"empty ID"}}),
            json!({"g-one":{"collapsed":null}}),
            json!({"g-one":{"name":1}}),
            json!({"g-one":{"parentFolderId":"g-one"}}),
        ] {
            let mut v = source.clone();
            v["layerFolders"] = value;
            bad.push(v);
        }
        for value in bad {
            assert!(
                decode_storage_project(value.to_string().as_bytes()).is_err(),
                "{value}"
            );
        }
        // Ordered v2 admits a valid static link; legacy and invalid graphs stay refused.
        let mut parented = source.clone();
        parented["layers"][0]["parentLayerUid"] = json!("L1");
        if version == 2 {
            assert_eq!(encoded(&admitted(&parented)), parented);
        } else {
            assert!(decode_storage_project(parented.to_string().as_bytes()).is_err());
        }
        for (key, value) in [
            ("parentLayerUid", json!("L0")),
            ("parentLayerUid", json!("absent")),
            ("solo", json!(false)),
            ("linkGroupId", json!("linked")),
        ] {
            let mut v = source.clone();
            v["layers"][0][key] = value;
            assert!(decode_storage_project(v.to_string().as_bytes()).is_err());
        }
        assert_eq!(encoded(&admitted(&source)), source);
    }
}

#[test]
fn original_bytes_reject_duplicate_map_ids_metadata_and_layer_members() {
    for raw in [
        r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[],"layerFolders":{"g":{},"g":{}}}"#,
        r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[],"layerFolders":{"g":{"collapsed":true,"collapsed":false}}}"#,
        r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L","folderId":null,"folderId":null}],"objects":[]}"#,
        r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L","shy":false,"shy":true}],"objects":[]}"#,
    ] {
        assert!(decode_storage_project(raw.as_bytes()).is_err());
    }
}

#[cfg(feature = "history")]
#[test]
fn existing_owner_fill_undo_redo_and_old_snapshot_preserve_the_organization_exactly() {
    use crate::history::NativeObjectHistory;
    for version in [1, 2] {
        let source = fixture(version, true);
        let mut owner = NativeObjectHistory::new("organization-owner", admitted(&source)).unwrap();
        let zero = owner.acquire_snapshot(0).unwrap();
        let raw = json!({"command":"object.fill.set","stableTarget":source["objects"][0]["target"],
            "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}})
        .to_string();
        let candidate = owner.prepare_fill_json(raw.as_bytes()).unwrap();
        assert_eq!(owner.commit_fill(candidate), Ok(1));
        assert_eq!(owner.history_depths(), (1, 0));
        let mut expected = source.clone();
        expected["objects"][0]["fill"] = json!({"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1});
        let one = owner.acquire_snapshot(1).unwrap();
        assert_eq!(encoded(one.document()), expected);
        assert_eq!(encoded(zero.document()), source);
        assert_eq!(owner.undo(1), Ok(2));
        assert_eq!(
            encoded(owner.acquire_snapshot(2).unwrap().document()),
            source
        );
        assert_eq!(owner.redo(2), Ok(3));
        let latest = owner.acquire_snapshot(3).unwrap();
        assert_eq!(encoded(latest.document()), expected);
        assert_eq!(admitted(&encoded(latest.document())), *latest.document());
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn exact_pinned_full_project_save_and_reopen_keep_folders_shy_and_existing_metadata() {
    use crate::history::NativeObjectHistory;
    use crate::object_document::{ProjectEnvelope, ProjectMetadata};
    let source = fixture(2, true);
    let project = ProjectEnvelope::no_media(
        ProjectMetadata::new(320, 180, 24.).unwrap(),
        admitted(&source),
    );
    let owner =
        NativeObjectHistory::from_admitted_project("organization-project", project).unwrap();
    let pin = owner.acquire_snapshot(0).unwrap();
    let saved = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let reopened = NativeObjectHistory::from_project_file("organization-reopened", &saved).unwrap();
    let after = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(encoded(after.document()), source);
    assert_eq!(
        after.project().unwrap().project(),
        pin.project().unwrap().project()
    );
    assert_eq!(reopened.history_depths(), (0, 0));
    assert!(owner.serialize_project_file(0, "foreign-snapshot").is_err());
}

#[test]
fn explicit_v1_to_v2_migration_carries_organization_without_mutating_source() {
    use crate::object_codec::{encode_ordered_project, migrate_ordered_project};
    let old = fixture(1, true);
    let expected = fixture(2, true);
    let document = admitted(&old);
    let migrated = migrate_ordered_project(
        &document,
        expected["compositionOrder"].to_string().as_bytes(),
    )
    .unwrap();
    let actual: Value =
        serde_json::from_slice(&encode_ordered_project(&migrated).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(encoded(&document), old);
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn invalid_folder_admission_and_stale_pin_leave_live_project_and_history_unchanged() {
    use crate::history::NativeObjectHistory;
    use crate::object_document::{ProjectEnvelope, ProjectMetadata};
    let source = fixture(2, true);
    let project = ProjectEnvelope::no_media(
        ProjectMetadata::new(320, 180, 24.).unwrap(),
        admitted(&source),
    );
    let owner = NativeObjectHistory::from_admitted_project("organization-atomic", project).unwrap();
    let pin = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, pin.snapshot_id()).unwrap();
    let mut invalid: Value = serde_json::from_slice(&before).unwrap();
    invalid["drawing"]["layers"][0]["folderId"] = json!("not-in-table");
    assert!(owner
        .prepare_project_replacement(pin.snapshot_id(), invalid.to_string().as_bytes())
        .is_err());
    assert!(owner
        .prepare_project_replacement("stale-snapshot", &before)
        .is_err());
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    assert_eq!(
        owner.acquire_snapshot(0).unwrap().snapshot_id(),
        pin.snapshot_id()
    );
    assert_eq!(
        owner.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    assert_eq!(encoded(pin.document()), source);
}
