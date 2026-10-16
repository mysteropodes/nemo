//! Controls for the montage timing kernel: each deliberate mistake, run through
//! the same oracle comparison as the shipped kernel, must be rejected by the
//! rows that exercise it (counts are what `tools/gen.mjs` measured on the JS
//! baseline with the equivalent source edit; the Rust side may add `C` rows).

use super::rows::{run, Impls, ORACLE, PRODUCTION, WALK_TOKEN};
use super::{
    storyboard_source_frame, LayerFrame, LayerNone, MontageMember, MontageTimingError, Retime,
    SourcePick,
};

/// `Math.round` instead of `Math.floor`.
fn round_instead_of_floor(
    trim_in: i64,
    src_len: i64,
    local: i64,
    duration: i64,
) -> Result<i64, MontageTimingError> {
    let scaled = (local as f64 * src_len as f64 / duration as f64 + 0.5).floor();
    let cap = (src_len - 1) as f64;
    let picked = if scaled < cap { scaled } else { cap };
    Ok(trim_in + picked as i64)
}

/// The `Math.min(srcLen - 1, ...)` clamp dropped.
fn without_clamp(
    trim_in: i64,
    src_len: i64,
    local: i64,
    duration: i64,
) -> Result<i64, MontageTimingError> {
    let scaled = (local as f64 * src_len as f64 / duration as f64).floor();
    Ok(trim_in + scaled as i64)
}

/// The `once` replay's `local * (srcLen / duration)` used for the montage frame.
fn unified_with_once(
    trim_in: i64,
    src_len: i64,
    local: i64,
    duration: i64,
) -> Result<i64, MontageTimingError> {
    let scaled = (local as f64 * (src_len as f64 / duration as f64)).floor();
    let cap = (src_len - 1) as f64;
    let picked = if scaled < cap { scaled } else { cap };
    Ok(trim_in + picked as i64)
}

/// `% montageTotal` dropped: the montage layer no longer loops.
fn without_wrap(
    frame: i64,
    placed_at: Option<i64>,
    total: i64,
) -> Result<LayerFrame, MontageTimingError> {
    if total == 0 {
        return Ok(LayerFrame::None(LayerNone::ZeroTotal));
    }
    let mlf = frame - placed_at.unwrap_or(0);
    if mlf < 0 {
        return Ok(LayerFrame::None(LayerNone::BeforePlacement));
    }
    Ok(LayerFrame::Local(mlf))
}

/// Defaults taken as 0 instead of derived from the symbol (no `ensureRetime`).
fn without_ensure_retime(member: &MontageMember) -> Result<Retime, MontageTimingError> {
    Ok(Retime {
        trim_in: member.trim_in.unwrap_or(0),
        trim_out: member.trim_out.unwrap_or(0),
        duration: member.duration.unwrap_or(0),
    })
}

fn with_pick(pick: SourcePick) -> Impls {
    Impls { pick, ..PRODUCTION }
}

/// Assert a mutant is rejected: at least `at_least` rows, only rows of `kinds`.
fn rejected(impls: &Impls, at_least: usize, kinds: &str) -> Vec<String> {
    let report = run(impls);
    println!("{} rows rejected", report.bad.len());
    assert!(
        report.bad.len() >= at_least,
        "slipped through: {} rows",
        report.bad.len()
    );
    assert!(
        report.bad.iter().all(|(k, _)| kinds.contains(*k)),
        "unexpected row kind rejected"
    );
    report.bad.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn round_instead_of_floor_is_rejected() {
    let bad = rejected(&with_pick(round_instead_of_floor), 55, "FLC");
    assert!(bad.iter().any(|b| b.contains("F I:P-48-12-a:0:9:20 ")));
}

#[test]
fn dropping_the_src_len_clamp_is_rejected() {
    let bad = rejected(&with_pick(without_clamp), 75, "FLC");
    assert!(bad.iter().any(|b| b.contains("F I:P-48-1-a:4:3:3 ")));
}

#[test]
fn using_the_once_formula_for_the_montage_frame_is_rejected() {
    let bad = rejected(&with_pick(unified_with_once), 38, "FLC");
    assert!(bad.iter().filter(|b| b.contains(": C ")).count() >= 35);
    assert!(bad.iter().any(|b| b.contains("F I:P-48-3-a:0:2:147 49 ")));
}

#[test]
fn dropping_the_total_wrap_is_rejected() {
    let impls = Impls {
        layer_frame: without_wrap,
        ..PRODUCTION
    };
    let bad = rejected(&impls, 413, "L");
    assert!(bad
        .iter()
        .any(|b| b.contains(&format!("L {} ~ 64 ", WALK_TOKEN))));
}

#[test]
fn accumulating_without_ensure_retime_is_rejected() {
    let impls = Impls {
        retime: without_ensure_retime,
        ..PRODUCTION
    };
    let bad = rejected(&impls, 648, "RTFLV");
    assert!(bad.iter().any(|b| b.contains(": T I:P-48-12-walk:~:~:~ ")));
    assert!(bad.iter().any(|b| b.contains(": R ")));
}

/// The grid really contains what the acceptance names, so the controls above
/// are not rejecting an empty class.
#[test]
fn oracle_covers_the_named_chain_classes() {
    let has = |needle: &str| ORACLE.lines().any(|l| l.contains(needle));
    assert!(has("T X|X 0"), "all dangling");
    assert!(has("T N|X|N 0"), "only non-instance");
    assert!(has("F I:P-48-1-a:3:3:1 0 "), "1-frame chain");
    assert!(has("F I:P-48-1-a:4:3:3 0 "), "zero-length source");
    assert!(has("F I:P-48-1-a:6:3:4 0 "), "inverted source");
    assert!(has("T I:P-48-1-a:0:9:0 0"), "zero duration");
    assert!(has("F I:P-48-12-a:0:9:20 "), "stretched");
    assert!(has("F I:P-48-12-a:0:9:5 "), "squeezed");
    assert!(has("T I:M:~:~:~ 24"), "missing symbol");
    assert!(
        has("T I:P-24-24-a:0:9:-4|I:P-24-24-b:0:9:10 6"),
        "negative duration"
    );
    assert!(has("V N|X|N E"), "empty montage view");
    assert_eq!(storyboard_source_frame(0, 2, 49, 98), Ok(1));
}
