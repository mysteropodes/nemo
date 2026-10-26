//! Original geometric split matching, evaluated before order-coherence and interpolation.
use super::*;
use crate::object_document::ObjectRecord;
pub(super) fn ordered_pieces(
    merged: &ObjectRecord,
    parts: &[ObjectRecord],
    indices: &[usize],
) -> R<Vec<(usize, ObjectRecord)>> {
    let features: Vec<_> = indices
        .iter()
        .map(|&i| (i, stroke_feat(&stroke(&parts[i]))))
        .collect();
    let sum = features.iter().map(|(_, f)| f.length).sum::<f64>();
    let path = PathSampler::new(&build_feat_path(&stroke(merged)));
    let mut order = features
        .into_iter()
        .map(|(i, f)| {
            Ok((
                i,
                f.length,
                path.nearest([f.cx, f.cy])?.map_or(i as f64, |l| l.offset),
            ))
        })
        .collect::<R<Vec<_>>>()?;
    order.sort_by(|a, b| a.2.total_cmp(&b.2));
    let (mut acc, mut prev) = (0., 0.);
    let mut out = vec![];
    for (position, &(i, len, _)) in order.iter().enumerate() {
        acc += len;
        let end = if position + 1 == order.len() {
            1.
        } else {
            (acc / sum.max(1.)).clamp(0.05, 0.95)
        };
        out.push((i, extract_piece(merged, prev, end, 24)?));
        prev = end;
    }
    Ok(out)
}
fn loose(a: Rect, b: Rect) -> bool {
    let m = 20f64.max(a.width().max(a.height()).min(b.width().max(b.height())) * 0.3);
    a.x0 - m < b.x1 && b.x0 - m < a.x1 && a.y0 - m < b.y1 && b.y0 - m < a.y1
}
fn backtracks(r: &ObjectRecord) -> bool {
    let s = stroke(r);
    let pts: Vec<_> = if is_vb(&s) {
        s.center_segments.unwrap().iter().map(|s| s.point).collect()
    } else {
        s.segments.iter().map(|s| s.point).collect()
    };
    if pts.len() < 3 {
        return false;
    }
    let a = pts[0];
    let b = pts[pts.len() - 1];
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    if len <= 1. {
        return false;
    }
    let u = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let mut max = f64::NEG_INFINITY;
    for p in pts {
        let proj = (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
        if max - proj > len * 0.25 {
            return true;
        }
        max = max.max(proj);
    }
    false
}
fn normalized(records: &[ObjectRecord]) -> (Vec<Feat>, (f64, f64, f64, f64)) {
    let mut f: Vec<_> = records.iter().map(|r| stroke_feat(&stroke(r))).collect();
    let b = union_bounds(&f);
    for f in &mut f {
        f.rel_x = (f.cx - b.0) / b.2;
        f.rel_y = (f.cy - b.1) / b.3;
    }
    (f, b)
}
pub(super) fn split_matches(
    a: &[ObjectRecord],
    b: &[ObjectRecord],
    pairs: &mut Vec<PairSpec>,
    un_a: &mut Vec<usize>,
    un_b: &mut Vec<usize>,
) -> R<()> {
    let (fa, ba) = normalized(a);
    let (fb, bb) = normalized(b);
    let norm = ((ba.0 + ba.2).max(bb.0 + bb.2) - ba.0.min(bb.0))
        .hypot((ba.1 + ba.3).max(bb.1 + bb.3) - ba.1.min(bb.1));
    for merged_b in [true, false] {
        let (merged, parts, fm, fp, un_merged, un_parts, bounds) = if merged_b {
            (b, a, &fb, &fa, &mut *un_b, &mut *un_a, bb)
        } else {
            (a, b, &fa, &fb, &mut *un_a, &mut *un_b, ba)
        };
        for mi in 0..merged.len() {
            if fm[mi].stype == "fill" || fm[mi].closed {
                continue;
            }
            let current = pairs
                .iter()
                .position(|p| !p.piece && if merged_b { p.b == mi } else { p.a == mi });
            let current = current.map(|i| (i, pairs[i].clone()));
            let mut candidates: Vec<_> = un_parts
                .iter()
                .copied()
                .filter(|&i| fp[i].stype != "fill" && loose(fp[i].bounds, fm[mi].bounds))
                .collect();
            if let Some((_, p)) = &current {
                let i = if merged_b { p.a } else { p.b };
                if fp[i].stype != "fill" && !fp[i].closed {
                    candidates.push(i);
                }
            }
            if !(2..=3).contains(&candidates.len()) {
                continue;
            }
            let sum = candidates.iter().map(|&i| fp[i].length).sum::<f64>();
            if sum < fm[mi].length * 0.55 || sum > fm[mi].length * 1.7 {
                continue;
            }
            let pieces = ordered_pieces(&merged[mi], parts, &candidates)?;
            let mut proposed = vec![];
            let mut valid = true;
            for (k, (i, piece)) in pieces.into_iter().enumerate() {
                let mut feat = stroke_feat(&stroke(&piece));
                feat.rel_x = (feat.cx - bounds.0) / bounds.2;
                feat.rel_y = (feat.cy - bounds.1) / bounds.3;
                let score = if merged_b {
                    match_sc(&fp[i], &feat, false, None, norm)
                } else {
                    match_sc(&feat, &fp[i], false, None, norm)
                };
                if score > 0.48 || backtracks(&piece) {
                    valid = false;
                    break;
                }
                let mut p = if merged_b {
                    PairSpec {
                        a: i,
                        b: mi,
                        a_data: a[i].clone(),
                        b_data: piece,
                        mi: 9000 + mi as i32 * 10 + k as i32,
                        score,
                        forced: false,
                        piece: true,
                        a_piece: false,
                        b_piece: true,
                    }
                } else {
                    PairSpec {
                        a: mi,
                        b: i,
                        a_data: piece,
                        b_data: b[i].clone(),
                        mi: 9500 + mi as i32 * 10 + k as i32,
                        score,
                        forced: false,
                        piece: true,
                        a_piece: true,
                        b_piece: false,
                    }
                };
                // A transient piece has no authored identity in the original baseline.
                if p.a_piece {
                    p.a_data.target.stroke_id.clear();
                }
                if p.b_piece {
                    p.b_data.target.stroke_id.clear();
                }
                proposed.push(p);
            }
            if !valid {
                continue;
            }
            let avg = proposed.iter().map(|p| p.score).sum::<f64>() / proposed.len() as f64;
            let baseline = candidates
                .iter()
                .map(|&i| {
                    current
                        .as_ref()
                        .filter(|(_, p)| if merged_b { p.a == i } else { p.b == i })
                        .map_or(0.95, |(_, p)| p.score)
                })
                .sum::<f64>()
                / candidates.len() as f64;
            if avg >= baseline - 0.03 {
                continue;
            }
            if let Some((i, _)) = current {
                pairs.remove(i);
            }
            for p in proposed {
                un_parts.retain(|&i| i != if merged_b { p.a } else { p.b });
                pairs.push(p);
            }
            un_merged.retain(|&i| i != mi);
        }
    }
    Ok(())
}
