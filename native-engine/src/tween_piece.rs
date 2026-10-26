//! Original arc-fraction pieces and anchored trim; all sampled through the Paper-compatible native sampler.
use super::*;
use crate::object_document::{ObjectRecord, Presence};
use crate::request_receipts::DispatchErrorCode;
pub(super) fn extract_piece(record: &ObjectRecord, f0: f64, f1: f64, n: usize) -> R<ObjectRecord> {
    let s = stroke(record);
    if is_vb(&s) {
        return Err(DispatchErrorCode::Unavailable);
    }
    // The original extractor constructs an OPEN path even for a closed source.
    let mut raw = s.clone();
    raw.closed = false;
    let p = PathSampler::new(&build_feat_path(&raw));
    let hl = (f1 - f0) * p.total / (n - 1) as f64 / 3.;
    let segs: Vec<_> = (0..n)
        .map(|i| {
            let t = f0 + (f1 - f0) * i as f64 / (n - 1) as f64;
            let off = (t * p.total).clamp(0., p.total);
            let pt = p.point_at(off);
            let (tx, ty) = p.tangent_at(off);
            SegOut {
                point: [pt.x, pt.y],
                handle_in: if i == 0 {
                    [0.; 2]
                } else {
                    [-tx * hl, -ty * hl]
                },
                handle_out: if i == n - 1 {
                    [0.; 2]
                } else {
                    [tx * hl, ty * hl]
                },
            }
        })
        .collect();
    let mut out = record.clone();
    out.geometry.segments = native_segments(&segs)?;
    out.geometry.closed = false;
    out.fill = Presence::Present(None);
    // extractStrokePiece drops extra metadata; trimmedStroke subsequently restores the style.
    out.vector_ext = Presence::Missing;
    out.effects = Presence::Missing;
    if let Presence::Present(v) = &mut out.vector {
        v.dash_array = Presence::Missing;
        v.dash_offset = Presence::Missing;
        v.miter_limit = Presence::Missing;
        v.paint_order = Presence::Missing;
    }
    Ok(out)
}
pub(super) enum VanishPlan {
    Fade,
    Trim { end: bool },
}
pub(super) fn vanish_plan(record: &ObjectRecord, others: &[ObjectRecord]) -> R<VanishPlan> {
    if texture(record).is_some_and(|t| !t.preset.missing())
        || record.geometry.closed
        || record.solid_fill().is_some()
        || record
            .vector_ext
            .value()
            .is_some_and(|v| !v.bitmap_brush.missing())
    {
        return Ok(VanishPlan::Fade);
    }
    let s = stroke(record);
    let Some(first) = s.segments.first() else {
        return Ok(VanishPlan::Fade);
    };
    let Some(last) = s.segments.last() else {
        return Ok(VanishPlan::Fade);
    };
    let width = record
        .vector
        .value()
        .and_then(|v| v.stroke_width.as_f64())
        .filter(|&v| v != 0.)
        .unwrap_or(3.);
    let tol = (width * 2.).max(9.);
    let (mut a, mut b) = (false, false);
    for r in others {
        let p = PathSampler::new(&build_feat_path(&stroke(r)));
        a |= p.nearest(first.point)?.is_some_and(|p| p.distance <= tol);
        b |= p.nearest(last.point)?.is_some_and(|p| p.distance <= tol);
        if a && b {
            break;
        }
    }
    Ok(VanishPlan::Trim { end: b && !a })
}
pub(super) fn vanish(
    record: &ObjectRecord,
    plan: &VanishPlan,
    keep: f64,
    frame: u32,
) -> R<Option<ObjectRecord>> {
    let keep = keep.clamp(0., 1.);
    if keep <= 0.02 {
        return Ok(None);
    }
    let mut out = record.clone();
    match plan {
        VanishPlan::Fade => {
            let Presence::Present(v) = &mut out.vector else {
                return Err(DispatchErrorCode::Unavailable);
            };
            v.opacity = number(v.opacity.as_f64().unwrap() * keep)?;
            if v.opacity.as_f64().unwrap() <= 0.02 {
                return Ok(None);
            }
        }
        VanishPlan::Trim { end } if keep < 0.999 => {
            let (f0, f1) = if *end { (1. - keep, 1.) } else { (0., keep) };
            let n = (poly_length(&stroke(record)) * keep / 8.)
                .round()
                .clamp(16., 100.) as usize;
            out = extract_piece(record, f0, f1, n)?;
            if let (Some(a), Presence::Present(b)) = (record.vector.value(), &mut out.vector) {
                b.dash_array = a.dash_array.clone();
                b.dash_offset = a.dash_offset.clone();
                b.miter_limit = a.miter_limit.clone();
                b.paint_order = a.paint_order.clone();
            }
            out.effects = record.effects.clone();
            if let Some(g) = record.fill_gradient() {
                let mut ext = crate::object_document::VectorExt::default();
                ext.fill_gradient = Presence::Present(g.clone());
                out.vector_ext = Presence::Present(ext);
            }
        }
        _ => {}
    }
    out.target.frame_scope.frame = frame;
    Ok(Some(out))
}
