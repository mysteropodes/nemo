//! Negative and single-flaw controls of the image-mesh binding kernel (L03-R).
//!
//! - every refusal is typed, stable and leaves the state value-equal to the one it started from;
//! - a flaw-free twin of the replay harness reproduces the whole oracle, and each single flaw of the
//!   binding rules (tag only the current frame, release without the reference scan, forget the
//!   video field, forget the component layers, derive the counter from the store) is caught by
//!   at least one oracle step, so the oracle is not vacuous. The flawed variants are test-local
//!   models built on the production primitives; production is never mutated.
use super::rows::{initial_state, mesh_of, run_op, scenarios, snapshot, target_of};
use super::tests::{at, mesh, state_of, still};
use super::{
    item_with_mesh_id, BindingError, BindingLayer, BindingSlot, MeshBindingState, SlotTarget,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_binding_ops::VideoFamily;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::geometry::RasterItemError;
use crate::object_document::{ImageMesh, ImageMeshAdmissionError, ImageMeshStore};
use serde_json::{json, Value};

#[test]
fn every_refusal_is_typed_and_leaves_the_state_unchanged() {
    let path = BindingLayer {
        frames: vec![vec![BindingSlot::NonRaster]],
        ..BindingLayer::default()
    };
    let base = state_of(&["im_1"], vec![vec![still(3, None), path]]);
    let nul = "im_\u{0}1".to_string();
    let long = "x".repeat(256);
    let missing_layer = SlotTarget {
        set: 0,
        layer: 5,
        frame: 0,
        slot: 0,
    };
    let missing_set = SlotTarget {
        set: 3,
        layer: 0,
        frame: 0,
        slot: 0,
    };
    let table: Vec<(&str, BindingError, &str)> = vec![
        ("layer", BindingError::UnknownLayer, "binding-unknown-layer"),
        ("set", BindingError::UnknownLayer, "binding-unknown-layer"),
        ("frame", BindingError::UnknownSlot, "binding-unknown-slot"),
        ("slot", BindingError::UnknownSlot, "binding-unknown-slot"),
        (
            "path",
            BindingError::TargetNotRaster,
            "binding-target-not-raster",
        ),
        (
            "nul",
            BindingError::Item(RasterItemError::Tag),
            "binding-invalid-mesh-tag",
        ),
        (
            "long",
            BindingError::Item(RasterItemError::Tag),
            "binding-invalid-mesh-tag",
        ),
    ];
    for (case, expected, code) in table {
        let mut state = base.clone();
        let outcome = match case {
            "layer" => state.propagate(missing_layer, Some("im_1")).map(|_| ()),
            "set" => state.detach(missing_set).map(|_| ()),
            "frame" => state.attach(at(0, 9, 0), mesh()).map(|_| ()),
            "slot" => state.propagate(at(0, 0, 4), None).map(|_| ()),
            "path" => state.propagate(at(1, 0, 0), Some("im_1")).map(|_| ()),
            "nul" => state.propagate(at(0, 0, 0), Some(nul.as_str())).map(|_| ()),
            _ => state
                .propagate(at(0, 0, 0), Some(long.as_str()))
                .map(|_| ()),
        };
        assert_eq!(outcome, Err(expected), "{case}");
        assert_eq!(expected.code(), code, "{case}");
        assert_eq!(state, base, "{case}: state unchanged");
    }
}

#[test]
fn a_refused_store_or_counter_publishes_nothing() {
    let base = state_of(&["im_1"], vec![vec![still(4, None)]]);
    let bad: ImageMesh = serde_json::from_str(
        r#"{"outline":[[0,0],[1,0],[1,1]],"verts":[[0,0],[1,0],[1,1]],"tris":[0,1,9]}"#,
    )
    .unwrap();
    let mut state = base.clone();
    assert_eq!(
        state.attach(at(0, 0, 0), bad),
        Err(BindingError::Mesh(
            ImageMeshAdmissionError::TriangleIndexOutOfRange
        ))
    );
    assert_eq!(state, base);
    let mut exhausted = base.clone();
    exhausted.sequence.observe("im_9007199254740991").unwrap();
    assert_eq!(
        exhausted.attach(at(0, 0, 0), mesh()),
        Err(BindingError::Mesh(
            ImageMeshAdmissionError::IdSequenceExhausted
        ))
    );
    assert_eq!(
        exhausted.duplicate_mesh("im_1"),
        Err(BindingError::Mesh(
            ImageMeshAdmissionError::IdSequenceExhausted
        ))
    );
    assert!((0..4).all(|frame| exhausted.mesh_id_at(at(0, frame, 0)) == Ok(None)));
    assert_eq!(exhausted.store, base.store);
    assert_eq!(
        BindingError::Mesh(ImageMeshAdmissionError::IdSequenceExhausted).code(),
        "binding-mesh-store-refused"
    );
}

#[test]
fn the_refused_remap_and_video_calls_do_not_partially_apply() {
    let mut state = state_of(&["im_1"], vec![vec![still(2, Some("im_1"))]]);
    let before = state.clone();
    assert_eq!(
        state.remap_duplicated_layer(0, 7),
        Err(BindingError::UnknownLayer)
    );
    assert_eq!(
        state.remap_duplicated_layer(2, 0),
        Err(BindingError::UnknownLayer)
    );
    assert_eq!(
        state.attach_video(VideoFamily::Provided, 0, 7, mesh()),
        Err(BindingError::UnknownLayer)
    );
    assert_eq!(
        state.detach_video(VideoFamily::Provided, 4, 0),
        Err(BindingError::UnknownLayer)
    );
    state.sequence.observe("im_9007199254740991").unwrap();
    let stuck = state.clone();
    assert_eq!(
        state.remap_duplicated_layer(0, 0),
        Err(BindingError::Mesh(
            ImageMeshAdmissionError::IdSequenceExhausted
        ))
    );
    assert_eq!(state, stuck);
    assert_eq!(before.store, state.store);
}

// ---- single-flaw models ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flaw {
    None,
    CurrentFrameOnly,
    ReleaseWithoutScan,
    IgnoreVideoField,
    IgnoreComponentSets,
    CounterFromStore,
}

fn referenced(state: &MeshBindingState, id: &str, flaw: Flaw) -> bool {
    let sets = if flaw == Flaw::IgnoreComponentSets {
        1
    } else {
        state.sets.len()
    };
    state.sets.iter().take(sets).flatten().any(|layer| {
        (flaw != Flaw::IgnoreVideoField && layer.video_mesh_id.as_deref() == Some(id))
            || layer
                .frames
                .iter()
                .flatten()
                .any(|slot| matches!(slot, BindingSlot::Raster(item) if item.mesh_id() == Some(id)))
    })
}

fn write_tag(state: &mut MeshBindingState, target: SlotTarget, tag: Option<&str>, flaw: Flaw) {
    state.raster_at(target).expect("target is a raster");
    let frames = state.sets[target.set][target.layer].frames.len();
    for frame in 0..frames {
        if flaw == Flaw::CurrentFrameOnly && frame != target.frame {
            continue;
        }
        if let Some(BindingSlot::Raster(item)) =
            state.sets[target.set][target.layer].frames[frame].get(target.slot)
        {
            let item = item_with_mesh_id(item, tag).expect("tag");
            state.sets[target.set][target.layer].frames[frame][target.slot] =
                BindingSlot::Raster(item);
        }
    }
}

/// Frames of the target's layer that hold a raster at the target's position.
fn rasters_at(state: &MeshBindingState, target: SlotTarget) -> usize {
    state.sets[target.set][target.layer]
        .frames
        .iter()
        .filter(|frame| matches!(frame.get(target.slot), Some(BindingSlot::Raster(_))))
        .count()
}

fn release(state: &mut MeshBindingState, id: &str, flaw: Flaw) -> bool {
    if id.is_empty() || (flaw != Flaw::ReleaseWithoutScan && referenced(state, id, flaw)) {
        return false;
    }
    let entries: Vec<(String, ImageMesh)> = state
        .store
        .entries()
        .iter()
        .filter(|e| e.0 != id)
        .cloned()
        .collect();
    state.store = ImageMeshStore::try_new(entries).unwrap();
    true
}

fn issue(state: &mut MeshBindingState, mesh: ImageMesh, flaw: Flaw) -> String {
    if flaw == Flaw::CounterFromStore {
        state.sequence = state.store.id_sequence().unwrap();
    }
    state.insert_new_mesh(mesh).unwrap()
}

/// The oracle ops whose rules a flaw touches; every other op runs the production code.
fn flawed_op(state: &mut MeshBindingState, op: &Value, flaw: Flaw) -> Value {
    match op["op"].as_str().unwrap() {
        "propagate" => {
            let target = target_of(op);
            write_tag(state, target, op["mesh"].as_str(), flaw);
            let written = rasters_at(state, target);
            let n = if flaw == Flaw::CurrentFrameOnly {
                1
            } else {
                written
            };
            json!({"n": n})
        }
        "attach" => {
            let id = issue(state, mesh_of(op), flaw);
            write_tag(state, target_of(op), Some(&id), flaw);
            json!({"id": id})
        }
        "detach" => {
            let target = target_of(op);
            let id = state.mesh_id_at(target).unwrap().map(str::to_string);
            let cleared = rasters_at(state, target);
            write_tag(state, target, None, flaw);
            let released = id.as_deref().is_some_and(|id| release(state, id, flaw));
            let cleared = if flaw == Flaw::CurrentFrameOnly {
                1
            } else {
                cleared
            };
            json!({"id": id, "released": released, "cleared": cleared})
        }
        "release" => json!(release(state, op["mesh"].as_str().unwrap(), flaw)),
        "isReferenced" => json!(referenced(state, op["mesh"].as_str().unwrap(), flaw)),
        "duplicate" => {
            let id = op["mesh"].as_str().unwrap();
            match state.store.get(id).cloned() {
                Some(source) => json!(issue(state, source, flaw)),
                None => Value::Null,
            }
        }
        _ => run_op(state, op),
    }
}

/// Number of oracle steps (result or whole state) at which `flaw` differs from the baseline.
fn deviations(flaw: Flaw) -> usize {
    let mut count = 0;
    for scenario in scenarios() {
        let mut state = initial_state(&scenario);
        for (step, op) in scenario.ops.iter().enumerate() {
            let result = flawed_op(&mut state, op, flaw);
            if result != scenario.results[step] || snapshot(&state) != scenario.snaps[step + 1] {
                count += 1;
                break;
            }
        }
    }
    count
}

#[test]
fn the_flaw_free_twin_reproduces_the_whole_oracle() {
    assert_eq!(deviations(Flaw::None), 0);
}

#[test]
fn tagging_only_the_current_frame_is_caught() {
    assert!(deviations(Flaw::CurrentFrameOnly) >= 3);
}

#[test]
fn releasing_without_the_reference_scan_is_caught() {
    assert!(deviations(Flaw::ReleaseWithoutScan) >= 3);
}

#[test]
fn forgetting_the_video_field_is_caught() {
    assert!(deviations(Flaw::IgnoreVideoField) >= 1);
}

#[test]
fn forgetting_the_component_layers_is_caught() {
    assert!(deviations(Flaw::IgnoreComponentSets) >= 1);
}

#[test]
fn deriving_the_counter_from_the_store_is_caught() {
    assert!(deviations(Flaw::CounterFromStore) >= 1);
}

#[test]
fn the_oracle_rows_pin_the_baseline_quirks() {
    let all = scenarios();
    let result = |name: &str, step: usize| -> Value {
        all.iter().find(|s| s.name == name).unwrap().results[step].clone()
    };
    // 24 of 24 frames on the corpus mesh; a mixed layer writes fewer frames than it has
    assert_eq!(
        result("corpus-mesh-propagates-to-all-24-frames", 0),
        json!({"n": 24})
    );
    assert_eq!(
        result(
            "propagate-counts-frames-without-a-raster-at-the-position",
            0
        ),
        json!({"n": 5})
    );
    // a still-referenced mesh survives the detach; an untagged detach releases nothing
    assert_eq!(
        result("detach-keeps-a-mesh-another-layer-still-points-at", 0)["released"],
        json!(false)
    );
    assert_eq!(
        result("detach-without-a-live-tag-still-clears-the-other-frames", 0)["id"],
        Value::Null
    );
    // releaseIfUnused is true for an absent id, false for the empty one
    assert_eq!(result("release-if-unused-matrix", 0), json!(false));
    assert_eq!(result("release-if-unused-matrix", 2), json!(true));
    // the counter is never lowered; a missing duplicate source spends no id
    assert_eq!(
        result(
            "a-released-id-is-never-reissued-because-the-counter-is-never-lowered",
            3
        ),
        json!({"id": "im_4"})
    );
    assert_eq!(
        result(
            "duplicate-is-a-fresh-id-deep-copy-and-a-missing-source-spends-no-id",
            2
        ),
        json!("im_7")
    );
    // the remap shares ids and leaves the dangling one; the video id follows the same map
    let remap = result(
        "duplicate-layer-remaps-frames-and-video-under-fresh-shared-ids",
        0,
    );
    assert_eq!(remap["pairs"], json!([["im_1", "im_3"], ["im_2", "im_4"]]));
    assert_eq!(remap["dangling"], json!(["im_9"]));
}
