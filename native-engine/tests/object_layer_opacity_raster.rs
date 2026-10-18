//! Real placed PNG -> raw layer factor -> shared viewport/export resource witness. Authored UNRUN.
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
fn opacity_changes_only_paint_and_reuses_the_same_retained_pixels_across_frames_and_reopen() {
    let mut owner = scene_owner();
    let imported = import(&mut owner, "opacity-image");
    let snap = scene_snapshot(&owner);
    let layer = snap.document().layers()[1].layer_uid().to_owned();
    let before = viewport(&owner, 7, None);
    assert_eq!(before.raster_image(0).unwrap().2, 1.0);
    let bytes=json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[25]}}).to_string();
    let candidate = owner.prepare_property_edit_json(bytes.as_bytes()).unwrap();
    assert_eq!(owner.commit_property_edit(candidate).unwrap(), 2);
    let current = viewport(&owner, 20, Some(&before));
    assert_eq!(current.raster_image(0).unwrap().2, 0.25);
    assert_eq!(before.raster_image(0).unwrap().2, 1.0);
    assert_eq!(
        current.raster_image(0).unwrap().0.data.id(),
        before.raster_image(0).unwrap().0.data.id()
    );
    assert_eq!(current.scene_resources().unwrap().total_charged_bytes(), 8);
    assert_eq!(current.scene_resources().unwrap().len(), 1);
    assert_eq!(current.plan().packet().source_frame_for_layer(1), Some(0));
    assert_eq!(current.plan().packet().frame(), 20);
    assert_eq!(
        current.raster_image(0).unwrap().0.data.as_ref(),
        &[0, 0, 255, 255, 255, 255, 255, 255]
    );
    assert_eq!(
        current.plan().commands().next().unwrap().paint()[3].as_f64(),
        Some(1.0),
        "other layer unchanged"
    );
    let plan = scene_plan(&scene_snapshot(&owner), 20);
    let exported =
        prepare_object_composition(&plan, &selector(plan.packet().snapshot(), 20)).unwrap();
    assert_eq!(exported.raster_image(0).unwrap().2, 0.25);
    let versions = current.plan().raster_resource_versions().unwrap();
    assert_eq!(versions, before.plan().raster_resource_versions().unwrap());
    let saved = saved_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("opacity-reopened", &saved).unwrap();
    let reopened_view = viewport(&reopened, 20, None);
    assert_eq!(reopened_view.raster_image(0).unwrap().2, 0.25);
    assert_eq!(
        reopened_view.plan().raster_resource_versions().unwrap(),
        versions
    );
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(owner.undo(2).unwrap(), 3);
    assert_eq!(
        viewport(&owner, 20, Some(&current))
            .raster_image(0)
            .unwrap()
            .2,
        1.0
    );
    assert_eq!(owner.redo(3).unwrap(), 4);
    assert_eq!(
        viewport(&owner, 20, Some(&before))
            .raster_image(0)
            .unwrap()
            .2,
        0.25
    );
    assert_eq!(current.raster_image(0).unwrap().2, 0.25);
    assert_eq!(imported["assetId"], "asset-opacity-image");
}
