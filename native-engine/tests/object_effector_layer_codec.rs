//! Token codec for the effector layer oracle rows (L04-C).
//!
//! A layer is one space-free token `KIND,TARGET,VISIBLE,BASEPOS,FALLOFF,
//! MOTION,S0..S9` (see the header of `fixtures/object-effector-layer/
//! oracle.txt`); a resolved effector prints as `X/Y,KIND,RADIUS,STRENGTH,
//! ANGLE,CHANNELS`. Reading and printing only, no computation.

use super::{EffectorLayer, EffectorLayerSamples, MotionDelta, Sampled};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{
    format_channels, hex, hex_of, js, label, parse_nums,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::Effector;

/// A string field: `-` undefined, `~` empty, else the text.
fn text(tok: &'static str) -> Option<&'static str> {
    match tok {
        "-" => None,
        "~" => Some(""),
        t => Some(t),
    }
}

/// `-` is a null `valueAtFrame` result, otherwise a list.
fn sample(tok: &str) -> Sampled {
    if tok == "-" {
        None
    } else {
        Some(parse_nums(tok))
    }
}

/// One layer token.
pub(crate) fn parse_layer(tok: &'static str) -> EffectorLayer<'static> {
    let f: Vec<&'static str> = tok.split(',').collect();
    assert_eq!(f.len(), 16, "layer token {tok}");
    let samples: Vec<Sampled> = f[6..16].iter().map(|t| sample(t)).collect();
    EffectorLayer {
        is_effector_layer: f[0] == "1",
        target_uid: text(f[1]),
        visible: match f[2] {
            "t" => Some(true),
            "f" => Some(false),
            _ => None,
        },
        pos: if f[3] == "-" {
            None
        } else {
            let (x, y) = f[3].split_once('/').expect("x/y");
            Some([hex(x), hex(y)])
        },
        falloff: label(f[4]),
        motion: if f[5] == "-" {
            None
        } else {
            let (dx, dy) = f[5].split_once('/').expect("dx/dy");
            Some(MotionDelta {
                dx: js(dx),
                dy: js(dy),
            })
        },
        samples: EffectorLayerSamples {
            radius: samples[0].clone(),
            strength: samples[1].clone(),
            angle: samples[2].clone(),
            off_position: samples[3].clone(),
            off_rotation: samples[4].clone(),
            off_scale: samples[5].clone(),
            off_opacity: samples[6].clone(),
            off_position_z: samples[7].clone(),
            off_rotation_x: samples[8].clone(),
            off_rotation_y: samples[9].clone(),
        },
    }
}

/// `e` or layers joined by `;`.
pub(crate) fn parse_layers(tok: &'static str) -> Vec<EffectorLayer<'static>> {
    if tok == "e" {
        return Vec::new();
    }
    tok.split(';').map(parse_layer).collect()
}

/// A resolved effector in the oracle's notation.
pub(crate) fn format_resolved(e: &Effector) -> String {
    let pos = e.pos.expect("resolved effectors carry a position");
    let channels = e
        .channels
        .as_deref()
        .expect("resolved effectors carry channels");
    let num = |n: JsNumber| match n {
        JsNumber::Value(v) => hex_of(v),
        other => panic!("resolved numbers are plain values, got {other:?}"),
    };
    format!(
        "{}/{},{},{},{},{},{}",
        hex_of(pos.x),
        hex_of(pos.y),
        e.falloff.label(),
        num(e.radius),
        num(e.strength),
        num(e.angle),
        format_channels(channels)
    )
}

/// `e` or resolved effectors joined by `;`.
pub(crate) fn format_resolved_list(list: &[Effector]) -> String {
    if list.is_empty() {
        return "e".to_string();
    }
    list.iter()
        .map(format_resolved)
        .collect::<Vec<_>>()
        .join(";")
}
