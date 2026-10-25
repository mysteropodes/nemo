//! Original two-geometry selection closure, complete identities and authored order.
use super::object_linked_fill_codec::linked_document;
use super::object_selection_edit_tests::{
    apply, bytes, current, delete, duplicate, frozen, owner_of, record_at, selection, split,
    strokes_at, value,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::object_group_kernels::{
    object_selection_compound as compound,
    object_selection_compound_clones::apply_compound_clones,
    object_selection_edit_clones::RevisionIds,
    object_selection_edit_frames::{EditMode, UnavailablePromotion},
    object_selection_edit_plan::{plan_delete, SelectionRequest},
};
use crate::object_document::{BitmapBrushFields, ObjectDocument};
use serde_json::{json, Value};
fn image(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&encode_storage_project(document).unwrap()).unwrap()
}
fn orders(document: &Value, stroke: &str) -> Value {
    document["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["strokeId"] == stroke)
        .unwrap()["order"]
        .clone()
}
fn routed(animated: bool) -> Value {
    let mut source = linked_document(2);
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"] =
        json!({"static":[-17]});
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][1]["order"] = if animated {
        json!({"track":{"keys":[{"frame":0,"v":[23]},{"frame":5,"v":[-23]}]}})
    } else {
        json!({"static":[23]})
    };
    source
}
#[test]
fn linked_copy_reads_actual_both_paths_without_mutation_or_independent_companion_selection() {
    for animated in [false, true] {
        let source = routed(animated);
        let owner = owner_of(&source);
        let before = frozen(&owner);
        let selected = compound::resolve(
            current(&owner).document(),
            "scene-root",
            "L",
            0,
            &["ribbon".into()],
        )
        .unwrap();
        assert_eq!(selected.anchors.len(), 1);
        assert_eq!(selected.records.len(), 2);
        assert_eq!(selected.records[0].target.stroke_id, "backdrop");
        assert_ne!(selected.records[0].geometry, selected.records[1].geometry);
        let payload = selection("object.clipboard.copy", "L", 0, "motion", &["ribbon"]);
        let clip = owner
            .copy_selection_json(payload.to_string().as_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(clip.records, selected.records);
        assert_eq!(frozen(&owner), before);
        assert!(compound::resolve(
            current(&owner).document(),
            "scene-root",
            "L",
            0,
            &["backdrop".into()]
        )
        .is_err());
        assert_eq!(value(&owner), source);
    }
}
#[test]
fn linked_delete_closes_pair_retains_unselected_order_and_has_exact_undo_redo() {
    for animated in [false, true] {
        let source = routed(animated);
        let mut owner = owner_of(&source);
        let before = bytes(&owner);
        apply(&mut owner, "delete", &delete("L", 0, "motion", &["ribbon"])).unwrap();
        let after = bytes(&owner);
        let result = value(&owner);
        assert_eq!(strokes_at(&result, "L", 0), ["other"]);
        assert_eq!(record_at(&result, "L", 0, "other"), source["objects"][2]);
        assert_eq!(orders(&result, "other"), orders(&source, "other"));
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
    }
}
#[test]
fn duplicate_uses_fresh_pair_id_own_geometry_and_default_pair_order_preserving_sources() {
    for animated in [false, true] {
        let source = routed(animated);
        let mut owner = owner_of(&source);
        let before = bytes(&owner);
        apply(
            &mut owner,
            "duplicate",
            &duplicate("L", 0, "motion", &["ribbon"]),
        )
        .unwrap();
        let after = value(&owner);
        let records = after["objects"].as_array().unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(&records[..4], source["objects"].as_array().unwrap());
        assert_eq!(orders(&after, "backdrop"), orders(&source, "backdrop"));
        assert_eq!(orders(&after, "ribbon"), orders(&source, "ribbon"));
        let backdrop = &records[4];
        let primary = &records[5];
        assert_eq!(backdrop["vectorExt"]["isLinkedFillCompanion"], true);
        assert_eq!(primary["vectorExt"]["isLinkedFillCompanion"], false);
        assert_ne!(primary["vectorExt"]["linkedFillId"], json!("link-original"));
        assert_eq!(
            primary["vectorExt"]["linkedFillId"],
            backdrop["vectorExt"]["linkedFillId"]
        );
        assert_eq!(
            backdrop["geometry"]["segments"][0]["point"]["x"],
            json!(7.0)
        );
        assert_eq!(
            primary["geometry"]["segments"][0]["point"]["x"],
            json!(12.0)
        );
        assert_eq!(
            primary["vectorExt"]["pressure"]["centerSegments"][0]["point"]["y"],
            json!(13.0)
        );
        for record in [backdrop, primary] {
            assert_eq!(
                orders(&after, record["target"]["strokeId"].as_str().unwrap()),
                json!({})
            );
        }
        let after = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
    }
}
#[test]
fn full_companion_group_membership_is_recreated_with_both_fresh_stroke_ids() {
    let mut source = linked_document(2);
    source["layers"][0]["groups"] = json!({"contextId":"scene-root","entries":[{"gid":"pair-group","order":["backdrop","ribbon"]}],
        "members":[{"strokeId":"backdrop","groupId":"pair-group"},{"strokeId":"ribbon","groupId":"pair-group"}]});
    let mut owner = owner_of(&source);
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L", 0, "motion", &["ribbon"]),
    )
    .unwrap();
    let after = value(&owner);
    let groups = &after["layers"][0]["groups"];
    assert_eq!(groups["entries"].as_array().unwrap().len(), 2);
    let fresh = &groups["entries"][1];
    assert_ne!(fresh["gid"], json!("pair-group"));
    let fresh_ids: Vec<_> = after["objects"].as_array().unwrap()[4..]
        .iter()
        .map(|record| record["target"]["strokeId"].clone())
        .collect();
    assert_eq!(fresh["order"], json!(fresh_ids));
    for id in fresh_ids {
        assert!(groups["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["strokeId"] == id && member["groupId"] == fresh["gid"]));
    }
    assert_eq!(
        groups["entries"][0],
        source["layers"][0]["groups"]["entries"][0]
    );
}
#[test]
fn split_counts_only_anchors_keeps_both_geometry_and_each_authored_order() {
    for animated in [false, true] {
        let source = routed(animated);
        let mut owner = owner_of(&source);
        let before = bytes(&owner);
        apply(&mut owner, "split", &split("L", 0)).unwrap();
        let after = value(&owner);
        assert_eq!(after["layers"].as_array().unwrap().len(), 3);
        let backdrop = after["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["target"]["strokeId"] == "backdrop")
            .unwrap();
        let primary = after["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["target"]["strokeId"] == "ribbon")
            .unwrap();
        assert_eq!(
            backdrop["target"]["layerUid"],
            primary["target"]["layerUid"]
        );
        assert_ne!(primary["target"]["layerUid"], json!("L"));
        assert_eq!(backdrop["geometry"], source["objects"][0]["geometry"]);
        assert_eq!(primary["geometry"], source["objects"][1]["geometry"]);
        assert_eq!(orders(&after, "backdrop"), orders(&source, "backdrop"));
        assert_eq!(orders(&after, "ribbon"), orders(&source, "ribbon"));
        let after = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
    }
}
#[test]
fn bitmap_companion_uses_same_resource_and_translates_existing_rectangle_with_anchor() {
    let source = BitmapBrushFields::test_drawing();
    let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let selected = compound::resolve(&document, "scene-root", "L", 0, &["anchor".into()]).unwrap();
    let mut next = document.clone();
    let mut ids = RevisionIds::new(1, &document);
    let result = apply_compound_clones(
        &mut next,
        0,
        "scene-root",
        0,
        &selected.records,
        &selected.rasters,
        &[None],
        None,
        12.,
        &mut ids,
    )
    .unwrap();
    assert_eq!(result.created.len(), 1);
    let after = image(&next);
    let original = &source["items"]["raster"][0]["item"];
    let raster = &after["items"]["raster"][1]["item"];
    assert_eq!(raster["assetId"], original["assetId"]);
    assert_eq!(raster["width"], original["width"]);
    assert_eq!(raster["height"], original["height"]);
    assert_eq!(raster["x"], json!(28.0));
    assert_eq!(raster["y"], json!(28.0));
    assert_eq!(
        raster["brushGroupId"],
        after["objects"][1]["vectorExt"]["bitmapBrush"]["brushGroupId"]
    );
    assert_ne!(raster["brushGroupId"], original["brushGroupId"]);
    let selected_ids = vec!["anchor".into()];
    let request = SelectionRequest {
        context_id: "scene-root",
        layer_uid: "L",
        frame: 0,
        mode: EditMode::Motion,
        stroke_ids: &selected_ids,
    };
    let removed = plan_delete(&document, &request, &UnavailablePromotion).unwrap();
    assert!(removed.document.objects.is_empty());
    assert!(compound::raster_entries(&removed.document).is_empty());
    assert_eq!(image(&document), source);
    let mut malformed = document.clone();
    if let crate::object_document::Presence::Present(items) = &mut malformed.items {
        items.raster = crate::object_document::Presence::Missing;
    }
    assert!(compound::resolve(&malformed, "scene-root", "L", 0, &selected_ids).is_err());
}
#[test]
fn exhausted_identity_provider_cannot_partially_materialize_a_compound() {
    use crate::object_document::object_group_kernels::object_clipboard_remap::{IdKind, IdSource};
    struct Once(bool);
    impl IdSource for Once {
        fn fresh(&mut self, _: IdKind, _: Option<&str>) -> Option<String> {
            if self.0 {
                self.0 = false;
                Some("one-fresh-id".into())
            } else {
                None
            }
        }
    }
    let source = linked_document(2);
    let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let selected = compound::resolve(&document, "scene-root", "L", 0, &["ribbon".into()]).unwrap();
    let mut next = document.clone();
    assert!(apply_compound_clones(
        &mut next,
        0,
        "scene-root",
        0,
        &selected.records,
        &[],
        &[None, None],
        None,
        12.,
        &mut Once(true)
    )
    .is_err());
    assert_eq!(next, document);
}
#[test]
fn bitmap_split_moves_actual_raster_with_anchor_and_preserves_resource_and_order() {
    use super::object_selection_edit_tests::shape;
    use crate::object_document::object_group_kernels::object_selection_edit_split::{
        plan_split, SplitLabels, SplitRequest,
    };
    let mut source = BitmapBrushFields::test_drawing();
    source["objects"]
        .as_array_mut()
        .unwrap()
        .push(shape("L", "other", 0, 60));
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L","strokeId":"other","naturalPosition":2,"order":{}}));
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][1]["order"] =
        json!({"static":[-8]});
    let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let result = plan_split(
        &document,
        &SplitRequest {
            context_id: "scene-root",
            layer_uid: "L",
            frame: 0,
            revision: 1,
            labels: SplitLabels {
                shape: "Shape",
                stroke: "Stroke",
            },
        },
    )
    .unwrap();
    let after = image(&result.document);
    assert_eq!(
        after["objects"][0]["target"]["layerUid"],
        after["items"]["raster"][0]["target"]["layerUid"]
    );
    assert_ne!(after["objects"][0]["target"]["layerUid"], json!("L"));
    assert_eq!(
        after["items"]["raster"][0]["item"],
        source["items"]["raster"][0]["item"]
    );
    assert_eq!(orders(&after, "companion"), orders(&source, "companion"));
    assert_eq!(image(&document), source);
}
#[test]
fn duplicate_and_detached_paste_remap_both_linked_trace_walls_after_companion_allocation() {
    for paste in [false, true] {
        let mut source = linked_document(2);
        source["objects"][0]["vectorExt"]["fillTrace"] =
            json!({"seed":[1,1],"walls":["ribbon","other"],"gapPx":0});
        source["objects"][1]["vectorExt"]["fillTrace"] =
            json!({"seed":[2,2],"walls":["backdrop","other"],"gapPx":0});
        let mut owner = owner_of(&source);
        let before = bytes(&owner);
        if paste {
            let payload = selection("object.clipboard.copy", "L", 0, "motion", &["ribbon"]);
            let clip = owner
                .copy_selection_json(payload.to_string().as_bytes())
                .unwrap()
                .unwrap();
            let detached: Value =
                serde_json::from_slice(&serde_json::to_vec(&clip).unwrap()).unwrap();
            let payload = json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"B","frame":0,"mode":"motion","clip":detached});
            let candidate = owner
                .prepare_clipboard_paste_json(payload.to_string().as_bytes())
                .unwrap();
            owner.commit_selection_edit(candidate).unwrap();
        } else {
            apply(
                &mut owner,
                "duplicate",
                &duplicate("L", 0, "motion", &["ribbon"]),
            )
            .unwrap();
        }
        let after = value(&owner);
        let records = after["objects"].as_array().unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(
            &records[..4],
            source["objects"].as_array().unwrap(),
            "original walls and unselected wall unchanged"
        );
        let backdrop = &records[4];
        let primary = &records[5];
        let backdrop_id = &backdrop["target"]["strokeId"];
        let primary_id = &primary["target"]["strokeId"];
        assert_ne!(backdrop_id, &json!("backdrop"));
        assert_ne!(primary_id, &json!("ribbon"));
        assert_eq!(
            backdrop["vectorExt"]["fillTrace"]["walls"],
            json!([primary_id, "other"])
        );
        assert_eq!(
            primary["vectorExt"]["fillTrace"]["walls"],
            json!([backdrop_id, "other"])
        );
        assert_eq!(
            backdrop["target"]["layerUid"],
            json!(if paste { "B" } else { "L" })
        );
        assert_eq!(owner.history_depths(), (1, 0));
        let after = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
    }
}

#[cfg(feature = "project_media_codec")]
#[test]
fn bitmap_duplicate_and_detached_paste_remap_trace_wall_to_fresh_raster_with_exact_history() {
    use super::object_selection_edit_tests::shape;
    use crate::history::NativeObjectHistory;
    for paste in [false, true] {
        let mut drawing = BitmapBrushFields::test_drawing();
        drawing["objects"][0]["vectorExt"]["fillTrace"] =
            json!({"seed":[8,8],"walls":["companion","wall"],"gapPx":0});
        drawing["objects"]
            .as_array_mut()
            .unwrap()
            .push(shape("L", "wall", 0, 60));
        drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"L","strokeId":"wall","naturalPosition":2,"order":{}}));
        let project = json!({"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":48,"canvasH":48,"fps":24},
            "drawing":drawing,"viewportReference":null,"assets":[{"id":"ink","name":"Ink","kind":"image","source":{"kind":"embedded","mediaType":"image/png","encoding":"base64","data":crate::object_codec::encoded_wire_test::PNG}}],
            "assetInventory":{"assetIds":["ink"],"placements":[]}});
        let mut owner = NativeObjectHistory::from_project_file(
            "compound-wall-remap",
            project.to_string().as_bytes(),
        )
        .unwrap();
        let saved = |owner: &NativeObjectHistory| {
            owner
                .acquire_snapshot(owner.content_revision())
                .unwrap()
                .serialize_project_file()
                .unwrap()
        };
        let before = saved(&owner);
        if paste {
            let payload = selection("object.clipboard.copy", "L", 0, "motion", &["anchor"]);
            let clip = owner
                .copy_selection_json(payload.to_string().as_bytes())
                .unwrap()
                .unwrap();
            let detached: Value =
                serde_json::from_slice(&serde_json::to_vec(&clip).unwrap()).unwrap();
            let payload = json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L","frame":0,"mode":"motion","clip":detached});
            let candidate = owner
                .prepare_clipboard_paste_json(payload.to_string().as_bytes())
                .unwrap();
            owner.commit_selection_edit(candidate).unwrap();
        } else {
            apply(
                &mut owner,
                "duplicate",
                &duplicate("L", 0, "motion", &["anchor"]),
            )
            .unwrap();
        }
        let after = saved(&owner);
        let result: Value = serde_json::from_slice(&after).unwrap();
        let result = &result["drawing"];
        assert_eq!(
            &result["objects"].as_array().unwrap()[..2],
            drawing["objects"].as_array().unwrap()
        );
        assert_eq!(result["items"]["raster"][0], drawing["items"]["raster"][0]);
        let raster_id = &result["items"]["raster"][1]["target"]["strokeId"];
        assert_ne!(raster_id, &json!("companion"));
        assert_eq!(
            result["objects"][2]["vectorExt"]["fillTrace"]["walls"],
            json!([raster_id, "wall"])
        );
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(saved(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(saved(&owner), after);
    }
}
