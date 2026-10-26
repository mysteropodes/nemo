//! nemo-test-harness: test-evaluation => expression_time_tests
use crate::evaluation::expression_time::{
    loop_plan, loop_value, step_time, to_frames, to_seconds, LoopMode, LoopPlan, LoopSide,
};

/// Bit pattern for comparison, with every NaN folded to one value. JS cannot
/// observe a NaN's sign or payload, and engines and CPUs disagree on them
/// (V8 on x86-64 yields 0xfff8…, JavaScriptCore and aarch64 yield 0x7ff8…);
/// -0 versus +0 stays distinct.
fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

fn same(actual: f64, expected: f64, label: &str) {
    assert_eq!(
        bits(actual),
        bits(expected),
        "{label}: {actual:?} != {expected:?}"
    );
}

fn same_vec(actual: &[f64], expected: &[f64], label: &str) {
    let a: Vec<u64> = actual.iter().map(|&x| bits(x)).collect();
    let e: Vec<u64> = expected.iter().map(|&x| bits(x)).collect();
    assert_eq!(a, e, "{label}: {actual:?} != {expected:?}");
}

/// The oracle's stub track: component `c` at frame `f` is `f * m[c] + k[c]`.
fn line<'a>(m: &'a [f64], k: &'a [f64]) -> impl FnMut(f64) -> Vec<f64> + 'a {
    move |f| m.iter().zip(k).map(|(mc, kc)| f * mc + kc).collect()
}

fn looped(keys: &[f64], frame: f64, mode: Option<&str>, n: f64, side: LoopSide) -> Vec<f64> {
    let plan = loop_plan(keys, frame, LoopMode::from_js(mode), n, side);
    loop_value(plan, line(&[2.0, -1.0], &[10.0, 0.5]))
}

#[test]
fn characterized_time_rows_match_javascript() {
    same(step_time(13.0, 4.0), 12.0, "snap down");
    same(step_time(-1.0, 4.0), -4.0, "floor, not truncate");
    same(step_time(7.5, 0.0), 7.5, "zero step");
    same(step_time(7.5, -2.0), 7.5, "negative step");
    same(step_time(7.5, f64::NAN), 7.5, "NaN step");
    same(step_time(7.5, f64::INFINITY), 7.5, "infinite step");
    same(step_time(-0.0, 4.0), -0.0, "-0 / 4 floors to -0");
    same(to_frames(1.5, 24.0), 36.0, "seconds to frames");
    same(to_seconds(36.0, 24.0), 1.5, "frames to seconds");
    same(to_seconds(36.0, 0.0), 0.0, "zero rate");
    same(to_seconds(36.0, -0.0), 0.0, "-0 rate is +0");
    same(to_seconds(-0.0, 24.0), -0.0, "-0 frames keeps sign");
}

#[test]
fn characterized_loop_rows_match_javascript() {
    let keys = [0.0, 10.0, 20.0];
    let after = LoopSide::After;
    let before = LoopSide::Before;
    // Inside the keyed range nothing loops.
    same_vec(
        &looped(&keys, 15.0, None, 0.0, after),
        &[40.0, -14.5],
        "inside",
    );
    // cycle: frame 25 replays frame 5.
    same_vec(
        &looped(&keys, 25.0, None, 0.0, after),
        &[20.0, -4.5],
        "cycle",
    );
    // pingpong: the second pass runs backwards, so 25 reads 15.
    same_vec(
        &looped(&keys, 25.0, Some("PingPong"), 0.0, after),
        &[40.0, -14.5],
        "pingpong",
    );
    // offset: one full pass adds (v(20) - v(0)) once.
    same_vec(
        &looped(&keys, 25.0, Some("offset"), 0.0, after),
        &[60.0, -24.5],
        "offset",
    );
    // continue: slope of the last frame of animation, carried 5 frames.
    same_vec(
        &looped(&keys, 25.0, Some("continue"), 0.0, after),
        &[60.0, -24.5],
        "continue",
    );
    // keyframes = 1 repeats only the last segment 10..20.
    same_vec(
        &looped(&keys, 25.0, None, 1.0, after),
        &[40.0, -14.5],
        "last segment",
    );
    // loopBefore mirrors from the first key.
    same_vec(
        &looped(&keys, -5.0, None, 0.0, before),
        &[40.0, -14.5],
        "before",
    );
    same_vec(
        &looped(&keys, -5.0, Some("bogus"), 1.0, before),
        &[20.0, -4.5],
        "unknown mode cycles first segment",
    );
    // Fewer than two keys, or an empty segment, reads the raw frame.
    same_vec(
        &looped(&[3.0], 9.0, None, 0.0, after),
        &[28.0, -8.5],
        "one key",
    );
    same_vec(
        &looped(&[4.0, 4.0], 9.0, None, 0.0, after),
        &[28.0, -8.5],
        "zero span",
    );
}

#[test]
fn javascript_or_zero_and_count_coercions_are_kept() {
    // `vi[c] || 0` turns NaN and -0 into +0 before the subtraction.
    let plan = LoopPlan::Continue {
        edge: 1.0,
        inner: 0.0,
        dist: 2.0,
    };
    let v = loop_value(plan, |f| if f == 1.0 { vec![3.0] } else { vec![f64::NAN] });
    same_vec(&v, &[9.0], "NaN inner reads 0");
    let short = loop_value(plan, |f| if f == 1.0 { vec![3.0, 4.0] } else { vec![1.0] });
    same_vec(&short, &[7.0, 12.0], "missing inner component reads 0");
    // A non-finite or negative count means the whole track.
    let keys = [0.0, 10.0, 20.0];
    for n in [f64::NAN, f64::INFINITY, -3.0, 0.9] {
        assert_eq!(
            loop_plan(&keys, 25.0, LoopMode::Cycle, n, LoopSide::After),
            LoopPlan::Repeat {
                sample: 5.0,
                offset: None
            },
            "count {n}"
        );
    }
    // A count at or past the key count also means the whole track.
    assert_eq!(
        loop_plan(&keys, 25.0, LoopMode::Cycle, 3.0, LoopSide::After),
        loop_plan(&keys, 25.0, LoopMode::Cycle, 0.0, LoopSide::After),
    );
    assert_eq!(LoopMode::from_js(Some("CONTINUE")), LoopMode::Continue);
    assert_eq!(LoopMode::from_js(Some("ping pong")), LoopMode::Cycle);
}

// Generated from `src/js/domain/animation/expression-time.js` and the verbatim
// `loopRaw` body of `src/js/motion.js` at `codex/native-remediation`
// 5fc7e31291dd3cb8fab974d8464df388c9c0d577 (N22F) against a stub track whose
// component `c` is `f * m[c] + k[c]`. One call per line; values are IEEE 754
// bit patterns in hex (`s<bits>` scalar, `v<bits>,...` vector).
const ORACLE: &str = include_str!("fixtures/expression-time/oracle.txt");

fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn scalar(token: &str) -> f64 {
    hex(token.strip_prefix('s').expect("scalar token"))
}

fn list(token: &str, tag: char) -> Vec<f64> {
    let body = token.strip_prefix(tag).expect("tagged list");
    if body.is_empty() {
        return Vec::new();
    }
    body.split(',').map(hex).collect()
}

fn decimal_list(token: &str) -> Vec<f64> {
    let body = token.strip_prefix('k').expect("key list");
    if body.is_empty() {
        return Vec::new();
    }
    body.split(',')
        .map(|x| x.parse().expect("key frame"))
        .collect()
}

#[test]
fn generated_oracle_matches_javascript_bit_for_bit() {
    let (mut rows, mut repeats, mut continues, mut raws) = (0, 0, 0, 0);
    for (line_no, row) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = row.split(' ').collect();
        let label = format!("oracle line {}: {row}", line_no + 1);
        match t[0] {
            "stepTime" => same(step_time(scalar(t[1]), scalar(t[2])), scalar(t[3]), &label),
            "toFrames" => same(to_frames(scalar(t[1]), scalar(t[2])), scalar(t[3]), &label),
            "toSeconds" => same(to_seconds(scalar(t[1]), scalar(t[2])), scalar(t[3]), &label),
            "loop" => {
                let side = if t[1] == "after" {
                    LoopSide::After
                } else {
                    LoopSide::Before
                };
                let mode = (t[2] != "-").then_some(t[2]);
                let keys = decimal_list(t[5]);
                let (m, k) = (list(t[6], 'm'), list(t[7], 'c'));
                let plan = loop_plan(
                    &keys,
                    scalar(t[4]),
                    LoopMode::from_js(mode),
                    scalar(t[3]),
                    side,
                );
                match plan {
                    LoopPlan::Raw(_) => raws += 1,
                    LoopPlan::Continue { .. } => continues += 1,
                    LoopPlan::Repeat { .. } => repeats += 1,
                }
                same_vec(&loop_value(plan, line(&m, &k)), &list(t[8], 'v'), &label);
            }
            op => panic!("unknown oracle op {op}"),
        }
        rows += 1;
    }
    assert_eq!(rows, 300, "oracle row count");
    assert!(
        raws > 20 && continues > 10 && repeats > 50,
        "branch coverage {raws}/{continues}/{repeats}"
    );
}
