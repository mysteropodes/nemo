//! Literal pins and invariants of the duplicator evaluation join (L04-L).
//!
//! Every number below was read out of the baseline oracle (the verbatim
//! `applyLayerDuplicator`) and, where it is closed form, derived by hand and
//! checked against it; none was produced by the Rust code under test. Each
//! pinned scenario also runs through the oracle sweep, so a literal that is
//! wrong cannot hide behind a matching sweep.

use super::test_codec::{all, document_of, named, Scenario, World};
use super::test_compare::{copies_of, run, run_with, unchanged_reason};
use super::{CopySet, Evaluation, UnchangedReason};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    close, same,
};

fn set_of(all: &[Scenario], name: &str) -> CopySet {
    copies_of(&run(named(all, name))).clone()
}

/// `[dx, dy, rot]` of every clone.
fn moves(set: &CopySet) -> Vec<[f64; 3]> {
    set.placements
        .iter()
        .map(|p| {
            [
                p.placement.matrix.dx,
                p.placement.matrix.dy,
                p.placement.matrix.rot,
            ]
        })
        .collect()
}

fn assert_moves(set: &CopySet, want: &[[f64; 3]], rel: f64) {
    let got = moves(set);
    assert_eq!(got.len(), want.len());
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        for axis in 0..3 {
            assert!(
                close(g[axis], w[axis], rel),
                "clone {k} axis {axis}: {g:?} vs {w:?}"
            );
        }
    }
}

#[test]
fn the_seeded_corpus_descriptor_places_three_clones_in_a_row() {
    let all = all();
    let set = set_of(&all, "corpus_ly_mig_dup");
    assert_eq!(set.count, 3.0);
    // the legacy effector sits at the origin with radius 1: it reaches no clone
    assert_moves(
        &set,
        &[[0.0, 0.0, 0.0], [150.0, 0.0, 0.0], [300.0, 0.0, 0.0]],
        0.0,
    );
    for p in &set.placements {
        assert_eq!((p.pivot.x, p.pivot.y), (370.0, 70.0));
        assert_eq!(p.placement.opacity_factor, 1.0);
        assert_eq!((p.placement.matrix.sx, p.placement.matrix.sy), (1.0, 1.0));
        assert_eq!(p.placement.hue_deg, 0.0);
        assert!(p.placement.dup_3d.is_none());
    }
    let ids: Vec<&str> = set.copies.iter().map(|c| c.clone_id.as_str()).collect();
    assert_eq!(ids, ["dup7_0", "dup7_1", "dup7_2"]);
    assert!(set
        .copies
        .iter()
        .all(|c| c.record.target.stroke_id() == "s_mig_3"));
}

#[test]
fn a_legacy_effector_that_reaches_the_clones_adds_its_offsets() {
    let all = all();
    let set = set_of(&all, "corpus_ly_mig_dup_reaching_effector");
    // weight 1 - distance / 400 at 30, 120 and 270 px: 0.925, 0.7, 0.325
    assert_moves(
        &set,
        &[[9.25, 0.0, 4.625], [157.0, 0.0, 3.5], [303.25, 0.0, 1.625]],
        1e-12,
    );
}

#[test]
fn a_radial_ring_of_six_sits_on_the_unit_circle_points() {
    let all = all();
    let set = set_of(&all, "radial_full_circle_6");
    let h = 86.60254037844386;
    assert_moves(
        &set,
        &[
            [100.0, 0.0, 0.0],
            [50.0, h, 0.0],
            [-50.0, h, 0.0],
            [-100.0, 0.0, 0.0],
            [-50.0, -h, 0.0],
            [50.0, -h, 0.0],
        ],
        1e-9,
    );
    for p in &set.placements {
        assert_eq!((p.pivot.x, p.pivot.y), (120.0, 120.0));
    }
}

#[test]
fn the_seeded_random_stagger_of_seed_42_is_a_fixed_table() {
    let all = all();
    let set = set_of(&all, "grid_all_random_seed_42");
    let first_three = &set.placements[..3];
    let dx = [2.426490046083927, 57.564577287063, 110.33984452486038];
    let dy = [0.7239321740344167, -6.319422932341695, -5.597637253347784];
    let rot = [6.344384282827377, -2.150093929376453, -8.723436305299401];
    let opacity = [0.9936177898757159, 0.9768035340867937, 1.0];
    let hue = [
        -1.3841472449712455,
        -0.24045639438554645,
        14.336772030219436,
    ];
    for (k, p) in first_three.iter().enumerate() {
        let m = &p.placement;
        assert!(close(m.matrix.dx, dx[k], 1e-12), "dx {k}");
        assert!(close(m.matrix.dy, dy[k], 1e-12), "dy {k}");
        assert!(close(m.matrix.rot, rot[k], 1e-12), "rot {k}");
        assert!(close(m.opacity_factor, opacity[k], 1e-12), "opacity {k}");
        assert!(close(m.hue_deg, hue[k], 1e-12), "hue {k}");
    }
}

#[test]
fn a_step_effector_ramps_with_the_clone_index() {
    let all = all();
    let set = set_of(&all, "eff_step_ramps_by_index");
    assert_moves(
        &set,
        &[
            [0.0, 0.0, 0.0],
            [50.0, 10.0, 0.0],
            [100.0, 20.0, 0.0],
            [150.0, 30.0, 0.0],
            [200.0, 40.0, 0.0],
        ],
        1e-12,
    );
    let hue: Vec<f64> = set.placements.iter().map(|p| p.placement.hue_deg).collect();
    assert_eq!(hue, [0.0, 22.5, 45.0, 67.5, 90.0]);
}

#[test]
fn an_open_path_spreads_five_clones_along_its_length() {
    let all = all();
    let set = set_of(&all, "path_open_5");
    assert_moves(
        &set,
        &[
            [0.0, 0.0, 0.0],
            [100.0, 25.0, 0.0],
            [200.0, 50.0, 0.0],
            [300.0, 25.0, 0.0],
            [400.0, 0.0, 0.0],
        ],
        1e-9,
    );
    let tangent = set_of(&all, "path_open_align_tangent");
    let rot: Vec<f64> = moves(&tangent).iter().map(|m| m[2]).collect();
    let a = 17.354024636261325;
    for (got, want) in rot.iter().zip([0.0, a, 0.0, -a, 0.0]) {
        assert!(close(*got, want, 1e-9), "{rot:?}");
    }
}

#[test]
fn an_effector_layer_pulls_the_clones_toward_its_origin() {
    let all = all();
    let set = set_of(&all, "el_position_offset");
    // effector at (170, 120), radius 150, offset (0, 40): weight 1 - distance / 150
    assert_moves(
        &set,
        &[
            [0.0, 26.66666666666667, 0.0],
            [50.0, 40.0, 0.0],
            [100.0, 26.66666666666667, 0.0],
            [150.0, 13.333333333333336, 0.0],
            [200.0, 0.0, 0.0],
        ],
        1e-12,
    );
}

#[test]
fn object_mode_cycles_the_pool_members_in_slot_order() {
    let all = all();
    let set = set_of(&all, "pool_cycles_three_members");
    let ids: Vec<&str> = set
        .copies
        .iter()
        .map(|c| c.record.target.stroke_id())
        .collect();
    assert_eq!(
        ids,
        ["m1a", "m2a", "m2b", "m3a", "m1a", "m2a", "m2b", "m3a", "m1a"]
    );
    let index: Vec<u32> = set.copies.iter().map(|c| c.dup_index).collect();
    assert_eq!(index, [0, 1, 1, 2, 3, 4, 4, 5, 6]);
    // an empty host: the pivot is the canvas center the members are recentred on
    assert!(set
        .placements
        .iter()
        .all(|p| (p.pivot.x, p.pivot.y) == (960.0, 540.0)));
    let dx: Vec<f64> = set
        .placements
        .iter()
        .map(|p| p.placement.matrix.dx)
        .collect();
    assert_eq!(dx, [0.0, 90.0, 180.0, 270.0, 360.0, 450.0, 540.0]);
}

fn host_frames(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("S host ").map(str::to_string))
        .collect()
}

#[test]
fn the_temporal_stagger_resamples_the_host_at_shifted_frames() {
    let all = all();
    // frame 10, offset 2 frames per clone, four clones, span 0..23
    let forward = run(named(&all, "tmp_forward"));
    let frame = |f: f64| format!("{:016x}", f.to_bits());
    assert_eq!(
        host_frames(&forward.calls),
        [frame(10.0), frame(8.0), frame(6.0), frame(4.0)]
    );
    let backward = run(named(&all, "tmp_backward"));
    assert_eq!(
        host_frames(&backward.calls),
        [frame(4.0), frame(6.0), frame(8.0), frame(10.0)]
    );
    let off = run(named(&all, "tmp_disabled"));
    assert!(host_frames(&off.calls).is_empty());
}

#[test]
fn a_longer_row_leaves_the_earlier_clones_where_they_were() {
    let all = all();
    let six = set_of(&all, "grid_all_random_6_clones");
    let seven = set_of(&all, "grid_all_random_7_clones");
    assert_eq!((six.placements.len(), seven.placements.len()), (6, 7));
    for k in 0..6 {
        assert_eq!(six.placements[k], seven.placements[k], "clone {k}");
        assert_eq!(six.copies[k], seven.copies[k], "copy {k}");
    }
}

#[test]
fn another_seed_changes_every_random_clone() {
    let all = all();
    let a = set_of(&all, "grid_all_random_6_clones");
    let b = set_of(&all, "grid_all_random_other_seed");
    assert_eq!(a.placements.len(), b.placements.len());
    for k in 0..a.placements.len() {
        assert_ne!(
            a.placements[k].placement, b.placements[k].placement,
            "clone {k}"
        );
    }
    // while the same seed is the same pattern on every run
    let again = set_of(&all, "grid_all_random_6_clones");
    assert_eq!(a, again);
}

#[test]
fn a_source_with_one_keyframe_ignores_the_temporal_offset() {
    let all = all();
    let on = set_of(&all, "tmp_one_keyframe_source");
    let off = set_of(&all, "tmp_one_keyframe_source_off");
    assert_eq!(on.copies, off.copies);
    assert_eq!(on.placements, off.placements);
}

#[test]
fn evaluation_only_reads_its_inputs_and_returns_detached_copies() {
    let all = all();
    for name in [
        "corpus_ly_mig_dup",
        "pool_two_members",
        "path_open_5",
        "el_all_channels",
    ] {
        let sc = named(&all, name);
        let (set_before, layers_before) = document_of(sc);
        let base_before = sc.base.clone();
        let run = run_with(sc, &set_before, &layers_before, &World::new(sc));
        let (set_after, layers_after) = document_of(sc);
        assert_eq!(set_before, set_after, "{name}: the document changed");
        assert_eq!(layers_before, layers_after, "{name}: the layers changed");
        assert_eq!(sc.base, base_before, "{name}: the host content changed");
        let copies = copies_of(&run);
        assert!(!copies.copies.is_empty(), "{name}");
        // a copy is its own value: identity is the clone id plus the index, the target is the source's
        let source_ids: Vec<&str> = sc.base.iter().map(|r| r.target.stroke_id()).collect();
        for c in &copies.copies {
            assert!(c.clone_id.starts_with("dup"), "{name}");
            if !source_ids.is_empty() {
                assert!(source_ids.contains(&c.record.target.stroke_id()), "{name}");
            }
        }
    }
}

#[test]
fn every_way_the_base_comes_back_unchanged_has_its_reason() {
    let all = all();
    let reason = |name: &str| unchanged_reason(&run(named(&all, name)));
    let cases = [
        (
            "guard_no_duplicator_returns_the_base",
            UnchangedReason::NoDuplicator,
        ),
        (
            "guard_edit_source_returns_the_base",
            UnchangedReason::EditingSource,
        ),
        (
            "guard_empty_base_returns_the_base",
            UnchangedReason::EmptyBase,
        ),
        (
            "guard_empty_descriptor_is_a_single_clone",
            UnchangedReason::SingleClone,
        ),
        (
            "grid_one_clone_returns_the_base",
            UnchangedReason::SingleClone,
        ),
        (
            "path_count_one_returns_the_base",
            UnchangedReason::SingleClone,
        ),
        (
            "pool_all_members_gone_returns_the_base",
            UnchangedReason::EmptyPool,
        ),
        (
            "pool_all_members_duplicators_returns_the_base",
            UnchangedReason::EmptyPool,
        ),
        (
            "path_uid_missing_returns_the_base",
            UnchangedReason::BrokenPath,
        ),
        (
            "path_uid_unresolved_returns_the_base",
            UnchangedReason::BrokenPath,
        ),
        (
            "path_layer_is_a_duplicator_returns_the_base",
            UnchangedReason::BrokenPath,
        ),
        (
            "path_layer_without_usable_stroke_returns_the_base",
            UnchangedReason::BrokenPath,
        ),
        (
            "path_zero_length_returns_the_base",
            UnchangedReason::BrokenPath,
        ),
    ];
    for (name, want) in cases {
        assert_eq!(reason(name), Some(want), "{name}");
    }
    // the Object-mode guard lets an empty host through, and the empty pool list builds nothing
    for name in [
        "guard_empty_base_in_objects_mode_continues",
        "pool_empty_uid_list_builds_no_copies",
    ] {
        assert!(
            copies_of(&run(named(&all, name))).copies.is_empty(),
            "{name}"
        );
    }
}

#[test]
fn the_clone_count_is_capped_by_the_mode() {
    let all = all();
    let counts = [
        ("grid_cap_900", 900.0),
        ("radial_cap_500", 500.0),
        ("path_count_cap_500", 500.0),
        ("grid_fractional_rows", 5.0),
        ("radial_fractional_count", 5.5),
    ];
    for (name, count) in counts {
        let result = run(named(&all, name));
        let set = copies_of(&result);
        assert!(same(set.count, count), "{name}: {}", set.count);
        assert_eq!(set.placements.len(), count.ceil() as usize, "{name}");
    }
}

#[test]
fn a_mode_label_that_is_not_grid_radial_or_path_raises_like_the_baseline() {
    let all = all();
    let result = run(named(&all, "other_mode_label_throws"));
    assert!(matches!(
        result.result,
        Err(super::EvaluateError::PathInfoRequired)
    ));
    assert!(!matches!(result.result, Ok(Evaluation::Copies(_))));
}
