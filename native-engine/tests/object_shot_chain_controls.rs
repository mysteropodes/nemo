//! Controls for the shot chain kernel: each deliberate mistake, run through the
//! same oracle comparison as the shipped kernel, must be rejected by the rows
//! that exercise it. Counts are what `tools/gen.mjs` measured on the JS baseline
//! with the equivalent source edit (the Rust side checks the same rows).

use super::modules::{js_round, ModuleRef};
use super::rows::{run, run_defaults, DefaultImpls, Impls, Report, DEFAULTS, ORACLE, PRODUCTION};
use super::{
    ensure_retime, reset_retime, split_chain_clip_with, ChainSplit, InstanceModule, Retime,
    ShotChainError, ShotHost, SplitRules, SymbolView, BASELINE_RULES,
};

/// Wanted source frames of the left half, before the two clamps.
fn wanted(src_len: i64, duration: i64, local: i64) -> f64 {
    let speed = duration as f64 / src_len as f64;
    local as f64 / speed
}

/// `Math.floor` instead of `Math.round`.
fn floor_pick(src_len: i64, duration: i64, local: i64) -> i64 {
    let kept = wanted(src_len, duration, local).floor().max(1.0);
    kept.min((src_len - 1) as f64) as i64
}

/// The `srcLen - 1` clamp dropped.
fn no_upper_clamp(src_len: i64, duration: i64, local: i64) -> i64 {
    js_round(wanted(src_len, duration, local)).max(1.0) as i64
}

/// The `Math.max(1, ...)` floor dropped.
fn no_lower_clamp(src_len: i64, duration: i64, local: i64) -> i64 {
    js_round(wanted(src_len, duration, local)).min((src_len - 1) as f64) as i64
}

fn split_with(
    host: &ShotHost,
    id: &str,
    local: i64,
    pick: super::LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    split_chain_clip_with(host, id, local, pick)
}

/// The new member inserted BEFORE the cut one.
fn inserted_before(
    host: &ShotHost,
    id: &str,
    local: i64,
    pick: super::LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    let mut split = split_with(host, id, local, pick)?;
    let at = split
        .chain
        .iter()
        .position(|r| *r == ModuleRef::New(0))
        .unwrap();
    split.chain.swap(at, at - 1);
    Ok(split)
}

/// The right half keeps the whole duration instead of `duration - local`.
fn right_keeps_duration(
    host: &ShotHost,
    id: &str,
    local: i64,
    pick: super::LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    let mut split = split_with(host, id, local, pick)?;
    split.right.duration = split.right.duration.map(|d| d + local);
    Ok(split)
}

/// The left half keeps the whole duration instead of `local`.
fn left_keeps_duration(
    host: &ShotHost,
    id: &str,
    local: i64,
    pick: super::LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    let mut split = split_with(host, id, local, pick)?;
    split.left.duration += split.right.duration.unwrap_or(0);
    Ok(split)
}

/// The right half skips one source frame.
fn gap_in_the_source(
    host: &ShotHost,
    id: &str,
    local: i64,
    pick: super::LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    let mut split = split_with(host, id, local, pick)?;
    split.right.trim_in = split.right.trim_in.map(|t| t + 1);
    Ok(split)
}

fn include_edge(local: i64, duration: i64) -> bool {
    local >= 0 && local < duration
}

fn closed_member(frame: i64, start: i64, duration: i64) -> bool {
    frame <= start + duration
}

fn duration_without_layer_fallback(symbol: &SymbolView) -> i64 {
    match symbol {
        SymbolView::Present { total_frames, .. } if *total_frames != 0 => *total_frames,
        _ => 24,
    }
}

fn ensure_without_plus_one(member: &InstanceModule) -> Result<Retime, ShotChainError> {
    let mut retime = ensure_retime(member)?;
    if member.duration.is_none() {
        retime.duration -= 1;
    }
    Ok(retime)
}

fn reset_without_minus_one(symbol: &SymbolView) -> Result<Retime, ShotChainError> {
    let retime = reset_retime(symbol)?;
    Ok(Retime {
        trim_in: 0,
        trim_out: retime.trim_out + 1,
        duration: retime.duration + 1,
    })
}

/// Assert a mutant is rejected: at least `at_least` rows, only rows of `kinds`.
fn rejected(report: Report, at_least: usize, kinds: &str) -> Vec<String> {
    assert!(
        report.bad.len() >= at_least,
        "slipped through: {} of {} rows",
        report.bad.len(),
        report.checked
    );
    assert!(
        report.bad.iter().all(|(k, _)| kinds.contains(*k)),
        "unexpected row kind rejected"
    );
    report.bad.into_iter().map(|(_, text)| text).collect()
}

fn with_pick(pick: super::LeftSrcLen) -> Impls {
    Impls { pick, ..PRODUCTION }
}

fn with_split(split: super::rows::SplitFn) -> Impls {
    Impls {
        split,
        ..PRODUCTION
    }
}

fn with_rules(rules: SplitRules) -> Impls {
    Impls {
        rules,
        ..PRODUCTION
    }
}

#[test]
fn floor_instead_of_round_is_rejected() {
    let bad = rejected(run(&with_pick(floor_pick)), 20, "S");
    assert!(bad
        .iter()
        .any(|b| b.starts_with("S I:P-48-12-a:0:9:20 m0 3 ")));
}

#[test]
fn dropping_the_upper_source_clamp_is_rejected() {
    let bad = rejected(run(&with_pick(no_upper_clamp)), 12, "S");
    assert!(bad
        .iter()
        .any(|b| b.starts_with("S I:P-48-12-a:0:9:20 m0 19 ")));
}

#[test]
fn dropping_the_one_frame_floor_is_rejected() {
    let bad = rejected(run(&with_pick(no_lower_clamp)), 11, "S");
    assert!(bad
        .iter()
        .any(|b| b.starts_with("S I:P-48-3-a:0:2:147 m0 1 ")));
}

#[test]
fn accepting_the_playhead_on_the_member_edge_is_rejected() {
    let rules = SplitRules {
        strictly_inside: include_edge,
        ..BASELINE_RULES
    };
    let bad = rejected(run(&with_rules(rules)), 41, "P");
    assert!(bad
        .iter()
        .any(|b| b.starts_with("P I:P-24-24-a:0:23:24 0 m0 ")));
}

#[test]
fn closing_the_member_interval_is_rejected() {
    let rules = SplitRules {
        in_member: closed_member,
        ..BASELINE_RULES
    };
    let bad = rejected(run(&with_rules(rules)), 1, "P");
    assert!(bad
        .iter()
        .any(|b| b.contains("I:P-24-24-b:0:9:-4|I:P-24-24-c:0:9:10 10 m2 ")));
}

#[test]
fn inserting_the_new_member_before_the_cut_one_is_rejected() {
    rejected(run(&with_split(inserted_before)), 241, "S");
}

#[test]
fn a_right_half_that_keeps_the_whole_duration_is_rejected() {
    rejected(run(&with_split(right_keeps_duration)), 241, "S");
}

#[test]
fn a_left_half_that_keeps_the_whole_duration_is_rejected() {
    rejected(run(&with_split(left_keeps_duration)), 241, "S");
}

#[test]
fn a_gap_between_the_halves_of_the_source_is_rejected() {
    rejected(run(&with_split(gap_in_the_source)), 241, "S");
}

#[test]
fn a_symbol_length_without_the_layer_fallback_is_rejected() {
    let impls = DefaultImpls {
        duration: duration_without_layer_fallback,
        ..DEFAULTS
    };
    let bad = rejected(run_defaults(&impls), 2, "D");
    assert!(bad.iter().any(|b| b == "D P-~-12-a 12"));
}

#[test]
fn a_duration_without_the_inclusive_end_is_rejected() {
    let impls = DefaultImpls {
        ensure: ensure_without_plus_one,
        ..DEFAULTS
    };
    let bad = rejected(run_defaults(&impls), 45, "R");
    assert!(bad.iter().any(|b| b == "R M ~ ~ ~ 0,23,24"));
}

#[test]
fn a_reset_that_forgets_the_last_frame_offset_is_rejected() {
    let impls = DefaultImpls {
        reset: reset_without_minus_one,
        ..DEFAULTS
    };
    let bad = rejected(run_defaults(&impls), 8, "Z");
    assert!(bad.iter().any(|b| b == "Z M 0,23,24"));
}

/// The production kernel passes the same comparison the mutants fail, and the
/// grid really contains the classes the controls above reject.
#[test]
fn the_unmutated_comparison_is_clean_and_the_classes_are_present() {
    assert!(run(&PRODUCTION).bad.is_empty());
    assert!(run_defaults(&DEFAULTS).bad.is_empty());
    let has = |prefix: &str| ORACLE.lines().any(|l| l.starts_with(prefix));
    assert!(has("S I:P-48-12-a:0:9:20 m0 19 "), "clamp class");
    assert!(has("S I:P-48-3-a:0:2:147 m0 1 "), "floor class");
    assert!(has("P I:P-24-24-a:0:23:24 0 m0 "), "edge class");
}
