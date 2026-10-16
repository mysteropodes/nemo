//! Pinned literals for the per-clone placement kernel (L04-C).
//!
//! Each pinned call is written here as typed inputs with a literal expected
//! descriptor. It is evaluated by the kernel AND must be found, bit for bit,
//! in the oracle: the generator emits exactly these calls, so a literal that
//! disagrees with the baseline fails the oracle lookup and a kernel that
//! disagrees with the literal fails the evaluation. The random table at the
//! end states the bits the baseline produced for seed 42, clones 0-3, with
//! every channel random.

use super::test_codec::{d3_of, pl_rows, place, values_of, Owner, PlInput};
use super::{CloneDeltas, RandMode};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::{
    Channel, Effector, EffectorPos, Falloff,
};
use crate::evaluation::object_component_kernels::object_effector_layer::{
    EffectorLayer, EffectorLayerSamples, MotionDelta,
};

fn v(x: f64) -> JsNumber {
    JsNumber::Value(x)
}

/// Seed 42, clone 2 of 5, mode offset `(20, -10)` rotated 30 degrees, no
/// random channel, no effector, 3D layer.
fn linear_input() -> PlInput {
    PlInput {
        seed: v(42.0),
        k: 2,
        count: 5.0,
        pivot: [100.0, 100.0],
        base: [20.0, -10.0, 30.0],
        deltas: CloneDeltas {
            pos: [10.0, 5.0],
            rot: 15.0,
            scale: [25.0, 50.0],
            opacity: -25.0,
            hue: v(45.0),
            pos_z: 40.0,
            rot_x: 5.0,
            rot_y: -5.0,
        },
        rand: RandMode::default(),
        is_3d: true,
        effs: vec![],
        canvas: [1920.0, 1080.0],
        owner: Owner::Absent,
        layers: vec![],
    }
}

/// Clone 0 of 1 at pivot `(100, 100)` shifted by `(100, 0)`, every own delta 0.
fn quiet_input() -> PlInput {
    PlInput {
        seed: v(1.0),
        k: 0,
        count: 1.0,
        base: [100.0, 0.0, 0.0],
        deltas: CloneDeltas {
            hue: JsNumber::Null,
            ..CloneDeltas::default()
        },
        is_3d: false,
        ..linear_input()
    }
}

fn pos_rot_effector() -> Effector {
    Effector {
        pos: Some(EffectorPos { x: 100.0, y: 100.0 }),
        falloff: Falloff::Radial,
        radius: v(200.0),
        strength: v(100.0),
        channels: Some(vec![
            Channel::new("position", vec![v(50.0), v(0.0)]),
            Channel::new("rotation", vec![v(10.0)]),
        ]),
        ..Effector::default()
    }
}

fn step_effector(channel: Channel, pos: Option<EffectorPos>) -> Effector {
    Effector {
        pos,
        falloff: Falloff::Step,
        channels: Some(vec![channel]),
        ..Effector::default()
    }
}

fn layer(visible: Option<bool>, motion: Option<MotionDelta>) -> EffectorLayer<'static> {
    EffectorLayer {
        is_effector_layer: true,
        target_uid: Some("dupA"),
        visible,
        pos: Some([100.0, 100.0]),
        falloff: Some("radial"),
        motion,
        samples: EffectorLayerSamples {
            radius: Some(vec![v(200.0)]),
            strength: Some(vec![v(100.0)]),
            off_position: Some(vec![v(100.0), v(0.0)]),
            ..EffectorLayerSamples::default()
        },
    }
}

fn same(got: &[f64], want: &[f64], what: &str) {
    let bits = |s: &[f64]| s.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(got), bits(want), "{what}");
}

/// The kernel gives the literal AND the oracle holds the same call and answer.
fn pin(what: &str, input: &PlInput, values: [f64; 7], d3: Option<[f64; 3]>) {
    let p = place(input);
    same(&values_of(&p), &values, &format!("{what} kernel"));
    assert_eq!(
        d3_of(&p).map(|d| d.map(f64::to_bits)),
        d3.map(|d| d.map(f64::to_bits)),
        "{what} dup3D"
    );
    let rows = pl_rows();
    let row = rows
        .iter()
        .find(|r| &r.input == input)
        .unwrap_or_else(|| panic!("{what} is not in the oracle"));
    same(&row.out.values, &values, &format!("{what} oracle"));
    assert_eq!(
        row.out.d3.map(|d| d.map(f64::to_bits)),
        d3.map(|d| d.map(f64::to_bits)),
        "{what} oracle dup3D"
    );
}

#[test]
fn linear_stagger_is_k_times_the_delta() {
    // k = 2: dx = 20 + 2*10, dy = -10 + 2*5, rot = 30 + 2*15, scale 1 + 2*25/100,
    // opacity 1 - 2*25/100, hue 2*45, and the 3D deltas 2*40, 2*5, 2*(-5).
    let on = linear_input();
    pin(
        "3D layer",
        &on,
        [40.0, 0.0, 60.0, 1.5, 2.0, 0.5, 90.0],
        Some([80.0, 10.0, -10.0]),
    );
    // The same clone on a flat layer carries no dup3D.
    let off = PlInput {
        is_3d: false,
        ..linear_input()
    };
    pin(
        "flat layer",
        &off,
        [40.0, 0.0, 60.0, 1.5, 2.0, 0.5, 90.0],
        None,
    );
}

#[test]
fn effector_contributions_add_into_the_stagger() {
    // The clone sits at (200, 100), 100 px from the effector: weight 0.5.
    let near = PlInput {
        effs: vec![pos_rot_effector()],
        ..quiet_input()
    };
    pin(
        "radial effector",
        &near,
        [125.0, 0.0, 5.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    // Strength 0 stays 0 (it is not the default 100): the effector is skipped.
    let silent = PlInput {
        effs: vec![Effector {
            strength: v(0.0),
            ..pos_rot_effector()
        }],
        ..quiet_input()
    };
    pin(
        "strength 0",
        &silent,
        [100.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    // A step effector of clone 0 of 1 has no weight (count <= 1).
    let step = PlInput {
        effs: vec![Effector {
            falloff: Falloff::Step,
            ..pos_rot_effector()
        }],
        ..quiet_input()
    };
    pin(
        "step, one clone",
        &step,
        [100.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    // Step weight 2 / (5 - 1) = 0.5 on the opacity channel: 1 + (-50) / 100.
    let op = step_effector(Channel::new("opacity", vec![v(-100.0)]), None);
    let ramp = PlInput {
        k: 2,
        count: 5.0,
        base: [0.0, 0.0, 0.0],
        effs: vec![op],
        ..quiet_input()
    };
    pin(
        "step ramp",
        &ramp,
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.5, 0.0],
        None,
    );
}

#[test]
fn a_third_axis_appears_only_on_a_3d_layer_where_it_moved() {
    let z = step_effector(
        Channel::new("positionZ", vec![v(10.0)]),
        Some(EffectorPos { x: 0.0, y: 0.0 }),
    );
    let flat = |k: u32, is_3d: bool| PlInput {
        k,
        is_3d,
        effs: vec![z.clone()],
        deltas: CloneDeltas {
            hue: JsNumber::Null,
            ..CloneDeltas::default()
        },
        ..linear_input()
    };
    let expect = [20.0, -10.0, 30.0, 1.0, 1.0, 1.0, 0.0];
    // Clone 4 of 5 weighs 4 / 4 = 1: dz = 10.
    pin("3D, moved", &flat(4, true), expect, Some([10.0, 0.0, 0.0]));
    pin("flat, moved", &flat(4, false), expect, None);
    // Clone 0 weighs 0: nothing moved, so there is no dup3D even on a 3D layer.
    pin("3D, not moved", &flat(0, true), expect, None);
}

#[test]
fn effector_layers_are_summed_only_with_an_owner_that_selects_them() {
    let with = |owner: Owner, layer: EffectorLayer<'static>| PlInput {
        owner,
        layers: vec![layer],
        ..quiet_input()
    };
    let dup_a = Owner::Layer(Some("dupA"));
    // Weight 0.5 at 100 px of 200: position channel [100, 0] adds 50.
    pin(
        "owner selects the layer",
        &with(dup_a, layer(None, None)),
        [150.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    // The nativeVideo call shape has no owner descriptor: layers are not read.
    pin(
        "no owner",
        &with(Owner::Absent, layer(None, None)),
        [100.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    pin(
        "other owner",
        &with(Owner::Layer(Some("dupB")), layer(None, None)),
        [100.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    pin(
        "hidden layer",
        &with(dup_a, layer(Some(false), None)),
        [100.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
    // The layer's Motion position moves the origin by 50: weight 0.75.
    let moved = Some(MotionDelta {
        dx: v(50.0),
        dy: v(0.0),
    });
    pin(
        "moved origin",
        &with(dup_a, layer(None, moved)),
        [175.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
        None,
    );
}

/// Seed 42, clones 0-3, every channel random, as the baseline computed them:
/// `[dx, dy, rot, sx, sy, opacityFactor, hueDeg]` bit patterns and the dup3D.
const RANDOM_TABLE: [([u64; 7], [u64; 3]); 4] = [
    (
        [
            0x403605a6b5b00000,
            0xc02508c099380000,
            0x40444977f9400000,
            0x3ff15b9d85680000,
            0x3fe5981351400000,
            0x3fef9313b2d00000,
            0xc003ee8539300000,
        ],
        [0xc032244a69c00000, 0x3ff3f58a80a00000, 0xc00d3cebb9600000],
    ),
    (
        [
            0x4031f87172940000,
            0xc015f1cb23a00000,
            0x403a6aa068360000,
            0x3ff180fab9d00000,
            0x3ff4eded48500000,
            0x3fee741d18100000,
            0xbfdbb358fd800000,
        ],
        [0xc02c80e12de00000, 0xbfd5e8f31a800000, 0xc0105d6e67300000],
    ),
    (
        [
            0x4027e6556b000000,
            0xc01801ba6a080000,
            0x402eec0042d00000,
            0x3ff0cb2664d80000,
            0x3ff6f63426d00000,
            0x3ff0000000000000,
            0x4039ce6271f80000,
        ],
        [0x40255a9187400000, 0x400b3acc72200000, 0xc007abc4a8200000],
    ),
    (
        [
            0x402f92b6c0400000,
            0xc01e71e5ab700000,
            0x4032b04154200000,
            0x3ff1491af1780000,
            0x3ff7c208f7500000,
            0x3feabc2daf800000,
            0x40225635eedc0000,
        ],
        [0x403fbb23f7000000, 0x400739a4d5300000, 0x3fd72e7635000000],
    ),
];

#[test]
fn the_random_table_matches_the_baseline_bits() {
    for (k, (values, d3)) in RANDOM_TABLE.iter().enumerate() {
        let input = PlInput {
            k: k as u32,
            rand: RandMode {
                position: true,
                rotation: true,
                scale: true,
                opacity: true,
                hue: true,
            },
            ..linear_input()
        };
        let want = values.map(f64::from_bits);
        pin(
            &format!("random clone {k}"),
            &input,
            want,
            Some(d3.map(f64::from_bits)),
        );
    }
    // Every clone differs from the next: the streams are per clone.
    let first = RANDOM_TABLE.map(|(v, _)| v[0]);
    assert!(first
        .iter()
        .enumerate()
        .all(|(i, a)| first[i + 1..].iter().all(|b| a != b)));
}
