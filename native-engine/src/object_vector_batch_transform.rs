//! #2372 Select move / scale / rotate of the stored vector records a batch `reshape` cannot rebuild from geometry alone.
//! A `reshape` with a `matrix` (the world gesture conjugated into the record's own space by the client) maps ONE named
//! record by that affine and carries what belongs to it, as the baseline select-bridge did to the Paper item and its
//! companions: a pressure ribbon's centreline is mapped (its widths follow the mean axis scale) and the outline rebuilt
//! from it with the stored width profile, the ribbon's linked fill is re-synced to that centreline, a texture anchor
//! takes its stamped copies (dabs) with it, a gradient fill takes its end points, and a dynamic shape goes through the
//! translate / scale kernels so its box and rect radii stay consistent. A bitmap brush derives its image from a baked
//! PNG and is refused. The result is the list of records to replace, in the cell the target names.
use crate::commands::object_fill_owner::parametric_edit;
use crate::commands::object_fill_owner::vector_edit::batch_ops::ribbon;
use crate::commands::object_fill_owner::vector_edit::is_vector_record;
use crate::object_document::{CenterSegment, ObjectDocument, ObjectGeometry, ObjectPoint, ObjectRecord, ObjectTarget, Presence, VectorExt};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

/// One transform names its record and the copies of its texture group; both are bounded like any gesture.
pub(super) const MAX_COPIES: usize = 512;

fn num(v: f64) -> Result<Number, DispatchErrorCode> {
    Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest)
}
fn xy(p: &ObjectPoint) -> Result<[f64; 2], DispatchErrorCode> {
    Ok([p.x.as_f64().ok_or(DispatchErrorCode::InvalidRequest)?, p.y.as_f64().ok_or(DispatchErrorCode::InvalidRequest)?])
}
fn point(p: [f64; 2]) -> Result<ObjectPoint, DispatchErrorCode> {
    Ok(ObjectPoint { x: num(p[0])?, y: num(p[1])? })
}
/// The affine of a point, or (`origin` false) of a relative handle.
fn apply(m: &[f64; 6], p: &ObjectPoint, origin: bool) -> Result<ObjectPoint, DispatchErrorCode> {
    let [x, y] = xy(p)?;
    let t = if origin { 1.0 } else { 0.0 };
    point([m[0] * x + m[2] * y + t * m[4], m[1] * x + m[3] * y + t * m[5]])
}
fn map_geometry(g: &mut ObjectGeometry, m: &[f64; 6]) -> Result<(), DispatchErrorCode> {
    if g.handle_space != "relative" {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    for s in &mut g.segments {
        (s.point, s.handle_in, s.handle_out) = (apply(m, &s.point, true)?, apply(m, &s.handle_in, false)?, apply(m, &s.handle_out, false)?);
    }
    Ok(())
}
fn map_gradient(record: &mut ObjectRecord, m: &[f64; 6]) -> Result<(), DispatchErrorCode> {
    if let Some(gradient) = record.vector_ext.value_mut().and_then(|e| e.fill_gradient.value_mut()) {
        for end in [&mut gradient.from, &mut gradient.to] {
            let p = apply(m, &point([end[0].as_f64().ok_or(DispatchErrorCode::InvalidRequest)?, end[1].as_f64().ok_or(DispatchErrorCode::InvalidRequest)?])?, true)?;
            *end = [p.x, p.y];
        }
    }
    Ok(())
}
/// Every stored piece of one record that follows the affine when its outline is not rebuilt: the geometry and a gradient.
fn map_record(record: &mut ObjectRecord, m: &[f64; 6]) -> Result<(), DispatchErrorCode> {
    map_geometry(&mut record.geometry, m)?;
    map_gradient(record, m)
}
/// A turn adds to the record's stored `boxAngle` (baseline: every rotate gesture adds its step to `data.boxAngle` of each selected
/// stroke, and `serP` keeps it), so the oriented selection box survives a frame change, an undo and a reload. Only a pure turn does:
/// a move, a scale (along the box axes or not) and a mirror leave the orientation as it is. An angle that returns to 0 is dropped
/// as `serP` drops a falsy one, and an extension left empty is dropped with it.
fn turn_box(record: &mut ObjectRecord, m: &[f64; 6]) -> Result<(), DispatchErrorCode> {
    const EPS: f64 = 1e-9;
    let [a, b, c, d, ..] = *m;
    let turned = b.atan2(a).to_degrees();
    if !((a - d).abs() < EPS && (b + c).abs() < EPS && (a.hypot(b) - 1.0).abs() < 1e-6) || turned.abs() < EPS {
        return Ok(());
    }
    let before = record.vector_ext.value().and_then(|e| e.box_angle.value()).and_then(Number::as_f64).unwrap_or(0.0);
    let after = (before + turned) % 360.0;
    let mut ext = record.vector_ext.value().cloned().unwrap_or_default();
    ext.box_angle = if after.abs() < EPS { Presence::Missing } else { Presence::Present(num(after)?) };
    record.vector_ext = if ext.is_empty() { Presence::Missing } else { Presence::Present(ext) };
    Ok(())
}
fn same_cell(a: &ObjectRecord, t: &ObjectTarget) -> bool {
    a.target.context_id == t.context_id && a.target.layer_uid == t.layer_uid && a.target.frame_scope == t.frame_scope
}

/// The records `target` and its companions become under `m` (`x' = m0 x + m2 y + m4`, `y' = m1 x + m3 y + m5`, record
/// space), the named record first.
pub(super) fn transform(source: &ObjectDocument, target: &ObjectTarget, m: [f64; 6]) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let det = m[0] * m[3] - m[1] * m[2];
    if !m.iter().all(|v| v.is_finite()) || !det.is_finite() || det.abs() < 1e-12 {
        return Err(InvalidRequest);
    }
    let existing = source.objects.iter().find(|r| &r.target == target).ok_or(DispatchErrorCode::NotFound)?;
    if !is_vector_record(existing) {
        return Err(InvalidRequest);
    }
    let ext = existing.vector_ext.value();
    if ext.is_some_and(|e| !e.bitmap_brush.missing()) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let texture = ext.and_then(|e| e.brush_texture.value());
    if texture.is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true)) {
        return Err(InvalidRequest);
    }
    let pressure = ext.and_then(|e| e.pressure.value()).filter(|p| !p.center_segments.missing());
    let derived = ext.is_some_and(|e| e.is_fill_shape.value() == Some(&true) || !e.param_shape.missing());
    let mut out = Vec::new();
    let mut fill_done = false;
    match (pressure, derived) {
        (Some(p), false) => {
            // The ribbon: centreline mapped, outline and linked fill rebuilt from it (baseline scaleCenterSegments + rebuildVectorBrushOutline).
            let k = (m[0].hypot(m[1]) + m[2].hypot(m[3])) / 2.0;
            let center = p.center_segments.value().ok_or(InvalidRequest)?;
            let moved = center
                .iter()
                .map(|c| Ok(CenterSegment { point: apply(&m, &c.point, true)?, handle_in: apply(&m, &c.handle_in, false)?,
                    handle_out: apply(&m, &c.handle_out, false)?, width: num(c.width.as_f64().ok_or(InvalidRequest)? * k)? }))
                .collect::<Result<Vec<_>, DispatchErrorCode>>()?;
            let (mut ribbon, fill) = ribbon::rebuild(source, existing, moved)?;
            map_gradient(&mut ribbon, &m)?;
            turn_box(&mut ribbon, &m)?;
            out.push(ribbon);
            if let Some(mut fill) = fill {
                map_gradient(&mut fill, &m)?;
                out.push(fill);
                fill_done = true;
            }
        }
        (_, true) if ext.is_some_and(|e| !e.param_shape.missing()) => {
            let mut shape = parametric_edit::transformed(existing, m)?;
            map_gradient(&mut shape, &m)?;
            out.push(shape);
        }
        _ => {
            let mut moved = existing.clone();
            map_record(&mut moved, &m)?;
            if let Some(p) = moved.vector_ext.value_mut().and_then(|e| e.pressure.value_mut()) {
                if let Some(center) = p.center_segments.value_mut() {
                    for c in center {
                        (c.point, c.handle_in, c.handle_out) = (apply(&m, &c.point, true)?, apply(&m, &c.handle_in, false)?, apply(&m, &c.handle_out, false)?);
                    }
                }
            }
            turn_box(&mut moved, &m)?;
            out.push(moved);
        }
    }
    // A fill that was not re-synced above (a ribbon without a rebuilt outline) and the stamped copies follow the same affine.
    let link = ext.and_then(|e| e.linked_fill_id.value());
    let group = texture.and_then(|t| t.brush_group_id.value());
    for record in source.objects.iter().filter(|r| same_cell(r, target) && r.target != *target && is_vector_record(r)) {
        let other = record.vector_ext.value();
        let fill = !fill_done && link.is_some() && other.is_some_and(|e| e.is_linked_fill_companion.value() == Some(&true) && e.linked_fill_id.value() == link);
        let copy = group.is_some() && other.and_then(|e| e.brush_texture.value()).is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true) && t.brush_group_id.value() == group);
        if fill || copy {
            let mut moved = record.clone();
            map_record(&mut moved, &m)?;
            out.push(moved);
        }
    }
    if out.len() > MAX_COPIES {
        return Err(InvalidRequest);
    }
    Ok(out)
}
