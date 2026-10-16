//! Oracle tests for the duplicator count and mode-offset kernel (L04-B, I07).
//!
//! `fixtures/duplicator-placement-modes/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-B leaf, which cuts `_duplicatorCount`,
//! `_duplicatorModeOffset` and `_resolveDuplicatorPath` out of `src/js/app.js`
//! at baseline `fbe4b8a3` and runs them verbatim in a Node `vm` over the
//! shipped Paper.js 0.12.18 (`src/paper-full.min.js`). The fixture header
//! carries the baseline blob ids and the row grammar.
//!
//! Numbers are IEEE 754 bit patterns in hex, `u` is undefined and `n` null.
//! Row kinds: `cnt` count and column clamp, `off` grid/radial offsets (and the
//! throws of a path-like mode without a path), `cand` a candidate layer,
//! `rsv` `_resolveDuplicatorPath` over it, `pth` path-mode offsets.
//!
//! Tolerance: grid offsets and every path position are `+ - * /` and `sqrt`
//! only, so they are compared bit for bit (`-0` included). Only values that
//! pass through `cos`, `sin` or `atan2` (radial dx/dy, path rotation) may
//! differ by `TRIG_ULPS` units in the last place, because libm is not
//! V8's fdlibm; `rot` of a radial row is `ang` itself and stays exact.

use super::{
    dup_mode, duplicator_count, duplicator_count_with_caps, grid_cols, mode_offset, resolve_path,
    DupMode, DuplicatorSpec, JsNumber, ModeOffset, ModeOffsetError, PathInfo, PathResolveError,
    PathSource, PathStroke,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use std::collections::HashMap;

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-modes/oracle.txt");
pub(crate) const TRIG_ULPS: u64 = 4;

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

/// Bit pattern with one canonical NaN.
pub(crate) fn bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

/// Distance in units in the last place on the ordered line (`-0` and `+0` meet).
pub(crate) fn ulps(a: f64, b: f64) -> u64 {
    let ordered = |x: f64| {
        let v = x.to_bits() as i64;
        if v < 0 {
            i64::MIN - v
        } else {
            v
        }
    };
    ordered(a).abs_diff(ordered(b))
}

/// Equal bit for bit (`tol == 0`) or within `tol` ulps; two NaNs match.
pub(crate) fn close(actual: f64, expected: f64, tol: u64) -> bool {
    if actual.is_nan() || expected.is_nan() {
        return actual.is_nan() && expected.is_nan();
    }
    if tol == 0 {
        actual.to_bits() == expected.to_bits()
    } else {
        ulps(actual, expected) <= tol
    }
}

pub(crate) fn js(tok: &str) -> JsNumber {
    match tok {
        "u" => JsNumber::Undefined,
        "n" => JsNumber::Null,
        h => JsNumber::Value(hex(h)),
    }
}

/// `-` undefined and `nl` null both leave the label absent; `~` is the empty label.
pub(crate) fn label(tok: &str) -> Option<&str> {
    match tok {
        "-" | "nl" => None,
        "~" => Some(""),
        m => Some(m),
    }
}

/// The rows of one kind, tokens after the kind word.
pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| l.split_whitespace().next() == Some(kind))
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect()
}

/// Fields mode rows cols count spx spy start end radius orient align.
pub(crate) fn spec_of<'a>(t: &[&'a str]) -> DuplicatorSpec<'a> {
    DuplicatorSpec {
        mode: label(t[0]),
        rows: js(t[1]),
        cols: js(t[2]),
        count: js(t[3]),
        spacing_x: js(t[4]),
        spacing_y: js(t[5]),
        start_angle: js(t[6]),
        end_angle: js(t[7]),
        radius: js(t[8]),
        radial_orient: t[9] == "1",
        path_align_tangent: t[10] == "1",
    }
}

/// One `_duplicatorModeOffset` call exactly as `applyLayerDuplicator` makes it.
#[derive(Clone, Copy)]
pub(crate) struct Probe {
    pub spec: DuplicatorSpec<'static>,
    pub k: u32,
    pub count: f64,
    pub cols: f64,
}

impl Probe {
    pub(crate) fn from_spec(spec: DuplicatorSpec<'static>, k: u32) -> Probe {
        Probe {
            spec,
            k,
            count: duplicator_count(&spec),
            cols: grid_cols(&spec),
        }
    }

    pub(crate) fn offset(&self, path: Option<&PathInfo>) -> Result<ModeOffset, ModeOffsetError> {
        mode_offset(&self.spec, self.k, self.count, self.cols, path)
    }
}

/// Compare one result with oracle tokens (`throw` or three bit patterns).
pub(crate) fn check3(
    out: Result<ModeOffset, ModeOffsetError>,
    want: &[&str],
    tol: [u64; 3],
) -> Result<(), String> {
    if want == ["throw"] {
        return match out {
            Err(ModeOffsetError::PathInfoRequired) => Ok(()),
            other => Err(format!("expected a throw, got {other:?}")),
        };
    }
    let got = out.map_err(|e| format!("unexpected {e:?}"))?;
    let have = [got.base_dx, got.base_dy, got.base_rot];
    for (i, ((h, w), t)) in have.iter().zip(want).zip(tol).enumerate() {
        let expected = hex(w);
        if !close(*h, expected, t) {
            return Err(format!("field {i}: {h} vs {expected}"));
        }
    }
    Ok(())
}

pub(crate) fn tolerance(mode: DupMode) -> [u64; 3] {
    match mode {
        DupMode::Radial => [TRIG_ULPS, TRIG_ULPS, 0],
        _ => [0, 0, 0],
    }
}

/// Mismatching `cnt` rows when the kernel is given these ceilings.
pub(crate) fn cnt_mismatches(grid_cap: f64, other_cap: f64) -> Vec<String> {
    let mut bad = Vec::new();
    for t in rows("cnt") {
        let spec = DuplicatorSpec {
            mode: label(t[0]),
            rows: js(t[1]),
            cols: js(t[2]),
            count: js(t[3]),
            ..DuplicatorSpec::default()
        };
        let count = duplicator_count_with_caps(&spec, grid_cap, other_cap);
        let cols = grid_cols(&spec);
        if bits(count) != bits(hex(t[5])) || bits(cols) != bits(hex(t[6])) {
            bad.push(format!("cnt {}", t.join(" ")));
        }
    }
    bad
}

/// Mismatching `off` rows after `tweak` changed the inputs of each probe.
pub(crate) fn off_mismatches(tweak: &dyn Fn(&mut Probe)) -> Vec<String> {
    let mut bad = Vec::new();
    for t in rows("off") {
        let mut probe = Probe::from_spec(spec_of(&t[..11]), t[11].parse().expect("k"));
        tweak(&mut probe);
        let tol = tolerance(dup_mode(probe.spec.mode));
        if let Err(e) = check3(probe.offset(None), &t[13..], tol) {
            bad.push(format!("off {}: {e}", t.join(" ")));
        }
    }
    bad
}

pub(crate) struct OwnedStroke {
    pub is_raster: bool,
    pub segments: Option<Vec<SegmentIn>>,
    pub closed: bool,
}

pub(crate) struct OwnedCandidate {
    pub has_duplicator: bool,
    pub strokes: Vec<OwnedStroke>,
}

fn pair(t: &str) -> [f64; 2] {
    let (x, y) = t.split_once(',').expect("pair");
    [hex(x), hex(y)]
}

fn handle(t: &str) -> Option<[f64; 2]> {
    if t == "-" {
        None
    } else {
        Some(pair(t))
    }
}

fn parse_stroke(s: &str) -> OwnedStroke {
    match s.as_bytes()[0] {
        b'R' => OwnedStroke {
            is_raster: true,
            segments: None,
            closed: false,
        },
        b'N' => OwnedStroke {
            is_raster: false,
            segments: None,
            closed: false,
        },
        b'P' => {
            let (closed, segs) = s[1..].split_once(':').expect("stroke");
            let segments = segs
                .split(';')
                .map(|g| {
                    let part: Vec<&str> = g.split('/').collect();
                    SegmentIn {
                        point: pair(part[0]),
                        handle_in: handle(part[1]),
                        handle_out: handle(part[2]),
                    }
                })
                .collect();
            OwnedStroke {
                is_raster: false,
                segments: Some(segments),
                closed: closed == "1",
            }
        }
        other => panic!("stroke kind {other}"),
    }
}

/// `-` is a uid that does not resolve.
pub(crate) fn parse_candidate(tok: &str) -> Option<OwnedCandidate> {
    if tok == "-" {
        return None;
    }
    let (head, body) = tok.split_once('@').expect("candidate");
    let strokes = if body.is_empty() {
        Vec::new()
    } else {
        body.split('|').map(parse_stroke).collect()
    };
    Some(OwnedCandidate {
        has_duplicator: head == "D1",
        strokes,
    })
}

pub(crate) fn resolve_owned(
    c: &Option<OwnedCandidate>,
) -> Result<Option<PathInfo>, PathResolveError> {
    match c {
        None => resolve_path(None),
        Some(c) => {
            let strokes: Vec<PathStroke> = c
                .strokes
                .iter()
                .map(|s| PathStroke {
                    is_raster: s.is_raster,
                    segments: s.segments.as_deref(),
                    closed: s.closed,
                })
                .collect();
            resolve_path(Some(&PathSource {
                has_duplicator: c.has_duplicator,
                strokes: &strokes,
            }))
        }
    }
}

pub(crate) fn candidates() -> HashMap<&'static str, Option<OwnedCandidate>> {
    rows("cand")
        .into_iter()
        .map(|t| (t[0], parse_candidate(t[1])))
        .collect()
}

/// The resolved path of every candidate that resolves.
pub(crate) fn infos() -> HashMap<&'static str, PathInfo> {
    candidates()
        .iter()
        .filter_map(|(name, c)| resolve_owned(c).ok().flatten().map(|i| (*name, i)))
        .collect()
}

/// Mismatching `pth` rows after `tweak` changed the inputs of each probe.
pub(crate) fn pth_mismatches(tweak: &dyn Fn(&mut Probe)) -> Vec<String> {
    let infos = infos();
    let mut bad = Vec::new();
    for t in rows("pth") {
        let spec = DuplicatorSpec {
            mode: label(t[1]),
            count: js(t[2]),
            path_align_tangent: t[3] == "1",
            ..DuplicatorSpec::default()
        };
        let mut probe = Probe::from_spec(spec, t[4].parse().expect("k"));
        tweak(&mut probe);
        if let Err(e) = check3(probe.offset(infos.get(t[0])), &t[6..], [0, 0, TRIG_ULPS]) {
            bad.push(format!("pth {}: {e}", t.join(" ")));
        }
    }
    bad
}

fn assert_none(what: &str, bad: Vec<String>) {
    assert!(
        bad.is_empty(),
        "{what}: {} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn count_and_cols_rows_match_baseline() {
    assert_eq!(rows("cnt").len(), 343);
    assert_none("cnt", cnt_mismatches(900.0, 500.0));
}

#[test]
fn grid_and_radial_rows_match_baseline() {
    let all = rows("off");
    assert_eq!(all.len(), 1377);
    let radial = all.iter().filter(|t| t[0] == "radial").count();
    assert_eq!(radial, 750);
    assert_none("off", off_mismatches(&|_| {}));
}

#[test]
fn a_path_like_mode_without_a_path_refuses_where_the_baseline_throws() {
    let throws: Vec<_> = rows("off")
        .into_iter()
        .filter(|t| t[13] == "throw")
        .collect();
    assert_eq!(throws.len(), 6);
    for t in throws {
        let probe = Probe::from_spec(spec_of(&t[..11]), t[11].parse().expect("k"));
        assert_eq!(
            probe.offset(None),
            Err(ModeOffsetError::PathInfoRequired),
            "{t:?}"
        );
    }
}

#[test]
fn path_resolution_rows_match_baseline() {
    let all = rows("rsv");
    assert_eq!(all.len(), 33);
    let cands = candidates();
    for t in all {
        let got = resolve_owned(&cands[t[0]]);
        let want = &t[2..];
        let ok = match (&got, want) {
            (Err(PathResolveError::MissingHandle), ["throw"]) => true,
            (Ok(None), ["-"]) => true,
            (Ok(Some(i)), [len, x, y, closed]) => {
                bits(i.length) == bits(hex(len))
                    && bits(i.start[0]) == bits(hex(x))
                    && bits(i.start[1]) == bits(hex(y))
                    && i.closed == (*closed == "1")
            }
            _ => false,
        };
        assert!(ok, "rsv {} => {want:?}, got {got:?}", t[0]);
    }
}

#[test]
fn path_mode_rows_match_baseline() {
    assert_eq!(rows("pth").len(), 1200);
    assert_none("pth", pth_mismatches(&|_| {}));
}
