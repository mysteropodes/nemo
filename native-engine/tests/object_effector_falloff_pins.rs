//! Pinned literals for the effector falloff kernel (L04-C).
//!
//! Each pinned case is a hand-derivable value written here as a literal, is
//! evaluated by the kernel AND must be found, bit for bit, in the oracle: the
//! generator emits exactly these cases, so a literal here that disagrees with
//! the baseline fails the oracle lookup, and a kernel that disagrees with the
//! literal fails the evaluation.

use super::test_codec::{bits, close, format_channels, hex, parse_effector, rows};
use super::tests::{applied, tolerant, weight_rows, ORACLE};
use super::{
    effector_channels, effector_weight, js_hypot, legacy_channels, migrated, Channel, ChannelProp,
    Effector, EffectorPos, Falloff,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

fn v(x: f64) -> JsNumber {
    JsNumber::Value(x)
}

fn eff(
    pos: Option<(f64, f64)>,
    falloff: Falloff,
    radius: JsNumber,
    strength: JsNumber,
    angle: JsNumber,
) -> Effector {
    Effector {
        pos: pos.map(|(x, y)| EffectorPos { x, y }),
        falloff,
        radius,
        strength,
        angle,
        ..Effector::default()
    }
}

/// `{pos: (100, 100), falloff: radial, radius: 200}` with the given strength.
fn radial(strength: JsNumber) -> Effector {
    eff(
        Some((100.0, 100.0)),
        Falloff::Radial,
        v(200.0),
        strength,
        JsNumber::Undefined,
    )
}

fn linear(angle: f64) -> Effector {
    eff(
        Some((100.0, 100.0)),
        Falloff::Linear,
        v(200.0),
        v(100.0),
        v(angle),
    )
}

fn step(strength: f64) -> Effector {
    eff(
        None,
        Falloff::Step,
        JsNumber::Undefined,
        v(strength),
        JsNumber::Undefined,
    )
}

/// (effector, instance position, k, count, applied weight).
fn weight_pins() -> Vec<(Effector, [f64; 2], u32, f64, f64)> {
    vec![
        (radial(v(100.0)), [100.0, 100.0], 0, 1.0, 1.0),
        (radial(v(100.0)), [200.0, 100.0], 0, 1.0, 0.5),
        // The edge of the radius and everything beyond it carry no weight.
        (radial(v(100.0)), [300.0, 100.0], 0, 1.0, 0.0),
        (radial(v(100.0)), [400.0, 100.0], 0, 1.0, 0.0),
        (radial(v(50.0)), [200.0, 100.0], 0, 1.0, 0.25),
        // A strength of 0 stays 0; only undefined and null fall back to 100.
        (radial(v(0.0)), [200.0, 100.0], 0, 1.0, 0.0),
        (radial(JsNumber::Undefined), [200.0, 100.0], 0, 1.0, 0.5),
        (radial(JsNumber::Null), [200.0, 100.0], 0, 1.0, 0.5),
        (radial(v(-100.0)), [100.0, 100.0], 0, 1.0, -1.0),
        // radius 0 reads as 1 (`eff.radius || 1`): 0.5 px from the centre.
        (
            eff(
                Some((100.0, 100.0)),
                Falloff::Radial,
                v(0.0),
                v(100.0),
                JsNumber::Undefined,
            ),
            [100.5, 100.0],
            0,
            1.0,
            0.5,
        ),
        (linear(0.0), [200.0, 100.0], 0, 1.0, 0.5),
        (linear(90.0), [100.0, 200.0], 0, 1.0, 0.5),
        (linear(180.0), [0.0, 100.0], 0, 1.0, 0.5),
        // Behind the origin the projection is negative and the weight clamps to 1.
        (linear(0.0), [0.0, 100.0], 0, 1.0, 1.0),
        // Step ramps by clone index and uses no position or radius.
        (step(100.0), [5.0, 5.0], 3, 7.0, 0.5),
        (step(100.0), [5.0, 5.0], 0, 7.0, 0.0),
        (step(100.0), [5.0, 5.0], 6, 7.0, 1.0),
        (step(100.0), [5.0, 5.0], 3, 1.0, 0.0),
        (step(50.0), [5.0, 5.0], 6, 7.0, 0.5),
        // A fractional count is not clamped: clone 7 of 7.5 weighs 7 / 6.5.
        (step(100.0), [5.0, 5.0], 7, 7.5, 7.0 / 6.5),
        (step(0.0), [5.0, 5.0], 6, 7.0, 0.0),
    ]
}

#[test]
fn pinned_weights_match_the_kernel_and_the_oracle() {
    let rows = weight_rows();
    for (n, (e, inst, k, count, want)) in weight_pins().iter().enumerate() {
        let w = effector_weight(e, inst[0], inst[1], *k, *count);
        assert!(
            close(applied(w), *want, tolerant(e)),
            "pin {n} kernel: {}",
            applied(w)
        );
        let row = rows
            .iter()
            .find(|r| &r.eff == e && r.inst == *inst && r.k == *k && r.count == *count)
            .unwrap_or_else(|| panic!("pin {n} is not in the oracle"));
        assert_eq!(bits(row.expected), bits(*want), "pin {n} oracle");
    }
}

/// (effector, the channels `effectorChannels` returns).
fn channel_pins() -> Vec<(Effector, Vec<Channel>)> {
    let base = Effector::default();
    let pos = |a: JsNumber, b: JsNumber| Some(vec![a, b]);
    vec![
        (
            Effector {
                offset_pos: pos(v(10.0), v(0.0)),
                ..base.clone()
            },
            vec![Channel::new("position", vec![v(10.0), v(0.0)])],
        ),
        (
            Effector {
                offset_pos: pos(v(0.0), v(0.0)),
                ..base.clone()
            },
            vec![],
        ),
        (
            Effector {
                offset_pos: pos(v(0.0), v(5.0)),
                ..base.clone()
            },
            vec![Channel::new("position", vec![v(0.0), v(5.0)])],
        ),
        // A missing element reads as 0 through `|| 0`.
        (
            Effector {
                offset_pos: pos(JsNumber::Undefined, v(4.0)),
                ..base.clone()
            },
            vec![Channel::new("position", vec![v(0.0), v(4.0)])],
        ),
        (
            Effector {
                offset_pos: Some(vec![v(3.0)]),
                ..base.clone()
            },
            vec![Channel::new("position", vec![v(3.0), v(0.0)])],
        ),
        (
            Effector {
                offset_rot: v(15.0),
                ..base.clone()
            },
            vec![Channel::new("rotation", vec![v(15.0)])],
        ),
        (
            Effector {
                offset_scale: pos(v(-20.0), v(20.0)),
                ..base.clone()
            },
            vec![Channel::new("scale", vec![v(-20.0), v(20.0)])],
        ),
        (
            Effector {
                offset_opacity: v(25.0),
                ..base.clone()
            },
            vec![Channel::new("opacity", vec![v(25.0)])],
        ),
        // All four legacy fields: position, rotation, scale, opacity, in that order.
        (
            Effector {
                offset_pos: pos(v(1.0), v(2.0)),
                offset_rot: v(3.0),
                offset_scale: pos(v(4.0), v(5.0)),
                offset_opacity: v(6.0),
                ..base.clone()
            },
            vec![
                Channel::new("position", vec![v(1.0), v(2.0)]),
                Channel::new("rotation", vec![v(3.0)]),
                Channel::new("scale", vec![v(4.0), v(5.0)]),
                Channel::new("opacity", vec![v(6.0)]),
            ],
        ),
        // `eff.channels` wins, an empty list included.
        (
            Effector {
                channels: Some(vec![]),
                offset_rot: v(15.0),
                ..base.clone()
            },
            vec![],
        ),
        (
            Effector {
                channels: Some(vec![Channel::new("position", vec![v(1.0), v(2.0)])]),
                offset_pos: pos(v(10.0), v(20.0)),
                ..base.clone()
            },
            vec![Channel::new("position", vec![v(1.0), v(2.0)])],
        ),
        (
            Effector {
                offset_rot: JsNumber::Null,
                ..base.clone()
            },
            vec![],
        ),
    ]
}

#[test]
fn pinned_channel_lists_match_the_kernel_and_the_oracle() {
    let oracle = rows(ORACLE, "chn");
    for (n, (e, want)) in channel_pins().iter().enumerate() {
        assert_eq!(&*effector_channels(e), want.as_slice(), "pin {n} kernel");
        let row = oracle
            .iter()
            .find(|t| &parse_effector(t[0]) == e)
            .unwrap_or_else(|| panic!("pin {n} is not in the oracle"));
        assert_eq!(row[2], format_channels(want), "pin {n} oracle");
        if e.channels.is_none() {
            assert_eq!(&legacy_channels(e), want, "pin {n} legacy fold");
        }
        assert_eq!(migrated(e).channels.as_deref(), Some(want.as_slice()));
    }
}

#[test]
fn hypot_pins_follow_the_math_hypot_row() {
    let oracle = rows(ORACLE, "hyp");
    let cases: [(f64, f64, f64); 8] = [
        (3.0, 4.0, 5.0),
        (0.0, 0.0, 0.0),
        (-0.0, 0.0, 0.0),
        (0.0, -3.0, 3.0),
        (f64::NAN, f64::INFINITY, f64::INFINITY),
        (f64::INFINITY, f64::NAN, f64::INFINITY),
        (f64::NAN, 1.0, f64::NAN),
        (1.0, f64::NEG_INFINITY, f64::INFINITY),
    ];
    for (x, y, want) in cases {
        assert_eq!(bits(js_hypot(x, y)), bits(want), "hypot({x}, {y})");
        let row = oracle
            .iter()
            .find(|t| bits(hex(t[0])) == bits(x) && bits(hex(t[1])) == bits(y))
            .unwrap_or_else(|| panic!("hypot({x}, {y}) is not in the oracle"));
        assert_eq!(bits(hex(row[3])), bits(want));
    }
}

#[test]
fn labels_read_like_the_baseline_comparisons() {
    for (label, want) in [
        (Some("step"), Falloff::Step),
        (Some("linear"), Falloff::Linear),
        (Some("radial"), Falloff::Radial),
        (None, Falloff::Radial),
        (Some(""), Falloff::Radial),
        (Some("Linear"), Falloff::Radial),
        (Some("STEP"), Falloff::Radial),
        (Some("bogus"), Falloff::Radial),
    ] {
        assert_eq!(Falloff::from_label(label), want, "{label:?}");
    }
    for (label, want) in [
        ("position", ChannelProp::Position),
        ("positionZ", ChannelProp::PositionZ),
        ("rotation", ChannelProp::Rotation),
        ("rotationX", ChannelProp::RotationX),
        ("rotationY", ChannelProp::RotationY),
        ("scale", ChannelProp::Scale),
        ("opacity", ChannelProp::Opacity),
        ("hue", ChannelProp::Hue),
        ("Position", ChannelProp::Unknown),
        ("positionz", ChannelProp::Unknown),
        ("", ChannelProp::Unknown),
        ("bogus", ChannelProp::Unknown),
    ] {
        assert_eq!(ChannelProp::from_label(label), want, "{label:?}");
    }
    assert_eq!(Falloff::Step.label(), "step");
    assert_eq!(Falloff::Linear.label(), "linear");
    assert_eq!(Falloff::Radial.label(), "radial");
}
