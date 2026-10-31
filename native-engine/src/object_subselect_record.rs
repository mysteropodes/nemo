//! Detached canonical-record projection of a subselect intent. Shared by preview and commit.
use crate::object_document::subselect::point_type::Kind;
use crate::object_document::subselect::{edit_typed, Anchor, Intent, Refusal};
use crate::object_document::{Presence, VectorExt};
use crate::object_document::{
    ObjectDocument, ObjectPoint, ObjectRecord, ObjectSegment, VECTOR_FAMILY,
};
use serde_json::Number;
#[path = "object_subselect_pressure.rs"]
mod pressure;
#[path = "object_subselect_style.rs"]
pub(crate) mod style;

/// The displayed frame may hold an earlier key. Admit promotion only when its
/// source and complete copied cell are representable by the native frame owner.
pub(crate) fn at_frame(
    document: &ObjectDocument,
    record: &ObjectRecord,
    frame: u32,
) -> Result<bool, Refusal> {
    use crate::object_document::{FrameScopeKind, RangeEvaluator};
    let target = &record.target;
    if frame >= document.total_frames
        || target.context_id != "scene-root"
        || target.frame_scope.kind != FrameScopeKind::Authored
    {
        return Err(Refusal::Invalid);
    }
    let layer = document
        .layers
        .iter()
        .find(|l| l.layer_uid == target.layer_uid)
        .ok_or(Refusal::Unsupported)?;
    let ranges = RangeEvaluator::for_document(document, "scene-root", frame)
        .map_err(|_| Refusal::Unsupported)?;
    if ranges
        .gates(&target.layer_uid)
        .map_err(|_| Refusal::Unsupported)?
        && !ranges
            .resolve(&target.layer_uid, true)
            .map_err(|_| Refusal::Unsupported)?
            .contains(frame)
    {
        return Err(Refusal::Unsupported);
    }
    if frame == target.frame_scope.frame {
        return Ok(false);
    }
    let timing = layer.timing.value().ok_or(Refusal::Unsupported)?;
    let (kind, source, _) = timing.exposure(frame);
    if kind != "held" || source != Some(target.frame_scope.frame) {
        return Err(Refusal::Unsupported);
    }
    let in_cell = |other: &crate::object_document::ObjectTarget| {
        other.context_id == target.context_id
            && other.layer_uid == target.layer_uid
            && other.frame_scope.kind == FrameScopeKind::Authored
            && [frame, target.frame_scope.frame].contains(&other.frame_scope.frame)
    };
    if document.items.value().is_some_and(|items| {
        items
            .raster
            .value()
            .is_some_and(|entries| entries.iter().any(|r| in_cell(&r.target)))
            || items
                .text
                .value()
                .is_some_and(|entries| entries.iter().any(|r| in_cell(&r.target)))
    }) {
        return Err(Refusal::Unsupported);
    }
    Ok(true)
}

fn pair(p: &ObjectPoint) -> Result<[f64; 2], Refusal> {
    Ok([
        p.x.as_f64().ok_or(Refusal::Invalid)?,
        p.y.as_f64().ok_or(Refusal::Invalid)?,
    ])
}
fn point(p: [f64; 2]) -> Result<ObjectPoint, Refusal> {
    Ok(ObjectPoint {
        x: Number::from_f64(p[0]).ok_or(Refusal::Invalid)?,
        y: Number::from_f64(p[1]).ok_or(Refusal::Invalid)?,
    })
}
// Keep API/MCP edits and both read paths inside the same admitted consumer scope.
pub(crate) fn admit(document: &ObjectDocument, record: &ObjectRecord) -> Result<(), Refusal> {
    if record.target.context_id != "scene-root" {
        return Err(Refusal::Unsupported);
    }
    let layer = document
        .layers
        .iter()
        .find(|l| l.layer_uid == record.target.layer_uid)
        .ok_or(Refusal::Unsupported)?;
    admit_layer(document, record, layer)
}
/// The admission of a record whose layer the CALLER resolved in the record's own context (#2256: Select's hit-test listing of an entered
/// component reads the component's layer, not a scene layer). The edit and preview routes keep `admit`, which is scene-root only.
pub(crate) fn admit_layer(
    document: &ObjectDocument,
    record: &ObjectRecord,
    layer: &crate::object_document::ObjectLayer,
) -> Result<(), Refusal> {
    if layer.three_d.value() == Some(&true) || layer.rig().is_some() {
        return Err(Refusal::Unsupported);
    }
    if document.property_holders.value().is_some_and(|holders| {
        holders.iter().any(|h| {
            h.target.context_id == record.target.context_id
                && h.target.layer_uid == record.target.layer_uid
                && h.target.stroke_id.value() == Some(&record.target.stroke_id)
                && h.properties.iter().any(|p| {
                    p.property.starts_with("vtx")
                        || (record
                            .vector_ext
                            .value()
                            .is_some_and(|e| !e.param_shape.missing())
                            && matches!(
                                p.property.as_str(),
                                "cornerTL"
                                    | "cornerTR"
                                    | "cornerBR"
                                    | "cornerBL"
                                    | "arcStart"
                                    | "arcSweep"
                                    | "arcInner"
                                    | "starInner"
                                    | "starCorner"
                            )
                            && p.track.value().is_some_and(|track| !track.keys.is_empty()))
                })
        })
    }) {
        return Err(Refusal::Unsupported);
    }
    Ok(())
}
pub(crate) fn anchors(record: &ObjectRecord) -> Result<Vec<Anchor>, Refusal> {
    if record.family != VECTOR_FAMILY {
        return Err(Refusal::Unsupported);
    }
    if has_pressure(record) {
        return pressure::anchors(record);
    }
    if !(2..=256).contains(&record.geometry.segments.len()) {
        return Err(Refusal::Limit);
    }
    // These consumers require their own geometry/companion projection before a commit can be admitted.
    if record
        .vector_ext
        .value()
        .is_some_and(|e| !e.bitmap_brush.missing() || !e.brush_texture.missing())
    {
        return Err(Refusal::Unsupported);
    }
    paint_anchors(record)
}
/// Paint pivots use the rendered ribbon, while editable anchors use its centerline.
pub(crate) fn paint_anchors(record: &ObjectRecord) -> Result<Vec<Anchor>, Refusal> {
    record
        .geometry
        .segments
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
/// #2356: the stored per-vertex point-type hints (`None` = inferred); a hint array that does not match the vertices is refused.
pub(crate) fn hints(record: &ObjectRecord) -> Result<Option<Vec<Kind>>, Refusal> {
    let Some(stored) = record.vector_ext.value().and_then(|e| e.point_types.value()) else {
        return Ok(None);
    };
    if stored.len() != record.geometry.segments.len() {
        return Err(Refusal::Invalid);
    }
    stored.iter().map(|t| Kind::parse(t).ok_or(Refusal::Invalid)).collect::<Result<Vec<_>, _>>().map(Some)
}
/// The type each vertex shows: its stored hint, else the one its handles show.
pub(crate) fn effective_types(record: &ObjectRecord, anchors: &[Anchor]) -> Result<Vec<&'static str>, Refusal> {
    let stored = hints(record)?;
    Ok((0..anchors.len())
        .map(|i| crate::object_document::subselect::point_type::effective(stored.as_deref(), anchors, i).name())
        .collect())
}
/// Store the hint array on the record. `None` leaves the record as it is (an absent hint stays absent).
fn store_hints(next: &mut ObjectRecord, hints: Option<Vec<Kind>>) {
    let Some(hints) = hints else { return };
    let names = hints.iter().map(|k| k.name().to_string()).collect();
    match next.vector_ext {
        Presence::Present(ref mut ext) => ext.point_types = Presence::Present(names),
        _ => {
            next.vector_ext = Presence::Present(VectorExt { point_types: Presence::Present(names), ..VectorExt::default() })
        }
    }
}
pub(crate) fn plan(record: &ObjectRecord, intent: &Intent) -> Result<ObjectRecord, Refusal> {
    if has_pressure(record) {
        return pressure::plan(record, intent);
    }
    let (edited, typed) = edit_typed(&anchors(record)?, hints(record)?.as_deref(), record.geometry.closed, false, intent)?;
    let mut next = record.clone();
    store_hints(&mut next, typed);
    let mut origins: Vec<Option<usize>> = (0..record.geometry.segments.len()).map(Some).collect();
    match intent {
        Intent::Insert { curve, .. } => origins.insert(curve + 1, None),
        Intent::Delete { indices } => origins.retain(|i| !indices.contains(&i.unwrap())),
        _ => {}
    }
    next.geometry.segments = edited
        .into_iter()
        .zip(origins)
        .map(|(a, origin)| {
            if let Some(i) = origin {
                let old = &record.geometry.segments[i];
                return Ok(ObjectSegment {
                    point: keep(&old.point, a.point)?,
                    handle_in: keep(&old.handle_in, a.handle_in)?,
                    handle_out: keep(&old.handle_out, a.handle_out)?,
                });
            }
            Ok(ObjectSegment {
                point: point(a.point)?,
                handle_in: point(a.handle_in)?,
                handle_out: point(a.handle_out)?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(next)
}

#[derive(Clone, Debug, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Preview {
    pub(crate) stable_target: crate::object_document::ObjectTarget,
    pub(crate) intent: Intent,
}
use crate::object_document::{object_deserialize, ObjectTarget};
use serde::Deserialize;
object_deserialize!(Preview {
    stable_target: ObjectTarget,
    intent: Intent
});

// A float geometry calculation must not round untouched canonical JSON integers.
// Also preserve absent edits and original numeric representation on surviving anchors.
fn keep(old: &ObjectPoint, value: [f64; 2]) -> Result<ObjectPoint, Refusal> {
    let mut p = point(value)?;
    if old
        .x
        .as_f64()
        .is_some_and(|n| n.to_bits() == value[0].to_bits())
    {
        p.x = old.x.clone();
    }
    if old
        .y
        .as_f64()
        .is_some_and(|n| n.to_bits() == value[1].to_bits())
    {
        p.y = old.y.clone();
    }
    Ok(p)
}

pub(crate) fn has_pressure(record: &ObjectRecord) -> bool {
    record
        .vector_ext
        .value()
        .is_some_and(|e| !e.pressure.missing())
}
pub(crate) fn closed(record: &ObjectRecord) -> bool {
    record.geometry.closed && !has_pressure(record)
}
/// #2362 hit-test listing: control-point bounds of the painted geometry in record space (handles are relative, and a
/// cubic lies inside its control polygon); a non-finite point leaves the bounds as they were.
pub(crate) fn painted_bounds(record: &ObjectRecord) -> [f64; 4] {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for s in &record.geometry.segments {
        let (Ok(p), Ok(i), Ok(o)) = (pair(&s.point), pair(&s.handle_in), pair(&s.handle_out)) else {
            continue;
        };
        for [x, y] in [p, [p[0] + i[0], p[1] + i[1]], [p[0] + o[0], p[1] + o[1]]] {
            b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
    }
    if b.iter().all(|v| v.is_finite()) { b } else { [0.0; 4] }
}
/// #2372 hit-test listing: the authored stroke width in record space when the record really paints a stroke, else 0 (a ribbon
/// and an unstroked fill have none beyond their outline, which `painted_bounds` already covers).
pub(crate) fn painted_stroke_width(record: &ObjectRecord) -> f64 {
    record
        .vector()
        .filter(|v| v.has_real_stroke && !has_pressure(record))
        .and_then(|v| v.stroke_width.as_f64())
        .filter(|w| w.is_finite() && *w > 0.0)
        .unwrap_or(0.0)
}
/// #2372 hit-test listing: the stored `boxAngle` in degrees, or None when the record has none or it is 0 (serP drops a falsy angle).
pub(crate) fn stored_box_angle(record: &ObjectRecord) -> Option<f64> {
    record
        .vector_ext
        .value()
        .and_then(|e| e.box_angle.value())
        .and_then(serde_json::Number::as_f64)
        .filter(|a| a.is_finite() && *a != 0.0)
}
/// #2372 hit-test listing: the record's `paramShape` (the declared intent of a dynamic rectangle, ellipse or star) without its generator `box`,
/// which is the body `object.parametric.set` takes back; None for a record that has none.
pub(crate) fn param_shape(record: &ObjectRecord) -> Option<serde_json::Value> {
    let shape = record.vector_ext.value()?.param_shape.value()?;
    let mut listed = serde_json::to_value(shape).ok()?;
    listed.as_object_mut()?.remove("box");
    Some(listed)
}
/// How many enclosing groups a listed record names (the innermost ones): one record's listing must stay inside the 4096-byte page.
const GROUP_PATH_DEPTH: usize = 4;
/// #2376 hit-test listing: the vector groups a stroke sits in, outermost first, from the layer's group table: its innermost group (the
/// member tag) then each `parent` up, each `{id, name?, mode?}` (`mode` is the stored combine mode, absent on a legacy group that reads as
/// `none`). None for a loose stroke. A parent cycle ends the walk where it repeats.
pub(crate) fn group_path(table: &crate::object_document::GroupTable, stroke_id: &str) -> Option<Vec<serde_json::Value>> {
    let mut gid = table.members.iter().find(|m| m.stroke_id == stroke_id)?.group_id.as_str();
    let mut chain: Vec<&str> = Vec::new();
    let mut path = Vec::new();
    while !chain.contains(&gid) && chain.len() < 32 {
        chain.push(gid);
        let entry = table.entries.iter().find(|e| e.gid == gid);
        let mut listed = serde_json::json!({ "id": gid });
        if let Some(name) = entry.and_then(|e| e.name.value()) {
            listed["name"] = serde_json::json!(name);
        }
        if let Some(mode) = entry.and_then(|e| e.combine_mode.value()) {
            listed["mode"] = serde_json::json!(mode);
        }
        path.push(listed);
        match entry.and_then(|e| e.parent.value()) {
            Some(parent) => gid = parent.as_str(),
            None => break,
        }
    }
    path.truncate(GROUP_PATH_DEPTH);
    path.reverse();
    Some(path)
}
/// #2362 hit-test listing: what a painted record is, so a reader can tell a ribbon's linked fill or a derived shape
/// from a plain stroke without the record itself.
pub(crate) fn hit_role(record: &ObjectRecord) -> &'static str {
    let ext = record.vector_ext.value();
    if has_pressure(record) {
        "ribbon"
    } else if ext.is_some_and(|e| e.is_linked_fill_companion.value() == Some(&true)) {
        "fill"
    } else if ext.is_some_and(|e| e.is_fill_shape.value() == Some(&true) || !e.param_shape.missing()) {
        "shape"
    } else if ext.is_some_and(|e| !e.bitmap_brush.missing() || !e.brush_texture.missing()) {
        "brush"
    } else {
        "plain"
    }
}
/// #2372 hit-test listing: what a record is to a brush (a texture `anchor`, one of its `copy` dabs, a `bitmap` anchor) and
/// the group they share.
pub(crate) fn brush_of(record: &ObjectRecord) -> (Option<&'static str>, Option<String>) {
    let ext = record.vector_ext.value();
    if let Some(texture) = ext.and_then(|e| e.brush_texture.value()) {
        let kind = if texture.is_brush_texture_copy.value() == Some(&true) { "copy" } else { "anchor" };
        return (Some(kind), texture.brush_group_id.value().cloned());
    }
    match ext.and_then(|e| e.bitmap_brush.value()) {
        Some(bitmap) => (Some("bitmap"), Some(bitmap.brush_group_id.clone())),
        None => (None, None),
    }
}
/// The linked-fill identity a ribbon and its fill share.
pub(crate) fn linked_fill(record: &ObjectRecord) -> Option<&String> {
    record.vector_ext.value().and_then(|e| e.linked_fill_id.value())
}
/// #2362 sculpt (hit-test reads only): a ribbon's centreline and its per-anchor widths, the geometry sculpt deforms.
pub(crate) fn center_anchors(record: &ObjectRecord) -> Result<(Vec<Anchor>, Vec<Number>), Refusal> {
    let center = record.vector_ext.value().and_then(|e| e.pressure.value()).and_then(|p| p.center_segments.value());
    let center = center.filter(|c| (2..=256).contains(&c.len())).ok_or(Refusal::Unsupported)?;
    let anchors = center
        .iter()
        .map(|s| Ok(Anchor { point: pair(&s.point)?, handle_in: pair(&s.handle_in)?, handle_out: pair(&s.handle_out)? }))
        .collect::<Result<_, Refusal>>()?;
    Ok((anchors, center.iter().map(|s| s.width.clone()).collect()))
}
