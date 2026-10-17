//! Controls for the effector falloff kernel: every deviation fed through the
//! real kernel must be caught by the oracle rows.
//!
//! A control changes ONE thing in how the real kernel is driven (the falloff
//! kind, the strength default, an input it ignores, which sum is read as which
//! output) and requires that at least a stated number of oracle rows then
//! mismatch. The measured counts (over the generated oracle, with an
//! independent port of the kernel) are in the comments; the asserted bounds
//! are lower than those counts. The unchanged drive is the baseline test of
//! `object_effector_falloff.rs`.

use super::test_codec::{close, format_channels, hex, parse_effector, rows};
use super::tests::{
    applied, descriptor, sum_rows, sums_of, three_d, tolerant, weight_rows, SumRow, WeightRow,
    ORACLE,
};
use super::{
    effector_channels, effector_weight, js_hypot, Channel, ChannelSums, Effector, Falloff,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

/// Weight rows whose expected value differs when `drive` supplies the weight.
fn weight_mismatches(drive: &dyn Fn(&WeightRow) -> f64) -> usize {
    weight_rows()
        .iter()
        .filter(|r| !close(applied(drive(r)), r.expected, tolerant(&r.eff)))
        .count()
}

/// The real weight of a (possibly altered) effector at the row's clone.
fn weight_with(row: &WeightRow, eff: &Effector, inst: [f64; 2]) -> f64 {
    effector_weight(eff, inst[0], inst[1], row.k, row.count)
}

fn drive_effector(change: impl Fn(&mut Effector)) -> usize {
    weight_mismatches(&|row| {
        let mut eff = row.eff.clone();
        change(&mut eff);
        weight_with(row, &eff, row.inst)
    })
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    assert_eq!(weight_mismatches(&|r| weight_with(r, &r.eff, r.inst)), 0);
}

#[test]
fn step_weight_by_spatial_distance_is_detected() {
    // 137 rows: a step effector read as radial (its pos and radius then matter).
    let n = drive_effector(|e| {
        if e.falloff == Falloff::Step {
            e.falloff = Falloff::Radial;
        }
    });
    assert!(n >= 100, "{n}");
}

#[test]
fn falloff_kind_confusions_are_detected() {
    // 243, 217 and 197 rows.
    let step_as_linear = drive_effector(|e| {
        if e.falloff == Falloff::Step {
            e.falloff = Falloff::Linear;
        }
    });
    let linear_as_radial = drive_effector(|e| {
        if e.falloff == Falloff::Linear {
            e.falloff = Falloff::Radial;
        }
    });
    let radial_as_linear = drive_effector(|e| {
        if e.falloff == Falloff::Radial {
            e.falloff = Falloff::Linear;
        }
    });
    assert!(step_as_linear >= 150, "{step_as_linear}");
    assert!(linear_as_radial >= 150, "{linear_as_radial}");
    assert!(radial_as_linear >= 150, "{radial_as_linear}");
}

#[test]
fn strength_default_applied_to_zero_is_detected() {
    // 75 rows each: undefined and null strengths read as 0 (or 1) instead of 100.
    for wrong in [0.0, 1.0] {
        let n = drive_effector(|e| {
            if !matches!(e.strength, JsNumber::Value(_)) {
                e.strength = JsNumber::Value(wrong);
            }
        });
        assert!(n >= 50, "default {wrong}: {n}");
    }
}

#[test]
fn a_wrong_radius_default_is_detected() {
    // 14 rows: radius undefined, null or 0 must read as 1, not 100.
    let n = drive_effector(|e| e.radius = JsNumber::Value(e.radius.or_default(100.0)));
    assert!(n >= 10, "{n}");
}

#[test]
fn ignored_inputs_are_detected() {
    // angle 114, pos 330, strength 450, angle read as radians 135 rows.
    let angle = drive_effector(|e| e.angle = JsNumber::Value(0.0));
    let pos = drive_effector(|e| e.pos = None);
    let strength = drive_effector(|e| e.strength = JsNumber::Value(100.0));
    let radians = drive_effector(|e| {
        if let JsNumber::Value(a) = e.angle {
            e.angle = JsNumber::Value(a * 180.0 / std::f64::consts::PI);
        }
    });
    assert!(angle >= 80, "angle {angle}");
    assert!(pos >= 200, "pos {pos}");
    assert!(strength >= 300, "strength {strength}");
    assert!(radians >= 80, "radians {radians}");
}

#[test]
fn swapped_instance_axes_are_detected() {
    // 136 rows.
    let n = weight_mismatches(&|r| weight_with(r, &r.eff, [r.inst[1], r.inst[0]]));
    assert!(n >= 80, "{n}");
}

#[test]
fn a_naive_hypot_is_detected() {
    // 284 of the 856 hypot rows differ from sqrt(x*x + y*y).
    let r = rows(ORACLE, "hyp");
    let naive = |x: f64, y: f64| (x * x + y * y).sqrt();
    let n = r
        .iter()
        .filter(|t| !close(naive(hex(t[0]), hex(t[1])), hex(t[3]), false))
        .count();
    assert!(n >= 200, "{n}");
    let real = r
        .iter()
        .filter(|t| !close(js_hypot(hex(t[0]), hex(t[1])), hex(t[3]), false))
        .count();
    assert_eq!(real, 0);
}

/// `chn` rows whose output differs when the channel list is read after `change`.
fn channel_mismatches(change: impl Fn(&mut Effector)) -> usize {
    rows(ORACLE, "chn")
        .iter()
        .filter(|t| {
            let mut eff = parse_effector(t[0]);
            change(&mut eff);
            format_channels(&effector_channels(&eff)) != t[2]
        })
        .count()
}

#[test]
fn channel_list_mistakes_are_detected() {
    // 144 rows: a stored channel list that hides the legacy fields.
    assert!(channel_mismatches(|e| e.channels = Some(Vec::new())) >= 100);
    // 9 rows: an explicit channel list that is ignored in favour of the legacy fields.
    assert!(channel_mismatches(|e| e.channels = None) >= 5);
    // 15 rows: the legacy rotation channel placed first.
    let rotation_first = rows(ORACLE, "chn")
        .iter()
        .filter(|t| {
            let eff = parse_effector(t[0]);
            let mut list: Vec<Channel> = effector_channels(&eff).to_vec();
            list.sort_by_key(|c| c.prop != "rotation");
            format_channels(&list) != t[2]
        })
        .count();
    assert!(rotation_first >= 10, "{rotation_first}");
    assert_eq!(channel_mismatches(|_| {}), 0);
}

/// Sum rows with a field that differs after `read` picks the outputs.
fn sum_mismatches(read: impl Fn(&ChannelSums) -> [f64; 7]) -> usize {
    sum_rows()
        .iter()
        .filter(|row: &&SumRow| {
            let tol = row.effs.iter().any(|e| e.falloff == Falloff::Linear);
            let sums = sums_of(row);
            read(&sums)
                .iter()
                .zip(&row.expected)
                .any(|(g, w)| !close(*g, *w, tol))
        })
        .count()
}

#[test]
fn channel_sum_mistakes_are_detected() {
    assert_eq!(sum_mismatches(descriptor), 0);
    // 38 rows: position x and y exchanged.
    let swap_pos = sum_mismatches(|s| {
        let mut d = descriptor(s);
        d.swap(0, 1);
        d
    });
    // 55 rows: scale x and y exchanged.
    let swap_scale = sum_mismatches(|s| {
        let mut d = descriptor(s);
        d.swap(3, 4);
        d
    });
    // 30 rows: opacity without the clamp.
    let no_clamp = sum_mismatches(|s| {
        let mut d = descriptor(s);
        d[5] = 1.0 + s.opacity / 100.0;
        d
    });
    // 66 rows: hue and rotation exchanged.
    let hue_rot = sum_mismatches(|s| {
        let mut d = descriptor(s);
        d.swap(2, 6);
        d
    });
    // 55 rows: the scale sum used as a fraction instead of a percentage.
    let not_percent = sum_mismatches(|s| {
        let mut d = descriptor(s);
        d[3] = 1.0 + s.scale[0];
        d[4] = 1.0 + s.scale[1];
        d
    });
    assert!(swap_pos >= 20, "{swap_pos}");
    assert!(swap_scale >= 30, "{swap_scale}");
    assert!(no_clamp >= 15, "{no_clamp}");
    assert!(hue_rot >= 40, "{hue_rot}");
    assert!(not_percent >= 30, "{not_percent}");
}

#[test]
fn three_d_axis_mistakes_are_detected() {
    // 60, 57 and 42 rows: rotX/rotY exchanged, z/rotX exchanged, rotY dropped.
    let count = |read: &dyn Fn(&ChannelSums) -> ChannelSums| {
        sum_rows()
            .iter()
            .filter(|row| {
                let tol = row.effs.iter().any(|e| e.falloff == Falloff::Linear);
                match (three_d(&read(&sums_of(row))), row.d3) {
                    (None, None) => false,
                    (Some(g), Some(w)) => (0..3).any(|i| !close(g[i], w[i], tol)),
                    _ => true,
                }
            })
            .count()
    };
    let swap_xy = count(&|s| ChannelSums {
        rot_x: s.rot_y,
        rot_y: s.rot_x,
        ..*s
    });
    let swap_zx = count(&|s| ChannelSums {
        pos_z: s.rot_x,
        rot_x: s.pos_z,
        ..*s
    });
    let drop_y = count(&|s| ChannelSums { rot_y: 0.0, ..*s });
    assert!(swap_xy >= 40, "{swap_xy}");
    assert!(swap_zx >= 40, "{swap_zx}");
    assert!(drop_y >= 30, "{drop_y}");
    assert_eq!(count(&|s| *s), 0);
}

#[test]
fn the_comparison_does_not_hide_real_differences() {
    assert!(close(1.0 + 1e-12, 1.0, true));
    assert!(
        !close(1.0 + 1e-6, 1.0, true),
        "a 1e-6 relative error is rejected"
    );
    assert!(
        !close(1.0 + 1e-15, 1.0, false),
        "exact compare rejects one ulp"
    );
    assert!(
        !close(0.0, -0.0, false),
        "exact compare keeps the sign of zero"
    );
    assert!(close(f64::NAN, f64::NAN, false));
    assert!(!close(f64::NAN, 0.0, true));
    assert!(!close(f64::INFINITY, f64::MAX, true));
    assert!(!close(1.0, f64::INFINITY, true));
    assert!(!close(f64::INFINITY, f64::NEG_INFINITY, true));
    assert!(close(f64::INFINITY, f64::INFINITY, true));
}
