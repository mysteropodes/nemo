//! L01-D (G14): admission of the `editable-vector-path` family. Pure checks over the decoded record;
//! the single native codec (object_codec.rs) delegates here for this family and stays byte-identical
//! for the legacy `closed-solid-fill-cubic-path` family. Every refusal is a typed codec error;
//! nothing is clamped, defaulted or inferred (no stroke is derived from a fallback colour).
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{ObjectRecord, SolidFill, VectorBody};
use serde_json::Number;

/// Same anchor bound as the legacy family (object_codec.rs); raising it is a lead decision.
const MAX_SEGMENTS: usize = 256;
const STROKE_CAPS: [&str; 3] = ["butt", "round", "square"];
const STROKE_JOINS: [&str; 3] = ["miter", "round", "bevel"];
const PAINT_ORDERS: [&str; 2] = ["fillFirst", "strokeFirst"];

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}
fn unsupported(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Unsupported, message)
}
fn finite(value: &Number) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}
fn non_negative(value: &Number) -> Option<f64> {
    finite(value).filter(|v| *v >= 0.0)
}

/// Admit one record of family `editable-vector-path` (schema and target are checked by the caller).
pub(crate) fn validate_vector_record(record: &ObjectRecord) -> Result<(), ObjectCodecError> {
    validate_geometry(record)?;
    match record.fill.value() {
        None => {
            return Err(invalid(
                "editable-vector-path requires an explicit fill (null or solid)",
            ))
        }
        Some(None) => {}
        Some(Some(fill)) => validate_paint(fill)?,
    }
    let body = record
        .vector
        .value()
        .ok_or_else(|| invalid("editable-vector-path requires the vector body"))?;
    validate_body(body)
}

fn validate_geometry(record: &ObjectRecord) -> Result<(), ObjectCodecError> {
    // `closed` is free: open and closed are independent of the anchors.
    let geometry = &record.geometry;
    if geometry.kind != "cubic-path"
        || geometry.coordinate_space != "document-world"
        || geometry.handle_space != "relative"
    {
        return Err(unsupported("unsupported cubic geometry"));
    }
    if !(2..=MAX_SEGMENTS).contains(&geometry.segments.len()) {
        return Err(invalid("cubic geometry requires 2..256 segments"));
    }
    for segment in &geometry.segments {
        for point in [&segment.point, &segment.handle_in, &segment.handle_out] {
            if finite(&point.x).is_none() || finite(&point.y).is_none() {
                return Err(invalid("cubic coordinates must be finite f64 values"));
            }
        }
    }
    Ok(())
}

fn validate_paint(paint: &SolidFill) -> Result<(), ObjectCodecError> {
    if paint.kind != "solid" {
        return Err(unsupported("only solid paint is supported"));
    }
    if !paint.valid_channels() {
        return Err(invalid(
            "solid paint channels must be finite and within 0..1",
        ));
    }
    Ok(())
}

fn validate_body(body: &VectorBody) -> Result<(), ObjectCodecError> {
    let stroke = match body.stroke.value() {
        None => {
            return Err(invalid(
                "vector body requires an explicit stroke (null or solid)",
            ))
        }
        Some(stroke) => stroke.as_ref(),
    };
    if let Some(paint) = stroke {
        validate_paint(paint)?;
    }
    // hasRealStroke is stored, but it can never disagree with the stroke slot: a fallback colour is
    // not an authored stroke and a real stroke needs its paint.
    if stroke.is_some() != body.has_real_stroke {
        return Err(invalid(
            "hasRealStroke must be true exactly when a stroke is present",
        ));
    }
    if non_negative(&body.stroke_width).is_none() {
        return Err(invalid("strokeWidth must be finite and not negative"));
    }
    if !STROKE_CAPS.contains(&body.stroke_cap.as_str()) {
        return Err(invalid("strokeCap must be butt, round or square"));
    }
    if !STROKE_JOINS.contains(&body.stroke_join.as_str()) {
        return Err(invalid("strokeJoin must be miter, round or bevel"));
    }
    if body
        .miter_limit
        .value()
        .is_some_and(|limit| non_negative(limit).is_none())
    {
        return Err(invalid("miterLimit must be finite and not negative"));
    }
    if let Some(dashes) = body.dash_array.value() {
        let mut total = 0.0;
        for dash in dashes {
            total += non_negative(dash)
                .ok_or_else(|| invalid("dashArray entries must be finite and not negative"))?;
        }
        if dashes.is_empty() || !(total > 0.0 && total.is_finite()) {
            return Err(invalid(
                "dashArray must be a non-empty pattern with a positive finite length",
            ));
        }
    }
    if body
        .dash_offset
        .value()
        .is_some_and(|offset| finite(offset).is_none())
    {
        return Err(invalid("dashOffset must be finite"));
    }
    if body
        .paint_order
        .value()
        .is_some_and(|order| !PAINT_ORDERS.contains(&order.as_str()))
    {
        return Err(invalid("paintOrder must be fillFirst or strokeFirst"));
    }
    if !finite(&body.opacity).is_some_and(|v| (0.0..=1.0).contains(&v)) {
        return Err(invalid("opacity must be finite and within 0..1"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_vector_record.rs"]
mod object_vector_record_tests;
