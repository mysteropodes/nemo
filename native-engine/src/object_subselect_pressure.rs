//! Canonical pressure-record projection. Rebuild one ribbon while retaining its authored pressure metadata.
use crate::object_document::pressure_geometry::{self as geometry, SegOut};
use crate::object_document::subselect::record::{keep, pair, point};
use crate::object_document::subselect::{edit, Anchor, Intent, Refusal};
use crate::object_document::{
    CenterSegment, ObjectRecord, ObjectSegment, Presence, PressureFields,
};

fn fields(record: &ObjectRecord) -> Result<&PressureFields, Refusal> {
    if record.family != crate::object_document::VECTOR_FAMILY {
        return Err(Refusal::Unsupported);
    }
    if !(2..=256).contains(&record.geometry.segments.len()) {
        return Err(Refusal::Limit);
    }
    if !record.geometry.closed {
        return Err(Refusal::Unsupported);
    }
    let ext = record.vector_ext.value().ok_or(Refusal::Unsupported)?;
    // These require complete companion/texture/Fill Brush projection, not just a new ribbon.
    if !ext.linked_fill_id.missing()
        || ext.is_linked_fill_companion.value() == Some(&true)
        || !ext.bitmap_brush.missing()
        || !ext.brush_texture.missing()
        || ext.is_fill_shape.value() == Some(&true)
        || !ext.param_shape.missing()
    {
        return Err(Refusal::Unsupported);
    }
    ext.pressure.value().ok_or(Refusal::Unsupported)
}

pub(super) fn anchors(record: &ObjectRecord) -> Result<Vec<Anchor>, Refusal> {
    let center = fields(record)?
        .center_segments
        .value()
        .ok_or(Refusal::Unsupported)?;
    if !(2..=256).contains(&center.len()) {
        return Err(Refusal::Limit);
    }
    center
        .iter()
        .map(|s| {
            Ok(Anchor {
                point: pair(&s.point)?,
                handle_in: pair(&s.handle_in)?,
                handle_out: pair(&s.handle_out)?,
            })
        })
        .collect()
}

pub(super) fn plan(record: &ObjectRecord, intent: &Intent) -> Result<ObjectRecord, Refusal> {
    // The original insertion path loses pressure widths. Do not invent new width semantics here.
    if matches!(intent, Intent::Insert { .. }) {
        return Err(Refusal::Unsupported);
    }
    let source = fields(record)?;
    let old = source.center_segments.value().ok_or(Refusal::Unsupported)?;
    let before = anchors(record)?;
    let edited = edit(&before, record.geometry.closed, true, intent)?;
    if edited == before {
        return Ok(record.clone());
    }
    let mut origins: Vec<usize> = (0..old.len()).collect();
    if let Intent::Delete { indices } = intent {
        origins.retain(|i| !indices.contains(i));
    }
    let center: Vec<CenterSegment> = edited
        .iter()
        .zip(origins)
        .map(|(a, i)| {
            Ok(CenterSegment {
                point: keep(&old[i].point, a.point)?,
                handle_in: keep(&old[i].handle_in, a.handle_in)?,
                handle_out: keep(&old[i].handle_out, a.handle_out)?,
                width: old[i].width.clone(),
            })
        })
        .collect::<Result<_, Refusal>>()?;
    let segments: Vec<SegOut> = edited
        .iter()
        .map(|a| SegOut {
            point: a.point,
            handle_in: a.handle_in,
            handle_out: a.handle_out,
        })
        .collect();
    let widths: Vec<f64> = center
        .iter()
        .map(|s| s.width.as_f64().ok_or(Refusal::Invalid))
        .collect::<Result<_, _>>()?;
    let profile = source
        .width_profile
        .value()
        .map(|stops| {
            stops
                .iter()
                .map(|s| {
                    Ok((
                        s.t.as_f64().ok_or(Refusal::Invalid)?,
                        s.width.as_f64().ok_or(Refusal::Invalid)?,
                    ))
                })
                .collect::<Result<Vec<_>, Refusal>>()
        })
        .transpose()?;
    // An absent or empty profile falls back to the surviving anchor widths in the original sampler.
    let outline = match profile.as_deref().filter(|p| !p.is_empty()) {
        Some(profile) => geometry::profiled_outline(&segments, profile),
        None => geometry::pressure_outline(&segments, &widths),
    }
    .map_err(|e| match e {
        geometry::Error::InvalidRequest => Refusal::Invalid,
        geometry::Error::Unavailable => Refusal::Unsupported,
    })?;
    if outline.len()
        > crate::object_document::object_group_kernels::object_boolean_ops::stack::MAX_SEGMENTS
    {
        return Err(Refusal::Limit);
    }
    let mut next = record.clone();
    next.geometry.segments = outline
        .iter()
        .map(|s| {
            Ok(ObjectSegment {
                point: point(s.point)?,
                handle_in: point(s.handle_in)?,
                handle_out: point(s.handle_out)?,
            })
        })
        .collect::<Result<_, Refusal>>()?;
    next.geometry.closed = true;
    let mut pressure = source.clone();
    pressure.center_segments = Presence::Present(center);
    if let Presence::Present(ext) = &mut next.vector_ext {
        ext.pressure = Presence::Present(pressure);
    }
    // applyBrushKeyline: the pressure ribbon keeps its fill and clears its own stroke.
    if let Presence::Present(vector) = &mut next.vector {
        if vector.stroke.value().is_some_and(Option::is_some) {
            vector.stroke = Presence::Present(None);
            vector.has_real_stroke = false;
        }
    }
    Ok(next)
}
