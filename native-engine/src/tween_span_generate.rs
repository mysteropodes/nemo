//! Plan one span; emit geometry and authored stacking before a single owner publication.
use super::*;
use crate::object_document::{
    tween::{tween_easing, TweenOptions, TweenSpan},
    ObjectDocument, ObjectRecord, Presence,
};
use crate::request_receipts::DispatchErrorCode;
struct PreparedPair {
    spec: PairSpec,
    geometry: PreparedInterpolation,
    id: String,
    controls: ([f64; 2], [f64; 2]),
    texture: Option<PairTexture>,
}
pub(super) fn generate_span(
    doc: &mut ObjectDocument,
    layer: &str,
    from: u32,
    to: u32,
    options: &TweenOptions,
    settings: Option<&TweenSpan>,
    custom: Option<&crate::object_document::ProjectPresetCatalog>,
) -> R<()> {
    let original_a = frame_records(doc, layer, from);
    let original_b = frame_records(doc, layer, to);
    let mut a: Vec<_> = original_a.iter().filter(|r| !is_dab(r)).cloned().collect();
    let mut b: Vec<_> = original_b.iter().filter(|r| !is_dab(r)).cloned().collect();
    // These branches are explicitly unavailable until their native emitters are joined below.
    if a.iter().chain(&b).any(|r| {
        r.is_pressure_ribbon()
            || r.vector_ext
                .value()
                .is_some_and(|v| !v.bitmap_brush.missing() || !v.is_linked_fill_companion.missing())
    }) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let manual = settings.and_then(|s| s.manual_strokes.value());
    let held: Vec<_> = a
        .iter()
        .filter(|r| manual.is_some_and(|ids| !ids.contains(&r.target.stroke_id)))
        .cloned()
        .collect();
    a.retain(|r| manual.is_none_or(|ids| ids.contains(&r.target.stroke_id)));
    if a.len() > 256 || b.len() > 256 {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let (specs, un_a, un_b) = pair_plan(&a, &b, settings.map_or(&[], |s| s.overrides.as_slice()))?;
    if a.is_empty() && b.is_empty() && specs.is_empty() {
        return Ok(());
    }
    let pending: Vec<_> = specs
        .iter()
        .map(|p| p.a_data.target.stroke_id.clone())
        .collect();
    for (i, &idx) in un_a.iter().enumerate() {
        if pending.contains(&a[idx].target.stroke_id) {
            a[idx].target.stroke_id = format!("twc_{from}_a{i}");
        }
    }
    for (i, &idx) in un_b.iter().enumerate() {
        if pending.contains(&b[idx].target.stroke_id) {
            b[idx].target.stroke_id = format!("twc_{to}_b{i}");
        }
    }
    let persist_a: Vec<_> = specs
        .iter()
        .map(|p| p.a_data.clone())
        .chain(held.iter().cloned())
        .collect();
    let persist_b: Vec<_> = specs.iter().map(|p| p.b_data.clone()).collect();
    let plans_a: Vec<_> = un_a
        .iter()
        .map(|&i| vanish_plan(&a[i], &persist_a))
        .collect::<R<_>>()?;
    let plans_b: Vec<_> = un_b
        .iter()
        .map(|&i| vanish_plan(&b[i], &persist_b))
        .collect::<R<_>>()?;
    let mut pairs = vec![];
    for spec in specs {
        let sa = stroke(&spec.a_data);
        let sb = stroke(&spec.b_data);
        let n = (poly_length(&sa).max(poly_length(&sb)) / 8.)
            .round()
            .min(150.)
            .max(options.resample_points as f64) as usize;
        let (ra, rb) = resample_pair(&sa, &sb, n, false);
        let rb = align_pair(&ra, &rb);
        let centroid = |s: &[SegOut]| {
            let p = s.iter().fold([0., 0.], |mut p, s| {
                p[0] += s.point[0];
                p[1] += s.point[1];
                p
            });
            [p[0] / s.len() as f64, p[1] / s.len() as f64]
        };
        let handles = doc
            .tween_motion_arc_handles(
                layer,
                from.into(),
                to.into(),
                spec.mi,
                centroid(&ra.segments),
                centroid(&rb.segments),
            )
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let geometry = PreparedInterpolation::new(
            ra.segments,
            rb.segments,
            sa.closed,
            sb.closed,
            Some((sa.segments.len(), sb.segments.len())),
        )
        .ok_or(DispatchErrorCode::InvalidRequest)?;
        let id = spec.a_data.target.stroke_id.clone();
        a[spec.a].target.stroke_id = id.clone();
        b[spec.b].target.stroke_id = id.clone();
        let texture = PairTexture::new(&spec.a_data, &spec.b_data, from, spec.mi, custom);
        pairs.push(PreparedPair {
            texture,
            spec,
            geometry,
            id,
            controls: (handles.out, handles.incoming),
        });
    }
    // Dab records keep their endpoint identity and authored order outside matching.
    for (frame, original, matched) in [(from, &original_a, &a), (to, &original_b, &b)] {
        let mut stamped = original.clone();
        let candidates: Vec<_> = original
            .iter()
            .filter(|r| {
                !is_dab(r)
                    && (frame == to || manual.is_none_or(|ids| ids.contains(&r.target.stroke_id)))
            })
            .collect();
        for r in &mut stamped {
            if let Some(i) = candidates
                .iter()
                .position(|old| old.target.stroke_id == r.target.stroke_id)
            {
                r.target.stroke_id = matched[i].target.stroke_id.clone();
            }
        }
        stamp_frame(doc, layer, frame, original, &stamped)?;
    }
    let index = doc
        .layers
        .iter()
        .position(|l| l.layer_uid == layer)
        .ok_or(DispatchErrorCode::NotFound)?;
    let timing = doc.layers[index]
        .timing
        .value()
        .ok_or(DispatchErrorCode::Unavailable)?
        .clone();
    let mut frames = vec![];
    let easing = settings
        .and_then(|s| s.easing.value())
        .filter(|p| !p.is_empty())
        .unwrap_or(&options.easing);
    for frame in from + 1..to {
        if options.skip_manual && timing.exposure(frame).2 {
            continue;
        }
        if options.step > 1 && (frame - from) % options.step != 0 {
            replace_frame(doc, layer, frame, vec![])?;
            continue;
        }
        let t = (frame - from) as f64 / (to - from) as f64;
        let et = tween_easing(easing, t);
        let mut out: Vec<(f64, ObjectRecord)> = vec![];
        for p in &pairs {
            let segs = p
                .geometry
                .evaluate(et, Some(p.controls))
                .ok_or(DispatchErrorCode::InvalidRequest)?;
            let mut record =
                interpolate_record(&p.spec.a_data, &p.spec.b_data, &segs, et, &p.id, frame)?;
            let ar = p.spec.a as f64 / a.len().saturating_sub(1).max(1) as f64;
            let br = p.spec.b as f64 / b.len().saturating_sub(1).max(1) as f64;
            let rank = ar + (br - ar) * et;
            if let Some(texture) = &p.texture {
                out.extend(
                    texture
                        .emit(&mut record, et)?
                        .into_iter()
                        .map(|r| (rank + 1e-4, r)),
                );
            }
            out.push((rank, record));
        }
        for (indices, plans, source, keep) in
            [(&un_a, &plans_a, &a, 1. - et), (&un_b, &plans_b, &b, et)]
        {
            for (&i, plan) in indices.iter().zip(plans) {
                let original = if std::ptr::eq(source, &a) {
                    &original_a
                } else {
                    &original_b
                };
                if matches!(plan, VanishPlan::Fade) {
                    let rank = i as f64 / source.len().saturating_sub(1).max(1) as f64;
                    out.extend(
                        faded_dabs(original, &source[i], keep, frame)?
                            .into_iter()
                            .map(|r| (rank + 1e-4, r)),
                    );
                }
                if let Some(record) = vanish(&source[i], plan, keep, frame)? {
                    out.push((
                        i as f64 / source.len().saturating_sub(1).max(1) as f64,
                        record,
                    ));
                }
            }
        }
        for (i, r) in held.iter().enumerate() {
            let mut r = r.clone();
            r.target.frame_scope.frame = frame;
            out.push((i as f64 / held.len().saturating_sub(1).max(1) as f64, r));
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        replace_frame(doc, layer, frame, out.into_iter().map(|(_, r)| r).collect())?;
        frames.push(frame);
    }
    doc.layers[index].timing = Presence::Present(
        timing
            .tween_samples(from, to, &frames, options.skip_manual, doc.total_frames)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?,
    );
    Ok(())
}
