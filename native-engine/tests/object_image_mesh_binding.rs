//! Behavior of the image-mesh binding kernel with literal expectations (L03-R, F17/F18). The
//! step-by-step comparison with the baseline lives in `object_image_mesh_binding_oracle.rs`; the
//! single-flaw and refusal controls in `object_image_mesh_binding_controls.rs`.
use super::rows::{layer_from, raster};
use super::{BindingError, BindingLayer, BindingSlot, MeshBindingState, SlotTarget};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_binding_ops::VideoFamily;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{ImageMesh, ImageMeshStore};
use serde_json::json;

const MESH: &str = r#"{"outline":[[0,0],[1,0],[1,1],[0,1]],"verts":[[0,0],[1,0],[1,1],[0,1]],"tris":[0,1,2,0,2,3],"offsets":[[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}"#;

pub(crate) fn mesh() -> ImageMesh {
    serde_json::from_str(MESH).expect("mesh literal")
}

pub(crate) fn store_of(ids: &[&str]) -> ImageMeshStore {
    ImageMeshStore::try_new(ids.iter().map(|id| (id.to_string(), mesh())).collect()).expect("store")
}

/// `frames` frames, each holding one raster at position 0, tagged `tag`.
pub(crate) fn still(frames: i64, tag: Option<&str>) -> BindingLayer {
    BindingLayer {
        frames: (0..frames).map(|x| vec![raster(x, tag)]).collect(),
        ..BindingLayer::default()
    }
}

pub(crate) fn state_of(ids: &[&str], sets: Vec<Vec<BindingLayer>>) -> MeshBindingState {
    MeshBindingState::new(store_of(ids), sets).expect("state")
}

pub(crate) fn at(layer: usize, frame: usize, slot: usize) -> SlotTarget {
    SlotTarget {
        set: 0,
        layer,
        frame,
        slot,
    }
}

fn tag_of(state: &MeshBindingState, layer: usize, frame: usize, slot: usize) -> Option<String> {
    state
        .mesh_id_at(at(layer, frame, slot))
        .expect("raster")
        .map(str::to_string)
}

#[test]
fn propagate_tags_every_frame_not_just_the_one_being_looked_at() {
    let mut state = state_of(&["im_1"], vec![vec![still(24, None)]]);
    let done = state.propagate(at(0, 7, 0), Some("im_1")).unwrap();
    assert_eq!((done.written, done.skipped), (24, 0));
    for frame in 0..24 {
        assert_eq!(
            tag_of(&state, 0, frame, 0).as_deref(),
            Some("im_1"),
            "{frame}"
        );
    }
    let done = state.propagate(at(0, 23, 0), None).unwrap();
    assert_eq!((done.written, done.skipped), (24, 0));
    assert!((0..24).all(|frame| tag_of(&state, 0, frame, 0).is_none()));
}

#[test]
fn a_frame_without_a_raster_at_the_position_is_left_alone_and_counted() {
    let layer = layer_from(&json!({"nv": 0, "vm": null, "fr": [
        [{"k": "r", "m": null, "x": 0}], [{"k": "p"}], [], [{"k": "r", "m": "im_9", "x": 3}],
    ]}));
    let mut state = state_of(&["im_1", "im_9"], vec![vec![layer]]);
    let done = state.propagate(at(0, 0, 0), Some("im_1")).unwrap();
    assert_eq!((done.written, done.skipped), (2, 2));
    assert_eq!(state.sets[0][0].frames[1], vec![BindingSlot::NonRaster]);
    assert!(state.sets[0][0].frames[2].is_empty());
    assert_eq!(tag_of(&state, 0, 3, 0).as_deref(), Some("im_1"));
}

#[test]
fn propagate_changes_the_mesh_id_member_and_nothing_else() {
    const BARE: &str = r#"{"assetId":"a1","x":200,"y":150.5,"width":64,"height":32.25,"opacity":0.5,"rotation":30,"isBitmapBrush":true,"brushGroupId":"bg_1","isBrushTextureCopy":false}"#;
    const TAGGED: &str = r#"{"assetId":"a1","x":200,"y":150.5,"width":64,"height":32.25,"opacity":0.5,"rotation":30,"isBitmapBrush":true,"brushGroupId":"bg_1","isBrushTextureCopy":false,"meshId":"im_1"}"#;
    let item = |text: &str| BindingSlot::Raster(RasterItem::decode(text).unwrap());
    let layer = BindingLayer {
        frames: vec![vec![item(BARE)], vec![item(BARE)]],
        ..BindingLayer::default()
    };
    let mut state = state_of(&["im_1"], vec![vec![layer]]);
    state.propagate(at(0, 1, 0), Some("im_1")).unwrap();
    let encoded = |state: &MeshBindingState, frame: usize| match &state.sets[0][0].frames[frame][0]
    {
        BindingSlot::Raster(item) => item.encode().unwrap(),
        BindingSlot::NonRaster => String::new(),
    };
    assert_eq!(encoded(&state, 0), TAGGED);
    assert_eq!(encoded(&state, 1), TAGGED);
    state.propagate(at(0, 0, 0), None).unwrap();
    assert_eq!(encoded(&state, 0), BARE);
    assert_eq!(encoded(&state, 1), BARE);
}

#[test]
fn an_empty_id_clears_the_tag_like_the_falsy_baseline_meshid() {
    let mut state = state_of(&["im_1"], vec![vec![still(3, Some("im_1"))]]);
    let done = state.propagate(at(0, 0, 0), Some("")).unwrap();
    assert_eq!(done.written, 3);
    assert!((0..3).all(|frame| tag_of(&state, 0, frame, 0).is_none()));
}

#[test]
fn detach_clears_every_frame_then_releases_the_unreferenced_mesh() {
    let mut state = state_of(&["im_1", "im_2"], vec![vec![still(24, Some("im_1"))]]);
    let done = state.detach(at(0, 9, 0)).unwrap();
    assert_eq!(done.mesh_id.as_deref(), Some("im_1"));
    assert_eq!((done.propagate.written, done.released), (24, true));
    assert_eq!(state.store.entries().len(), 1);
    assert!(state.store.get("im_1").is_none() && state.store.get("im_2").is_some());
    assert!((0..24).all(|frame| tag_of(&state, 0, frame, 0).is_none()));
}

#[test]
fn a_second_reference_in_another_layer_a_component_or_a_video_keeps_the_mesh() {
    let video = BindingLayer {
        native_video: true,
        video_mesh_id: Some("im_1".to_string()),
        ..BindingLayer::default()
    };
    let cases: Vec<(&str, MeshBindingState)> = vec![
        (
            "other layer",
            state_of(
                &["im_1"],
                vec![vec![still(4, Some("im_1")), still(2, Some("im_1"))]],
            ),
        ),
        (
            "component set",
            state_of(
                &["im_1"],
                vec![vec![still(4, Some("im_1"))], vec![still(1, Some("im_1"))]],
            ),
        ),
        (
            "video field",
            state_of(&["im_1"], vec![vec![still(4, Some("im_1")), video]]),
        ),
    ];
    for (label, mut state) in cases {
        let done = state.detach(at(0, 0, 0)).unwrap();
        assert_eq!(
            (done.propagate.written, done.released),
            (4, false),
            "{label}"
        );
        assert!(state.store.get("im_1").is_some(), "{label}");
        assert!(state.is_referenced("im_1"), "{label}");
    }
}

#[test]
fn detaching_an_untagged_raster_still_clears_the_other_frames_and_releases_nothing() {
    let layer = layer_from(&json!({"nv": 0, "vm": null, "fr": [
        [{"k": "r", "m": null, "x": 0}], [{"k": "r", "m": "im_1", "x": 1}],
    ]}));
    let mut state = state_of(&["im_1"], vec![vec![layer]]);
    let done = state.detach(at(0, 0, 0)).unwrap();
    assert_eq!(
        (done.mesh_id, done.propagate.written, done.released),
        (None, 2, false)
    );
    assert!(state.store.get("im_1").is_some());
    assert!(!state.is_referenced("im_1"));
}

#[test]
fn attach_issues_the_next_id_stores_the_mesh_and_tags_every_frame() {
    let mut state = state_of(&["im_1", "im_4"], vec![vec![still(24, None)]]);
    let done = state.attach(at(0, 3, 0), mesh()).unwrap();
    assert_eq!(done.mesh_id, "im_5");
    assert_eq!((done.propagate.written, done.replaced), (24, None));
    assert_eq!(state.store.entries().len(), 3);
    assert_eq!(state.store.get("im_5"), Some(&mesh()));
    assert!((0..24).all(|frame| tag_of(&state, 0, frame, 0).as_deref() == Some("im_5")));
}

#[test]
fn scrubbing_resolves_one_mesh_id_on_every_frame_after_an_attach() {
    let mut state = state_of(&[], vec![vec![still(24, None)]]);
    let id = state.attach(at(0, 0, 0), mesh()).unwrap().mesh_id;
    let seen: Vec<Option<String>> = (0..24).map(|frame| tag_of(&state, 0, frame, 0)).collect();
    assert!(seen.iter().all(|tag| tag.as_deref() == Some(id.as_str())));
    assert!(state.dangling_mesh_ids().is_empty());
}

#[test]
fn attaching_over_an_existing_tag_reports_and_keeps_the_old_mesh() {
    let mut state = state_of(&["im_1"], vec![vec![still(5, Some("im_1"))]]);
    let done = state.attach(at(0, 0, 0), mesh()).unwrap();
    assert_eq!(
        (done.mesh_id.as_str(), done.replaced.as_deref()),
        ("im_2", Some("im_1"))
    );
    assert!(state.store.get("im_1").is_some());
    assert!(!state.is_referenced("im_1"));
    assert_eq!(state.release_if_unused("im_1"), Ok(true));
    assert!(state.store.get("im_1").is_none());
}

#[test]
fn the_counter_is_state_and_a_release_never_lowers_it() {
    let mut state = state_of(&[], vec![vec![still(2, None)]]);
    assert_eq!(state.attach(at(0, 0, 0), mesh()).unwrap().mesh_id, "im_1");
    state.detach(at(0, 0, 0)).unwrap();
    assert!(state.store.entries().is_empty());
    assert_eq!(state.sequence.last(), 1);
    // a counter re-derived from the (now empty) store would hand `im_1` out a second time
    assert_eq!(state.store.id_sequence().unwrap().last(), 0);
    assert_eq!(state.attach(at(0, 0, 0), mesh()).unwrap().mesh_id, "im_2");
}

#[test]
fn release_if_unused_answers_true_for_any_unreferenced_id_and_false_for_empty() {
    let mut state = state_of(&["im_1"], vec![vec![still(1, Some("im_1"))]]);
    assert_eq!(state.release_if_unused(""), Ok(false));
    assert_eq!(state.release_if_unused("im_1"), Ok(false));
    assert_eq!(state.release_if_unused("im_77"), Ok(true));
    assert_eq!(state.store.entries().len(), 1);
}

#[test]
fn duplicate_is_a_deep_copy_under_a_fresh_id_and_a_missing_source_spends_no_id() {
    let mut state = state_of(&["im_1"], vec![vec![still(1, None)]]);
    assert_eq!(state.duplicate_mesh("im_9"), Ok(None));
    assert_eq!(state.duplicate_mesh(""), Ok(None));
    assert_eq!(state.sequence.last(), 1);
    assert_eq!(state.duplicate_mesh("im_1"), Ok(Some("im_2".to_string())));
    assert_eq!(state.store.get("im_2"), state.store.get("im_1"));
    // the copy is an independent record: rebuilding the store with the first entry edited leaves it alone
    let edited: ImageMesh = serde_json::from_str(
        &MESH.replace("[[0,0],[0,0],[0,0],[0,0]]", "[[0.5,0],[0,0],[0,0],[0,0]]"),
    )
    .unwrap();
    let entries = vec![
        ("im_1".to_string(), edited.clone()),
        ("im_2".to_string(), state.store.get("im_2").unwrap().clone()),
    ];
    let next = ImageMeshStore::try_new(entries).unwrap();
    assert_eq!(next.get("im_1"), Some(&edited));
    assert_eq!(next.get("im_2"), Some(&mesh()));
}

#[test]
fn remapping_a_duplicated_layer_shares_one_fresh_id_per_old_id_and_leaves_dangling_ones() {
    let copy = layer_from(&json!({"nv": 1, "vm": "im_1", "fr": [
        [{"k": "r", "m": "im_1", "x": 0}, {"k": "p"}, {"k": "r", "m": "im_2", "x": 1}],
        [{"k": "r", "m": "im_1", "x": 2}], [{"k": "r", "m": "im_9", "x": 3}], [{"k": "r", "m": null, "x": 4}],
    ]}));
    let mut state = state_of(&["im_1", "im_2"], vec![vec![still(1, Some("im_1")), copy]]);
    let done = state.remap_duplicated_layer(0, 1).unwrap();
    let pairs: Vec<(&str, &str)> = done
        .pairs
        .iter()
        .map(|p| (p.0.as_str(), p.1.as_str()))
        .collect();
    assert_eq!(pairs, vec![("im_1", "im_3"), ("im_2", "im_4")]);
    assert_eq!(done.dangling, vec!["im_9".to_string()]);
    assert_eq!(state.sets[0][1].video_mesh_id.as_deref(), Some("im_3"));
    assert_eq!(tag_of(&state, 1, 1, 0).as_deref(), Some("im_3"));
    assert_eq!(tag_of(&state, 1, 2, 0).as_deref(), Some("im_9"));
    assert_eq!(tag_of(&state, 0, 0, 0).as_deref(), Some("im_1"));
    assert_eq!(state.dangling_mesh_ids(), vec!["im_9".to_string()]);
}

#[test]
fn dangling_ids_are_listed_once_in_scan_order() {
    let video = BindingLayer {
        native_video: true,
        video_mesh_id: Some("im_8".to_string()),
        ..BindingLayer::default()
    };
    let state = state_of(
        &["im_1"],
        vec![
            vec![still(2, Some("im_1")), video],
            vec![still(2, Some("im_9"))],
        ],
    );
    assert_eq!(
        state.dangling_mesh_ids(),
        vec!["im_8".to_string(), "im_9".to_string()]
    );
}

#[test]
fn video_mesh_operations_need_a_native_video_layer_and_the_provided_family() {
    let video = BindingLayer {
        native_video: true,
        ..BindingLayer::default()
    };
    let mut state = state_of(&[], vec![vec![video, still(1, None)]]);
    let unavailable = state.attach_video(VideoFamily::Unavailable, 0, 0, mesh());
    assert_eq!(unavailable, Err(BindingError::VideoLayerUnavailable));
    assert_eq!(
        unavailable.unwrap_err().code(),
        "binding-video-layer-unavailable"
    );
    assert_eq!(
        state.detach_video(VideoFamily::Unavailable, 0, 0),
        Err(BindingError::VideoLayerUnavailable)
    );
    assert!(state.store.entries().is_empty());
    assert_eq!(
        state.attach_video(VideoFamily::Provided, 0, 1, mesh()),
        Ok(None)
    );
    assert_eq!(
        state.attach_video(VideoFamily::Provided, 0, 0, mesh()),
        Ok(Some("im_1".to_string()))
    );
    assert_eq!(state.sets[0][0].video_mesh_id.as_deref(), Some("im_1"));
    assert_eq!(state.detach_video(VideoFamily::Provided, 0, 0), Ok(true));
    assert!(state.store.entries().is_empty());
    assert_eq!(state.detach_video(VideoFamily::Provided, 0, 0), Ok(false));
}

#[test]
fn a_component_layer_set_is_addressed_like_the_scene_set() {
    let mut state = state_of(&[], vec![vec![still(1, None)], vec![still(6, None)]]);
    let target = SlotTarget {
        set: 1,
        layer: 0,
        frame: 2,
        slot: 0,
    };
    let done = state.attach(target, mesh()).unwrap();
    assert_eq!(done.propagate.written, 6);
    assert_eq!(state.mesh_id_at(at(0, 0, 0)), Ok(None));
    assert_eq!(
        state.mesh_id_at(SlotTarget { frame: 5, ..target }),
        Ok(Some("im_1"))
    );
}
