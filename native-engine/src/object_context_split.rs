//! Element-splitting plan for a Component entry (L04-H, D33 + F14).
//!
//! `splitLayerIntoElementsCore` (`src/js/app.js`) and the silent auto-split of
//! `enterComponentLayer` (`src/js/motion.js`) at baseline `fbe4b8a3`, reduced to a
//! PLAN: which layers of an entered Component are replaced by one layer per shape,
//! and what each replacement carries. The plan names source stroke POSITIONS (the
//! consumer moves the very same stroke records), never mints a stroke id, and
//! applies nothing; the document edit is the consumer's sealed transaction.
//!
//! Baseline facts kept (each is a row of the oracle):
//! - elements are the strokes of the reference frame minus brush-texture copies
//!   and linked-fill companions; a linked owner takes its companion's fill for the
//!   label; a layer needs 2+ elements and no `symbolId` to split;
//! - the pieces are split BY POSITION in each stored keyframe (piece `e < n - 1`
//!   takes `strokes[e]`, the last piece everything from `n - 1` on, so a
//!   texture copy of an earlier shape lands in the last piece and a keyframe with
//!   more strokes than elements overflows into it); a frame that is neither a
//!   keyframe nor interpolated becomes a blank frame;
//! - a piece is named `<layer name or "Layer"> \u{2014} <label>`; a shape's own
//!   element Motion is promoted to the piece; the layer's outward references and
//!   render toggles are carried by [`CARRY_RULES`]; a frozen matte source is
//!   carried by every piece, a legacy matte and the `layerUid` by the last one,
//!   and the combine groups only where a member landed in the last piece;
//! - the auto-split on entry runs only for a Component holding exactly ONE layer,
//!   records one undo entry when it will split, and walks the layers highest index
//!   first. A component that was already split holds N > 1 layers, or layers of one
//!   element, so a second entry plans nothing.
//!
//! Flat layers are never converted: nothing here creates a Component or a
//! `symbolId`; `has_symbol` is only read, to refuse. Converting is an explicit
//! request of its own transaction (L04-M). Strokes that lack an id are listed in
//! [`SplitPlan::unstamped`] for the owner to assign; the kernel does not mint one.

use crate::evaluation::object_component_kernels::object_context_split_facts::{
    element_label, layer_elements, present, rule_applies, unstamped_positions, CarryMode,
    ElementWords, FieldState, FrameFacts, LayerFacts, CARRY_RULES,
};

#[cfg(test)]
#[path = "../tests/object_context_split_carry.rs"]
mod carry_tests;
#[cfg(test)]
#[path = "../tests/object_context_split_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_context_split_entry.rs"]
mod entry_tests;
#[cfg(test)]
#[path = "../tests/object_context_split_pinned.rs"]
mod pinned_tests;
#[cfg(test)]
#[path = "../tests/object_context_split.rs"]
mod tests;

/// One frame of a piece: the flags of the source frame and the positions of the
/// source strokes it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PieceFrame {
    pub is_keyframe: bool,
    pub is_interpolated: bool,
    pub stroke_indices: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CarriedField {
    pub field: &'static str,
    pub mode: CarryMode,
}

/// Which matte fields a piece carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PieceMatte {
    pub mode: bool,
    pub source_uid: bool,
    pub more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SplitPiece {
    pub name: String,
    pub element_stroke_id: Option<String>,
    pub frames: Vec<PieceFrame>,
    pub promote_motion: bool,
    pub promote_motion_static: bool,
    pub carry: Vec<CarriedField>,
    pub matte: PieceMatte,
    pub keeps_layer_uid: bool,
    pub kept_groups: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SplitPlan {
    pub layer_index: usize,
    pub pieces: Vec<SplitPiece>,
    pub unstamped: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SplitRefusal {
    /// The layer is a Component instance (`ld.symbolId`).
    HasSymbol,
    TooFewElements {
        count: usize,
    },
    /// The manual command is refused while a Component is open.
    ComponentOpen,
}

fn piece_frame(frame: &FrameFacts, e: usize, n: usize) -> PieceFrame {
    if !(frame.is_keyframe || frame.is_interpolated) {
        return PieceFrame {
            is_keyframe: false,
            is_interpolated: false,
            stroke_indices: Vec::new(),
        };
    }
    let len = frame.strokes.len();
    let stroke_indices = if e < n - 1 {
        if e < len {
            vec![e]
        } else {
            Vec::new()
        }
    } else {
        (n - 1..len).collect()
    };
    PieceFrame {
        is_keyframe: frame.is_keyframe,
        is_interpolated: frame.is_interpolated,
        stroke_indices,
    }
}

/// `splitLayerIntoElementsCore(li)` as a plan.
pub(crate) fn plan_split(
    layer_index: usize,
    layer: &LayerFacts,
    words: &ElementWords,
) -> Result<SplitPlan, SplitRefusal> {
    if layer.has_symbol {
        return Err(SplitRefusal::HasSymbol);
    }
    let elements = layer_elements(layer);
    let n = elements.len();
    if n < 2 {
        return Err(SplitRefusal::TooFewElements { count: n });
    }
    let base = layer
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Layer");
    let uid_state = layer.state("matteSourceLayerUid");
    let mut pieces = Vec::with_capacity(n);
    for (e, entry) in elements.iter().enumerate() {
        let last = e == n - 1;
        let frames: Vec<PieceFrame> = layer.frames.iter().map(|f| piece_frame(f, e, n)).collect();
        let motion = entry
            .stroke_id
            .as_ref()
            .and_then(|id| layer.element_motion.get(id));
        let carry = CARRY_RULES
            .iter()
            .filter(|r| rule_applies(r.when, layer.state(r.field)))
            .map(|r| CarriedField {
                field: r.field,
                mode: r.mode,
            })
            .collect();
        let mode_present = layer.state("matteMode") != FieldState::Undefined;
        let matte = if uid_state.truthy() {
            PieceMatte {
                mode: mode_present,
                source_uid: true,
                more: layer.state("mattesMore") == FieldState::Truthy,
            }
        } else {
            PieceMatte {
                mode: last && mode_present,
                ..PieceMatte::default()
            }
        };
        let kept_groups = match (&layer.groups, last) {
            (Some(groups), true) => groups_in_last_piece(layer, &frames, groups),
            _ => Vec::new(),
        };
        pieces.push(SplitPiece {
            name: format!(
                "{} \u{2014} {}",
                base,
                element_label(entry, e, layer, words)
            ),
            element_stroke_id: entry.stroke_id.clone(),
            frames,
            promote_motion: motion.map_or(false, |m| m.has_motion),
            promote_motion_static: motion.map_or(false, |m| m.has_motion_static),
            carry,
            matte,
            keeps_layer_uid: last && layer.state("layerUid").truthy(),
            kept_groups,
        });
    }
    Ok(SplitPlan {
        layer_index,
        pieces,
        unstamped: unstamped_positions(layer),
    })
}

/// The combine groups that still have a member in the last piece.
fn groups_in_last_piece(
    layer: &LayerFacts,
    last_frames: &[PieceFrame],
    groups: &[String],
) -> Vec<String> {
    let mut used: Vec<&str> = Vec::new();
    for (frame, piece) in layer.frames.iter().zip(last_frames) {
        for &i in &piece.stroke_indices {
            if let Some(gid) = present(&frame.strokes[i].group_id) {
                used.push(gid);
            }
        }
    }
    groups
        .iter()
        .filter(|g| used.contains(&g.as_str()))
        .cloned()
        .collect()
}

/// `splitLayerIntoElements(li)`: the manual command, refused while a Component is open.
pub(crate) fn plan_manual_split(
    open_context: Option<&str>,
    layer_index: usize,
    layer: &LayerFacts,
    words: &ElementWords,
) -> Result<SplitPlan, SplitRefusal> {
    if open_context.is_some() {
        return Err(SplitRefusal::ComponentOpen);
    }
    plan_split(layer_index, layer, words)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntrySkip {
    /// Only a Component holding exactly one layer is auto-split.
    NotSingleLayer { layer_count: usize },
}

/// What entering a Component plans: one undo entry when anything will split, and
/// the splits highest layer index first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntrySplitPlan {
    pub record_undo: bool,
    pub splits: Vec<SplitPlan>,
    pub skipped: Option<EntrySkip>,
}

/// The silent auto-split of `enterComponentLayer`, over the layers of the entered
/// Component with their reference strokes at the entry playhead.
pub(crate) fn plan_entry_split(layers: &[LayerFacts], words: &ElementWords) -> EntrySplitPlan {
    if layers.len() != 1 {
        return EntrySplitPlan {
            record_undo: false,
            splits: Vec::new(),
            skipped: Some(EntrySkip::NotSingleLayer {
                layer_count: layers.len(),
            }),
        };
    }
    let will_split = layers
        .iter()
        .any(|l| !l.has_symbol && layer_elements(l).len() >= 2);
    let splits = layers
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, l)| plan_split(i, l, words).ok())
        .collect();
    EntrySplitPlan {
        record_undo: will_split,
        splits,
        skipped: None,
    }
}
