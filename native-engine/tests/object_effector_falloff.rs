//! Oracle tests for the effector falloff kernel (L04-C, I08/I09).
//!
//! `fixtures/object-effector-falloff/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-C leaf, which cuts `effectorChannels` and
//! `_duplicatorClonePlacement` out of `src/js/app.js` at baseline `fbe4b8a3`
//! and runs them verbatim in a Node `vm` (with the verbatim `seededRng`). The
//! header of the fixture carries the baseline blob ids and the row grammar.
//!
//! Row kinds: `hyp` `Math.hypot`, `chn` `effectorChannels`, `wgt` the weight
//! of one effector at one clone (read as `dx` of the baseline placement with a
//! single position channel `[1, 0]`; 0 when the baseline skips the effector),
//! `sum` every channel of an effector list added into the clone sums.
//!
//! Tolerance: everything is `+ - * /`, `sqrt`, comparisons and the V8 hypot
//! algorithm, compared bit for bit (`-0` included), except rows whose
//! effector list holds a LINEAR falloff: its `cos`/`sin` are libm calls, so
//! those rows are compared within `1e-9 + 1e-12 * |expected|`.

use super::test_codec::{
    close, format_channels, has_linear, hex, parse_effector, parse_effectors, rows,
};
use super::{
    apply_effector, clamp01, effector_channels, effector_weight, is_skipped, js_hypot, migrated,
    truthy, ChannelSums, Falloff,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-effector-falloff/oracle.txt");

/// One `wgt` row.
pub(crate) struct WeightRow {
    pub eff: super::Effector,
    pub inst: [f64; 2],
    pub k: u32,
    pub count: f64,
    pub expected: f64,
}

pub(crate) fn weight_rows() -> Vec<WeightRow> {
    rows(ORACLE, "wgt")
        .iter()
        .map(|t| WeightRow {
            eff: parse_effector(t[0]),
            inst: [hex(t[1]), hex(t[2])],
            k: t[3].parse().expect("k"),
            count: hex(t[4]),
            expected: hex(t[6]),
        })
        .collect()
}

/// The weight the clone loop would apply: 0 when the baseline skips it.
pub(crate) fn applied(w: f64) -> f64 {
    if is_skipped(w) {
        0.0
    } else {
        w
    }
}

/// Tolerance flag of a row.
pub(crate) fn tolerant(eff: &super::Effector) -> bool {
    eff.falloff == Falloff::Linear
}

/// One `sum` row: the effector list, pivot, k, count and the expected outputs
/// (dx dy rot sx sy opacityFactor hueDeg) plus the optional dup3D triple.
pub(crate) struct SumRow {
    pub effs: Vec<super::Effector>,
    pub pivot: [f64; 2],
    pub k: u32,
    pub count: f64,
    pub expected: [f64; 7],
    pub d3: Option<[f64; 3]>,
}

pub(crate) fn sum_rows() -> Vec<SumRow> {
    rows(ORACLE, "sum")
        .iter()
        .map(|t| {
            let mut expected = [0.0; 7];
            for (slot, tok) in expected.iter_mut().zip(&t[6..13]) {
                *slot = hex(tok);
            }
            SumRow {
                effs: parse_effectors(t[0]),
                pivot: [hex(t[1]), hex(t[2])],
                k: t[3].parse().expect("k"),
                count: hex(t[4]),
                expected,
                d3: if t[13] == "-" {
                    None
                } else {
                    let v: Vec<f64> = t[13].split('/').map(hex).collect();
                    Some([v[0], v[1], v[2]])
                },
            }
        })
        .collect()
}

/// The sums of a row's effectors starting from zero clone deltas.
pub(crate) fn sums_of(row: &SumRow) -> ChannelSums {
    let mut sums = ChannelSums::zero();
    for eff in &row.effs {
        apply_effector(
            &mut sums,
            eff,
            row.pivot[0] + 0.0,
            row.pivot[1] + 0.0,
            row.k,
            row.count,
        );
    }
    sums
}

/// The baseline's own mapping from sums to the descriptor, with base offset 0:
/// `baseDx + posK[0]`, `1 + scaleK / 100`, `max(0, min(1, 1 + opK / 100))`.
pub(crate) fn descriptor(s: &ChannelSums) -> [f64; 7] {
    [
        0.0 + s.pos[0],
        0.0 + s.pos[1],
        0.0 + s.rot,
        1.0 + s.scale[0] / 100.0,
        1.0 + s.scale[1] / 100.0,
        clamp01(1.0 + s.opacity / 100.0),
        s.hue,
    ]
}

/// `dzK || drxK || dryK`.
pub(crate) fn three_d(s: &ChannelSums) -> Option<[f64; 3]> {
    let any = [s.pos_z, s.rot_x, s.rot_y]
        .iter()
        .any(|v| truthy(JsNumber::Value(*v)));
    if any {
        Some([s.pos_z, s.rot_x, s.rot_y])
    } else {
        None
    }
}

#[test]
fn hypot_rows_match_math_hypot() {
    let r = rows(ORACLE, "hyp");
    assert_eq!(r.len(), 856);
    for t in &r {
        let (x, y, h) = (hex(t[0]), hex(t[1]), hex(t[3]));
        assert!(close(js_hypot(x, y), h, false), "hypot({}, {})", t[0], t[1]);
    }
}

#[test]
fn channel_rows_match_effector_channels() {
    let r = rows(ORACLE, "chn");
    assert_eq!(r.len(), 194);
    for t in &r {
        let eff = parse_effector(t[0]);
        let list = effector_channels(&eff);
        assert_eq!(format_channels(&list), t[2], "effector {}", t[0]);
        // The in-place migration of the baseline is a returned value: it
        // stores the same list, keeps the legacy fields and is idempotent.
        let stored = migrated(&eff);
        assert_eq!(
            stored.channels.as_deref().map(format_channels).as_deref(),
            Some(t[2])
        );
        // Derived `PartialEq` is false on NaN (43 of the 194 rows carry one),
        // so the records are compared through `Debug`, which prints NaN and
        // the sign of zero.
        assert_eq!(
            format!("{:?}", migrated(&stored)),
            format!("{:?}", stored),
            "second call changes nothing"
        );
        assert_eq!(
            format!("{:?}", stored.offset_pos),
            format!("{:?}", eff.offset_pos)
        );
        assert_eq!(
            format!("{:?}", stored.offset_scale),
            format!("{:?}", eff.offset_scale)
        );
        assert_eq!(format_channels(&effector_channels(&stored)), t[2]);
    }
}

#[test]
fn weight_rows_match_the_baseline_weight() {
    let all = weight_rows();
    assert_eq!(all.len(), 1348);
    for row in &all {
        let w = effector_weight(&row.eff, row.inst[0], row.inst[1], row.k, row.count);
        assert!(
            close(applied(w), row.expected, tolerant(&row.eff)),
            "{:?} at {:?} k {} count {}: {} vs {}",
            row.eff.falloff,
            row.inst,
            row.k,
            row.count,
            applied(w),
            row.expected
        );
    }
}

#[test]
fn sum_rows_match_the_baseline_channel_sums() {
    let all = sum_rows();
    assert_eq!(all.len(), 300);
    for (n, row) in all.iter().enumerate() {
        let sums = sums_of(row);
        let tol = has_linear(&row.effs);
        for (i, (got, want)) in descriptor(&sums).iter().zip(&row.expected).enumerate() {
            assert!(
                close(*got, *want, tol),
                "row {n} field {i}: {got} vs {want}"
            );
        }
        match (three_d(&sums), row.d3) {
            (None, None) => {}
            (Some(g), Some(w)) => {
                for i in 0..3 {
                    assert!(close(g[i], w[i], tol), "row {n} 3d {i}");
                }
            }
            (g, w) => panic!("row {n}: dup3D presence {g:?} vs {w:?}"),
        }
    }
}

#[test]
fn oracle_covers_the_families() {
    let w = weight_rows();
    let count = |f: Falloff, nonzero: bool| {
        w.iter()
            .filter(|r| r.eff.falloff == f && (r.expected != 0.0) == nonzero)
            .count()
    };
    for f in [Falloff::Radial, Falloff::Linear, Falloff::Step] {
        assert!(count(f, true) >= 100, "{f:?} rows that apply");
        assert!(count(f, false) >= 100, "{f:?} rows that are skipped");
    }
    let strength =
        |pred: &dyn Fn(JsNumber) -> bool| w.iter().filter(|r| pred(r.eff.strength)).count();
    assert!(strength(&|s| s == JsNumber::Undefined) >= 90);
    assert!(strength(&|s| s == JsNumber::Null) >= 20);
    assert!(strength(&|s| s == JsNumber::Value(0.0)) >= 100);
    assert!(
        w.iter().filter(|r| r.expected < 0.0).count() >= 50,
        "negative strength"
    );
    assert!(
        w.iter()
            .filter(|r| r.eff.radius == JsNumber::Undefined)
            .count()
            >= 15
    );
    assert!(
        w.iter()
            .filter(|r| r.count.is_nan() || r.count.is_infinite())
            .count()
            >= 50
    );
    let s = sum_rows();
    for prop in [
        "position",
        "positionZ",
        "rotation",
        "rotationX",
        "rotationY",
        "scale",
        "opacity",
        "hue",
        "bogus",
    ] {
        let n = s
            .iter()
            .filter(|r| {
                r.effs
                    .iter()
                    .any(|e| effector_channels(e).iter().any(|c| c.prop == prop))
            })
            .count();
        assert!(n >= 15, "sum rows with a {prop} channel: {n}");
    }
    assert!(s.iter().filter(|r| r.d3.is_some()).count() >= 60);
    assert!(s.iter().filter(|r| r.d3.is_none()).count() >= 150);
    let c = rows(ORACLE, "chn");
    assert!(c.iter().filter(|t| t[2] != "e").count() >= 140);
    assert!(
        c.iter()
            .filter(|t| parse_effector(t[0]).channels.is_some())
            .count()
            >= 8
    );
}
