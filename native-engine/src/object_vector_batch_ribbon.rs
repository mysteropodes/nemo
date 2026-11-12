//! #2362 vector sculpt on a pressure ribbon. The baseline sculpt moves the ribbon's centreline
//! (`data.centerSegments`, anchors it may densify under the brush) and `rebuildVectorBrushOutline`
//! regenerates the ribbon from it with the stroke's own width profile, then re-syncs its linked fill
//! backdrop to that same centreline (tools.js @1065e9d8). The outline comes from the shared pressure
//! producer, exactly as Subselect and Trim derive it.
use crate::object_document::object_group_kernels::object_boolean_ops::stack::MAX_SEGMENTS;
use crate::object_document::pressure_geometry::{pressure_outline, profiled_outline, SegOut};
use crate::object_document::{
    CenterSegment, ObjectDocument, ObjectPoint, ObjectRecord, ObjectSegment, ObjectTarget, Presence,
};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

fn xy(p: &ObjectPoint) -> Option<[f64; 2]> {
    Some([p.x.as_f64()?, p.y.as_f64()?])
}

fn point(p: [f64; 2]) -> Option<ObjectPoint> {
    Some(ObjectPoint {
        x: Number::from_f64(p[0])?,
        y: Number::from_f64(p[1])?,
    })
}

/// The ribbon at `target` rebuilt on `center`, and its linked fill in the same cell re-synced to it.
pub(super) fn reshape(
    source: &ObjectDocument,
    target: &ObjectTarget,
    center: Vec<CenterSegment>,
) -> Result<(ObjectRecord, Option<ObjectRecord>), DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let existing = source
        .objects
        .iter()
        .find(|r| &r.target == target)
        .ok_or(DispatchErrorCode::NotFound)?;
    let ext = existing.vector_ext.value().ok_or(InvalidRequest)?;
    // A texture anchor, bitmap brush, fill or parametric shape derives its paint elsewhere.
    if existing.family != crate::object_document::VECTOR_FAMILY
        || !existing.geometry.closed
        || ext.is_linked_fill_companion.value() == Some(&true)
        || ext.is_fill_shape.value() == Some(&true)
        || !ext.param_shape.missing()
        || !ext.bitmap_brush.missing()
        || !ext.brush_texture.missing()
        || !(2..=256).contains(&center.len())
    {
        return Err(InvalidRequest);
    }
    rebuild(source, existing, center)
}

/// A pressure ribbon rebuilt on `center` from its own width profile, and its linked fill re-synced to it. The caller has
/// decided that this record may be rebuilt (a sculpt refuses a texture anchor; a Select transform carries it).
pub(super) fn rebuild(
    source: &ObjectDocument,
    existing: &ObjectRecord,
    center: Vec<CenterSegment>,
) -> Result<(ObjectRecord, Option<ObjectRecord>), DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let target = &existing.target;
    let ext = existing.vector_ext.value().ok_or(InvalidRequest)?;
    let pressure = ext.pressure.value().ok_or(InvalidRequest)?;
    if pressure.center_segments.missing() || !(2..=256).contains(&center.len()) {
        return Err(InvalidRequest);
    }
    let segments: Vec<SegOut> = center
        .iter()
        .map(|c| {
            Some(SegOut {
                point: xy(&c.point)?,
                handle_in: xy(&c.handle_in)?,
                handle_out: xy(&c.handle_out)?,
            })
        })
        .collect::<Option<_>>()
        .ok_or(InvalidRequest)?;
    let widths: Vec<f64> = center
        .iter()
        .map(|c| c.width.as_f64().filter(|w| w.is_finite()))
        .collect::<Option<_>>()
        .ok_or(InvalidRequest)?;
    let profile = match pressure.width_profile.value() {
        Some(stops) => Some(
            stops
                .iter()
                .map(|s| Some((s.t.as_f64()?, s.width.as_f64()?)))
                .collect::<Option<Vec<_>>>()
                .ok_or(InvalidRequest)?,
        ),
        None => None,
    };
    // An absent or empty profile falls back to the anchor widths, as the baseline sampler does.
    let outline = match profile.as_deref().filter(|p| !p.is_empty()) {
        Some(profile) => profiled_outline(&segments, profile),
        None => pressure_outline(&segments, &widths),
    }
    .map_err(|_| DispatchErrorCode::Unavailable)?;
    if outline.len() > MAX_SEGMENTS {
        return Err(DispatchErrorCode::Unavailable);
    }
    let native = |s: &SegOut| {
        Some(ObjectSegment {
            point: point(s.point)?,
            handle_in: point(s.handle_in)?,
            handle_out: point(s.handle_out)?,
        })
    };
    let mut ribbon = existing.clone();
    ribbon.geometry.segments = outline
        .iter()
        .map(native)
        .collect::<Option<_>>()
        .ok_or(InvalidRequest)?;
    // The fill takes the centreline exactly as sent (tools.js: linkedFill.segments = sampled.centerSegments).
    let sides: Vec<ObjectSegment> = center
        .iter()
        .map(|c| ObjectSegment {
            point: c.point.clone(),
            handle_in: c.handle_in.clone(),
            handle_out: c.handle_out.clone(),
        })
        .collect();
    let mut fields = pressure.clone();
    fields.center_segments = Presence::Present(center);
    if let Presence::Present(ext) = &mut ribbon.vector_ext {
        ext.pressure = Presence::Present(fields);
    }
    // applyBrushKeyline: the pressure ribbon keeps its fill and clears its own stroke.
    if let Presence::Present(vector) = &mut ribbon.vector {
        if vector.stroke.value().is_some_and(Option::is_some) {
            vector.stroke = Presence::Present(None);
            vector.has_real_stroke = false;
        }
    }
    let fill = ext.linked_fill_id.value().and_then(|link| {
        source.objects.iter().find(|r| {
            r.target.context_id == target.context_id
                && r.target.layer_uid == target.layer_uid
                && r.target.frame_scope == target.frame_scope
                && r.vector_ext.value().is_some_and(|e| {
                    e.is_linked_fill_companion.value() == Some(&true)
                        && e.linked_fill_id.value() == Some(link)
                })
        })
    });
    let fill = match fill {
        Some(found) => {
            let mut fill = found.clone();
            fill.geometry.segments = sides;
            fill.geometry.closed = true;
            Some(fill)
        }
        None => None,
    };
    Ok((ribbon, fill))
}
