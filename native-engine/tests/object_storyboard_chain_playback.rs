//! Cross-check of the StoryBoard montage frame mapping against the component
//! playback kernel (L04-A) that replays a montage-view segment.
//!
//! `enterMontageView` turns every chain member into a `once` layer with
//! `symSpeed = srcLen / duration`, `symPlacedAt` = the member's start,
//! `symTrimIn/symTrimOut`, and `resolveSymbolFrameIdx` plays it. `montageStrokesAt`
//! (what the montage preview and montage layers use) maps the same frame with
//! `trimIn + min(srcLen - 1, floor(local * srcLen / duration))`. The two formulas
//! are NOT the same arithmetic: `local * (srcLen / duration)` can land just below an
//! integer that `local * srcLen / duration` hits exactly. The baseline keeps both
//! and so does the native side; the grid below pins every disagreement
//! (`X` rows, produced by running both baseline functions verbatim) and the
//! fixture-chain test shows the real kernels agree on every frame a member owns; a hand-built
//! chain on a pinned point exercises the divergence through the same comparison.

use super::tests::{chain_of, rows, world, ORACLE};
use super::{ComponentSymbol, ModuleKind, StoryModule, StoryboardChain, SymbolTable};
use crate::evaluation::object_component_kernels::object_component_playback::{
    resolve_component_frame, PlayMode, PlaybackInput,
};
use crate::evaluation::object_montage_timing::{
    montage_source_frame, montage_view_segments, storyboard_source_frame,
};
use std::collections::BTreeSet;

const TRIM_IN: i64 = 3;
const PLACED: i64 = 7;

fn once_input(speed: f64, placed_at: i64, trim_in: i64, trim_out: i64) -> PlaybackInput {
    PlaybackInput {
        play_mode: PlayMode::Once,
        speed: Some(speed),
        placed_at: Some(placed_at as f64),
        trim_in: Some(trim_in as f64),
        trim_out: Some(trim_out as f64),
        ..PlaybackInput::default()
    }
}

/// Every (srcLen, duration, local) of the grid where the montage mapping and the
/// `once` replay differ, found with the NATIVE kernels, equals the baseline's `X` rows.
#[test]
fn formula_grid_diverges_exactly_where_the_baseline_does() {
    let mut got: Vec<String> = Vec::new();
    let mut points = 0u32;
    for src_len in 1..=60i64 {
        for duration in 1..=60i64 {
            for local in 0..duration {
                points += 1;
                let montage = storyboard_source_frame(TRIM_IN, src_len, local, duration).unwrap();
                let input = once_input(
                    src_len as f64 / duration as f64,
                    PLACED,
                    TRIM_IN,
                    TRIM_IN + src_len - 1,
                );
                let once = resolve_component_frame(&input, 1000.0, PLACED + local, None).raw();
                if once != montage as f64 {
                    got.push(format!("X {src_len} {duration} {local} {montage} {once}"));
                }
            }
        }
    }
    assert_eq!(points, 109_800);
    let want: Vec<String> = rows("X").iter().map(|f| f.join(" ")).collect();
    assert_eq!(want.len(), 18);
    assert_eq!(got, want);
}

/// The first divergent point, with literals, so the grid test cannot pass by both
/// sides drifting together: 23 * 26 / 46 is exactly 13, 23 * (26 / 46) is 12.999...
#[test]
fn the_first_divergent_point_is_pinned_with_literals() {
    assert_eq!(storyboard_source_frame(3, 26, 23, 46), Ok(16));
    let input = once_input(26.0 / 46.0, 7, 3, 28);
    assert_eq!(
        resolve_component_frame(&input, 1000.0, 30, None).raw(),
        15.0
    );
    assert!(ORACLE.lines().any(|l| l == "X 26 46 23 16 15"));
}

/// Replays every member of `ids` as its montage-view segment through the L04-A kernel and
/// compares with what `montageStrokesAt` answers. Only frames whose montage selection belongs
/// to the segment's own member are comparable: a negative `duration` earlier in the chain makes
/// segments overlap, and there `montageStrokesAt` answers from the earlier member; frames below
/// zero (a member that starts after a negative `duration`) are skipped, as the typed
/// `NegativeFrame` error is unreachable in the baseline. Returns
/// `(checked, explained)` and asserts that every difference is a pinned `X` point, one frame
/// higher on the montage side.
fn compare_chain(
    sc: &StoryboardChain<'_>,
    known: &BTreeSet<(i64, i64, i64)>,
    name: &str,
    ids: &[String],
) -> (u32, u32) {
    let members = sc.members(ids).unwrap();
    if members.is_empty() {
        return (0, 0);
    }
    let spec = montage_view_segments(&members).unwrap();
    let (mut checked, mut explained) = (0u32, 0u32);
    for (index, segment) in spec.segments.iter().enumerate() {
        let src_len = segment.sym_trim_out - segment.sym_trim_in + 1;
        let duration = segment.out_point - segment.in_point + 1;
        if src_len < 1 || duration < 1 || src_len > 60 || duration > 60 {
            continue;
        }
        let input = once_input(
            segment.sym_speed,
            segment.sym_placed_at,
            segment.sym_trim_in,
            segment.sym_trim_out,
        );
        for frame in segment.in_point..=segment.out_point {
            // A member after a negative `duration` (c_r23) starts below zero; a negative
            // montage frame is the typed `NegativeFrame` error here and unreachable in the
            // baseline, so those frames are not comparable.
            if frame < 0 {
                continue;
            }
            let owner = montage_source_frame(&members, frame)
                .unwrap()
                .expect("a frame inside a member");
            if owner.member_index != index {
                continue;
            }
            let local = frame - segment.sym_placed_at;
            let once = resolve_component_frame(&input, 1000.0, frame, None).raw();
            checked += 1;
            if once == owner.src_frame as f64 {
                continue;
            }
            explained += 1;
            assert!(
                known.contains(&(src_len, duration, local)),
                "{name} frame {frame}: {} vs {once} is not a pinned point",
                owner.src_frame
            );
            assert_eq!(owner.src_frame as f64 - once, 1.0, "{name} frame {frame}");
        }
    }
    (checked, explained)
}

fn known_points() -> BTreeSet<(i64, i64, i64)> {
    rows("X")
        .iter()
        .map(|f| {
            (
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
            )
        })
        .collect()
}

/// Real chains: each member replayed as its montage-view segment through the L04-A
/// kernel gives the frame `montageStrokesAt` asked of the Component, on every frame the
/// montage selection assigns to that member. None of the fixture chains lands on an `X`
/// point (the divergence proof is the grid test above and the hand-built chain below);
/// the 109,800-point grid, not this test, carries that claim.
#[test]
fn fixture_chains_replayed_through_the_component_kernel_agree() {
    let w = world();
    let sc = chain_of(&w);
    let known = known_points();
    let (mut checked, mut explained) = (0u32, 0u32);
    for (name, ids) in &w.chains {
        let (c, e) = compare_chain(&sc, &known, name, ids);
        checked += c;
        explained += e;
    }
    // generator-measured on the baseline functions: 1512 frames compared (frames >= 0 only), none differs
    assert!(ORACLE
        .lines()
        .any(|l| l == "# crosscheck checked 1512 explained 0"));
    assert_eq!((checked, explained), (1512, 0));
}

/// A hand-built chain that DOES land on a pinned point (trimIn 3, srcLen 26, duration 46,
/// local 23), so the known-point branch of `compare_chain` is exercised: exactly one frame
/// of the 46 differs, by one.
#[test]
fn a_hand_built_chain_on_a_pinned_point_diverges_by_exactly_one_frame() {
    let mut symbols = SymbolTable::new();
    symbols.insert(
        "s".to_string(),
        ComponentSymbol {
            name: None,
            total_frames: Some(100),
            layers: Vec::new(),
        },
    );
    let modules = vec![StoryModule {
        id: "m".to_string(),
        kind: ModuleKind::Instance,
        symbol_id: "s".to_string(),
        trim_in: Some(3),
        trim_out: Some(28),
        duration: Some(46),
    }];
    let sc = StoryboardChain::components_only(&modules, &symbols).unwrap();
    let ids = vec!["m".to_string()];
    let (checked, explained) = compare_chain(&sc, &known_points(), "hand", &ids);
    assert_eq!((checked, explained), (46, 1));
    assert_eq!(sc.selection_at(&ids, 23).unwrap().unwrap().src_frame, 16);
}
