//! Baseline Motion metadata. Values are descriptive, never another state owner.
use crate::object_document::{ExpressionControl, PropertyMetadata};
use serde_json::Number;

fn values(input: &[f64]) -> Vec<Number> {
    input
        .iter()
        .map(|v| {
            if v.fract() == 0.0 {
                Number::from(*v as i64)
            } else {
                Number::from_f64(*v).expect("finite catalog constant")
            }
        })
        .collect()
}

fn metadata(dimension: usize, unit: &'static str, default: Option<&[f64]>) -> PropertyMetadata {
    PropertyMetadata {
        dimension,
        unit,
        default_value: default.map(values),
        animatable: true,
        control_type: None,
    }
}

/// Frozen motion.js PROP_DIM/UNIT/DEFAULT plus its opt-in paint consumers.
/// A missing paint default means inherit the actual source paint, not zero.
pub(super) fn vertex_index(property: &str) -> Option<usize> {
    let digits=property.strip_prefix("vtx")?;
    if digits.is_empty() || !digits.bytes().all(|b|b.is_ascii_digit()) { return None; }
    digits.parse::<usize>().ok()
}

pub(super) fn builtin(property: &str) -> Option<PropertyMetadata> {
    Some(match property {
        "position" | "anchor" | "dupOffsetPos" => metadata(2, "px", Some(&[0., 0.])),
        "scale" => metadata(2, "%", Some(&[100., 100.])),
        "dupOffsetScale" => metadata(2, "%", Some(&[0., 0.])),
        "rotation" | "rotationX" | "rotationY" | "dupOffsetRot" | "dupOffsetHue"
        | "dupOffsetRotX" | "dupOffsetRotY" | "arcStart" => metadata(1, "°", Some(&[0.])),
        "positionZ" | "dupOffsetPosZ" | "cornerTL" | "cornerTR" | "cornerBR" | "cornerBL"
        | "starCorner" => metadata(1, "px", Some(&[0.])),
        "opacity" | "matteOn" | "trimEnd" | "pathInfluence" => metadata(1, "%", Some(&[100.])),
        "dupOffsetOpacity" | "parentBlend" | "trimStart" | "trimOffset" | "pathPercent"
        | "arcInner" => metadata(1, "%", Some(&[0.])),
        "arcSweep" => metadata(1, "°", Some(&[359.9])),
        "starInner" => metadata(1, "%", Some(&[50.])),
        "volume" => metadata(1, "", Some(&[1.])),
        "order" => metadata(1, "", Some(&[0.])),
        "timeRemap" => metadata(1, "f", Some(&[0.])),
        "timeLinkInOffset" | "timeLinkOutOffset" => {
            let mut value = metadata(1, "f", Some(&[0.]));
            value.animatable = false;
            value
        }
        "fillColor" | "strokeColor" => metadata(4, "", None),
        "strokeWidth" => metadata(1, "px", None),
        "brushSize" => metadata(1, "%", None),
        _ if vertex_index(property).is_some() => metadata(2,"px",Some(&[0.,0.])),
        _ => return None,
    })
}

pub(super) fn control(control: &ExpressionControl) -> Option<PropertyMetadata> {
    let mut value = match control.kind.as_str() {
        "number" | "checkbox" => metadata(1, "", Some(&[0.])),
        "angle" => metadata(1, "°", Some(&[0.])),
        "point" => metadata(2, "px", Some(&[0., 0.])),
        "color" => metadata(4, "", Some(&[255., 255., 255., 255.])),
        _ => return None,
    };
    if let Some(default) = control.default_value.value() {
        value.default_value = Some(default.clone());
    }
    value.control_type = Some(control.kind.clone());
    Some(value)
}
