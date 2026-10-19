//! Native inputs formerly supplied by tools.js _computeExactCrossings: transverse
//! intersections, self intersections, endpoint-to-wall projections and0.75px cut
//! deduplication. The retained graph and curve reconstruction own the resulting face.
use super::super::geometry::{segment_crossing, CrossingIn, Wall};
use super::polygon;

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub(super) fn find(walls: &[Wall], tolerance: f64) -> Vec<CrossingIn> {
    let paths: Vec<_> = walls.iter().map(|w| polygon(&w.segments, false)).collect();
    let tables: Vec<Vec<f64>> = paths
        .iter()
        .map(|p| {
            let mut total = 0.0;
            let mut values = vec![0.0];
            for pair in p.windows(2) {
                total += distance(pair[0], pair[1]);
                values.push(total);
            }
            values
        })
        .collect();
    let mut hits = Vec::new();
    for i in 0..paths.len() {
        for j in i..paths.len() {
            let a = &paths[i];
            let b = &paths[j];
            let la = *tables[i].last().unwrap_or(&0.0);
            let lb = *tables[j].last().unwrap_or(&0.0);
            if la < 1e-9 || lb < 1e-9 {
                continue;
            }
            for ki in 0..a.len().saturating_sub(1) {
                for kj in 0..b.len().saturating_sub(1) {
                    if i == j
                        && (kj <= ki + 1 || (ki == 0 && kj == b.len() - 2 && a.first() == a.last()))
                    {
                        continue;
                    }
                    if let Some((pt, t, u)) = segment_crossing(a[ki], a[ki + 1], b[kj], b[kj + 1]) {
                        hits.push(CrossingIn {
                            wall: i,
                            frac: (tables[i][ki] + t * (tables[i][ki + 1] - tables[i][ki])) / la,
                            pt,
                        });
                        hits.push(CrossingIn {
                            wall: j,
                            frac: (tables[j][kj] + u * (tables[j][kj + 1] - tables[j][kj])) / lb,
                            pt,
                        });
                    }
                }
            }
        }
    }
    for (i, path) in paths.iter().enumerate() {
        for endpoint in path.first().into_iter().chain(path.last()) {
            for (j, other) in paths.iter().enumerate() {
                if i == j {
                    continue;
                }
                let total = *tables[j].last().unwrap_or(&0.0);
                if total < 1e-9 {
                    continue;
                }
                let mut closest = None;
                let mut best = tolerance;
                for k in 0..other.len().saturating_sub(1) {
                    let a = other[k];
                    let b = other[k + 1];
                    let d = [b[0] - a[0], b[1] - a[1]];
                    let n = d[0] * d[0] + d[1] * d[1];
                    if n < 1e-18 {
                        continue;
                    }
                    let t = (((endpoint[0] - a[0]) * d[0] + (endpoint[1] - a[1]) * d[1]) / n)
                        .clamp(0.0, 1.0);
                    let p = [a[0] + t * d[0], a[1] + t * d[1]];
                    let delta = distance(*endpoint, p);
                    if delta <= best {
                        best = delta;
                        closest = Some(CrossingIn {
                            wall: j,
                            frac: (tables[j][k] + t * (tables[j][k + 1] - tables[j][k])) / total,
                            pt: p,
                        });
                    }
                }
                if let Some(cut) = closest {
                    hits.push(cut);
                }
            }
        }
    }
    // Keep authored polyline corners as real cut nodes. Otherwise arc-length
    // resampling can step over a sharp corner inside one long self-crossing
    // wall and round off a region whose exact straight-edge area is known.
    for (wall, input) in walls.iter().enumerate() {
        let total = *tables[wall].last().unwrap_or(&0.0);
        if total < 1e-9 {
            continue;
        }
        for anchor in input
            .segments
            .iter()
            .skip(1)
            .take(input.segments.len().saturating_sub(2))
        {
            if anchor.handle_in != [0.0; 2] || anchor.handle_out != [0.0; 2] {
                continue;
            }
            if let Some((i, &pt)) = paths[wall]
                .iter()
                .enumerate()
                .find(|(_, p)| distance(**p, anchor.point) < 1e-9)
            {
                hits.push(CrossingIn {
                    wall,
                    frac: tables[wall][i] / total,
                    pt,
                });
            }
        }
    }
    // A genuine closed sub-arc can start and end at the same crossing. The
    // retained graph deliberately rejects a==b noise edges, so give that arc
    // one real interior cut rather than dropping an entire enclosed face.
    let mut splits = Vec::new();
    for (wall, path) in paths.iter().enumerate() {
        let total = *tables[wall].last().unwrap_or(&0.0);
        if total < 1e-9 || path.len() < 3 {
            continue;
        }
        let mut cuts: Vec<_> = hits
            .iter()
            .filter(|c| c.wall == wall)
            .map(|c| (c.frac, c.pt))
            .collect();
        cuts.push((0.0, path[0]));
        cuts.push((1.0, *path.last().unwrap()));
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        cuts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
        for pair in cuts.windows(2) {
            if distance(pair[0].1, pair[1].1) > 0.75 {
                continue;
            }
            let far = path
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    tables[wall][*i] / total > pair[0].0 && tables[wall][*i] / total < pair[1].0
                })
                .max_by(|a, b| distance(*a.1, pair[0].1).total_cmp(&distance(*b.1, pair[0].1)));
            if let Some((i, &pt)) = far {
                if distance(pt, pair[0].1) > 1.5 {
                    splits.push(CrossingIn {
                        wall,
                        frac: tables[wall][i] / total,
                        pt,
                    });
                }
            }
        }
    }
    hits.extend(splits);
    hits.sort_by(|a, b| a.wall.cmp(&b.wall).then(a.frac.total_cmp(&b.frac)));
    let mut kept: Vec<CrossingIn> = Vec::new();
    for hit in hits {
        let length = *tables[hit.wall].last().unwrap_or(&0.0);
        if kept
            .last()
            .is_none_or(|last| last.wall != hit.wall || (hit.frac - last.frac) * length > 0.75)
        {
            kept.push(hit);
        }
    }
    kept
}
