//! Closed wire scalars of the paint-bucket command; no geometry or state authority.
use serde_json::Value;
#[derive(Clone, Copy)]
pub(super) enum Field {
    Point,
    Color,
    Opacity,
    Flag,
}
pub(super) fn kind(token: &str) -> Option<Field> {
    match token {
        "point" => Some(Field::Point),
        "color" => Some(Field::Color),
        "opacity" => Some(Field::Opacity),
        "flag" => Some(Field::Flag),
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
    }
}
