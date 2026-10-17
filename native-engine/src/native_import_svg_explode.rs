//! SVG import explode kernel (L10-D, lane 10 / #2116; ID G35).
//!
//! Pure port of the geometry decisions of the agreed desktop baseline (`fbe4b8a3`) that turn
//! the leaves of an imported SVG into flat paths: `explodeLeaf` of `src/js/svg-import.js`
//! and the keyhole helpers `_polyClosestPair` / `_mergeHoleIntoExterior` of `src/js/tools.js`.
//! Paper.js stays the SVG parser, matrix baker and fitter: the kernel input is the post-bake,
//! post-fit leaf list the Paper adapter produces. Every number Paper computes (segment
//! points and handles, each compound child's `clockwise` flag and `bounds`, the leaf style)
//! is received as data; no curve-bounds or area mathematics is rewritten here.
//!
//! Baseline facts the kernel keeps on purpose:
//! - a plain `Path` leaf passes through untouched ([`PathOrigin::Passthrough`]);
//! - a `CompoundPath` splits its children into exteriors (`clockwise`) and holes. With no hole
//!   every exterior (or every child when there is no exterior, which only an empty compound
//!   can reach) becomes an independent path wearing the compound's own fill, stroke, stroke
//!   width and opacity. With holes each exterior, in order, is merged with the holes its
//!   bounds contain (`ext.bounds.contains(h.bounds)`, Paper's `Rectangle#contains`), one
//!   keyhole slit per hole, folded over the growing segment list;
//! - a hole contained by no exterior simply vanishes in the baseline, and so does an entire
//!   compound with holes and no exterior (an all counter-clockwise compound). The kernel keeps
//!   that output exactly but reports it as data ([`ExplodedLeaf::unclaimed_holes`]), so nothing
//!   is silently absent;
//! - `_polyClosestPair` reads `a[i].x`, but `explodeLeaf` passes `segsOf` entries whose
//!   `point` is an `[x, y]` array. Every distance is NaN, no pair beats `Infinity`, and the
//!   baseline therefore always slits at exterior segment 0 and hole segment 0
//!   ([`HolePairing::BaselineArrayPoints`], the default). The real nearest-pair search of the
//!   function ([`closest_pair`], [`HolePairing::ClosestPoints`]) is kept, verified against
//!   the baseline function called with `{x, y}` points, as a lead-selectable alternative;
//! - `_mergeHoleIntoExterior` copies segment values exactly (point, handleIn, handleOut) and
//!   revisits the slit anchor exactly once: `ext[0..=i]`, then `hole[j..]` wrapped around
//!   plus `hole[j]` again, then `ext[i]` again, then `ext[i+1..]`. The merged path is closed.
//!
//! The baseline merge helper yields an invalid path for an empty contour (the oracle's `Mo`
//! `invalid` rows); that case is unreachable from Paper input, so the kernel reports it as the
//! typed refusal [`ExplodeError::EmptyContour`] and the whole plan is refused, so nothing
//! publishes half an import.

/// A 2-D point or vector as the two doubles Paper reports.
pub(crate) type Pt = [f64; 2];

/// One path segment: point plus the two handles, exactly as `segsOf` reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Seg {
    pub(crate) point: Pt,
    pub(crate) handle_in: Pt,
    pub(crate) handle_out: Pt,
}

/// Paper `Rectangle` of an item's `bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Bounds {
    /// Paper `Rectangle#contains(rect)` (`_containsRectangle`): the inner rectangle's
    /// edges lie on or inside this one. NaN never satisfies a comparison.
    pub(crate) fn contains(&self, inner: &Bounds) -> bool {
        inner.x >= self.x
            && inner.y >= self.y
            && inner.x + inner.width <= self.x + self.width
            && inner.y + inner.height <= self.y + self.height
    }
}

/// Fill, stroke, stroke width and opacity of a leaf. Paints are the adapter's serialized
/// Paper colors (`None` is Paper's `null`); the kernel only copies them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LeafStyle {
    pub(crate) fill: Option<String>,
    pub(crate) stroke: Option<String>,
    pub(crate) stroke_width: f64,
    pub(crate) opacity: f64,
}

/// A contour: its segments and whether Paper reports it closed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Contour {
    pub(crate) segments: Vec<Seg>,
    pub(crate) closed: bool,
}

/// One child of a `CompoundPath` with the two facts the classification reads.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompoundChild {
    pub(crate) contour: Contour,
    pub(crate) clockwise: bool,
    pub(crate) bounds: Bounds,
}

/// A baked, fitted leaf of the imported tree (`collectLeaves` collects `Path` and
/// `CompoundPath` items).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImportLeaf {
    Path {
        contour: Contour,
        style: LeafStyle,
    },
    Compound {
        children: Vec<CompoundChild>,
        style: LeafStyle,
    },
}

/// Which input an exploded path came from (child indices are positions in the compound).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PathOrigin {
    /// A plain `Path` leaf, unchanged.
    Passthrough,
    /// A compound child promoted to an independent path (no holes in the compound).
    Island { child: usize },
    /// An exterior merged with the holes its bounds contain (possibly none).
    Merged { exterior: usize, holes: Vec<usize> },
}

/// One path of the import, in insertion order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExplodedPath {
    pub(crate) origin: PathOrigin,
    pub(crate) contour: Contour,
    pub(crate) style: LeafStyle,
}

/// The paths one leaf produces plus the holes no exterior claimed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExplodedLeaf {
    pub(crate) paths: Vec<ExplodedPath>,
    /// Compound child indices of holes that no exterior's bounds contain (the baseline
    /// drops them; for an all counter-clockwise compound this lists every child).
    pub(crate) unclaimed_holes: Vec<usize>,
}

/// Why a leaf cannot be exploded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExplodeError {
    /// A merge reached a contour with no segments (the baseline merge helper is invalid there).
    EmptyContour { child: usize },
}

/// How the keyhole pair (exterior segment, hole segment) is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HolePairing {
    /// What `explodeLeaf` really does: `[x, y]` array points make every distance NaN, so the
    /// pair is always (0, 0).
    BaselineArrayPoints,
    /// The nearest pair of segment points (`_polyClosestPair` over `{x, y}` points).
    ClosestPoints,
}

/// `_polyClosestPair`: the first pair, in exterior-major order, with the smallest squared
/// distance (strict `<`); (0, 0) when either list is empty or no distance is below
/// `Infinity` (including NaN distances).
pub(crate) fn closest_pair(a: &[Pt], b: &[Pt]) -> (usize, usize) {
    let mut best_d = f64::INFINITY;
    let mut best = (0usize, 0usize);
    for (i, p) in a.iter().enumerate() {
        for (j, q) in b.iter().enumerate() {
            let dx = p[0] - q[0];
            let dy = p[1] - q[1];
            let d = dx * dx + dy * dy;
            if d < best_d {
                best_d = d;
                best = (i, j);
            }
        }
    }
    best
}

/// The slit pair `pairing` selects for merging `hole` into `ext`.
pub(crate) fn pick_pair(pairing: HolePairing, ext: &[Seg], hole: &[Seg]) -> (usize, usize) {
    match pairing {
        HolePairing::BaselineArrayPoints => (0, 0),
        HolePairing::ClosestPoints => {
            let a: Vec<Pt> = ext.iter().map(|s| s.point).collect();
            let b: Vec<Pt> = hole.iter().map(|s| s.point).collect();
            closest_pair(&a, &b)
        }
    }
}

/// `_mergeHoleIntoExterior` for a chosen pair `(i, j)`: `ext[0..=i]`, `hole[j..]` wrapped
/// with `hole[j]` repeated at the end, `ext[i]` again, `ext[i+1..]`. `None` when either
/// contour is empty or the pair is out of range (the baseline pushes `undefined` there).
pub(crate) fn merge_hole_into_exterior(
    ext: &[Seg],
    hole: &[Seg],
    pair: (usize, usize),
) -> Option<Vec<Seg>> {
    let (i, j) = pair;
    if ext.is_empty() || hole.is_empty() || i >= ext.len() || j >= hole.len() {
        return None;
    }
    let mut out: Vec<Seg> = Vec::with_capacity(ext.len() + hole.len() + 2);
    out.extend_from_slice(&ext[..=i]);
    for h in 0..=hole.len() {
        out.push(hole[(j + h) % hole.len()]);
    }
    out.push(ext[i]);
    out.extend_from_slice(&ext[i + 1..]);
    Some(out)
}

/// `explodeLeaf` with the baseline's effective hole pairing.
pub(crate) fn explode_leaf(leaf: &ImportLeaf) -> Result<ExplodedLeaf, ExplodeError> {
    explode_leaf_with(leaf, HolePairing::BaselineArrayPoints)
}

/// `explodeLeaf` with an explicit keyhole pairing.
pub(crate) fn explode_leaf_with(
    leaf: &ImportLeaf,
    pairing: HolePairing,
) -> Result<ExplodedLeaf, ExplodeError> {
    match leaf {
        ImportLeaf::Path { contour, style } => Ok(ExplodedLeaf {
            paths: vec![ExplodedPath {
                origin: PathOrigin::Passthrough,
                contour: contour.clone(),
                style: style.clone(),
            }],
            unclaimed_holes: Vec::new(),
        }),
        ImportLeaf::Compound { children, style } => explode_compound(children, style, pairing),
    }
}

fn explode_compound(
    children: &[CompoundChild],
    style: &LeafStyle,
    pairing: HolePairing,
) -> Result<ExplodedLeaf, ExplodeError> {
    let exteriors: Vec<usize> = (0..children.len())
        .filter(|&k| children[k].clockwise)
        .collect();
    let holes: Vec<usize> = (0..children.len())
        .filter(|&k| !children[k].clockwise)
        .collect();
    let mut paths: Vec<ExplodedPath> = Vec::new();
    if holes.is_empty() {
        let source: Vec<usize> = if exteriors.is_empty() {
            (0..children.len()).collect()
        } else {
            exteriors
        };
        for k in source {
            paths.push(ExplodedPath {
                origin: PathOrigin::Island { child: k },
                contour: children[k].contour.clone(),
                style: style.clone(),
            });
        }
        return Ok(ExplodedLeaf {
            paths,
            unclaimed_holes: Vec::new(),
        });
    }
    let mut claimed = vec![false; children.len()];
    for &e in &exteriors {
        let mine: Vec<usize> = holes
            .iter()
            .copied()
            .filter(|&h| children[e].bounds.contains(&children[h].bounds))
            .collect();
        let mut segments: Vec<Seg> = children[e].contour.segments.clone();
        for &h in &mine {
            claimed[h] = true;
            let hole = &children[h].contour.segments;
            let pair = pick_pair(pairing, &segments, hole);
            let ext_was_empty = segments.is_empty();
            segments = match merge_hole_into_exterior(&segments, hole, pair) {
                Some(merged) => merged,
                None => {
                    let child = if ext_was_empty { e } else { h };
                    return Err(ExplodeError::EmptyContour { child });
                }
            };
        }
        paths.push(ExplodedPath {
            origin: PathOrigin::Merged {
                exterior: e,
                holes: mine,
            },
            contour: Contour {
                segments,
                closed: true,
            },
            style: style.clone(),
        });
    }
    let unclaimed_holes: Vec<usize> = holes.iter().copied().filter(|&h| !claimed[h]).collect();
    Ok(ExplodedLeaf {
        paths,
        unclaimed_holes,
    })
}

#[path = "native_import_svg_plan.rs"]
pub(crate) mod plan;

#[cfg(test)]
#[path = "../tests/native_import_svg_explode_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/native_import_svg_explode.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_svg_explode_pins.rs"]
mod pins;

#[cfg(test)]
#[path = "../tests/native_import_svg_explode_controls.rs"]
mod controls;
