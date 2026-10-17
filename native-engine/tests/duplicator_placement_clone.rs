//! Oracle tests for the per-clone placement kernel (L04-C, I07/I08).
//!
//! `fixtures/duplicator-placement-clone/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-C leaf, which cuts
//! `_duplicatorClonePlacement` and `effectorChannels` out of `src/js/app.js`
//! and `seededRng` out of `src/js/tools.js` at baseline `fbe4b8a3`, loads the
//! whole of `src/js/effector-layer.js`, and runs all of it verbatim in a Node
//! `vm`. Each `pl` row is one call of the baseline function (grammar in the
//! fixture header); calls with an owner descriptor also resolve the effector
//! layers listed in the row, with the Motion values stubbed from the row.
//!
//! Tolerance: the descriptor is plain `f64` arithmetic in the baseline's
//! operand order, so rows are compared bit for bit (`-0` included), except
//! rows whose effector list holds a LINEAR falloff (a libm `cos`/`sin`), which
//! are compared within `1e-9 + 1e-12 * |expected|`. The generator drops every
//! row where such a difference could flip a truthiness decision (`dup3D`, the
//! `if (!w)` skip).

use super::test_codec::{d3_of, effective, kernel_input, pl_rows, place, values_of, Owner, PlRow};
use super::{clone_placement, clone_placement_with_draws};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{
    close, has_linear,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::Falloff;

#[test]
fn placement_rows_match_the_baseline() {
    let all = pl_rows();
    assert_eq!(all.len(), 765);
    for (n, row) in all.iter().enumerate() {
        let p = place(&row.input);
        let tol = has_linear(&effective(&row.input));
        for (i, (g, w)) in values_of(&p).iter().zip(&row.out.values).enumerate() {
            assert!(close(*g, *w, tol), "row {n} field {i}: {g} vs {w}");
        }
        match (d3_of(&p), row.out.d3) {
            (None, None) => {}
            (Some(g), Some(w)) => {
                for i in 0..3 {
                    assert!(
                        close(g[i], w[i], tol),
                        "row {n} dup3D {i}: {} vs {}",
                        g[i],
                        w[i]
                    );
                }
            }
            (g, w) => panic!("row {n}: dup3D presence {g:?} vs {w:?}"),
        }
    }
}

#[test]
fn a_clone_depends_on_nothing_but_its_inputs() {
    let all = pl_rows();
    let bits = |row: &PlRow| {
        let p = place(&row.input);
        (
            values_of(&p).map(f64::to_bits),
            d3_of(&p).map(|d| d.map(f64::to_bits)),
        )
    };
    let first: Vec<_> = all.iter().map(bits).collect();
    // Same inputs twice give identical bits (NaN payloads included).
    let again: Vec<_> = all.iter().map(bits).collect();
    assert_eq!(first, again);
    // The reverse order of evaluation changes nothing: no clone reads another
    // clone's stream or any hidden state.
    let mut backwards: Vec<_> = all.iter().rev().map(bits).collect();
    backwards.reverse();
    assert_eq!(first, backwards);
}

#[test]
fn only_effector_inputs_that_the_math_reads_change_a_clone() {
    let mut touched = 0;
    for row in pl_rows() {
        let before = place(&row.input);
        // `_fromLayer` marks the origin of an effector and is never read.
        let mut tagged = row.input.clone();
        tagged
            .effs
            .iter_mut()
            .for_each(|e| e.from_layer = !e.from_layer);
        // The legacy offset fields are ignored once `channels` is present.
        let mut legacy = row.input.clone();
        for e in legacy.effs.iter_mut().filter(|e| e.channels.is_some()) {
            e.offset_rot = JsNumber::Value(77.0);
            e.offset_pos = Some(vec![JsNumber::Value(5.0), JsNumber::Value(5.0)]);
            touched += 1;
        }
        for changed in [&tagged, &legacy] {
            let after = place(changed);
            assert_eq!(
                values_of(&after).map(f64::to_bits),
                values_of(&before).map(f64::to_bits)
            );
            assert_eq!(
                d3_of(&after).map(|d| d.map(f64::to_bits)),
                d3_of(&before).map(|d| d.map(f64::to_bits))
            );
        }
    }
    assert!(touched >= 100, "{touched}");
}

#[test]
fn own_draws_are_what_the_public_entry_point_uses() {
    use crate::evaluation::object_component_kernels::duplicator_placement_rng::clone_draws;
    for row in pl_rows().iter().take(200) {
        let effs = effective(&row.input);
        let input = kernel_input(&row.input, &effs);
        let direct = clone_placement(&input);
        let seamed = clone_placement_with_draws(&input, &clone_draws(input.seed, input.k));
        assert_eq!(
            values_of(&direct).map(f64::to_bits),
            values_of(&seamed).map(f64::to_bits)
        );
    }
}

#[test]
fn oracle_covers_the_placement_families() {
    let all = pl_rows();
    let n = |pred: &dyn Fn(&PlRow) -> bool| all.iter().filter(|r| pred(r)).count();
    assert!(n(&|r| r.input.rand.position) >= 300);
    assert!(n(&|r| r.input.rand.rotation) >= 300);
    assert!(n(&|r| r.input.rand.scale) >= 300);
    assert!(n(&|r| r.input.rand.opacity) >= 300);
    assert!(n(&|r| r.input.rand.hue) >= 300);
    assert!(n(&|r| !r.input.effs.is_empty()) >= 250);
    for f in [Falloff::Radial, Falloff::Linear, Falloff::Step] {
        assert!(
            n(&|r| r.input.effs.iter().any(|e| e.falloff == f)) >= 50,
            "{f:?}"
        );
    }
    // Effector layers: resolved with an owner, and ignored without one.
    assert!(
        n(&|r| matches!(r.input.owner, Owner::Layer(_))
            && effective(&r.input).len() > r.input.effs.len())
            >= 40
    );
    assert!(n(&|r| r.input.owner == Owner::Absent && !r.input.layers.is_empty()) >= 20);
    assert!(n(&|r| r.out.d3.is_some()) >= 200);
    assert!(n(&|r| r.out.d3.is_none() && r.input.is_3d) >= 100);
    assert!(n(&|r| !r.input.is_3d) >= 200);
    assert!(n(&|r| r.input.seed == JsNumber::Undefined) >= 200);
    assert!(n(&|r| r.input.seed == JsNumber::Null) >= 3);
    assert!(
        n(&|r| matches!(r.input.seed, JsNumber::Value(s) if s > 4.0e9 && s < 4_294_967_296.0))
            >= 40
    );
    assert!(n(&|r| matches!(r.input.seed, JsNumber::Value(s) if s < 0.0)) >= 3);
    assert!(n(&|r| r.input.deltas.hue == JsNumber::Undefined) >= 150);
    assert!(n(&|r| r.input.deltas.hue == JsNumber::Null) >= 80);
    assert!(n(&|r| matches!(r.input.deltas.hue, JsNumber::Value(h) if h.is_nan())) >= 3);
    assert!(n(&|r| r.input.k == 0) >= 80);
    assert!(n(&|r| r.input.k >= 100) >= 8);
    assert!(n(&|r| r.out.values[5] == 0.0) >= 50, "opacity clamped to 0");
    assert!(
        n(&|r| r.out.values[5] == 1.0) >= 300,
        "opacity clamped to 1"
    );
    assert!(n(&|r| r.out.values.iter().any(|v| !v.is_finite())) >= 8);
}
