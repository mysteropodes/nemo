//! Oracle tests for the component playback frame kernel (L04-A, I15/D33).
//!
//! `fixtures/object-component-playback/oracle.txt` (`ORACLE`, sha256
//! 425766fda32bd2a58d4475aab100192c72b454fac8104a44f94d6c9b6057bda2) was produced by
//! `tools/gen_oracle.cjs` of the L04-A leaf (generator sha256
//! db4b17cee8f061cfe250d8032617db4cdcf32238d198102d8d8b1357162606a8), which cuts `resolveSymbolFrameIdx`
//! and the blank-hold loop out of `src/js/app.js` and the `componentFrame`
//! write out of `src/js/timeline.js` at baseline `fbe4b8a3` and runs them
//! verbatim in a Node `vm`; every resolve row without outer keys or Time Remap
//! is also checked there against `symbolFrameAt` of `tests/fixtures/lib/reference.cjs`.
//! The fixture header carries the baseline blob ids.
//!
//! Rows are space separated; `-` is undefined, `~` a null entry of the outer
//! `frames[]`, `""` the empty mode label. Numbers are JavaScript decimal
//! strings (shortest round trip, so exact). Row kinds: `R` resolve, `B` held
//! blank, `O` blank-then-resolve outcome (`b` = blank), `A` componentFrame write.
//! A raw result compares with `==` (so `-0` is `0`) except that two `NaN`s match.

use super::{
    evaluate_component_playback, is_blank_held, nearest_outer_key, resolve_component_frame,
    OuterKey, PlayMode, PlaybackInput, PlaybackOutcome, ResolvedFrame,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-playback/oracle.txt");

pub(crate) struct Row {
    pub(crate) input: PlaybackInput,
    pub(crate) total: f64,
    pub(crate) frame: i64,
    pub(crate) remap: Option<f64>,
}

pub(crate) fn num(tok: &str) -> Option<f64> {
    if tok == "-" {
        None
    } else {
        Some(
            tok.parse::<f64>()
                .unwrap_or_else(|_| panic!("number {tok}")),
        )
    }
}

pub(crate) fn key(tok: &str) -> OuterKey {
    if tok == "~" {
        return OuterKey::default();
    }
    let p: Vec<&str> = tok.split(':').collect();
    OuterKey {
        is_keyframe: p[0] == "1",
        component_frame: if p[1] == "n" { None } else { num(p[1]) },
        blank_override: match p[2] {
            "-" => None,
            "0" => Some(false),
            "1" => Some(true),
            other => panic!("blank token {other}"),
        },
    }
}

pub(crate) fn keys(tok: &str) -> Vec<OuterKey> {
    if tok == "-" {
        Vec::new()
    } else {
        tok.split(',').map(key).collect()
    }
}

/// Ten fields: mode speed placedAt single trimIn trimOut total frame remap keys.
pub(crate) fn row(f: &[&str]) -> Row {
    let mode = match f[0] {
        "-" => None,
        "\"\"" => Some(""),
        label => Some(label),
    };
    Row {
        input: PlaybackInput {
            play_mode: PlayMode::from_js(mode),
            speed: num(f[1]),
            placed_at: num(f[2]),
            single_frame: num(f[3]),
            trim_in: num(f[4]),
            trim_out: num(f[5]),
            outer_keys: keys(f[9]),
        },
        total: num(f[6]).expect("total"),
        frame: f[7].parse::<i64>().expect("frame"),
        // `null` is a timeRemap flag whose sampled value is null: the port is None.
        remap: if f[8] == "null" { None } else { num(f[8]) },
    }
}

pub(crate) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(' ').collect::<Vec<&'static str>>())
        .filter(|f| f[0] == tag)
        .collect()
}

pub(crate) fn same(actual: f64, expected: f64) -> bool {
    (actual.is_nan() && expected.is_nan()) || actual == expected
}

pub(crate) fn resolved(r: &Row) -> f64 {
    resolve_component_frame(&r.input, r.total, r.frame, r.remap).raw()
}

/// Hand-picked cases whose literal results come from the baseline function
/// (Node `vm`, same extraction as the generator). Each is the case a named
/// deviation in `object_component_playback_controls.rs` must get wrong.
/// Fields: mode speed placedAt single trimIn trimOut total frame remap keys.
pub(crate) static PINNED: [(&str, [&str; 10], f64); 13] = [
    (
        "operand_order",
        ["-", "3", "2", "-", "-", "-", "100", "12", "-", "-"],
        30.0,
    ),
    (
        "negative_elapsed",
        ["-", "-", "6", "-", "-", "-", "5", "2", "-", "-"],
        0.0,
    ),
    (
        "speed_zero",
        ["-", "0", "-", "-", "-", "-", "5", "3", "-", "-"],
        3.0,
    ),
    (
        "pingpong_cycle",
        ["pingpong", "-", "-", "-", "-", "-", "4", "5", "-", "-"],
        1.0,
    ),
    (
        "pingpong_short",
        ["pingpong", "-", "-", "-", "-", "-", "1", "7", "-", "-"],
        0.0,
    ),
    (
        "once_clamp",
        ["once", "-", "-", "-", "-", "-", "5", "9", "-", "-"],
        4.0,
    ),
    (
        "once_trim_out_zero",
        ["once", "-", "-", "-", "0", "0", "10", "4", "-", "-"],
        0.0,
    ),
    (
        "single_clamp",
        ["single", "-", "-", "99", "-", "-", "5", "0", "-", "-"],
        4.0,
    ),
    (
        "total_clamp",
        ["-", "-", "-", "-", "-", "-", "0", "3", "-", "-"],
        0.0,
    ),
    (
        "held_break",
        [
            "once",
            "-",
            "-",
            "-",
            "-",
            "-",
            "9",
            "3",
            "-",
            "1:6:-,~,1:-:-,~",
        ],
        3.0,
    ),
    (
        "remap_vs_component",
        ["single", "-", "-", "3", "-", "-", "5", "0", "4", "1:1:-"],
        1.0,
    ),
    (
        "remap_round_half_down",
        [
            "single",
            "9",
            "-",
            "3",
            "-",
            "-",
            "5",
            "3",
            "0.49999999999999994",
            "-",
        ],
        0.0,
    ),
    (
        "remap_tie_up",
        ["single", "9", "-", "3", "-", "-", "5", "3", "0.5", "-"],
        1.0,
    ),
];

fn raw(mode: &str, total: f64, frame: i64, set: &[(usize, &str)]) -> f64 {
    let mut f = ["-"; 10];
    f[0] = mode;
    f[6] = "1";
    f[7] = "0";
    for (i, v) in set {
        f[*i] = *v;
    }
    let mut r = row(&f);
    r.total = total;
    r.frame = frame;
    resolved(&r)
}

#[test]
fn resolve_rows_match_baseline() {
    let all = rows("R");
    assert_eq!(all.len(), 2099);
    for f in &all {
        let r = row(&f[1..11]);
        let expected = num(f[11]).expect("expected");
        let got = resolved(&r);
        assert!(
            same(got, expected),
            "{}: {got:?} != {expected:?}",
            f.join(" ")
        );
    }
}

#[test]
fn held_blank_rows_match_baseline() {
    let all = rows("B");
    assert_eq!(all.len(), 224);
    for f in &all {
        let got = is_blank_held(&keys(f[2]), f[1].parse::<i64>().expect("frame"));
        assert_eq!(got, f[3] == "1", "{}", f.join(" "));
    }
}

#[test]
fn outcome_rows_match_baseline() {
    let all = rows("O");
    assert_eq!(all.len(), 448);
    let mut blanks = 0;
    for f in &all {
        let r = row(&f[1..11]);
        let got = evaluate_component_playback(&r.input, r.total, r.frame, r.remap);
        match (got, f[11]) {
            (PlaybackOutcome::Blank, "b") => blanks += 1,
            (PlaybackOutcome::Frame(v), tok) if tok != "b" => {
                assert!(
                    same(v.raw(), num(tok).expect("expected")),
                    "{}",
                    f.join(" ")
                )
            }
            (other, _) => panic!("{}: got {other:?}", f.join(" ")),
        }
    }
    assert!(
        blanks > 0 && blanks < all.len(),
        "both outcomes occur: {blanks}"
    );
}

#[test]
fn component_frame_write_rows_match_baseline() {
    let all = rows("A");
    assert_eq!(all.len(), 54);
    for f in &all {
        let mut k = key(&format!("{}:{}:{}", f[1], f[2], f[3]));
        let applied = k.assign_component_frame(num(f[4]).expect("picked"));
        assert_eq!(applied, f[1] == "1", "{}", f.join(" "));
        let after = key(&format!("{}:{}:{}", f[5], f[6], f[7]));
        assert_eq!(k, after, "{}", f.join(" "));
    }
}

#[test]
fn pinned_literals_match_baseline_and_the_oracle() {
    let all = rows("R");
    for (name, fields, expected) in PINNED.iter() {
        let got = resolved(&row(fields));
        assert!(same(got, *expected), "{name}: {got:?} != {expected:?}");
        // The literal is not a private number: the generator emits the same
        // case as an oracle row and the baseline function produced its value.
        let hit = all.iter().find(|f| f[1..11] == fields[..]);
        let oracle = hit.map(|f| num(f[11]).expect("oracle value"));
        assert!(
            oracle.is_some_and(|v| same(v, *expected)),
            "{name} oracle row"
        );
    }
}

#[test]
fn modes_play_the_baseline_formulas() {
    // loop: floor(elapsed) % total, elapsed = max(0, frame - placedAt) * speed.
    assert_eq!(raw("-", 5.0, 9, &[(1, "1.5")]), 3.0);
    assert_eq!(raw("loop", 7.0, 9, &[(1, "0.5")]), 4.0);
    assert_eq!(raw("-", 7.0, 9, &[(2, "100"), (1, "3")]), 0.0);
    // once: min(trimOut, trimIn + floor(elapsed)); trimOut defaults to total - 1.
    assert_eq!(raw("once", 10.0, 9, &[(4, "2"), (5, "6")]), 6.0);
    assert_eq!(raw("once", 10.0, 2, &[(4, "2"), (5, "6")]), 4.0);
    assert_eq!(raw("once", 9.0, 9, &[(5, "2.5")]), 2.5);
    // pingpong: cycle (total - 1) * 2, a total below two holds frame 0.
    let sweep: Vec<f64> = (0..9).map(|f| raw("pingpong", 4.0, f, &[])).collect();
    assert_eq!(sweep, vec![0.0, 1.0, 2.0, 3.0, 2.0, 1.0, 0.0, 1.0, 2.0]);
    assert_eq!(raw("pingpong", 0.0, 7, &[]), 0.0);
    // single: floor(singleFrame) clamped to the component.
    assert_eq!(raw("single", 5.0, 50, &[(3, "2.7")]), 2.0);
    assert_eq!(raw("single", 5.0, 0, &[(3, "-3")]), 0.0);
    // lfsSettings-shaped input: no outer keys, same fields, same kernel.
    assert_eq!(raw("pingpong", 6.0, 11, &[(1, "1.5"), (2, "2")]), 3.0);
    assert_eq!(raw("once", 6.0, 20, &[(1, "0.5"), (4, "1"), (5, "4")]), 4.0);
    // Unknown labels, including a different case, play as a loop.
    assert_eq!(raw("bogus", 5.0, 7, &[]), 2.0);
    assert_eq!(raw("Once", 5.0, 7, &[]), 2.0);
}

#[test]
fn held_component_frame_wins_until_the_next_outer_key() {
    let held = |spec: &str, frame: i64| raw("once", 9.0, frame, &[(9, spec)]);
    assert_eq!(held("1:6:-,~,~,~", 3), 6.0);
    // The nearest keyframe wins even without a componentFrame.
    assert_eq!(held("1:6:-,~,1:-:-,~", 3), 3.0);
    assert_eq!(held("1:6:-,~,1:1:-,~", 3), 1.0);
    // A non-keyframe entry never carries a hold.
    assert_eq!(held("1:6:-,0:2:-,~,~", 3), 6.0);
    // A held 0 is a real pick, not "absent"; null reads as absent.
    assert_eq!(raw("-", 9.0, 7, &[(9, "1:0:-")]), 0.0);
    assert_eq!(held("1:n:-", 3), 3.0);
    // Clamped into the component, fractions floored, scan starts at len - 1.
    assert_eq!(raw("-", 5.0, 0, &[(9, "1:99:-")]), 4.0);
    assert_eq!(raw("-", 0.0, 0, &[(9, "1:99:-")]), 0.0);
    assert_eq!(raw("-", -3.0, 0, &[(9, "1:99:-")]), 0.0);
    assert_eq!(raw("-", 9.0, 500, &[(9, "1:2.9:-")]), 2.0);
    // Before the first key (or with a negative playhead) nothing is held.
    assert_eq!(held("0:6:-,1:5:-", 0), 0.0);
    assert_eq!(held("1:6:-", -1), 0.0);
}

#[test]
fn time_remap_overrides_modes_but_not_a_held_component_frame() {
    let remap = |rv: &str| raw("single", 5.0, 3, &[(1, "9"), (3, "3"), (8, rv)]);
    assert_eq!(remap("0.5"), 1.0);
    assert_eq!(remap("2.5"), 3.0);
    assert_eq!(remap("0.49999999999999994"), 0.0);
    assert_eq!(remap("7.4"), 4.0);
    assert_eq!(remap("-3"), 0.0);
    // None (absent, or a sampler that returned null) leaves the mode alone.
    assert_eq!(raw("single", 5.0, 3, &[(3, "3"), (8, "null")]), 3.0);
    assert_eq!(raw("single", 5.0, 3, &[(3, "3")]), 3.0);
    // The held key is read first; the remap then applies to everything else.
    assert_eq!(
        raw("single", 5.0, 0, &[(3, "3"), (8, "4"), (9, "1:1:-")]),
        1.0
    );
    assert_eq!(raw("single", 5.0, 3, &[(3, "1"), (8, "4")]), 4.0);
    assert_eq!(raw("-", 1.0, 0, &[(8, "4")]), 0.0);
    assert_eq!(raw("-", 0.0, 0, &[(8, "4")]), 0.0);
}

#[test]
fn blank_hold_is_held_and_distinguishes_absent_false_true() {
    let blank = |spec: &str, frame: i64| is_blank_held(&keys(spec), frame);
    assert!(blank("1:-:1,~,~", 0) && blank("1:-:1,~,~", 2) && blank("1:-:1,~,~", 10));
    // A later keyframe ends the hold, whatever it says about the component.
    assert!(blank("1:-:1,~,1:-:-,~", 1));
    assert!(!blank("1:-:1,~,1:-:-,~", 2) && !blank("1:-:1,~,1:-:-,~", 3));
    assert!(!blank("1:-:1,1:-:0", 1));
    // absent and false do not hide; a non-keyframe flag is ignored.
    assert!(!blank("1:-:-", 0) && !blank("1:-:0", 0) && !blank("0:-:1", 0));
    assert!(blank("1:-:1,0:-:0", 1));
    assert!(!blank("-", 0) && !blank("1:-:1", -1));
    assert_eq!(
        nearest_outer_key(&keys("0:-:-,1:2:-,~"), 5),
        Some(&key("1:2:-"))
    );
    assert_eq!(nearest_outer_key(&keys("0:-:-,~"), 5), None);
}

#[test]
fn blank_beats_the_held_component_frame_and_a_later_key_restores_playback() {
    let at = |spec: &str, frame: i64| {
        let r = row(&["-", "-", "-", "-", "-", "-", "9", "0", "-", spec]);
        evaluate_component_playback(&r.input, r.total, frame, None)
    };
    let shown = |v: f64| PlaybackOutcome::Frame(ResolvedFrame(v));
    assert_eq!(at("1:3:1", 0), PlaybackOutcome::Blank);
    assert_eq!(at("1:3:0", 0), shown(3.0));
    assert_eq!(at("1:-:1,~,1:2:-", 1), PlaybackOutcome::Blank);
    assert_eq!(at("1:-:1,~,1:2:-", 2), shown(2.0));
}

#[test]
fn assigning_a_component_frame_clears_the_blank_flag_on_keyframes_only() {
    let mut k = key("1:-:1");
    assert!(k.assign_component_frame(4.0));
    assert_eq!(
        k,
        OuterKey {
            is_keyframe: true,
            component_frame: Some(4.0),
            blank_override: None
        }
    );
    let mut f = key("1:-:0");
    assert!(f.assign_component_frame(0.0));
    assert_eq!(f.blank_override, None);
    let mut plain = key("0:2:1");
    assert!(!plain.assign_component_frame(7.0));
    assert_eq!(plain, key("0:2:1"));
    // The written 0 holds frame 0 even in a loop at frame 7.
    let r = row(&["-", "-", "-", "-", "-", "-", "9", "7", "-", "-"]);
    let mut input = r.input.clone();
    input.outer_keys = vec![f];
    assert_eq!(resolve_component_frame(&input, 9.0, 7, None).raw(), 0.0);
}

#[test]
fn resolved_frames_read_like_javascript_array_indexes() {
    assert_eq!(ResolvedFrame(4.0).index(5), Some(4));
    assert_eq!(ResolvedFrame(4.0).index(4), None);
    assert_eq!(ResolvedFrame(-0.0).index(1), Some(0));
    assert_eq!(ResolvedFrame(-3.0).index(5), None);
    assert_eq!(ResolvedFrame(2.5).index(5), None);
    assert_eq!(ResolvedFrame(f64::NAN).index(5), None);
    assert_eq!(ResolvedFrame(f64::INFINITY).index(5), None);
    // A negative speed reaches negative positions exactly as the baseline does.
    assert_eq!(raw("-", 5.0, 3, &[(1, "-1")]), -3.0);
    assert_eq!(
        resolve_component_frame(&PlaybackInput::default(), 5.0, 3, None).index(5),
        Some(3)
    );
    assert_eq!(ResolvedFrame(raw("-", 5.0, 3, &[(1, "-1")])).index(5), None);
    // NaN inputs propagate as in JavaScript instead of being clamped away.
    assert!(raw("-", 5.0, 0, &[(9, "1:NaN:-")]).is_nan());
    assert!(raw("single", 5.0, 3, &[(8, "NaN")]).is_nan());
    assert_eq!(raw("single", 5.0, 3, &[(3, "NaN")]), 0.0);
}

#[test]
fn play_mode_labels_are_exact() {
    assert_eq!(PlayMode::from_js(Some("single")), PlayMode::Single);
    assert_eq!(PlayMode::from_js(Some("once")), PlayMode::Once);
    assert_eq!(PlayMode::from_js(Some("pingpong")), PlayMode::PingPong);
    assert_eq!(PlayMode::from_js(Some("loop")), PlayMode::Loop);
    assert_eq!(PlayMode::from_js(Some("PingPong")), PlayMode::Loop);
    assert_eq!(PlayMode::from_js(Some("")), PlayMode::Loop);
    assert_eq!(PlayMode::from_js(None), PlayMode::Loop);
    assert_eq!(PlayMode::default(), PlayMode::Loop);
}
