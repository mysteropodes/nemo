//! Oracle tests for effector layers resolved into effectors (L04-C, I09).
//!
//! `fixtures/object-effector-layer/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-C leaf, which runs the WHOLE of
//! `src/js/effector-layer.js` at baseline `fbe4b8a3` verbatim in a Node `vm`
//! and calls `window.SMEffectorLayer.resolvedEffectorsFor` over a
//! `state.layers` array built from each row. `SMMotion.layerMotionAt` and
//! `SMMotion.valueAtFrame` are stubs that return the values written in the
//! row (Root D10/D11 own the real sampling), so the rows cover the layer
//! filter (`isEffectorLayer`, target, visibility, order), the origin, the
//! property defaults and the channel assembly. All arithmetic is `+`, so every
//! row is compared bit for bit (through the printed bit patterns).
//!
//! Row: `rl CW CH DUPUID LAYERS => EFFS` (grammar in the fixture header).

use super::test_codec::{format_resolved_list, parse_layers};
use super::{
    effective_effectors, resolve_layer_effector, resolved_effectors_for, EffectorLayer,
    EffectorLayerSamples, MotionDelta,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{hex, rows};
use crate::evaluation::object_component_kernels::object_effector_falloff::{
    Channel, Effector, EffectorPos, Falloff,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-effector-layer/oracle.txt");

/// One `rl` row.
pub(crate) struct LayerRow {
    pub canvas: [f64; 2],
    pub uid: Option<&'static str>,
    pub layers: Vec<EffectorLayer<'static>>,
    pub expected: &'static str,
}

pub(crate) fn uid_of(tok: &'static str) -> Option<&'static str> {
    match tok {
        "-" => None,
        "~" => Some(""),
        t => Some(t),
    }
}

pub(crate) fn layer_rows() -> Vec<LayerRow> {
    rows(ORACLE, "rl")
        .iter()
        .map(|t| LayerRow {
            canvas: [hex(t[0]), hex(t[1])],
            uid: uid_of(t[2]),
            layers: parse_layers(t[3]),
            expected: t[5],
        })
        .collect()
}

#[test]
fn layer_rows_match_effector_layer_js() {
    let all = layer_rows();
    assert_eq!(all.len(), 389);
    for (n, row) in all.iter().enumerate() {
        let got = resolved_effectors_for(row.uid, &row.layers, row.canvas[0], row.canvas[1]);
        assert_eq!(format_resolved_list(&got), row.expected, "row {n}");
    }
}

fn v(x: f64) -> JsNumber {
    JsNumber::Value(x)
}

/// A layer that targets `dupA` with a position offset of `[120, 0]` and every
/// other property sampled as null.
fn base_layer() -> EffectorLayer<'static> {
    EffectorLayer {
        is_effector_layer: true,
        target_uid: Some("dupA"),
        samples: EffectorLayerSamples {
            off_position: Some(vec![v(120.0), v(0.0)]),
            ..EffectorLayerSamples::default()
        },
        ..EffectorLayer::default()
    }
}

fn resolved(pos: (f64, f64), falloff: Falloff, p: [f64; 3], channels: Vec<Channel>) -> Effector {
    Effector {
        pos: Some(EffectorPos { x: pos.0, y: pos.1 }),
        falloff,
        radius: v(p[0]),
        strength: v(p[1]),
        angle: v(p[2]),
        channels: Some(channels),
        from_layer: true,
        ..Effector::default()
    }
}

fn position_120() -> Vec<Channel> {
    vec![Channel::new("position", vec![v(120.0), v(0.0)])]
}

/// (canvas, duplicator uid, layers, the effectors the baseline returns).
type Pin = (
    [f64; 2],
    Option<&'static str>,
    Vec<EffectorLayer<'static>>,
    Vec<Effector>,
);

fn pins() -> Vec<Pin> {
    let hd = [1920.0, 1080.0];
    let default_effector = resolved(
        (960.0, 540.0),
        Falloff::Radial,
        [200.0, 100.0, 0.0],
        position_120(),
    );
    let tuned = EffectorLayer {
        pos: Some([10.0, 20.0]),
        falloff: Some("linear"),
        motion: Some(MotionDelta {
            dx: v(5.0),
            dy: v(-5.0),
        }),
        samples: EffectorLayerSamples {
            radius: Some(vec![v(300.0)]),
            strength: Some(vec![v(50.0)]),
            angle: Some(vec![v(90.0)]),
            off_rotation: Some(vec![v(15.0)]),
            ..EffectorLayerSamples::default()
        },
        ..base_layer()
    };
    let silent = EffectorLayer {
        samples: EffectorLayerSamples::default(),
        ..base_layer()
    };
    let kind = |label: &'static str| EffectorLayer {
        falloff: Some(label),
        ..base_layer()
    };
    let centred = |f: Falloff| resolved((500.0, 250.0), f, [200.0, 100.0, 0.0], position_120());
    let off = EffectorLayer {
        visible: Some(false),
        ..base_layer()
    };
    let unflagged = EffectorLayer {
        is_effector_layer: false,
        ..base_layer()
    };
    vec![
        (hd, Some("dupA"), vec![], vec![]),
        (
            hd,
            Some("dupA"),
            vec![base_layer()],
            vec![default_effector.clone()],
        ),
        // No uid, or an empty one, selects nothing.
        (hd, None, vec![base_layer()], vec![]),
        (hd, Some(""), vec![base_layer()], vec![]),
        // Every offset sampled as null: the layer contributes nothing.
        (hd, Some("dupA"), vec![silent], vec![]),
        (
            [800.0, 600.0],
            Some("dupA"),
            vec![tuned],
            vec![resolved(
                (15.0, 15.0),
                Falloff::Linear,
                [300.0, 50.0, 90.0],
                vec![Channel::new("rotation", vec![v(15.0)])],
            )],
        ),
        // Layer order is kept; an unknown label is radial.
        (
            [1000.0, 500.0],
            Some("dupA"),
            vec![kind("step"), kind("linear"), kind("bogus")],
            vec![
                centred(Falloff::Step),
                centred(Falloff::Linear),
                centred(Falloff::Radial),
            ],
        ),
        // Only an explicit `visible: false` hides a layer; the target and the
        // layer-kind flag must match.
        (
            hd,
            Some("dupA"),
            vec![
                off,
                base_layer(),
                EffectorLayer {
                    visible: Some(true),
                    ..base_layer()
                },
                EffectorLayer {
                    target_uid: Some("dupB"),
                    ..base_layer()
                },
                unflagged.clone(),
                unflagged,
            ],
            vec![default_effector.clone(), default_effector],
        ),
        (
            hd,
            Some("dupA"),
            vec![EffectorLayer {
                target_uid: None,
                ..base_layer()
            }],
            vec![],
        ),
    ]
}

#[test]
fn pinned_layers_match_the_kernel_and_the_oracle() {
    let oracle = layer_rows();
    for (n, (canvas, uid, layers, want)) in pins().iter().enumerate() {
        let got = resolved_effectors_for(*uid, layers, canvas[0], canvas[1]);
        assert_eq!(&got, want, "pin {n} kernel");
        let row = oracle
            .iter()
            .find(|r| r.canvas == *canvas && r.uid == *uid && &r.layers == layers)
            .unwrap_or_else(|| panic!("pin {n} is not in the oracle"));
        assert_eq!(row.expected, format_resolved_list(want), "pin {n} oracle");
    }
}

#[test]
fn a_single_layer_resolves_like_the_list_entry_point() {
    let layer = base_layer();
    let one = resolve_layer_effector(&layer, 1920.0, 1080.0).expect("a nonzero channel");
    let listed = resolved_effectors_for(Some("dupA"), &[layer], 1920.0, 1080.0);
    assert_eq!(listed, vec![one.clone()]);
    assert!(one.from_layer);
    let none = EffectorLayer {
        samples: EffectorLayerSamples::default(),
        ..base_layer()
    };
    assert_eq!(resolve_layer_effector(&none, 1920.0, 1080.0), None);
}

#[test]
fn inline_effectors_come_before_layer_effectors() {
    let inline = vec![
        Effector {
            radius: v(1.0),
            ..Effector::default()
        },
        Effector {
            radius: v(2.0),
            ..Effector::default()
        },
    ];
    let layers =
        resolved_effectors_for(Some("dupA"), &[base_layer(), base_layer()], 1920.0, 1080.0);
    let all = effective_effectors(&inline, layers.clone());
    assert_eq!(all.len(), 4);
    assert_eq!(&all[..2], inline.as_slice());
    assert_eq!(&all[2..], layers.as_slice());
    // No layer effectors: the list is the inline one, unchanged.
    assert_eq!(effective_effectors(&inline, vec![]), inline);
}

#[test]
fn oracle_covers_the_layer_families() {
    let all = layer_rows();
    assert!(all.iter().filter(|r| r.expected != "e").count() >= 140);
    assert!(
        all.iter()
            .filter(|r| r.expected.matches(';').count() >= 1)
            .count()
            >= 50
    );
    let layers = || all.iter().flat_map(|r| r.layers.iter());
    assert!(layers().filter(|l| l.visible == Some(false)).count() >= 30);
    assert!(layers().filter(|l| !l.is_effector_layer).count() >= 30);
    assert!(layers().filter(|l| l.motion.is_some()).count() >= 80);
    assert!(layers().filter(|l| l.pos.is_none()).count() >= 80);
    assert!(layers().filter(|l| l.target_uid != Some("dupA")).count() >= 50);
    assert!(
        all.iter()
            .filter(|r| r.uid.is_none() || r.uid == Some(""))
            .count()
            >= 30
    );
    for label in [Some("linear"), Some("step"), Some("bogus"), None] {
        assert!(
            layers().filter(|l| l.falloff == label).count() >= 20,
            "{label:?}"
        );
    }
    for f in ["linear", "step", "radial"] {
        assert!(
            all.iter()
                .filter(|r| r.expected.contains(&format!(",{f},")))
                .count()
                >= 15,
            "{f}"
        );
    }
}
