//! Same-colour fill merge, commit half (the tail of `fillMergeSameColor`,
//! L02-D): which layer children disappear, where the united shape lands, and
//! which regeneration tags (`fillSeeds`, `fillWalls`, `fillGapPx`) the new
//! parts receive.
//!
//! The union geometry and its island topology come from the provider; the
//! kernel only orders and tags. Every participant (the new fill and EVERY
//! absorbed child) is removed, including a child whose union failed: the
//! baseline loses that child's shape, so the plan reports it in `failed`
//! instead of hiding it.
use crate::object_document::object_group_kernels::object_boolean_topology::{
    plan_insert, InsertPlan, InsertRequest, MergeError, PaintArg, ResultShape, SourceData,
    UniteDecision,
};

/// `data.fillSeeds` / `fillSeed` / `fillWalls` / `fillGapPx` of one participant
/// (all `None` when the participant has no `data`).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ParticipantTags {
    /// `fillSeeds` when it is an array (an empty array hides `fillSeed`).
    pub(crate) fill_seeds: Option<Vec<[f64; 2]>>,
    pub(crate) fill_seed: Option<[f64; 2]>,
    pub(crate) fill_walls: Option<Vec<String>>,
    pub(crate) fill_gap_px: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MergedTags {
    pub(crate) seeds: Vec<[f64; 2]>,
    pub(crate) walls: Vec<String>,
    pub(crate) gap_px: f64,
}

/// Union of the participants' tracking: seeds concatenated in participant
/// order, wall ids de-duplicated in first-seen order, the largest gap.
pub(crate) fn merge_tags(participants: &[ParticipantTags]) -> MergedTags {
    let mut merged = MergedTags {
        seeds: Vec::new(),
        walls: Vec::new(),
        gap_px: 0.0,
    };
    for participant in participants {
        match (&participant.fill_seeds, &participant.fill_seed) {
            (Some(seeds), _) => merged.seeds.extend(seeds.iter().copied()),
            (None, Some(seed)) => merged.seeds.push(*seed),
            (None, None) => {}
        }
        if let Some(walls) = &participant.fill_walls {
            for wall in walls {
                if !merged.walls.contains(wall) {
                    merged.walls.push(wall.clone());
                }
            }
        }
        if let Some(gap) = participant.fill_gap_px {
            if gap > merged.gap_px {
                merged.gap_px = gap;
            }
        }
    }
    merged
}

/// What is written on every new part when the merge carries seeds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppliedTags {
    pub(crate) seeds: Vec<[f64; 2]>,
    /// `None` leaves whatever `fillWalls` the part already had.
    pub(crate) walls: Option<Vec<String>>,
    /// `mergedGapPx || 24`.
    pub(crate) gap_px: f64,
}

/// Tags are applied only when at least one seed was gathered.
pub(crate) fn applied_tags(merged: &MergedTags) -> Option<AppliedTags> {
    if merged.seeds.is_empty() {
        return None;
    }
    let walls = if merged.walls.is_empty() {
        None
    } else {
        Some(merged.walls.clone())
    };
    let gap_px = if merged.gap_px != 0.0 && !merged.gap_px.is_nan() {
        merged.gap_px
    } else {
        24.0
    };
    Some(AppliedTags {
        seeds: merged.seeds.clone(),
        walls,
        gap_px,
    })
}

pub(crate) struct MergeCommitInput<'a> {
    /// Children in the layer before anything is removed.
    pub(crate) layer_len: usize,
    pub(crate) new_index: usize,
    /// Every absorbed child (including skipped near-identical and failed ones).
    pub(crate) absorbed: &'a [usize],
    pub(crate) unite: &'a UniteDecision,
    /// Island topology of the united result, from the provider.
    pub(crate) shape: &'a ResultShape,
    /// `colorHex8(newFill.fillColor)`.
    pub(crate) fill_hex8: &'a str,
    pub(crate) opacity: f64,
    /// `newFill.data`, the source of the carried data.
    pub(crate) data: Option<&'a SourceData>,
    /// New fill first, then the absorbed children in `absorbed` order.
    pub(crate) participants: &'a [ParticipantTags],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MergeCommit {
    /// Children to remove, ascending layer indices (new fill and all absorbed).
    pub(crate) removed: Vec<usize>,
    pub(crate) insert: InsertPlan,
    pub(crate) tags: Option<AppliedTags>,
    /// Final layer index of the first part (`parts[0]`), `None` when there is no part.
    pub(crate) first_index: Option<usize>,
    /// Children removed although their union failed (their shape is lost).
    pub(crate) lost: Vec<usize>,
}

/// Highest participant's slot, adjusted for the removals below it
/// (`topIdx - removedBelow`, clamped to the shortened layer), then the island
/// plan with `preserveGeometry = true` and no stroke.
pub(crate) fn plan_merge_commit(input: &MergeCommitInput) -> Result<MergeCommit, MergeError> {
    if input.participants.len() != input.absorbed.len() + 1 {
        return Err(MergeError::ParticipantMismatch {
            expected: input.absorbed.len() + 1,
            given: input.participants.len(),
        });
    }
    let mut removed = vec![input.new_index];
    removed.extend(input.absorbed.iter().copied());
    for (position, &index) in removed.iter().enumerate() {
        if index >= input.layer_len {
            return Err(MergeError::IndexOutOfRange(index));
        }
        if removed[..position].contains(&index) {
            return Err(MergeError::DuplicateIndex(index));
        }
    }
    let top = removed.iter().copied().max().unwrap_or(0);
    let below = removed.iter().filter(|&&index| index < top).count();
    let remaining = input.layer_len - removed.len();
    let insert_at = (top - below).min(remaining);
    let request = InsertRequest {
        insert_at,
        layer_len: remaining,
        fill: PaintArg::Set(input.fill_hex8.to_string()),
        opacity: Some(input.opacity),
        stroke: None,
        src: input.data,
        preserve_geometry: true,
    };
    let insert = plan_insert(input.shape, &request).map_err(MergeError::Topology)?;
    let first_index = insert.islands.first().map(|island| island.index);
    removed.sort_unstable();
    Ok(MergeCommit {
        removed,
        insert,
        tags: applied_tags(&merge_tags(input.participants)),
        first_index,
        lost: input.unite.failed.clone(),
    })
}
