//! Closed wire scalars of the paint-bucket and Fill Brush commands; no geometry or state authority.
use serde_json::Value;
#[derive(Clone, Copy)]
pub(super) enum Field {
    Point,
    Color,
    Opacity,
    Flag,
    /// A Fill Brush outline: 3 to 256 closed-path segments, each point and relative handle finite.
    Outline,
    /// `state.fillBrushMode`.
    Placement,
    /// Mask mode's tag for the committed Fill Brush shape, or `false` when Mask mode is off.
    Mask,
}
pub(super) fn kind(token: &str) -> Option<Field> {
    match token {
        "point" => Some(Field::Point),
        "color" => Some(Field::Color),
        "opacity" => Some(Field::Opacity),
        "flag" => Some(Field::Flag),
        "outline" => Some(Field::Outline),
        "placement" => Some(Field::Placement),
        "mask" => Some(Field::Mask),
        _ => None,
    }
}
pub(super) fn check(field: Field, value: &Value) -> bool {
    match field {
        Field::Point => value.as_array().is_some_and(|p| {
            p.len() == 2
                && p.iter().all(|n| {
                    n.as_f64()
                        .is_some_and(|n| n.is_finite() && n.abs() <= 1.0e12)
                })
        }),
        Field::Color => value.as_str().is_some_and(|s| {
            s.len() == 9
                && s.starts_with('#')
                && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        }),
        Field::Opacity => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
        Field::Flag => value.is_boolean(),
        Field::Outline => value.as_array().is_some_and(|segments| {
            (3..=256).contains(&segments.len())
                && segments.iter().all(|s| {
                    s.as_object().is_some_and(|o| {
                        o.len() == 3
                            && ["point", "handleIn", "handleOut"]
                                .iter()
                                .all(|k| o.get(*k).is_some_and(|p| check(Field::Point, p)))
                    })
                })
        }),
        Field::Placement => value
            .as_str()
            .is_some_and(|mode| matches!(mode, "above" | "below" | "merge")),
        Field::Mask => {
            value == &Value::Bool(false)
                || value.as_str().is_some_and(|m| {
                    !m.is_empty() && m.len() <= 64 && !m.chars().any(char::is_control)
                })
        }
    }
}
