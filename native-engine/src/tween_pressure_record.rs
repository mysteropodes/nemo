//! Pressure records use the same native centreline fields as editing and rendering.
use crate::object_document::{ObjectRecord, Presence, PressureFields, VectorExt};
use crate::tween_kernels::*;
use crate::request_receipts::DispatchErrorCode;

pub(super) fn set_pressure(record: &mut ObjectRecord, center: &[SegOut], widths: &[f64]) -> R<()> {
    let segments = native_segments(center)?;
    let center_segments = segments
        .into_iter()
        .zip(widths)
        .map(|(s, &w)| {
            Ok(crate::object_document::CenterSegment {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
                width: number(w)?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let mut ext = record
        .vector_ext
        .value()
        .cloned()
        .unwrap_or_else(VectorExt::default);
    ext.pressure = Presence::Present(PressureFields {
        center_segments: Presence::Present(center_segments),
        width_profile: Presence::Missing,
        stroke_profile: Presence::Missing,
        profile_base: Presence::Missing,
    });
    record.vector_ext = Presence::Present(ext);
    let outline = pressure_outline(center, widths).map_err(|error| match error {
        crate::object_document::pressure_geometry::Error::InvalidRequest => {
            DispatchErrorCode::InvalidRequest
        }
        crate::object_document::pressure_geometry::Error::Unavailable => {
            DispatchErrorCode::Unavailable
        }
    })?;
    if outline.len()
        > crate::object_document::object_group_kernels::object_boolean_ops::stack::MAX_SEGMENTS
    {
        return Err(crate::request_receipts::DispatchErrorCode::Unavailable);
    }
    record.geometry.segments = native_segments(&outline)?;
    record.geometry.closed = true;
    Ok(())
}

pub(super) fn interpolate_pressure(
    record: &mut ObjectRecord,
    center: &[SegOut],
    a: &[f64],
    b: &[f64],
    t: f64,
) -> R<()> {
    let width = |w: f64| if w == 0. { 1. } else { w };
    let widths: Vec<_> = a
        .iter()
        .zip(b)
        .map(|(&a, &b)| width(a) + (width(b) - width(a)) * t)
        .collect();
    set_pressure(record, center, &widths)?;
    if let Presence::Present(v) = &mut record.vector {
        v.stroke = Presence::Present(None);
        v.stroke_width = 3.into();
        v.stroke_cap = "round".into();
        v.stroke_join = "round".into();
    }
    Ok(())
}
