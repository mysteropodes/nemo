//! Replay of the baseline oracle through the production binding kernel (L03-R).
//!
//! Each scenario starts from a store and layer sets the generator built with the real baseline
//! functions, then applies the recorded operations one by one. After EVERY operation the result and
//! the whole state (store key order, store content, every layer of every set) must equal what the
//! real `image-mesh.js` / `duplicateLayer` produced. The expected values are the oracle's literals.
use super::rows::{initial_state, run_op, scenarios, snapshot};

pub(crate) const SCENARIOS: [&str; 16] = [
    "corpus-mesh-propagates-to-all-24-frames",
    "propagate-counts-frames-without-a-raster-at-the-position",
    "propagate-overwrites-differing-tags-and-spares-other-positions",
    "attach-tags-every-frame-then-detach-releases-and-the-counter-keeps-rising",
    "detach-keeps-a-mesh-another-layer-still-points-at",
    "detach-keeps-a-mesh-a-component-layer-still-points-at",
    "detach-keeps-a-mesh-a-video-layer-still-points-at",
    "detach-without-a-live-tag-still-clears-the-other-frames",
    "release-if-unused-matrix",
    "is-referenced-by-frame-video-and-component-sources",
    "duplicate-is-a-fresh-id-deep-copy-and-a-missing-source-spends-no-id",
    "a-released-id-is-never-reissued-because-the-counter-is-never-lowered",
    "attach-over-an-existing-tag-leaves-the-old-mesh-in-the-store",
    "duplicate-layer-remaps-frames-and-video-under-fresh-shared-ids",
    "duplicate-layer-with-a-lone-or-missing-video-mesh",
    "video-layer-mesh-attach-replace-and-detach",
];

fn replay(name: &str) {
    let all = scenarios();
    let scenario = all
        .iter()
        .find(|scenario| scenario.name == name)
        .unwrap_or_else(|| panic!("scenario {name} is in the oracle"));
    let mut state = initial_state(scenario);
    assert_eq!(snapshot(&state), scenario.snaps[0], "{name}: loaded state");
    assert_eq!(scenario.ops.len(), scenario.results.len(), "{name}");
    assert_eq!(scenario.ops.len() + 1, scenario.snaps.len(), "{name}");
    for (step, op) in scenario.ops.iter().enumerate() {
        let result = run_op(&mut state, op);
        assert_eq!(
            result, scenario.results[step],
            "{name}: result of op {step} {op}"
        );
        assert_eq!(
            snapshot(&state),
            scenario.snaps[step + 1],
            "{name}: state after op {step} {op}"
        );
    }
}

#[test]
fn the_oracle_holds_exactly_the_pinned_scenarios_and_operations() {
    let all = scenarios();
    let names: Vec<&str> = all.iter().map(|scenario| scenario.name.as_str()).collect();
    assert_eq!(names, SCENARIOS.to_vec());
    let ops: usize = all.iter().map(|scenario| scenario.ops.len()).sum();
    assert_eq!(ops, 56);
}

#[test]
fn replay_corpus_mesh_propagates_to_all_24_frames() {
    replay("corpus-mesh-propagates-to-all-24-frames");
}

#[test]
fn replay_propagate_counts_frames_without_a_raster_at_the_position() {
    replay("propagate-counts-frames-without-a-raster-at-the-position");
}

#[test]
fn replay_propagate_overwrites_differing_tags_and_spares_other_positions() {
    replay("propagate-overwrites-differing-tags-and-spares-other-positions");
}

#[test]
fn replay_attach_tags_every_frame_then_detach_releases_and_the_counter_keeps_rising() {
    replay("attach-tags-every-frame-then-detach-releases-and-the-counter-keeps-rising");
}

#[test]
fn replay_detach_keeps_a_mesh_another_layer_still_points_at() {
    replay("detach-keeps-a-mesh-another-layer-still-points-at");
}

#[test]
fn replay_detach_keeps_a_mesh_a_component_layer_still_points_at() {
    replay("detach-keeps-a-mesh-a-component-layer-still-points-at");
}

#[test]
fn replay_detach_keeps_a_mesh_a_video_layer_still_points_at() {
    replay("detach-keeps-a-mesh-a-video-layer-still-points-at");
}

#[test]
fn replay_detach_without_a_live_tag_still_clears_the_other_frames() {
    replay("detach-without-a-live-tag-still-clears-the-other-frames");
}

#[test]
fn replay_release_if_unused_matrix() {
    replay("release-if-unused-matrix");
}

#[test]
fn replay_is_referenced_by_frame_video_and_component_sources() {
    replay("is-referenced-by-frame-video-and-component-sources");
}

#[test]
fn replay_duplicate_is_a_fresh_id_deep_copy_and_a_missing_source_spends_no_id() {
    replay("duplicate-is-a-fresh-id-deep-copy-and-a-missing-source-spends-no-id");
}

#[test]
fn replay_a_released_id_is_never_reissued_because_the_counter_is_never_lowered() {
    replay("a-released-id-is-never-reissued-because-the-counter-is-never-lowered");
}

#[test]
fn replay_attach_over_an_existing_tag_leaves_the_old_mesh_in_the_store() {
    replay("attach-over-an-existing-tag-leaves-the-old-mesh-in-the-store");
}

#[test]
fn replay_duplicate_layer_remaps_frames_and_video_under_fresh_shared_ids() {
    replay("duplicate-layer-remaps-frames-and-video-under-fresh-shared-ids");
}

#[test]
fn replay_duplicate_layer_with_a_lone_or_missing_video_mesh() {
    replay("duplicate-layer-with-a-lone-or-missing-video-mesh");
}

#[test]
fn replay_video_layer_mesh_attach_replace_and_detach() {
    replay("video-layer-mesh-attach-replace-and-detach");
}
