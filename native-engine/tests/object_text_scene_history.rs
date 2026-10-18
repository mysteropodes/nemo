//! Real Text/font transactions and storage keep the exact scene/resource owner.
use super::*;
use crate::request_receipts::OpacityRequest;
const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
#[cfg(feature = "font_asset_wire")]
#[test]
fn admitted_document_font_edit_undo_redo_and_reopen_keep_real_ink_and_versions() {
    let mut o = owner(&drawing());
    let initial = support::head(&o);
    let initial_plan = plan(&initial, 0);
    let initial_witness =
        prepare_object_composition(&initial_plan, &selector(&initial, 0)).unwrap();
    let request = OpacityRequest::history_stage(
        "scene-font-import",
        o.instance_id(),
        o.document_id(),
        Some(o.content_revision()),
        "command.document.object.asset.font.import",
        json!({"command":"object.asset.font.import",
            "documentSnapshotId":initial.snapshot_id(),"lifecycleGeneration":1,"source":{"kind":"bundled","fontKey":"Roboto-Regular"}}),
    );
    let result = o.dispatch_font_import(&request, FONT).unwrap();
    assert!(result.ok, "{:?}", result.error);
    let asset = result.result.unwrap()["assetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let imported_revision = o.content_revision();
    support::run(
        &mut o,
        &json!({"command":"object.text.edit","stableTarget":support::target("L0","text"),
        "patch":{"font":{"kind":"asset","assetId":asset},"text":"AA"}}),
    )
    .unwrap();
    let edited = support::head(&o);
    let edited_plan = plan(&edited, 7);
    assert_eq!(
        labels(&edited_plan),
        ["vector", "scene-root:L0:text", "image"]
    );
    let edited_witness = prepare_object_composition(&edited_plan, &selector(&edited, 7)).unwrap();
    let id = format!("font:{asset}");
    let resource = edited_witness.scene_resources().unwrap().get(&id).unwrap();
    assert_eq!(resource.charged_bytes(), FONT.len() as u64);
    assert_eq!(
        resource.version(),
        initial_witness
            .scene_resources()
            .unwrap()
            .get("font:Roboto-Regular")
            .unwrap()
            .version()
    );
    let revision = o.content_revision();
    o.undo(revision).unwrap();
    let undone = support::head(&o);
    let p = plan(&undone, 0);
    assert!(p
        .scene_resource_versions()
        .unwrap()
        .iter()
        .any(|(id, _)| id == "font:Roboto-Regular"));
    let revision = o.content_revision();
    o.redo(revision).unwrap();
    assert!(o.content_revision() > imported_revision);
    let redone = support::head(&o);
    let redone_plan = plan(&redone, 7);
    let stored = o
        .serialize_project_file(o.content_revision(), redone.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("scene-font-reopen", &stored).unwrap();
    let new_snapshot = support::head(&reopened);
    let new_plan = plan(&new_snapshot, 7);
    assert_eq!(labels(&new_plan), labels(&redone_plan));
    assert_eq!(
        new_plan.scene_resource_versions().unwrap(),
        redone_plan.scene_resource_versions().unwrap()
    );
    let glyphs = |p: &ObjectPaintPlan| {
        ordered_entries(p)
            .filter_map(|e| {
                if let PaintEntry::Text(t) = e {
                    Some(t.item.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(glyphs(&new_plan), glyphs(&redone_plan));
    assert_eq!(
        new_snapshot
            .document()
            .item_records()
            .unwrap()
            .text_entries()[0]
            .target,
        initial.document().item_records().unwrap().text_entries()[0].target
    );
    // Older plans still retain the old exact native snapshot and original font.
    initial_witness
        .verify_scene_resources(
            &initial_plan
                .scene_resource_versions()
                .unwrap()
                .into_iter()
                .collect(),
        )
        .unwrap();
    assert_ne!(
        initial_witness.document_snapshot_id(),
        new_snapshot.snapshot_id()
    );
}
#[test]
fn linked_font_never_uses_bundled_substitute_and_preserves_owner_bytes() {
    let mut doc = drawing();
    doc["items"]["text"][0]["block"]["font"] = json!({"kind":"asset","assetId":"font-1"});
    let o = NativeObjectHistory::from_admitted_project(
        "linked-text",
        support::project(&doc, support::inventory()),
    )
    .unwrap();
    let before = support::state(&o);
    let snap = support::head(&o);
    let pins = selector(&snap, 0);
    let packet = prepare_object_exposure_frame(&snap, &pins).unwrap();
    assert!(matches!(
        prepare_object_render_scene(&packet, &pins),
        Err(ObjectSceneError::UnsupportedGeometry {
            source_object_index: 2,
            ..
        })
    ));
    assert_eq!(support::state(&o), before);
}
#[test]
fn duplicate_blocks_share_one_font_charge_and_explicit_empty_key_does_not_reuse_old_text() {
    let mut doc = drawing();
    let mut next = doc["items"]["text"][0].clone();
    next["target"]["strokeId"] = json!("second-text");
    next["block"]["anchorTopLeft"]["x"] = json!(100);
    doc["items"]["text"].as_array_mut().unwrap().push(next);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L0","strokeId":"second-text","naturalPosition":3,"order":{}}));
    doc["layers"][0]["timing"]["keys"]
        .as_array_mut()
        .unwrap()
        .push(json!({"frame":4}));
    let o = owner(&doc);
    let snap = support::head(&o);
    let p = plan(&snap, 3);
    assert_eq!(p.packet().text_records().len(), 2);
    assert_eq!(
        p.scene_resource_versions()
            .unwrap()
            .iter()
            .filter(|(id, _)| id.starts_with("font:"))
            .count(),
        1
    );
    let w = prepare_object_composition(&p, &selector(&snap, 3)).unwrap();
    assert_eq!(
        w.scene_resources()
            .unwrap()
            .get("font:Roboto-Regular")
            .unwrap()
            .charged_bytes(),
        FONT.len() as u64
    );
    let empty = plan(&snap, 4);
    assert_eq!(empty.packet().text_records().len(), 0);
    assert_eq!(ordered_entries(&empty).count(), 0);
    assert!(empty.scene_resource_versions().unwrap().is_empty());
}
