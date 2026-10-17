//! Glyph contour kernel (G38b / L06-B): the outline half of
//! `buildGlyphPaths` in `src/js/vector-text-bridge.js` at fbe4b8a3, as pure
//! data in and out.
//!
//! An opentype.js glyph path (`M`/`L`/`Q`/`C`/`Z` commands, coordinates
//! already in pixel space) becomes cubic contours whose handles are OFFSETS
//! from their own anchor (`otPathToContours`); each contour is classified as an
//! exterior or a hole of its tightest bounding-box container
//! (`contourBBox`, `bboxContains`, `bboxArea`); holes are merged into their
//! exterior with the baseline "keyhole slit" (`contoursToMergedPaths`,
//! `_mergeHoleIntoExterior`); and the faux-italic shear is applied to every
//! finished path the way Paper.js applies `Path.transform`.
//!
//! Baseline behavior reproduced on purpose, not repaired:
//! * Quadratics convert as `c1 = p0 + 2/3 (q - p0)`, `c2 = p + 2/3 (q - p)`
//!   and the stored handle is `c - anchor`, so the handle carries that
//!   rounding (it is not `2/3 * (q - p0)` computed directly).
//! * `Z` drops the last point only when it is within 0.01 of the first on both
//!   axes, hands that point's `handleIn` to segment 0, and leaves a one-point
//!   contour alone. A second `Z` applies the same test again.
//! * Classification is bbox containment with `<=` (so equal boxes contain
//!   each other and BOTH become holes, producing no output path), the tightest
//!   container wins by strictly smaller area (the first one found keeps a tie),
//!   and there is no winding test. A hole inside a hole is a hole of the inner
//!   container, whose own holes are then never merged: the island vanishes.
//! * The slit always lands on index `(0, 0)`. The baseline calls
//!   `_polyClosestPair` with `[x, y]` arrays and reads `.x` / `.y`, which are
//!   `undefined`, so every distance is `NaN`, no comparison succeeds and the
//!   initial pair `(0, 0)` is returned. The merged order is
//!   `ext[0], hole[0..], hole[0], ext[0], ext[1..]`.
//! * The shear moves anchors by the matrix and handles as `T(anchor + handle)
//!   - T(anchor)` (Paper transforms handles as absolute points), skipping a
//!   handle that Paper's `isZero` (both axes within 1e-12) calls zero.
//!
//! Native-only guards, all refusals before any output: a non-finite command
//! coordinate or shear value, a segment command before the first `M` (the
//! baseline throws a `TypeError` there), and a non-finite sheared coordinate.
//! The per-path [`PathFlag`] report mirrors what `ClosedCubicPath::new`
//! (`render_geometry.rs`) will refuse (2..=256 segments, curves degenerate at
//! f32 precision) so the scene join can refuse a glyph instead of dropping it.
//! Color, group identity and `data.*` tags belong to the scene join, not here.
//!
//! This module imports nothing from the crate.

#[path = "text_glyph_contours_shear.rs"]
pub(crate) mod shear;

pub(crate) use shear::{path_flags, shear_segments, ItalicShear, PathFlag};

/// One opentype.js path command, coordinates in final pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OutlineCommand {
    Move {
        x: f64,
        y: f64,
    },
    Line {
        x: f64,
        y: f64,
    },
    Quad {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    Cubic {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Close,
}

impl OutlineCommand {
    fn is_finite(&self) -> bool {
        match *self {
            OutlineCommand::Move { x, y } | OutlineCommand::Line { x, y } => {
                x.is_finite() && y.is_finite()
            }
            OutlineCommand::Quad { x1, y1, x, y } => [x1, y1, x, y].iter().all(|v| v.is_finite()),
            OutlineCommand::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => [x1, y1, x2, y2, x, y].iter().all(|v| v.is_finite()),
            OutlineCommand::Close => true,
        }
    }
}

/// A Paper-style segment: anchor plus handles relative to that anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphSegment {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// `contourBBox`: the box of the anchors only (handles never count).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContourBox {
    pub(crate) min_x: f64,
    pub(crate) min_y: f64,
    pub(crate) max_x: f64,
    pub(crate) max_y: f64,
}

/// Why a glyph outline was refused (nothing is produced).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphContourError {
    /// Command `command` carries a NaN or infinite coordinate.
    NonFiniteCoordinate { command: usize },
    /// Command `command` is `L`/`Q`/`C` before any `M` (the baseline throws).
    MissingMove { command: usize },
    /// The shear skew or baseline is NaN or infinite.
    NonFiniteShear,
    /// The shear produced a non-finite coordinate (an overflow).
    NonFiniteGeometry,
}

/// One finished glyph path: closed, fill and stroke are the scene join's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlyphPath {
    pub(crate) segments: Vec<GlyphSegment>,
    pub(crate) flags: Vec<PathFlag>,
}

fn anchor(x: f64, y: f64) -> GlyphSegment {
    GlyphSegment {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

fn current(
    contours: &mut [Vec<GlyphSegment>],
    command: usize,
) -> Result<&mut Vec<GlyphSegment>, GlyphContourError> {
    contours
        .last_mut()
        .ok_or(GlyphContourError::MissingMove { command })
}

/// `otPathToContours`.
pub(crate) fn outline_to_contours(
    commands: &[OutlineCommand],
) -> Result<Vec<Vec<GlyphSegment>>, GlyphContourError> {
    let mut contours: Vec<Vec<GlyphSegment>> = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        if !command.is_finite() {
            return Err(GlyphContourError::NonFiniteCoordinate { command: index });
        }
        match *command {
            OutlineCommand::Move { x, y } => contours.push(vec![anchor(x, y)]),
            OutlineCommand::Line { x, y } => current(&mut contours, index)?.push(anchor(x, y)),
            OutlineCommand::Quad { x1, y1, x, y } => {
                let cur = current(&mut contours, index)?;
                let last = cur.len() - 1;
                let p0 = cur[last].point;
                let c1x = p0[0] + 2.0 / 3.0 * (x1 - p0[0]);
                let c1y = p0[1] + 2.0 / 3.0 * (y1 - p0[1]);
                let c2x = x + 2.0 / 3.0 * (x1 - x);
                let c2y = y + 2.0 / 3.0 * (y1 - y);
                cur[last].handle_out = [c1x - p0[0], c1y - p0[1]];
                cur.push(GlyphSegment {
                    point: [x, y],
                    handle_in: [c2x - x, c2y - y],
                    handle_out: [0.0, 0.0],
                });
            }
            OutlineCommand::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let cur = current(&mut contours, index)?;
                let last = cur.len() - 1;
                let p0 = cur[last].point;
                cur[last].handle_out = [x1 - p0[0], y1 - p0[1]];
                cur.push(GlyphSegment {
                    point: [x, y],
                    handle_in: [x2 - x, y2 - y],
                    handle_out: [0.0, 0.0],
                });
            }
            OutlineCommand::Close => {
                // A `Z` before any `M` does nothing (the baseline tests `cur &&`).
                if let Some(cur) = contours.last_mut() {
                    if cur.len() > 1 {
                        let first = cur[0].point;
                        let last = cur[cur.len() - 1];
                        if (first[0] - last.point[0]).abs() < 0.01
                            && (first[1] - last.point[1]).abs() < 0.01
                        {
                            cur[0].handle_in = last.handle_in;
                            cur.pop();
                        }
                    }
                }
            }
        }
    }
    Ok(contours)
}

/// `contourBBox`.
pub(crate) fn contour_bbox(segments: &[GlyphSegment]) -> ContourBox {
    let mut b = ContourBox {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for s in segments {
        let p = s.point;
        if p[0] < b.min_x {
            b.min_x = p[0];
        }
        if p[0] > b.max_x {
            b.max_x = p[0];
        }
        if p[1] < b.min_y {
            b.min_y = p[1];
        }
        if p[1] > b.max_y {
            b.max_y = p[1];
        }
    }
    b
}

/// `bboxContains(a, b)`: `a` contains `b`, edges included.
pub(crate) fn bbox_contains(a: &ContourBox, b: &ContourBox) -> bool {
    a.min_x <= b.min_x && a.min_y <= b.min_y && a.max_x >= b.max_x && a.max_y >= b.max_y
}

/// `bboxArea`.
pub(crate) fn bbox_area(b: &ContourBox) -> f64 {
    (b.max_x - b.min_x).max(0.0) * (b.max_y - b.min_y).max(0.0)
}

/// The `container` of each contour in `contoursToMergedPaths`: the tightest
/// (strictly smaller area wins, the first found keeps a tie) other contour
/// whose box contains it, `None` for an exterior.
pub(crate) fn contour_containers(contours: &[Vec<GlyphSegment>]) -> Vec<Option<usize>> {
    let boxes: Vec<ContourBox> = contours.iter().map(|c| contour_bbox(c)).collect();
    let mut containers: Vec<Option<usize>> = vec![None; boxes.len()];
    for (i, inner) in boxes.iter().enumerate() {
        for (j, outer) in boxes.iter().enumerate() {
            if i == j || !bbox_contains(outer, inner) {
                continue;
            }
            let replace = match containers[i] {
                None => true,
                Some(k) => bbox_area(&boxes[k]) > bbox_area(outer),
            };
            if replace {
                containers[i] = Some(j);
            }
        }
    }
    containers
}

/// The merges `contoursToMergedPaths` performs, in call order: for every
/// exterior (in contour order) the contours whose container it is. A hole
/// whose container is itself a hole never appears.
pub(crate) fn merge_plan(containers: &[Option<usize>]) -> Vec<(usize, usize)> {
    let mut plan = Vec::new();
    for (exterior, container) in containers.iter().enumerate() {
        if container.is_some() {
            continue;
        }
        for (hole, owner) in containers.iter().enumerate() {
            if *owner == Some(exterior) {
                plan.push((exterior, hole));
            }
        }
    }
    plan
}

/// The slit at explicit indices: `ext[0..=pi]`, the whole hole ring starting
/// at `pj` and back to `pj`, `ext[pi]` again, then the rest of `ext`. `None`
/// when an index is out of range or a ring is empty.
pub(crate) fn slit_merge_at(
    ext: &[GlyphSegment],
    hole: &[GlyphSegment],
    pi: usize,
    pj: usize,
) -> Option<Vec<GlyphSegment>> {
    if pi >= ext.len() || pj >= hole.len() {
        return None;
    }
    let mut out = Vec::with_capacity(ext.len() + hole.len() + 2);
    out.extend_from_slice(&ext[..=pi]);
    for h in 0..=hole.len() {
        out.push(hole[(pj + h) % hole.len()]);
    }
    out.push(ext[pi]);
    out.extend_from_slice(&ext[pi + 1..]);
    Some(out)
}

/// `_mergeHoleIntoExterior` as the baseline actually runs it: the pair is
/// always `(0, 0)` (see the module doc). An empty ring is returned unchanged;
/// the baseline cannot reach that case.
pub(crate) fn slit_merge(ext: &[GlyphSegment], hole: &[GlyphSegment]) -> Vec<GlyphSegment> {
    slit_merge_at(ext, hole, 0, 0).unwrap_or_else(|| ext.to_vec())
}

/// `contoursToMergedPaths`.
pub(crate) fn contours_to_merged_paths(contours: &[Vec<GlyphSegment>]) -> Vec<Vec<GlyphSegment>> {
    let containers = contour_containers(contours);
    let plan = merge_plan(&containers);
    let mut out = Vec::new();
    for (index, contour) in contours.iter().enumerate() {
        if containers[index].is_some() {
            continue;
        }
        let mut merged = contour.clone();
        for (_, hole) in plan.iter().filter(|(exterior, _)| *exterior == index) {
            merged = slit_merge(&merged, &contours[*hole]);
        }
        out.push(merged);
    }
    out
}

fn all_finite(segments: &[GlyphSegment]) -> bool {
    segments.iter().all(|s| {
        s.point
            .iter()
            .chain(&s.handle_in)
            .chain(&s.handle_out)
            .all(|v| v.is_finite())
    })
}

/// `buildGlyphPaths` minus color: contours, classification, slit merge, then
/// the faux-italic shear when `italic` is present and its skew is non-zero
/// (the baseline tests `italicSkew` for truthiness).
pub(crate) fn glyph_paths(
    commands: &[OutlineCommand],
    italic: Option<&ItalicShear>,
) -> Result<Vec<GlyphPath>, GlyphContourError> {
    let contours = outline_to_contours(commands)?;
    let mut paths = Vec::new();
    for merged in contours_to_merged_paths(&contours) {
        let segments = match italic {
            Some(shear) if shear.skew() != 0.0 => shear_segments(&merged, shear),
            _ => merged,
        };
        if !all_finite(&segments) {
            return Err(GlyphContourError::NonFiniteGeometry);
        }
        let flags = path_flags(&segments);
        paths.push(GlyphPath { segments, flags });
    }
    Ok(paths)
}

#[cfg(test)]
#[path = "../tests/text_glyph_contours_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/text_glyph_contours_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/text_glyph_contours_shear.rs"]
mod shear_tests;
#[cfg(test)]
#[path = "../tests/text_glyph_contours.rs"]
mod tests;
