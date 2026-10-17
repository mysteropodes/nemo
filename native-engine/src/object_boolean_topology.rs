//! Boolean result topology kernel (G19 + G18, L02-D): the pure rules that turn
//! the output of a polygon boolean / erase provider into an ordered island
//! plan, plus the insert, eraser and same-colour fill-merge admission rules.
//!
//! Mirrors `_polygonsToPaperItem`, `flattenBooleanResult`, `insertBooleanResult`
//! and `carryBooleanData` (`src/js/tools.js`), the decision half of
//! `eraseAtPoint` and the candidate / union / commit bookkeeping of
//! `fillMergeSameColor` at fbe4b8a3. No polygon mathematics lives here: the
//! boolean fold, the keyhole slit (`_mergeHoleIntoExterior`), the degenerate
//! loop cleanup and every geometric probe stay with the provider, and the
//! kernel only validates what comes back and decides order, pairing, style and
//! identity. Deliberate differences from the baseline (each reported, never
//! silent): a hole that no exterior's bounds contain is a typed refusal
//! instead of being dropped; a hole of fewer than three points is a typed
//! refusal instead of a bogus extra island; an erase that leaves nothing
//! deletes its target instead of throwing.
use std::fmt;

#[path = "object_boolean_erase.rs"]
mod erase;
#[path = "object_boolean_fill_commit.rs"]
mod fill_commit;
#[path = "object_boolean_fill_merge.rs"]
mod fill_merge;
#[path = "object_boolean_insert.rs"]
mod insert;

#[allow(unused_imports)]
pub(crate) use erase::{
    erase_stroke_info, plan_erase_polygons, plan_erase_shape, wasm_result_plausible, EraseOutcome,
    ErasePlan, EraseTarget,
};
#[allow(unused_imports)]
pub(crate) use fill_commit::{
    applied_tags, merge_tags, plan_merge_commit, AppliedTags, MergeCommit, MergeCommitInput,
    MergedTags, ParticipantTags,
};
#[allow(unused_imports)]
pub(crate) use fill_merge::{
    is_near_identical_area, merge_candidates, plan_union, resolve_union, select_absorbed, Absorbed,
    AreaFacts, FillFacts, MergeDecision, MergeError, ProbeError, TouchProbe, UnavailableTouch,
    UnionPlan, UniteDecision,
};
#[allow(unused_imports)]
pub(crate) use insert::{
    carry_boolean_data, plan_insert, DataCarry, DataValue, InsertPlan, InsertRequest, PaintArg,
    PlacedIsland, SourceData, StrokeIdAction, StrokeInfo, KEEP_DATA_ALL,
};

/// Axis-aligned rectangle with Paper.js `Rectangle` semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Bounds {
    /// Bounds of a path with no segments.
    pub(crate) const EMPTY: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    /// Bounds of a straight-segment polygon (min / max over the points).
    pub(crate) fn of_points(points: &[[f64; 2]]) -> Bounds {
        let first = match points.first() {
            Some(point) => *point,
            None => return Bounds::EMPTY,
        };
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (first[0], first[1], first[0], first[1]);
        for point in points {
            if point[0] < min_x {
                min_x = point[0];
            }
            if point[1] < min_y {
                min_y = point[1];
            }
            if point[0] > max_x {
                max_x = point[0];
            }
            if point[1] > max_y {
                max_y = point[1];
            }
        }
        Bounds {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    /// `Rectangle.contains(rectangle)`: inclusive on every edge.
    pub(crate) fn contains(&self, other: &Bounds) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }

    fn check(&self, ring: usize) -> Result<(), TopologyError> {
        let finite = self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite();
        if !finite {
            return Err(TopologyError::NonFiniteBounds { ring });
        }
        if self.width < 0.0 || self.height < 0.0 {
            return Err(TopologyError::NegativeExtent { ring });
        }
        Ok(())
    }
}

/// One child of a compound result: its winding flag, bounds and segment count.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ring {
    pub(crate) clockwise: bool,
    pub(crate) bounds: Bounds,
    pub(crate) segments: usize,
}

/// A result that is one plain path (no holes, no islands).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathFacts {
    pub(crate) bounds: Bounds,
    pub(crate) segments: usize,
}

/// What a boolean / erase provider returned.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ResultShape {
    /// No polygon at all (`_polygonsToPaperItem` returns null).
    Empty,
    Single(PathFacts),
    Compound(Vec<Ring>),
}

impl ResultShape {
    /// `eraseAtPoint`'s `hasArea`: a compound with a child, or a path with a segment.
    pub(crate) fn has_area(&self) -> bool {
        match self {
            ResultShape::Empty => false,
            ResultShape::Single(path) => path.segments > 0,
            ResultShape::Compound(rings) => !rings.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TopologyError {
    NonFinitePoint {
        polygon: usize,
    },
    NonFiniteBounds {
        ring: usize,
    },
    NegativeExtent {
        ring: usize,
    },
    /// A hole of fewer than three points inside a kept polygon.
    DegenerateHole {
        polygon: usize,
        hole: usize,
    },
    /// Holes (child indices) that no exterior's bounds contain.
    UnpairedHoles(Vec<usize>),
    /// A ring the provider returned with fewer than three segments.
    UnusableRing {
        ring: usize,
        segments: usize,
    },
    TargetOutOfRange {
        index: usize,
        layer_len: usize,
    },
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Provider polygon: `{exterior, holes}` of `boolean_op_multi` / `erase_at_point`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Polygon {
    pub(crate) exterior: Vec<[f64; 2]>,
    pub(crate) holes: Vec<Vec<[f64; 2]>>,
}

/// A polygon list admitted as a result shape, plus the dropped polygon indices.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Admission {
    pub(crate) shape: ResultShape,
    pub(crate) dropped: Vec<usize>,
}

fn all_finite(points: &[[f64; 2]]) -> bool {
    points.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}

/// `_polygonsToPaperItem`: polygons with fewer than three exterior points are
/// dropped (reported in `dropped`); one hole-free polygon is a plain path;
/// anything else is a compound whose exteriors are clockwise and holes are not,
/// in polygon order (exterior first, then its holes).
pub(crate) fn admit_polygons(polygons: &[Polygon]) -> Result<Admission, TopologyError> {
    let mut dropped = Vec::new();
    let mut kept = Vec::new();
    for (index, polygon) in polygons.iter().enumerate() {
        if polygon.exterior.len() < 3 {
            dropped.push(index);
        } else {
            kept.push(index);
        }
    }
    if kept.is_empty() {
        return Ok(Admission {
            shape: ResultShape::Empty,
            dropped,
        });
    }
    for &index in &kept {
        let polygon = &polygons[index];
        let finite = all_finite(&polygon.exterior)
            && polygon.holes.iter().all(|hole| all_finite(hole.as_slice()));
        if !finite {
            return Err(TopologyError::NonFinitePoint { polygon: index });
        }
        if let Some(hole) = polygon.holes.iter().position(|hole| hole.len() < 3) {
            return Err(TopologyError::DegenerateHole {
                polygon: index,
                hole,
            });
        }
    }
    if kept.len() == 1 && polygons[kept[0]].holes.is_empty() {
        let exterior = &polygons[kept[0]].exterior;
        return Ok(Admission {
            shape: ResultShape::Single(PathFacts {
                bounds: Bounds::of_points(exterior),
                segments: exterior.len(),
            }),
            dropped,
        });
    }
    let mut rings = Vec::new();
    for &index in &kept {
        let polygon = &polygons[index];
        rings.push(Ring {
            clockwise: true,
            bounds: Bounds::of_points(&polygon.exterior),
            segments: polygon.exterior.len(),
        });
        for hole in &polygon.holes {
            rings.push(Ring {
                clockwise: false,
                bounds: Bounds::of_points(hole),
                segments: hole.len(),
            });
        }
    }
    Ok(Admission {
        shape: ResultShape::Compound(rings),
        dropped,
    })
}

/// How `flattenBooleanResult` produces an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IslandBuild {
    /// The result itself (one plain path).
    Single,
    /// An exterior child taken as is (compound without holes).
    Flat,
    /// A path rebuilt from an exterior with its holes slit-merged in.
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IslandPlan {
    pub(crate) build: IslandBuild,
    /// Child index of the exterior (0 for a single path).
    pub(crate) ring: usize,
    /// Child indices of the holes merged into it, in merge order.
    pub(crate) holes: Vec<usize>,
    /// `_eraseDegenerateSelfLoops` runs on this island (single and merged only).
    pub(crate) cleanup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopologyPlan {
    pub(crate) islands: Vec<IslandPlan>,
    pub(crate) exterior_count: usize,
    pub(crate) hole_count: usize,
    /// Holes whose bounds sit inside more than one exterior (all of them cut).
    pub(crate) shared_holes: Vec<usize>,
}

/// `flattenBooleanResult` classification. Island order is exterior order; a
/// hole is merged into every exterior whose bounds contain it, in hole order.
/// A hole contained by no exterior is refused (the baseline dropped it).
pub(crate) fn classify(shape: &ResultShape) -> Result<TopologyPlan, TopologyError> {
    match shape {
        ResultShape::Empty => Ok(TopologyPlan {
            islands: Vec::new(),
            exterior_count: 0,
            hole_count: 0,
            shared_holes: Vec::new(),
        }),
        ResultShape::Single(path) => {
            path.bounds.check(0)?;
            Ok(TopologyPlan {
                islands: vec![IslandPlan {
                    build: IslandBuild::Single,
                    ring: 0,
                    holes: Vec::new(),
                    cleanup: true,
                }],
                exterior_count: 1,
                hole_count: 0,
                shared_holes: Vec::new(),
            })
        }
        ResultShape::Compound(rings) => classify_rings(rings),
    }
}

fn classify_rings(rings: &[Ring]) -> Result<TopologyPlan, TopologyError> {
    for (index, ring) in rings.iter().enumerate() {
        ring.bounds.check(index)?;
    }
    let exteriors: Vec<usize> = (0..rings.len()).filter(|&i| rings[i].clockwise).collect();
    let holes: Vec<usize> = (0..rings.len()).filter(|&i| !rings[i].clockwise).collect();
    if holes.is_empty() {
        let islands = exteriors
            .iter()
            .map(|&ring| IslandPlan {
                build: IslandBuild::Flat,
                ring,
                holes: Vec::new(),
                cleanup: false,
            })
            .collect();
        return Ok(TopologyPlan {
            islands,
            exterior_count: exteriors.len(),
            hole_count: 0,
            shared_holes: Vec::new(),
        });
    }
    let mut pair_count = vec![0usize; rings.len()];
    let mut islands = Vec::new();
    for &exterior in &exteriors {
        let mine: Vec<usize> = holes
            .iter()
            .copied()
            .filter(|&hole| rings[exterior].bounds.contains(&rings[hole].bounds))
            .collect();
        for &hole in &mine {
            pair_count[hole] += 1;
        }
        islands.push(IslandPlan {
            build: IslandBuild::Merged,
            ring: exterior,
            holes: mine,
            cleanup: true,
        });
    }
    let unpaired: Vec<usize> = holes
        .iter()
        .copied()
        .filter(|&hole| pair_count[hole] == 0)
        .collect();
    if !unpaired.is_empty() {
        return Err(TopologyError::UnpairedHoles(unpaired));
    }
    let shared_holes = holes
        .iter()
        .copied()
        .filter(|&hole| pair_count[hole] > 1)
        .collect();
    Ok(TopologyPlan {
        islands,
        exterior_count: exteriors.len(),
        hole_count: holes.len(),
        shared_holes,
    })
}

#[cfg(test)]
#[path = "../tests/object_boolean_topology_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_controls_commit.rs"]
mod controls_commit;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_controls_erase.rs"]
mod controls_erase;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_controls_merge.rs"]
mod controls_merge;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_erase.rs"]
mod erase_tests;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_merge.rs"]
mod merge_tests;
#[cfg(test)]
#[path = "../tests/object_boolean_topology_mutations.rs"]
mod mutations;
#[cfg(test)]
#[path = "../tests/object_boolean_topology.rs"]
mod tests;
