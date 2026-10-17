//! Scenario codec and runner shared by the apply and temporal oracle tests.
//!
//! A scenario (`scn` .. `end`, see the headers of the two fixtures) is one
//! call of the verbatim baseline `applyLayerDuplicator`: its inputs, every
//! placement descriptor it received, every resample it made and the strokes
//! it returned. [`run`] replays the inputs through the real kernel
//! (`build_copies_with`) with the logged descriptors behind the placement
//! port and the logged resamples behind the resample port; [`mismatch`]
//! compares everything the kernel produced with what the baseline returned.

use super::{
    build_copies_with, BuildError, CopyRequest, SpanBounds, TemporalOffset, TemporalRules,
};
use crate::evaluation::object_component_kernels::duplicator_placement_apply::{
    Dup3d, DuplicatorCopy, PlacementApply, UnavailableField,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::truthy;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    close, fill_matches, hex, parse_list, parse_wants, same, segments_of, Want,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{Descriptor, Pivot};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::object_document::ObjectRecord;

/// Relative tolerance of rows that went through `sin`/`cos`.
pub(crate) const TRIG: f64 = 1e-9;

/// One logged call of `_duplicatorClonePlacement`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LogRow {
    pub k: u32,
    pub pivot: Pivot,
    pub place: PlacementApply,
}

#[derive(Debug, Clone)]
pub(crate) struct Scenario {
    pub name: String,
    pub seed: JsNumber,
    pub count: f64,
    pub frame: f64,
    pub canvas_w: f64,
    pub canvas_h: f64,
    pub can_resample: bool,
    /// `(enabled, offsetFrames, direction)` of `dup.timeOffset`, absent as `None`.
    pub temporal: Option<(bool, f64, Option<String>)>,
    pub span: SpanBounds,
    pub base: Vec<ObjectRecord>,
    pub table: Vec<(u64, Vec<ObjectRecord>)>,
    pub calls: Vec<f64>,
    pub log: Vec<LogRow>,
    /// `None` is `out same` (the base is returned untouched).
    pub out: Option<Vec<Want>>,
}

fn js_number(tok: &str) -> JsNumber {
    match tok {
        "u" => JsNumber::Undefined,
        "n" => JsNumber::Null,
        h => JsNumber::Value(hex(h)),
    }
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("cfg field {key} in {line}"))
}

fn parse_log(t: &[&str]) -> LogRow {
    let n: Vec<f64> = t[3..10].iter().map(|h| hex(h)).collect();
    LogRow {
        k: t[0].parse().expect("k"),
        pivot: Pivot {
            x: hex(t[1]),
            y: hex(t[2]),
        },
        place: PlacementApply {
            matrix: Descriptor {
                dx: n[0],
                dy: n[1],
                rot: n[2],
                sx: n[3],
                sy: n[4],
            },
            opacity_factor: n[5],
            hue_deg: n[6],
            dup_3d: if t[10] == "-" {
                None
            } else {
                let v: Vec<f64> = t[10].split(':').map(hex).collect();
                Some(Dup3d {
                    dz: v[0],
                    drx: v[1],
                    dry: v[2],
                })
            },
        },
    }
}

/// Every scenario of an oracle file, in file order.
pub(crate) fn scenarios(oracle: &'static str) -> Vec<Scenario> {
    let mut out = Vec::new();
    let mut cur: Option<Scenario> = None;
    for line in oracle.lines().filter(|l| !l.starts_with('#')) {
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        let toks: Vec<&str> = rest.split_whitespace().collect();
        match kind {
            "scn" => {
                cur = Some(Scenario {
                    name: rest.to_string(),
                    seed: JsNumber::Undefined,
                    count: 0.0,
                    frame: 0.0,
                    canvas_w: 0.0,
                    canvas_h: 0.0,
                    can_resample: false,
                    temporal: None,
                    span: SpanBounds {
                        start: 0.0,
                        len: 1.0,
                    },
                    base: Vec::new(),
                    table: Vec::new(),
                    calls: Vec::new(),
                    log: Vec::new(),
                    out: None,
                });
            }
            "cfg" => {
                let sc = cur.as_mut().expect("scn before cfg");
                sc.seed = js_number(field(line, "seed="));
                sc.count = hex(field(line, "count="));
                sc.frame = hex(field(line, "frame="));
                sc.canvas_w = hex(field(line, "cw="));
                sc.canvas_h = hex(field(line, "ch="));
                sc.can_resample = field(line, "can_resample=") == "1";
                let t = field(line, "temporal=");
                if t != "-" {
                    let p: Vec<&str> = t.split('/').collect();
                    let dir = match p[2] {
                        "-" => None,
                        "~" => Some(String::new()),
                        d => Some(d.to_string()),
                    };
                    sc.temporal = Some((p[0] == "1", hex(p[1]), dir));
                }
                let s = field(line, "span=");
                if s != "-" {
                    let p: Vec<&str> = s.split('/').collect();
                    sc.span = SpanBounds {
                        start: hex(p[0]),
                        len: hex(p[1]),
                    };
                }
            }
            "base" => cur.as_mut().expect("scn").base = parse_list(toks[0]),
            "rs" => cur
                .as_mut()
                .expect("scn")
                .table
                .push((hex(toks[0]).to_bits(), parse_list(toks[1]))),
            "rsl" => {
                cur.as_mut().expect("scn").calls = if toks[0] == "-" {
                    Vec::new()
                } else {
                    toks.iter().map(|h| hex(h)).collect()
                }
            }
            "log" => cur.as_mut().expect("scn").log.push(parse_log(&toks)),
            "out" => {
                cur.as_mut().expect("scn").out = if toks[0] == "same" {
                    None
                } else {
                    Some(parse_wants(toks[0]))
                }
            }
            "end" => out.push(cur.take().expect("scn")),
            // clone-id rows of the apply fixture are read by their own tests
            // through rows(ORACLE, "cid")
            "cid" => {}
            other => panic!("unknown row {other}"),
        }
    }
    assert!(cur.is_none(), "unterminated scenario");
    out
}

/// The logged descriptor of clone `k`; a control that adds clones reuses the
/// last logged one, and a scenario without any (count 1) uses the identity.
fn placement_for(sc: &Scenario, k: u32) -> PlacementApply {
    sc.log
        .get(k as usize)
        .or_else(|| sc.log.last())
        .map(|row| row.place)
        .unwrap_or(PlacementApply {
            matrix: Descriptor {
                rot: 0.0,
                sx: 1.0,
                sy: 1.0,
                dx: 0.0,
                dy: 0.0,
            },
            opacity_factor: 1.0,
            hue_deg: 0.0,
            dup_3d: None,
        })
}

/// What the kernel did for one scenario.
pub(crate) struct Run {
    pub result: Result<Option<Vec<DuplicatorCopy>>, BuildError>,
    pub calls: Vec<f64>,
    pub pivots: Vec<(u32, Pivot)>,
}

/// Replay a scenario through the real kernel under `rules`.
pub(crate) fn run(sc: &Scenario, rules: TemporalRules) -> Run {
    run_tweaked(sc, rules, &|place| place)
}

/// [`run`] with every logged placement passed through `tweak` first, so a
/// control can feed one wrong descriptor field through the real kernel.
pub(crate) fn run_tweaked(
    sc: &Scenario,
    rules: TemporalRules,
    tweak: &dyn Fn(PlacementApply) -> PlacementApply,
) -> Run {
    let temporal = sc
        .temporal
        .as_ref()
        .map(|(enabled, off, dir)| TemporalOffset {
            enabled: *enabled,
            offset_frames: *off,
            direction: dir.as_deref(),
        });
    let req = CopyRequest {
        seed: sc.seed,
        count: sc.count,
        frame_idx: sc.frame,
        canvas_w: sc.canvas_w,
        canvas_h: sc.canvas_h,
        temporal,
        span: sc.span,
        can_resample: sc.can_resample,
    };
    let mut calls = Vec::new();
    let mut pivots = Vec::new();
    let mut place = |k: u32, pivot: Pivot| {
        pivots.push((k, pivot));
        tweak(placement_for(sc, k))
    };
    let mut resample = |at: f64| {
        calls.push(at);
        sc.table
            .iter()
            .find(|(bits, _)| *bits == at.to_bits())
            .map(|(_, list)| list.clone())
            .unwrap_or_default()
    };
    let result = build_copies_with(rules, &sc.base, &req, &mut place, &mut resample);
    Run {
        result,
        calls,
        pivots,
    }
}

fn expected_gaps(place: &PlacementApply) -> Vec<UnavailableField> {
    let mut gaps = Vec::new();
    if place.opacity_factor != 1.0 {
        gaps.push(UnavailableField::Opacity);
    }
    if truthy(place.hue_deg) {
        gaps.push(UnavailableField::StrokePaint);
    }
    gaps
}

fn copy_mismatch(sc: &Scenario, want: &Want, got: &DuplicatorCopy) -> Option<String> {
    let place = &sc.log[want.dup_index as usize].place;
    let fail = |what: &str| Some(format!("{} copy {}: {what}", sc.name, want.clone_id));
    if got.record.target.stroke_id() != want.id {
        return fail("the source stroke id is not inherited");
    }
    if got.clone_id != want.clone_id || got.dup_index != want.dup_index {
        return fail("per-copy identity");
    }
    if got.record.geometry.closed != want.closed || !fill_matches(&got.record, &want.fill) {
        return fail("closure or fill");
    }
    let (g3, w3) = (got.dup_3d, want.dup_3d);
    let same3 = match (g3, w3) {
        (None, None) => true,
        (Some(g), Some(w)) => same(g.dz, w[0]) && same(g.drx, w[1]) && same(g.dry, w[2]),
        _ => false,
    };
    if !same3 {
        return fail("dup3D");
    }
    if got.unavailable != expected_gaps(place) {
        return fail("unavailable fields");
    }
    match want.opacity {
        Some(o) if same(o, place.opacity_factor) => {}
        _ => return fail("baseline opacity is not 1 * opacityFactor"),
    }
    let rows = segments_of(&got.record);
    if rows.len() != want.segs.len() {
        return fail("segment count");
    }
    let exact = !truthy(place.matrix.rot);
    for (g, w) in rows.iter().zip(&want.segs) {
        let ok = g.iter().zip(w).all(|(a, b)| {
            if exact {
                same(*a, *b)
            } else {
                close(*a, *b, TRIG)
            }
        });
        if !ok {
            return fail("geometry");
        }
    }
    None
}

/// The first difference between what the kernel produced and the baseline.
pub(crate) fn mismatch(sc: &Scenario, run: &Run) -> Option<String> {
    let fail = |what: &str| Some(format!("{}: {what}", sc.name));
    let Ok(result) = &run.result else {
        return fail("the kernel returned an error");
    };
    if run.calls.len() != sc.calls.len()
        || run.calls.iter().zip(&sc.calls).any(|(a, b)| !same(*a, *b))
    {
        return fail("resample calls");
    }
    if run.pivots.len() != sc.log.len() {
        return fail("placement calls");
    }
    for ((k, pivot), row) in run.pivots.iter().zip(&sc.log) {
        if *k != row.k || !same(pivot.x, row.pivot.x) || !same(pivot.y, row.pivot.y) {
            return fail("the pivot a clone was placed around");
        }
    }
    match (result, &sc.out) {
        (None, None) => None,
        (Some(got), Some(want)) => {
            if got.len() != want.len() {
                return fail("copy count");
            }
            got.iter()
                .zip(want)
                .find_map(|(g, w)| copy_mismatch(sc, w, g))
        }
        _ => fail("returned the base versus built copies"),
    }
}
