//! Row codec and drive helpers for the clone placement oracle tests (L04-C).
//!
//! A `pl` row is `SEED K COUNT PIVX PIVY BDX BDY BROT DPX DPY DROT DSX DSY DOP
//! DHUE DZ DRX DRY MASK IS3D EFFS CW CH UID LAYERS => dx dy rot sx sy
//! opacityFactor hueDeg D3` (grammar in the header of `fixtures/
//! duplicator-placement-clone/oracle.txt`). Reading and driving only: the
//! effector list a row implies is assembled here exactly as the baseline
//! does (`dup.effectors`, plus the resolved effector layers when the call has
//! an `ownerLd`), through the kernels under test.

use super::{clone_placement, CloneDeltas, ClonePlacement, ClonePlacementInput, Dup3D, RandMode};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{
    hex, js, parse_effectors, rows,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::Effector;
use crate::evaluation::object_component_kernels::object_effector_layer::test_codec::parse_layers;
use crate::evaluation::object_component_kernels::object_effector_layer::{
    effective_effectors, resolved_effectors_for, EffectorLayer,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-clone/oracle.txt");

/// The `ownerLd` argument of the baseline call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Owner {
    /// The nativeVideo call shape: no layer descriptor, layers are never read.
    Absent,
    /// A descriptor whose `layerUid` is the given value (`None` = undefined).
    Layer(Option<&'static str>),
}

/// Everything one call of the baseline function was given.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlInput {
    pub seed: JsNumber,
    pub k: u32,
    pub count: f64,
    pub pivot: [f64; 2],
    pub base: [f64; 3],
    pub deltas: CloneDeltas,
    pub rand: RandMode,
    pub is_3d: bool,
    pub effs: Vec<Effector>,
    pub canvas: [f64; 2],
    pub owner: Owner,
    pub layers: Vec<EffectorLayer<'static>>,
}

/// The expected descriptor: `dx dy rot sx sy opacityFactor hueDeg` and `dup3D`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlOut {
    pub values: [f64; 7],
    pub d3: Option<[f64; 3]>,
}

pub(crate) struct PlRow {
    pub input: PlInput,
    pub out: PlOut,
}

fn flag(tok: &str) -> bool {
    tok == "1"
}

fn owner_of(tok: &'static str) -> Owner {
    match tok {
        "@" => Owner::Absent,
        "-" => Owner::Layer(None),
        "~" => Owner::Layer(Some("")),
        t => Owner::Layer(Some(t)),
    }
}

pub(crate) fn pl_rows() -> Vec<PlRow> {
    rows(ORACLE, "pl")
        .iter()
        .map(|t| {
            let n = |i: usize| hex(t[i]);
            let mask: Vec<bool> = t[18].chars().map(|c| c == '1').collect();
            let mut values = [0.0; 7];
            for (slot, tok) in values.iter_mut().zip(&t[26..33]) {
                *slot = hex(tok);
            }
            PlRow {
                input: PlInput {
                    seed: js(t[0]),
                    k: t[1].parse().expect("k"),
                    count: n(2),
                    pivot: [n(3), n(4)],
                    base: [n(5), n(6), n(7)],
                    deltas: CloneDeltas {
                        pos: [n(8), n(9)],
                        rot: n(10),
                        scale: [n(11), n(12)],
                        opacity: n(13),
                        hue: js(t[14]),
                        pos_z: n(15),
                        rot_x: n(16),
                        rot_y: n(17),
                    },
                    rand: RandMode {
                        position: mask[0],
                        rotation: mask[1],
                        scale: mask[2],
                        opacity: mask[3],
                        hue: mask[4],
                    },
                    is_3d: flag(t[19]),
                    effs: parse_effectors(t[20]),
                    canvas: [n(21), n(22)],
                    owner: owner_of(t[23]),
                    layers: parse_layers(t[24]),
                },
                out: PlOut {
                    values,
                    d3: if t[33] == "-" {
                        None
                    } else {
                        let v: Vec<f64> = t[33].split('/').map(hex).collect();
                        Some([v[0], v[1], v[2]])
                    },
                },
            }
        })
        .collect()
}

/// `dup.effectors`, then the resolved effector layers when there is an owner.
pub(crate) fn effective(input: &PlInput) -> Vec<Effector> {
    match input.owner {
        Owner::Absent => input.effs.clone(),
        Owner::Layer(uid) => effective_effectors(
            &input.effs,
            resolved_effectors_for(uid, &input.layers, input.canvas[0], input.canvas[1]),
        ),
    }
}

/// The kernel input of a row over a given effector list.
pub(crate) fn kernel_input<'a>(input: &PlInput, effs: &'a [Effector]) -> ClonePlacementInput<'a> {
    ClonePlacementInput {
        seed: input.seed,
        k: input.k,
        count: input.count,
        pivot: input.pivot,
        base: input.base,
        deltas: input.deltas,
        rand: input.rand,
        is_3d_layer: input.is_3d,
        effectors: effs,
    }
}

/// The kernel's answer for a row.
pub(crate) fn place(input: &PlInput) -> ClonePlacement {
    let effs = effective(input);
    clone_placement(&kernel_input(input, &effs))
}

/// The descriptor in the oracle's field order.
pub(crate) fn values_of(p: &ClonePlacement) -> [f64; 7] {
    [p.dx, p.dy, p.rot, p.sx, p.sy, p.opacity_factor, p.hue_deg]
}

pub(crate) fn d3_of(p: &ClonePlacement) -> Option<[f64; 3]> {
    p.dup_3d.map(|Dup3D { dz, drx, dry }| [dz, drx, dry])
}
