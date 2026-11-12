//! The `vtxN` Motion offsets of an element holder (baseline `applyPathVertexOffsets`) for the vector evaluation join and the hit-test listing.
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamSegment;
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::PathSegment;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    finite, from_param, from_path, is_vector_record, run_param_shape, run_param_shape_with,
    run_path_fx, stored_from_record, EvalSegment, VectorEvalError,
};
use crate::object_document::{ObjectLayer, ObjectPoint, ObjectSegment};
use crate::object_document::{ObjectRecord, Presence, PropertyHolder, PropertyHolderTarget};
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

/// Baseline `applyPathVertexOffsets`: each `vtxN` track is a `[dx, dy]` OFFSET added to anchor N's point only (handles are
/// unchanged), after path effects/duplicator placement, before a parametric rebuild and BEFORE the element and layer pose. An unkeyed vertex is `[0, 0]`; a
/// vertex driven by an enabled expression stays unavailable here (the raw sampler would silently ignore it).
fn apply_vertex_offsets(
    record: &ObjectRecord,
    mut segments: Vec<EvalSegment>,
    offsets: Vec<(usize, [f64; 2])>,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    // A ribbon's vtxN addresses its centerline, never its baked outline.
    // Valid ribbons are handled before this stage. A pressure record without
    // a usable centerline must not address its baked outline as source nodes.
    if record
        .vector_ext
        .value()
        .is_some_and(|ext| ext.pressure.value().is_some())
        && offsets.iter().any(|(_, off)| *off != [0., 0.])
    {
        return Err(VectorEvalError::BrushVertexUnavailable);
    }
    for (index, [dx, dy]) in offsets {
        let Some(segment) = segments.get_mut(index) else {
            continue;
        };
        segment.point = [segment.point[0] + dx, segment.point[1] + dy];
        if !segment.point.iter().all(|v| v.is_finite()) {
            return Err(VectorEvalError::NonFiniteInput);
        }
    }
    Ok(segments)
}

/// The `vtxN` offsets of the record's element holder at `frame`, by vertex index (only authored vertices appear).
pub(crate) fn vertex_offsets(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
) -> Result<Vec<(usize, [f64; 2])>, VectorEvalError> {
    let target = PropertyHolderTarget {
        context_id: record.target.context_id.clone(),
        layer_uid: record.target.layer_uid.clone(),
        stroke_id: Presence::Present(record.target.stroke_id.clone()),
    };
    let Some(holder) = snapshot
        .document()
        .property_holders()
        .and_then(|holders| holders.iter().find(|holder| holder.target() == &target))
    else {
        return Ok(Vec::new());
    };
    let mut offsets = Vec::new();
    for property in holder.properties() {
        let Some(index) = PropertyHolder::vertex_index(property.property()) else {
            continue;
        };
        if property.expression.value().is_some_and(|code| code.enabled) {
            return Err(VectorEvalError::ExpressionUnavailable { property: "vtx" });
        }
        let sampled = sample_property_raw(snapshot, &target, property.property(), f64::from(frame))
            .map_err(|error| VectorEvalError::Sample {
                property: "vtx",
                error,
            })?;
        let (Some(dx), Some(dy)) = (sampled.first(), sampled.get(1)) else {
            return Err(VectorEvalError::Sample {
                property: "vtx",
                error: PropertySampleError::Unavailable,
            });
        };
        offsets.push((index, [*dx, *dy]));
    }
    Ok(offsets)
}

/// Baseline path-local order after path effects/placement: vertex offsets, then
/// animated parametric rebuild. The rebuild uses the OFFSET bounds, replacing
/// points and handles rather than offsetting the new shape's vertex N.
pub(crate) fn vertex_then_param(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    anchors: &[PathSegment],
    frame: u32,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    if let Some(outline) = ribbon_motion(snapshot, record, frame)? {
        return Ok(outline);
    }
    let offsets = vertex_offsets(snapshot, record, frame)?;
    if offsets.iter().all(|(_, offset)| *offset == [0., 0.]) {
        // Preserve the common no-vertex-motion path: no intermediate segment
        // arrays, and an idle shape never invokes the deferred anchor builder.
        return Ok(match run_param_shape(snapshot, record, anchors, frame)? {
            Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
            None => anchors.iter().map(from_path).collect(),
        });
    }
    let segments = apply_vertex_offsets(record, anchors.iter().map(from_path).collect(), offsets)?;
    Ok(
        match run_param_shape_with(snapshot, record, frame, || {
            segments
                .iter()
                .map(|s| ParamSegment {
                    point: s.point,
                    handle_in: s.handle_in,
                    handle_out: s.handle_out,
                })
                .collect()
        })? {
            Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
            None => segments,
        },
    )
}

/// Detached geometry in the record's local paint space, before element/layer pose.
/// Shared by the hit listing, its paged contour reader and element-pivot consumer.
pub(crate) fn painted_record(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    layer: &ObjectLayer,
    frame: u32,
) -> Result<ObjectRecord, VectorEvalError> {
    if !is_vector_record(record) {
        return Ok(record.clone());
    }
    let stored = stored_from_record(record, Some(layer))?;
    let after = run_path_fx(stored.stack.as_deref(), &stored.stroke, frame)?;
    let segments = vertex_then_param(
        snapshot,
        record,
        after.segments.as_deref().unwrap_or(&[]),
        frame,
    )?;
    let point = |p: [f64; 2]| -> Result<ObjectPoint, VectorEvalError> {
        Ok(ObjectPoint {
            x: Number::from_f64(p[0]).ok_or(VectorEvalError::NonFiniteInput)?,
            y: Number::from_f64(p[1]).ok_or(VectorEvalError::NonFiniteInput)?,
        })
    };
    let mut output = record.clone();
    output.geometry.closed = after.closed && !segments.is_empty();
    output.geometry.segments = segments
        .into_iter()
        .map(|s| {
            Ok(ObjectSegment {
                point: point(s.point)?,
                handle_in: point(s.handle_in)?,
                handle_out: point(s.handle_out)?,
            })
        })
        .collect::<Result<_, VectorEvalError>>()?;
    Ok(output)
}

fn element_holder<'a>(
    snapshot: &'a ObjectSnapshot,
    record: &ObjectRecord,
) -> Option<&'a PropertyHolder> {
    snapshot.document().property_holders()?.iter().find(|h| {
        h.target().context_id() == record.target.context_id
            && h.target().layer_uid() == record.target.layer_uid
            && h.target().stroke_id() == Some(record.target.stroke_id.as_str())
    })
}
fn authored(holder: &PropertyHolder, key: &str) -> bool {
    holder.properties().iter().any(|p| {
        p.property() == key
            && (p.static_value.value().is_some()
                || p.track.value().is_some_and(|t| !t.keys.is_empty()))
    })
}
fn has_trim(holder: &PropertyHolder) -> bool {
    ["trimStart", "trimEnd", "trimOffset"]
        .iter()
        .any(|key| authored(holder, key))
}
fn scalar(
    snapshot: &ObjectSnapshot,
    holder: &PropertyHolder,
    key: &'static str,
    frame: u32,
) -> Result<f64, VectorEvalError> {
    if holder
        .properties()
        .iter()
        .any(|p| p.property() == key && p.expression.value().is_some_and(|e| e.enabled))
    {
        return Err(VectorEvalError::ExpressionUnavailable { property: key });
    }
    let values =
        sample_property_raw(snapshot, holder.target(), key, f64::from(frame)).map_err(|error| {
            VectorEvalError::Sample {
                property: key,
                error,
            }
        })?;
    values
        .first()
        .copied()
        .filter(|v| v.is_finite())
        .ok_or(VectorEvalError::NonFiniteInput)
}
/// Source vertex identity for Motion: a brush's stored centerline, with offsets
/// only when Trim does not take precedence. Widths and relative handles survive.
pub(crate) fn presented_center(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
) -> Result<Vec<crate::object_document::CenterSegment>, VectorEvalError> {
    let pressure = record
        .vector_ext
        .value()
        .and_then(|e| e.pressure.value())
        .ok_or(VectorEvalError::BrushVertexUnavailable)?;
    let mut center = pressure
        .center_segments
        .value()
        .filter(|v| v.len() >= 2)
        .cloned()
        .ok_or(VectorEvalError::BrushVertexUnavailable)?;
    if element_holder(snapshot, record).is_some_and(has_trim) {
        return Ok(center);
    }
    for (index, off) in vertex_offsets(snapshot, record, frame)? {
        if let Some(s) = center.get_mut(index) {
            s.point.x = Number::from_f64(finite(&s.point.x)? + off[0])
                .ok_or(VectorEvalError::NonFiniteInput)?;
            s.point.y = Number::from_f64(finite(&s.point.y)? + off[1])
                .ok_or(VectorEvalError::NonFiniteInput)?;
        }
    }
    Ok(center)
}
/// Ordinary ribbon Motion at one immutable frame. Trim wins over BOTH vertex
/// and Brush Size in the frozen engine bridge; outside that branch, apply the
/// offsets to the centerline and scale sampled widths before one outline build.
fn ribbon_motion(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
) -> Result<Option<Vec<EvalSegment>>, VectorEvalError> {
    use crate::object_document::pressure_geometry::{self as geometry, SegOut};
    let Some(pressure) = record.vector_ext.value().and_then(|e| e.pressure.value()) else {
        return Ok(None);
    };
    let Some(holder) = element_holder(snapshot, record) else {
        return Ok(None);
    };
    let trimmed = has_trim(holder);
    let size = authored(holder, "brushSize");
    let vertices = holder.properties().iter().any(|p| {
        PropertyHolder::vertex_index(p.property()).is_some() && authored(holder, p.property())
    });
    if !trimmed && !size && !vertices {
        return Ok(None);
    };
    let ext = record.vector_ext.value().unwrap();
    // The dab producer currently fits an affine from the stored outline. A
    // deformed centerline needs its own stamp walk, not that unrelated affine.
    if !ext.brush_texture.missing() || !ext.bitmap_brush.missing() || !ext.param_shape.missing() {
        return Err(VectorEvalError::BrushVertexUnavailable);
    };
    let raw = pressure
        .center_segments
        .value()
        .filter(|v| v.len() >= 2)
        .ok_or(VectorEvalError::BrushVertexUnavailable)?;
    let center = if trimmed {
        raw.clone()
    } else {
        presented_center(snapshot, record, frame)?
    };
    let pair = |p: &ObjectPoint| Ok([finite(&p.x)?, finite(&p.y)?]);
    let segments = center
        .iter()
        .map(|s| {
            Ok(SegOut {
                point: pair(&s.point)?,
                handle_in: pair(&s.handle_in)?,
                handle_out: pair(&s.handle_out)?,
            })
        })
        .collect::<Result<Vec<_>, VectorEvalError>>()?;
    let widths = center
        .iter()
        .map(|s| finite(&s.width))
        .collect::<Result<Vec<_>, _>>()?;
    let profile = pressure
        .width_profile
        .value()
        .map(|v| {
            v.iter()
                .map(|s| Ok((finite(&s.t)?, finite(&s.width)?)))
                .collect::<Result<Vec<_>, VectorEvalError>>()
        })
        .transpose()?
        .unwrap_or_default();
    // Frozen applyPathVertexOffsets drops .width on each changed segment.
    // With no dense profile this produces NaN widths. Keep authored widths
    // intact and expose that exact legacy defect rather than silently fix it.
    if !trimmed
        && profile.is_empty()
        && vertex_offsets(snapshot, record, frame)?
            .iter()
            .any(|(i, offset)| *i < raw.len() && *offset != [0., 0.])
    {
        return Err(VectorEvalError::PressureWidthBaselineDefect);
    }
    let (pts, mut sampled) = if trimmed {
        trim_ribbon_samples(
            &segments,
            &profile,
            [
                scalar(snapshot, holder, "trimStart", frame)?,
                scalar(snapshot, holder, "trimEnd", frame)?,
                scalar(snapshot, holder, "trimOffset", frame)?,
            ],
        )?
    } else {
        geometry::sample_centerline(
            &segments,
            &widths,
            (!profile.is_empty()).then_some(profile.as_slice()),
        )
        .map_err(|_| VectorEvalError::BrushVertexUnavailable)?
    };
    if pts.len() < 2 {
        return Ok(Some(Vec::new()));
    };
    if !trimmed && size {
        let scale = scalar(snapshot, holder, "brushSize", frame)? / 100.;
        for width in &mut sampled {
            *width *= scale;
        }
    }
    if !pts
        .iter()
        .flatten()
        .chain(sampled.iter())
        .all(|n| n.is_finite())
    {
        return Err(VectorEvalError::NonFiniteInput);
    };
    let zero_spine = segments.iter().all(|s| {
        s.point == segments[0].point && s.handle_in == [0., 0.] && s.handle_out == [0., 0.]
    });
    // The 20-step Trim polyline can invent epsilon travel on an exact zero
    // spine. Its tiny reversal/crossing cleanup is unresolved even when the
    // jitter happens to be collinear; an actually empty window stays admitted.
    if zero_spine && trimmed && pts.iter().any(|p| *p != pts[0]) {
        return Err(VectorEvalError::PressureCrossingUnavailable);
    }
    // Do not reject matching collapsed spines wholesale. Only duplicate side
    // curves with non-collinear epsilon handles need the missing degenerate
    // crossing disposition (Paper resolves the frozen 30,40 dot to 4 anchors;
    // native gives 19; the collinear 10,20 case already matches at 19).
    if pts.iter().all(|p| *p == pts[0]) && sampled.iter().all(|w| *w == sampled[0]) {
        let raw = geometry::outline_samples_raw(&pts, &sampled)
            .map_err(|_| VectorEvalError::BrushVertexUnavailable)?;
        if raw.iter().zip(raw.iter().cycle().skip(1)).any(|(a, b)| {
            a.point == b.point
                && a.handle_out[0] * b.handle_in[1] - a.handle_out[1] * b.handle_in[0] != 0.
        }) {
            return Err(VectorEvalError::PressureCrossingUnavailable);
        }
    }
    let outline = geometry::outline_samples(&pts, &sampled)
        .map_err(|_| VectorEvalError::BrushVertexUnavailable)?;
    if outline
        .iter()
        .flat_map(|s| s.point.iter().chain(&s.handle_in).chain(&s.handle_out))
        .any(|n| !n.is_finite())
    {
        return Err(VectorEvalError::NonFiniteInput);
    };
    Ok(Some(
        outline
            .into_iter()
            .map(|s| EvalSegment {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect(),
    ))
}

/// Frozen applyTrimToVectorBrush: 20 samples per cubic; clamp the open window,
/// sample widths along that polyline, then step by the ORIGINAL sample spacing.
pub(crate) fn trim_ribbon_samples(
    center: &[crate::object_document::pressure_geometry::SegOut],
    profile: &[(f64, f64)],
    window: [f64; 3],
) -> Result<(Vec<[f64; 2]>, Vec<f64>), VectorEvalError> {
    use crate::object_document::pressure_geometry::width_at_frac;
    let [start, end, offset] = window;
    if end + offset <= start + offset {
        return Ok((vec![], vec![]));
    };
    let mut pts = vec![center[0].point];
    for pair in center.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        for i in 1..=20 {
            let t = i as f64 / 20.;
            let u = 1. - t;
            let (aa, bb, cc, dd) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
            pts.push([0, 1].map(|j| {
                aa * a.point[j]
                    + bb * (a.point[j] + a.handle_out[j])
                    + cc * (b.point[j] + b.handle_in[j])
                    + dd * b.point[j]
            }));
        }
    }
    let mut lengths = vec![0.];
    for p in pts.windows(2) {
        lengths.push(
            lengths.last().unwrap()
                + ((p[1][0] - p[0][0]) * (p[1][0] - p[0][0])
                    + (p[1][1] - p[0][1]) * (p[1][1] - p[0][1]))
                    .sqrt(),
        );
    }
    let total = *lengths.last().unwrap();
    // The frozen bridge maps null (zero-length trim spine) to no paint.
    if total <= 0. {
        return Ok((vec![], vec![]));
    };
    if !total.is_finite() {
        return Err(VectorEvalError::NonFiniteInput);
    };
    let fallback = profile.last().map_or(1., |p| p.1);
    let widths = lengths
        .iter()
        .map(|l| width_at_frac(profile, l / total).unwrap_or(fallback))
        .collect::<Vec<_>>();
    let from = (start + offset).clamp(0., 100.) / 100. * total;
    let to = (end + offset).clamp(0., 100.) / 100. * total;
    if to <= from {
        return Ok((vec![], vec![]));
    };
    let at = |distance: f64| {
        for i in 1..lengths.len() {
            if lengths[i] >= distance {
                let span = lengths[i] - lengths[i - 1];
                let t = if span > 0. {
                    (distance - lengths[i - 1]) / span
                } else {
                    0.
                };
                return (
                    [0, 1].map(|j| pts[i - 1][j] + (pts[i][j] - pts[i - 1][j]) * t),
                    widths[i - 1] + (widths[i] - widths[i - 1]) * t,
                );
            }
        }
        (*pts.last().unwrap(), *widths.last().unwrap())
    };
    let (first, fw) = at(from);
    let (mut out, mut ws) = (vec![first], vec![fw]);
    let step = total / (pts.len() - 1).max(1) as f64;
    let mut distance = step;
    while distance < to - from {
        if out.len() > 8192 {
            return Err(VectorEvalError::BrushVertexUnavailable);
        };
        let (p, w) = at(from + distance);
        out.push(p);
        ws.push(w);
        distance += step;
    }
    let (p, w) = at(to);
    out.push(p);
    ws.push(w);
    Ok((out, ws))
}
