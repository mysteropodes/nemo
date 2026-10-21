//! Actual flat-entry admission; no GPU/device/cache/target is constructed.
use crate::compositor::object_compositor::check_flat_masks;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectCompositionWitness,
};
use crate::revision::ObjectSnapshot;
use serde_json::json;

fn retained_witness(
    masked: bool,
) -> (
    NativeObjectHistory,
    ObjectSnapshot,
    ObjectCompositionWitness,
) {
    let mut value = paint_test_document(&[7]);
    if masked {
        for record in value["objects"].as_array_mut().unwrap() {
            if record["target"]["layerUid"] != "L0" {
                continue;
            }
            record["family"] = json!("editable-vector-path");
            record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
                "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":0});
            record["vectorExt"] = json!({"isMask":true,"maskMode":"subtract"});
        }
    }
    let owner = NativeObjectHistory::new(
        "mask-flat-admission",
        decode_storage_project(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 7,
    };
    let packet = prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let witness = prepare_object_composition(&plan, &pins).unwrap();
    (owner, snapshot, witness)
}

#[test]
fn actual_flat_entry_refuses_retained_masks_even_when_witness_has_no_joined_effect_plan() {
    let (owner, snapshot, witness) = retained_witness(true);
    let before = encode_storage_project(snapshot.document()).unwrap();
    assert!(
        witness.scene_effects().is_none(),
        "exercise the actual production fallback gap"
    );
    assert_eq!(
        witness.plan().mask_inputs().layer(1).unwrap().masks().len(),
        3
    );
    assert_eq!(
        witness.plan().commands().len(),
        2,
        "mask seeds were filtered, not forgotten"
    );
    let error = check_flat_masks(&witness).unwrap_err();
    assert_eq!(
        error.to_string(),
        "native vector masks require joined layer composition"
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn actual_zero_mask_witness_keeps_the_existing_flat_admission() {
    let (owner, snapshot, witness) = retained_witness(false);
    let before = encode_storage_project(snapshot.document()).unwrap();
    assert!(witness.scene_effects().is_none());
    assert!(witness.plan().mask_inputs().layers().is_empty());
    assert_eq!(witness.plan().commands().len(), 5);
    assert!(check_flat_masks(&witness).is_ok());
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}
