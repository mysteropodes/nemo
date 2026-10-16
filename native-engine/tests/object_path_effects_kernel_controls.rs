//! Controls for the path-effect kernel: deliberate mistakes the oracle comparison must catch, a tolerance
//! pair that proves the sin-hash bound is neither vacuous nor deaf, oracle coverage guards, and the native
//! sample-budget refusal (the one behaviour the baseline does not have).

use super::super::{
    apply, apply_zigzag, js_round, param_value, PathFxEntry, PathFxError, PathFxStroke,
    PathSegment, WalkSample, MAX_SAMPLES, P1,
};
use super::{run, Stroke, ORACLE};

/// A mistake injected around the real kernel (inputs before the call, outputs after it).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// The zigzag side flipped (exactly `size` negated).
    SideSign,
    /// An open path loses its appended last anchor.
    DropOpenTail,
    /// `f64::round` where JavaScript rounds half toward +infinity.
    StdRound,
    /// `floor(x + 0.5)`, wrong at 0.49999999999999994.
    FloorPlusHalf,
    /// Ties to even.
    TiesEven,
    /// `enabled` ignored.
    SkipEnabled,
    /// The stack applied back to front.
    ReverseStack,
    /// An absent/null parameter read as 0.
    DefaultsAsZero,
    /// An explicit 0 read as absent (the type default).
    ZeroAsDefault,
    /// The roughen amount (and noise abscissa) scaled by this factor.
    AmountScale(f64),
}

impl Mutation {
    pub(crate) fn entry(self, mut e: PathFxEntry) -> PathFxEntry {
        match self {
            Mutation::SideSign if e.fx_type == "zigzag" => e.params[0] = Some(-param_value(&e, P1)),
            Mutation::SkipEnabled => e.enabled = None,
            Mutation::DefaultsAsZero => e.params = e.params.map(|p| Some(p.unwrap_or(0.0))),
            Mutation::ZeroAsDefault => e.params = e.params.map(|p| p.filter(|v| *v != 0.0)),
            Mutation::AmountScale(k) if e.fx_type == "roughen" => {
                e.params[0] = Some(param_value(&e, P1) * k)
            }
            _ => {}
        }
        e
    }

    pub(crate) fn list(
        self,
        l: Option<Vec<Option<PathFxEntry>>>,
    ) -> Option<Vec<Option<PathFxEntry>>> {
        let mut l: Vec<Option<PathFxEntry>> =
            l?.into_iter().map(|e| e.map(|e| self.entry(e))).collect();
        if self == Mutation::ReverseStack {
            l.reverse();
        }
        Some(l)
    }

    pub(crate) fn walk(self, samples: &mut Vec<WalkSample>, closed: bool) {
        if self == Mutation::DropOpenTail && !closed {
            samples.pop();
        }
    }

    pub(crate) fn tail(self, input: &Stroke, out: &mut Stroke) {
        if self == Mutation::DropOpenTail && !input.closed {
            out.segments.as_mut().expect("segments").pop();
        }
    }

    pub(crate) fn round(self, x: f64) -> f64 {
        match self {
            Mutation::StdRound => x.round(),
            Mutation::FloorPlusHalf => (x + 0.5).floor(),
            Mutation::TiesEven => x.round_ties_even(),
            _ => js_round(x),
        }
    }

    pub(crate) fn x(self, x: f64) -> f64 {
        match self {
            Mutation::AmountScale(k) => x * k,
            _ => x,
        }
    }
}

#[test]
fn flipping_the_zigzag_side_is_caught() {
    let r = run(Mutation::SideSign);
    assert!(
        r.bad_in("zig") > 0 && r.bad_in("apply") > 0,
        "{} zig, {} apply",
        r.bad_in("zig"),
        r.bad_in("apply")
    );
}

#[test]
fn dropping_the_open_endpoint_is_caught() {
    let r = run(Mutation::DropOpenTail);
    assert!(r.bad_in("walk") > 0 && r.bad_in("zig") > 0 && r.bad_in("rough") > 0);
}

#[test]
fn each_wrong_rounding_is_caught_by_the_round_pins() {
    for m in [
        Mutation::StdRound,
        Mutation::FloorPlusHalf,
        Mutation::TiesEven,
    ] {
        assert!(run(m).bad_in("round") > 0, "{m:?}");
    }
    // The trap and the ties, directly: each wrong rounding fails on a value the baseline answers differently.
    assert_eq!((0.49999999999999994f64 + 0.5).floor(), 1.0);
    assert_eq!(js_round(0.49999999999999994), 0.0);
    assert_eq!((-0.5f64).round(), -1.0);
    assert_eq!(js_round(-0.5).to_bits(), (-0.0f64).to_bits());
    assert_eq!(2.5f64.round_ties_even(), 2.0);
    assert_eq!(js_round(2.5), 3.0);
}

#[test]
fn ignoring_enabled_or_reordering_the_stack_is_caught() {
    let skip = run(Mutation::SkipEnabled);
    assert!(skip.bad_in("stack") > 0 && skip.bad_in("apply") > 0);
    let rev = run(Mutation::ReverseStack);
    assert!(rev.bad_in("stack") > 0 && rev.bad_in("apply") > 0);
}

#[test]
fn null_defaults_and_explicit_zero_stay_distinct() {
    let zero = run(Mutation::DefaultsAsZero);
    assert!(zero.bad_in("pv") > 0 && zero.bad_in("zig") > 0 && zero.bad_in("rough") > 0);
    let default = run(Mutation::ZeroAsDefault);
    assert!(default.bad_in("pv") > 0 && default.bad_in("zig") > 0 && default.bad_in("rough") > 0);
}

#[test]
fn the_sin_hash_bound_is_neither_vacuous_nor_deaf() {
    // Outside: a 1e-6 relative error is ~4 orders above the bound and is caught on each sin-built op.
    let out = run(Mutation::AmountScale(1.0 + 1e-6));
    assert!(out.bad_in("rough") > 0 && out.bad_in("noise") > 0 && out.bad_in("apply") > 0);
    // Inside: 1e-14 is far below the bound and passes everywhere, so a zero tolerance would be a bug too.
    let inside = run(Mutation::AmountScale(1.0 + 1e-14));
    assert!(inside.bad.is_empty(), "{:?}", inside.bad.first());
}

#[test]
fn the_oracle_still_covers_the_branches_the_controls_rely_on() {
    let has = |needle: &str| ORACLE.lines().any(|l| l.contains(needle));
    assert!(
        has("pfx1 round 3fdfffffffffffff 0"),
        "0.49999999999999994 trap"
    );
    assert!(has("pfx1 round 8000000000000000 8000000000000000"), "-0");
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 stack") && l.contains(":^:") && l.contains("|")),
        "enabled null"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 stack") && l.contains(":!:")),
        "enabled absent"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 stack") && l.contains("wobble")),
        "unknown type"
    );
    assert!(
        ORACLE.lines().any(|l| l.starts_with("pfx1 stack ^|")),
        "null entry"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 apply") && l.contains(":f:")),
        "disabled effect"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 zig") && l.ends_with(" =")),
        "returns the input"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 apply") && l.contains("/r/") && l.contains("=")),
        "raster"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 walk") && l.contains(" 0 8 ")),
        "open walk"
    );
    assert!(
        ORACLE
            .lines()
            .any(|l| l.starts_with("pfx1 walk") && l.contains(" 1 8 ")),
        "closed walk"
    );
}

fn closed_stroke(n: usize) -> Stroke {
    let segments = (0..n)
        .map(|i| PathSegment {
            point: [i as f64 * 10.0, (i % 3) as f64],
            handle_in: None,
            handle_out: None,
        })
        .collect();
    PathFxStroke {
        is_raster: false,
        closed: true,
        segments: Some(segments),
        rest: super::Rest {
            width: None,
            tag: "g".into(),
        },
    }
}

fn zig(p2: f64) -> PathFxEntry {
    PathFxEntry {
        fx_type: "zigzag".into(),
        enabled: None,
        params: [Some(5.0), Some(p2), None],
    }
}

#[test]
fn an_unbounded_sample_count_is_refused_and_publishes_nothing() {
    let sd = closed_stroke(4);
    // Ridges 131072 -> perCurve 262144 -> 4 curves * 262144 + 1 > 2^20 (the +1 bounds an open path's tail).
    assert_eq!(MAX_SAMPLES, 1 << 20);
    for p2 in [131_072.0, 1e9, 1e300, f64::INFINITY] {
        assert_eq!(
            apply_zigzag(&sd, &zig(p2)),
            Err(PathFxError::SampleBudget),
            "p2 {p2}"
        );
    }
    // A NaN count draws nothing, so it is the baseline's "fewer than two points": the input itself.
    assert_eq!(apply_zigzag(&sd, &zig(f64::NAN)), Ok(None));
    // A large but bounded count is evaluated normally: 4 curves * 2 * 1000.
    let big = apply_zigzag(&sd, &zig(1000.0))
        .expect("budget")
        .expect("changed");
    assert_eq!(big.segments.as_ref().map(Vec::len), Some(8000));
    // All or nothing through the stack: the second effect's refusal discards the first effect's output.
    let list = vec![Some(zig(2.0)), Some(zig(1e9))];
    let strokes = vec![sd.clone()];
    assert_eq!(
        apply(&strokes, Some(list.as_slice()), None).err(),
        Some(PathFxError::SampleBudget)
    );
    assert_eq!(strokes, vec![sd]);
}
