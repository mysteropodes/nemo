//! Original project annotations, distinct from app-local user identity/preferences.
use super::*;
use serde_json::{json, Value};
fn metadata() -> ProjectMetadata {
    ProjectMetadata::new(320, 180, 24.0).unwrap()
}

#[test]
fn workspace_preserves_missing_null_empty_and_false_without_ui_defaults() {
    let raw = json!({"layerPanelWidth":null,"toolsPanelWidth":"","propsPanelWidth":"280px",
        "propsPanelCollapsed":false,"timelineHeight":null,"panelOrder":[],"motionColumns":"[]"});
    let workspace: ProjectWorkspaceRecord = serde_json::from_value(raw.clone()).unwrap();
    let value = metadata().with_annotations(Some(workspace), None).unwrap();
    let (workspace, comments) = value.annotations();
    assert!(comments.is_none());
    assert_eq!(serde_json::to_value(workspace.unwrap()).unwrap(), raw);
    let empty: ProjectWorkspaceRecord = serde_json::from_value(json!({})).unwrap();
    assert_eq!(serde_json::to_value(empty).unwrap(), json!({}));
    assert_eq!(metadata().annotations(), (None, None));
}

#[test]
fn comment_records_preserve_optional_author_attribution_long_text_and_empty_values() {
    let raw = json!({"id":"c_1","x":-0.5,"y":3.25,"frame":-2.5,"authorId":"author","authorName":"",
        "authorColor":"#AbC","text":"注釈\n".repeat(1025),"createdAt":1234.5,"resolved":false});
    let comment: ProjectCommentRecord = serde_json::from_value(raw.clone()).unwrap();
    let value = metadata()
        .with_annotations(None, Some(vec![comment]))
        .unwrap();
    let comments = value.annotations().1.unwrap();
    assert_eq!(serde_json::to_value(&comments[0]).unwrap(), raw);
    let partial: ProjectCommentRecord = serde_json::from_value(json!({"text":""})).unwrap();
    assert_eq!(serde_json::to_value(partial).unwrap(), json!({"text":""}));
}

#[test]
fn nonfinite_comment_coordinates_refuse_and_other_metadata_allocations_survive() {
    let before = metadata()
        .with_composition_metadata(Some(vec![(3.5, None, None)]), None, None, None)
        .unwrap();
    let ptr = before.composition_markers().unwrap().as_ptr();
    let value = before
        .clone()
        .with_annotations(Some(ProjectWorkspaceRecord::default()), Some(vec![]))
        .unwrap();
    assert_eq!(value.composition_markers().unwrap().as_ptr(), ptr);
    assert!(value.annotations().1.unwrap().is_empty());
    assert_eq!(before.annotations(), (None, None));
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in ["x", "y", "frame", "createdAt"] {
            let mut comment = ProjectCommentRecord::default();
            match field {
                "x" => comment.x = Presence::Present(number),
                "y" => comment.y = Presence::Present(number),
                "frame" => comment.frame = Presence::Present(number),
                _ => comment.created_at = Presence::Present(number),
            }
            assert_eq!(
                before.clone().with_annotations(None, Some(vec![comment])),
                Err(ProjectMetadataError::InvalidAnnotations)
            );
        }
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
mod wire {
    use super::*;
    use crate::history::NativeObjectHistory;
    fn input(version: u32, fields: Value) -> Vec<u8> {
        let mut project = json!({"canvasW":320,"canvasH":180,"fps":24});
        project
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut data = json!({"format":"nemo.native-project-document","formatVersion":version,"project":project,
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
            "viewportReference":null,"assets":[]});
        if version == 2 {
            data["assetInventory"] = json!({"assetIds":[],"placements":[]});
        }
        data.to_string().into_bytes()
    }
    fn save(owner: &NativeObjectHistory) -> Vec<u8> {
        let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
        owner
            .serialize_project_file(pin.content_revision(), pin.snapshot_id())
            .unwrap()
    }
    fn fields(owner: &NativeObjectHistory) -> Value {
        serde_json::from_slice::<Value>(&save(owner)).unwrap()["project"].clone()
    }

    #[test]
    fn workspace_and_attributed_comments_both_versions_survive_real_history_save_fresh_reopen() {
        let expected = json!({"workspace":{"layerPanelWidth":null,"propsPanelCollapsed":false,"panelOrder":["transform","effects"],"motionColumns":"[]"},
            "comments":[{"id":"c_1","x":-4.5,"y":2.25,"frame":3.0,"authorId":"one","authorName":"Writer","authorColor":"#AbC",
                "text":"Comment\nwith unicode 日本語","createdAt":1000.0,"resolved":false},{"text":""}]});
        for version in [1, 2] {
            let mut owner = NativeObjectHistory::from_project_file(
                "annotations",
                &input(version, expected.clone()),
            )
            .unwrap();
            let before = save(&owner);
            let actual = fields(&owner);
            for name in ["workspace", "comments"] {
                assert_eq!(actual.get(name), expected.get(name));
            }
            assert!(actual.get("userProfile").is_none());
            let edit=owner.prepare_composition_delta_json(br#"{"command":"object.project.composition.edit","action":{"kind":"addMarker","frame":3}}"#).unwrap();
            owner.commit_project_settings(edit).unwrap();
            let after = save(&owner);
            let changed = fields(&owner);
            for name in ["workspace", "comments"] {
                assert_eq!(changed.get(name), expected.get(name));
            }
            let reopened =
                NativeObjectHistory::from_project_file("annotations-fresh", &after).unwrap();
            assert_eq!(save(&reopened), after);
            owner.undo(1).unwrap();
            assert_eq!(save(&owner), before);
            owner.redo(2).unwrap();
            assert_eq!(save(&owner), after);
        }
    }

    #[test]
    fn original_capture_without_props_panel_preserves_null_panel_order() {
        // src/js/ui.js::_secOrderNow returns null when #props-panel is absent.
        // SMWorkspace.capture includes that value; exportJSON saves the workspace.
        let captured = json!({"layerPanelWidth":null,"toolsPanelWidth":null,
            "propsPanelWidth":null,"propsPanelCollapsed":false,"timelineHeight":null,
            "panelOrder":null,"motionColumns":null});
        for version in [1, 2] {
            let owner = NativeObjectHistory::from_project_file(
                "nullable-panel-order", &input(version, json!({"workspace":captured.clone()}))
            ).unwrap();
            assert_eq!(fields(&owner)["workspace"], captured);
            let saved = save(&owner);
            let reopened = NativeObjectHistory::from_project_file("nullable-panel-fresh", &saved).unwrap();
            assert_eq!(save(&reopened), saved);
            assert_eq!(fields(&reopened)["workspace"]["panelOrder"], Value::Null);
            assert_eq!(owner.history_depths(), (0, 0));
        }
    }

    #[test]
    fn absent_and_explicit_empty_annotations_are_distinct_and_never_authored_by_reads() {
        for expected in [
            json!({}),
            json!({"workspace":{},"comments":[]}),
            json!({"workspace":{"layerPanelWidth":null,"motionColumns":null},"comments":[{}]}),
        ] {
            let owner = NativeObjectHistory::from_project_file(
                "annotation-presence",
                &input(1, expected.clone()),
            )
            .unwrap();
            let first = save(&owner);
            let actual = fields(&owner);
            for name in ["workspace", "comments"] {
                assert_eq!(actual.get(name), expected.get(name));
            }
            assert_eq!(save(&owner), first);
            assert_eq!(owner.history_depths(), (0, 0));
        }
    }

    #[test]
    fn annotations_are_closed_object_records_and_never_import_local_profile_data() {
        for value in [
            json!({"workspace":[]}),
            json!({"workspace":{"propsPanelCollapsed":null}}),
            json!({"workspace":{"panelOrder":false}}),
            json!({"workspace":{"unknown":true}}),
            json!({"comments":null}),
            json!({"comments":[[]]}),
            json!({"comments":[{"authorName":null}]}),
            json!({"comments":[{"createdAt":"now"}]}),
            json!({"comments":[{"resolved":0}]}),
            json!({"comments":[{"userProfile":{"name":"local"}}]}),
            json!({"userProfile":{"name":"local"}}),
        ] {
            assert!(
                NativeObjectHistory::from_project_file("annotation-invalid", &input(1, value))
                    .is_err()
            );
        }
        let raw =
            String::from_utf8(input(1, json!({"comments":[{"authorName":"Writer"}]}))).unwrap();
        let duplicate = raw.replace(
            "\"authorName\":\"Writer\"",
            "\"authorName\":\"Writer\",\"authorName\":\"Other\"",
        );
        assert_ne!(raw, duplicate);
        assert!(NativeObjectHistory::from_project_file(
            "annotation-duplicate",
            duplicate.as_bytes()
        )
        .is_err());
    }
}
