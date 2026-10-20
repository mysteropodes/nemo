//! Actual PNG owner -> exposure packet -> pinned scene -> charged resource controls.
//! CPU tests authored UNRUN; the explicit ignored Metal test also requires the shared GPU slot.
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    object_raster_resource_versions, ordered_entries, prepare_object_composition,
    prepare_object_paint_plan, prepare_object_render_scene, prepare_object_viewport_composition,
    ObjectCompositionWitness, ObjectPaintPlan, PaintEntry,
};
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

fn scene_owner() -> NativeObjectHistory {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut vector = cases["records"][0].clone();
    vector["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"drawing","strokeId":"red-square"});
    vector["geometry"]["segments"] =
        json!([[24,24],[40,24],[40,40],[24,40]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    vector["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let bytes = serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
            "layers":[{"layerUid":"drawing","timing":timing}],"objects":[vector],
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"drawing","naturalPosition":0,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"drawing","strokeId":"red-square","naturalPosition":0,"order":{}}]}]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}}})).unwrap();
    NativeObjectHistory::from_project_file("image-scene", &bytes).unwrap()
}
fn import(owner: &mut NativeObjectHistory, id: &str) -> Value {
    let input = asset_request(
        owner,
        id,
        "attach",
        json!({
        "documentSnapshotId":current_snapshot_id(owner),"lifecycleGeneration":1,"name":"still.png","mediaType":"image/png",
        "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}}),
    );
    let reply = owner
        .dispatch_image_import(&input, &oracle_bytes("blue-2x1"))
        .unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    serde_json::to_value(reply).unwrap()["result"].clone()
}
fn scene_snapshot(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn scene_plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_exposure_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
fn viewport(
    owner: &NativeObjectHistory,
    frame: u32,
    prior: Option<&ObjectCompositionWitness>,
) -> ObjectCompositionWitness {
    let snapshot = scene_snapshot(owner);
    let plan = scene_plan(&snapshot, frame);
    prepare_object_viewport_composition(&plan, &selector(&snapshot, frame), prior).unwrap()
}

#[test]
fn real_import_joins_vector_and_raster_in_one_pinned_order_and_keeps_contour_api() {
    let mut owner = scene_owner();
    import(&mut owner, "scene-first");
    let snapshot = scene_snapshot(&owner);
    for frame in [0, 7, 20] {
        let plan = scene_plan(&snapshot, frame);
        assert_eq!(
            plan.commands().len(),
            1,
            "the original contour API is unchanged"
        );
        let sequence: Vec<_> = ordered_entries(&plan)
            .map(|entry| match entry {
                PaintEntry::Contour(command) => {
                    format!("vector:{}", command.source().record().target().stroke_id())
                }
                PaintEntry::Raster(index) => format!("raster:{index}"),
                PaintEntry::Text(_) => panic!("PNG-only fixture gained unexpected Text"),
                PaintEntry::Duplicator(_) => {
                    panic!("original fixture gained unexpected Duplicator")
                }
                PaintEntry::Component(_) => panic!("PNG-only fixture gained unexpected Component"),
            })
            .collect();
        assert_eq!(sequence, ["vector:red-square", "raster:0"]);
        assert_eq!(plan.packet().raster_records().len(), 1);
        let mut wrong = selector(&snapshot, frame);
        wrong.document_snapshot_id = "foreign";
        assert!(prepare_object_composition(&plan, &wrong).is_err());
    }
}

#[test]
fn real_pixels_charge_once_reuse_across_held_frames_and_duplicate_placements_and_retire_on_undo() {
    let mut owner = scene_owner();
    let first = import(&mut owner, "resource-first");
    let witness = viewport(&owner, 0, None);
    let resources = witness.scene_resources().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources.total_charged_bytes(), 8);
    assert_eq!(
        resources.entries()[0].resource_id(),
        format!("raster-asset:{}", first["assetId"].as_str().unwrap())
    );
    let image = witness.raster_images().next().unwrap();
    let image_id = image.data.id();
    assert_eq!(image.data.as_ref(), &[0, 0, 255, 255, 255, 255, 255, 255]);
    let (retained, _) = image.data.clone().into_raw_parts();
    let weak = std::sync::Arc::downgrade(&retained);
    let held = viewport(&owner, 20, Some(&witness));
    assert_eq!(held.raster_images().next().unwrap().data.id(), image_id);
    let second = import(&mut owner, "resource-duplicate");
    assert_eq!(first["assetId"], second["assetId"]);
    let duplicate = viewport(&owner, 7, Some(&held));
    assert_eq!(duplicate.raster_images().count(), 1);
    assert_eq!(duplicate.plan().packet().raster_records().len(), 2);
    assert_eq!(
        duplicate.raster_images().next().unwrap().data.id(),
        image_id
    );
    assert_eq!(
        duplicate.scene_resources().unwrap().total_charged_bytes(),
        8
    );
    let snapshot = scene_snapshot(&owner);
    let plan = scene_plan(&snapshot, 7);
    let declared = object_raster_resource_versions(&plan)
        .unwrap()
        .into_iter()
        .collect();
    assert!(duplicate.verify_scene_resources(&declared).is_ok());
    assert!(duplicate
        .verify_scene_resources(&std::collections::BTreeMap::new())
        .is_err());
    let mut skew = declared.clone();
    *skew.values_mut().next().unwrap() = "other-version".into();
    assert!(duplicate.verify_scene_resources(&skew).is_err());
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    let empty = viewport(&owner, 0, Some(&duplicate));
    assert!(!empty.has_scene_images());
    assert!(!duplicate.reference_matches_plan(empty.plan()));
    assert!(empty.scene_resources().is_none());
    drop(witness);
    drop(held);
    drop(duplicate);
    assert!(
        weak.upgrade().is_some(),
        "the renderer Blob alone retains its actual decoded charge"
    );
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn native_save_reload_and_history_preserve_the_actual_scene_and_resource_version() {
    let mut owner = scene_owner();
    import(&mut owner, "saved");
    let original = viewport(&owner, 20, None);
    let versions = original.scene_resources().unwrap().declared_versions();
    let saved = saved_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
    let restored = viewport(&reopened, 20, None);
    assert_eq!(
        restored.scene_resources().unwrap().declared_versions(),
        versions
    );
    assert_eq!(
        restored.raster_images().next().unwrap().data.as_ref(),
        original.raster_images().next().unwrap().data.as_ref()
    );
    owner.undo(owner.content_revision()).unwrap();
    assert!(!viewport(&owner, 20, Some(&original)).has_scene_images());
    owner.redo(owner.content_revision()).unwrap();
    let redone = viewport(&owner, 20, Some(&original));
    assert_eq!(
        redone.raster_images().next().unwrap().data.id(),
        original.raster_images().next().unwrap().data.id()
    );
}

#[test]
#[ignore = "requires the explicitly coordinated real Metal/GPU validation slot"]
fn gpu_actual_import_raster_and_vector_compose_through_the_scheduled_production_path() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let mut owner = scene_owner();
    let result = import(&mut owner, "gpu-png");
    let edit=serde_json::to_vec(&json!({"command":"object.raster.resize","stableTarget":result["target"],"width":64,"height":32})).unwrap();
    let prepared = owner.prepare_item_edit_json(&edit).unwrap();
    owner.commit_item_edit(prepared).unwrap();
    let snapshot = scene_snapshot(&owner);
    let plan = scene_plan(&snapshot, 0);
    let witness = prepare_object_composition(&plan, &selector(&snapshot, 0)).unwrap();
    let output = OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap();
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        0,
        "final",
        output,
        object_raster_resource_versions(&plan).unwrap(),
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: 0,
        scope: ObjectScheduleScope::Authored,
    };
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(snapshot, pins, key).unwrap();
    let live =
        prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness).unwrap();
    let mut gpu = Compositor::new().expect("real Metal required");
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let pixels = gpu
        .readback_scheduled_object_rgba8(&result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec();
    let at = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
    assert_eq!(at(8, 32), [0, 0, 255, 255]);
    assert_eq!(at(56, 32), [255, 255, 255, 255]);
    assert_ne!(
        at(28, 32),
        [255, 0, 0, 255],
        "the later raster paints over the red vector in the SAME native order"
    );
}

#[test]
fn unsupported_raster_brush_range_and_context_are_refused_by_the_actual_production_stages() {
    let mut owner = scene_owner();
    import(&mut owner, "unsupported");
    for kind in ["brush", "range"] {
        let mut stored: Value = serde_json::from_slice(&saved_bytes(&owner)).unwrap();
        if kind == "brush" {
            stored["drawing"]["items"]["raster"][0]["item"]["isBitmapBrush"] = json!(true);
        } else {
            stored["drawing"]["layers"][1]["range"] = json!({"inPoint":3,"outPoint":7});
        }
        let reopened = NativeObjectHistory::from_project_file(
            "unsupported",
            &serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
        let snapshot = scene_snapshot(&reopened);
        let pins = selector(&snapshot, 0);
        let packet = prepare_object_exposure_frame(&snapshot, &pins);
        if kind == "brush" {
            assert!(matches!(
                prepare_object_render_scene(&packet.unwrap(), &pins),
                Err(crate::object_render_scene::ObjectSceneError::UnsupportedGeometry { .. })
            ));
        } else {
            // D27 admits range selection for rasters through the same packet as vectors.
            assert_eq!(packet.unwrap().source_frame_for_layer(1), None);
            let before = saved_bytes(&reopened);
            let history = reopened.history_depths();
            for (frame, visible) in [(2, false), (3, true), (7, true), (8, false), (20, false)] {
                let pins = selector(&snapshot, frame);
                let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
                assert_eq!(packet.frame(), frame);
                assert_eq!(packet.source_frame_for_layer(0), Some(0));
                assert_eq!(packet.source_frame_for_layer(1), visible.then_some(0));
                assert_eq!(packet.layer_opacity(1), Ok(1.0));
                assert_eq!(packet.raster_records().len(), usize::from(visible));
                let scene = prepare_object_render_scene(&packet, &pins).unwrap();
                let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
                let entries: Vec<_> = ordered_entries(&plan)
                    .map(|entry| match entry {
                        PaintEntry::Contour(_) => "vector",
                        PaintEntry::Raster(_) => "raster",
                        PaintEntry::Text(_) => panic!("PNG-only fixture gained unexpected Text"),
                        PaintEntry::Duplicator(_) => {
                            panic!("original fixture gained unexpected Duplicator")
                        }
                        PaintEntry::Component(_) => {
                            panic!("PNG-only fixture gained unexpected Component")
                        }
                    })
                    .collect();
                let expected = if visible {
                    vec!["vector", "raster"]
                } else {
                    vec!["vector"]
                };
                assert_eq!(entries, expected);
                assert_eq!(saved_bytes(&reopened), before);
                assert_eq!(reopened.history_depths(), history);
            }
        }
    }
    let snapshot = scene_snapshot(&owner);
    let mut pins = selector(&snapshot, 0);
    pins.context_id = "unsupported-context";
    assert!(prepare_object_exposure_frame(&snapshot, &pins).is_err());
    pins.context_id = "scene-root";
    pins.scope_kind = FrameScopeKind::Reference;
    assert!(prepare_object_exposure_frame(&snapshot, &pins).is_err());
}
