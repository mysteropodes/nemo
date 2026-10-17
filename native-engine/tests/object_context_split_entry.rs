//! Entry tests: entering a Component through the real workspace kernel and planning
//! the silent auto-split (`enterComponentLayer`), the idempotence of a second entry,
//! and the rule that a flat layer is never converted by entering.
//!
//! The `H`/`G`/`G2` oracle rows run the verbatim `enterComponentLayer` of
//! `src/js/motion.js` twice over the same Component: `G` after the first entry
//! (final playhead, `pushUndo` calls, layer count and names), `G2` after leaving and
//! entering again. The playhead comes from the L04-H workspace kernel's entry frame.

use super::tests::{enc, list, with_reference, world, EntryCase};
use super::{plan_entry_split, plan_split, EntrySkip, EntrySplitPlan, SplitPlan, SplitRefusal};
use crate::evaluation::object_component_kernels::object_context_split_facts::{
    ElementWords, FieldState, FrameFacts, LayerFacts, StrokeFacts,
};
use crate::evaluation::object_component_kernels::object_context_workspace::{
    ContextInfo, WorkspaceState, WorkspaceView,
};

pub(crate) type EntryPlanner<'a> = &'a dyn Fn(&[LayerFacts], &ElementWords) -> EntrySplitPlan;

/// The layers after applying `plan`: each planned layer is replaced by its pieces
/// (stroke records moved by position, nothing created). Reference strokes are not
/// carried; a later entry recomputes them at its own playhead.
pub(crate) fn apply_plan(layer: &LayerFacts, plan: &SplitPlan) -> Vec<LayerFacts> {
    plan.pieces
        .iter()
        .map(|piece| LayerFacts {
            name: Some(piece.name.clone()),
            frames: layer
                .frames
                .iter()
                .zip(&piece.frames)
                .map(|(source, pf)| FrameFacts {
                    is_keyframe: pf.is_keyframe,
                    is_interpolated: pf.is_interpolated,
                    strokes: pf
                        .stroke_indices
                        .iter()
                        .map(|&i| source.strokes[i].clone())
                        .collect(),
                })
                .collect(),
            ..LayerFacts::default()
        })
        .collect()
}

fn apply_all(layers: &[LayerFacts], plan: &EntrySplitPlan) -> Vec<LayerFacts> {
    let mut out: Vec<LayerFacts> = layers
        .iter()
        .map(|l| LayerFacts {
            reference_strokes: Vec::new(),
            ..l.clone()
        })
        .collect();
    // Highest index first, as planned: earlier indices stay valid.
    for split in &plan.splits {
        let pieces = apply_plan(&layers[split.layer_index], split);
        out.splice(split.layer_index..split.layer_index + 1, pieces);
    }
    out
}

fn names(layers: &[LayerFacts]) -> String {
    list(
        layers
            .iter()
            .map(|l| enc(l.name.as_deref().unwrap_or("")))
            .collect(),
    )
}

fn scene_view() -> WorkspaceView {
    WorkspaceView {
        current_frame: 0,
        wa_in: 0,
        wa_out: 99,
        active_layer_idx: 0,
        total_frames: 100,
        fps: 24.0,
    }
}

/// The playhead the workspace kernel gives an entry with `resolved` as target frame.
pub(crate) fn entry_frame(case: &EntryCase) -> i64 {
    let catalog = vec![ContextInfo {
        context_id: "S".to_string(),
        total_frames: case.total,
        fps: None,
    }];
    let mut ws = WorkspaceState::new(scene_view());
    ws.enter_context(&catalog, &mut Vec::new(), "S", Some(case.resolved));
    ws.view.current_frame
}

/// The `G` and `G2` rows of an entry, with the entry plan produced by `planner`.
pub(crate) fn entry_rows(case: &EntryCase, planner: EntryPlanner<'_>) -> (String, String) {
    let words = case.specs[0].words.clone();
    let frame = entry_frame(case);
    let first: Vec<LayerFacts> = case
        .specs
        .iter()
        .map(|s| with_reference(&s.layer, frame))
        .collect();
    let plan = planner(&first, &words);
    let after = apply_all(&first, &plan);
    let g = format!(
        "G {} frame={} undo={} layers={} names={}",
        case.id,
        frame,
        plan.record_undo as u8,
        after.len(),
        names(&after)
    );
    let second: Vec<LayerFacts> = after.iter().map(|l| with_reference(l, frame)).collect();
    let plan2 = planner(&second, &words);
    let again = apply_all(&second, &plan2);
    let g2 = format!(
        "G2 {} undo={} layers={} names={}",
        case.id,
        plan2.record_undo as u8,
        again.len(),
        names(&again)
    );
    (g, g2)
}

/// `(rows compared, rows that differ)` over every entry scenario.
pub(crate) fn compare_entries(planner: EntryPlanner<'_>) -> (usize, usize) {
    let (_, entries) = world();
    let (mut total, mut wrong) = (0, 0);
    for case in &entries {
        let (g, g2) = entry_rows(case, planner);
        total += 2;
        wrong += (g != case.g) as usize + (g2 != case.g2) as usize;
    }
    (total, wrong)
}

fn s(id: &str) -> StrokeFacts {
    StrokeFacts {
        stroke_id: Some(id.to_string()),
        has_fill: true,
        ..StrokeFacts::default()
    }
}

fn layer(count: usize) -> LayerFacts {
    let strokes: Vec<StrokeFacts> = (0..count).map(|i| s(&format!("s{i}"))).collect();
    LayerFacts {
        name: Some("Hero".to_string()),
        reference_strokes: strokes.clone(),
        frames: vec![FrameFacts {
            is_keyframe: true,
            is_interpolated: false,
            strokes,
        }],
        ..LayerFacts::default()
    }
}

fn words() -> ElementWords {
    ElementWords {
        shape: "Forme".to_string(),
        stroke: "Trait".to_string(),
    }
}

#[test]
fn every_baseline_entry_row_is_reproduced() {
    let (total, wrong) = compare_entries(&plan_entry_split);
    assert_eq!(total, 86);
    assert_eq!(
        wrong, 0,
        "{wrong} of {total} entry rows differ from the baseline"
    );
    let (_, entries) = world();
    let undo = |e: &EntryCase| e.g.contains(" undo=1 ");
    // The world reaches both outcomes: entries that split (with an undo entry) and that do not.
    assert!(entries.iter().any(undo) && entries.iter().any(|e| !undo(e)));
    assert!(entries.iter().any(|e| e.specs.len() > 1));
}

#[test]
fn a_second_entry_plans_nothing() {
    let (_, entries) = world();
    let mut split_entries = 0;
    for case in &entries {
        let frame = entry_frame(case);
        let words = case.specs[0].words.clone();
        let first: Vec<LayerFacts> = case
            .specs
            .iter()
            .map(|s| with_reference(&s.layer, frame))
            .collect();
        let plan = plan_entry_split(&first, &words);
        let after: Vec<LayerFacts> = apply_all(&first, &plan)
            .iter()
            .map(|l| with_reference(l, frame))
            .collect();
        let again = plan_entry_split(&after, &words);
        assert!(
            again.splits.is_empty() && !again.record_undo,
            "{} splits again",
            case.id
        );
        if !plan.splits.is_empty() {
            split_entries += 1;
            // N pieces: the single-layer guard is what stops the second pass.
            assert_eq!(
                again.skipped,
                Some(EntrySkip::NotSingleLayer {
                    layer_count: after.len()
                }),
                "{}",
                case.id
            );
        }
    }
    assert!(split_entries > 5, "only {split_entries} entries split");
}

#[test]
fn only_the_last_piece_can_ever_hold_two_elements() {
    // Piece e < n - 1 takes at most one stroke per frame, so re-planning it refuses.
    let (cases, _) = world();
    let mut checked = 0;
    for case in &cases {
        let layer = with_reference(&case.spec.layer, case.spec.play);
        let Ok(plan) = plan_split(0, &layer, &case.spec.words) else {
            continue;
        };
        let pieces = apply_plan(&layer, &plan);
        for (e, piece) in pieces.iter().enumerate().take(pieces.len() - 1) {
            let again = plan_split(0, &with_reference(piece, case.spec.play), &case.spec.words);
            assert!(
                matches!(again, Err(SplitRefusal::TooFewElements { count }) if count <= 1),
                "{} piece {e}",
                case.spec.id
            );
            checked += 1;
        }
    }
    assert!(checked > 50);
}

#[test]
fn entry_gating_follows_the_baseline() {
    // oracle: G e1 (undo=1), G e4 (two layers), G eOne, G eTwoLayers
    let one = plan_entry_split(&[layer(1)], &words());
    assert_eq!(
        (one.record_undo, one.splits.len(), one.skipped.clone()),
        (false, 0, None)
    );

    let split = plan_entry_split(&[layer(2)], &words());
    assert!(split.record_undo && split.skipped.is_none());
    assert_eq!(split.splits.len(), 1);
    assert_eq!(split.splits[0].layer_index, 0);
    assert_eq!(split.splits[0].pieces.len(), 2);

    // Two layers, each with two shapes: left exactly as the user built them.
    let two = plan_entry_split(&[layer(2), layer(2)], &words());
    assert_eq!(
        two.skipped,
        Some(EntrySkip::NotSingleLayer { layer_count: 2 })
    );
    assert!(!two.record_undo && two.splits.is_empty());
    let none = plan_entry_split(&[], &words());
    assert_eq!(
        none.skipped,
        Some(EntrySkip::NotSingleLayer { layer_count: 0 })
    );

    // A Component instance layer is never split and records no undo.
    let mut instance = layer(3);
    instance.has_symbol = true;
    let plan = plan_entry_split(&[instance], &words());
    assert_eq!((plan.record_undo, plan.splits.len()), (false, 0));
}

#[test]
fn the_split_of_a_bundled_layer_is_final_after_one_entry() {
    // oracle: P two, G e1/G2 e1: Hero -> N layers; the same Component entered again keeps them.
    let l = layer(3);
    let plan = plan_entry_split(std::slice::from_ref(&l), &words());
    let pieces = apply_plan(&l, &plan.splits[0]);
    assert_eq!(
        pieces
            .iter()
            .map(|p| p.name.clone().unwrap())
            .collect::<Vec<_>>(),
        [
            "Hero \u{2014} Forme 1",
            "Hero \u{2014} Forme 2",
            "Hero \u{2014} Forme 3"
        ]
    );
    let again: Vec<LayerFacts> = pieces.iter().map(|p| with_reference(p, 0)).collect();
    let plan2 = plan_entry_split(&again, &words());
    assert!(plan2.splits.is_empty() && !plan2.record_undo);
    // Each piece holds exactly one element, so even a direct per-layer plan refuses.
    for piece in &again {
        assert_eq!(
            plan_split(0, piece, &words()),
            Err(SplitRefusal::TooFewElements { count: 1 })
        );
    }
}

#[test]
fn a_flat_layer_is_never_converted_by_animating_or_entering() {
    // CLAUDE.md section 8: keying a flat layer does not auto-convert it; conversion is an
    // explicit request of its own transaction (L04-M). This kernel can only read `has_symbol`.
    let mut animated = layer(1);
    for f in [
        "blendMode",
        "inPoint",
        "effects",
        "followPath",
        "expressions",
    ] {
        animated.fields.insert(f.to_string(), FieldState::Truthy);
    }
    let plan = plan_entry_split(std::slice::from_ref(&animated), &words());
    assert!(plan.splits.is_empty() && !plan.record_undo && plan.skipped.is_none());
    assert!(!animated.has_symbol);
    // No production line creates, sets or converts to a Component.
    for src in [
        include_str!("../src/object_context_split.rs"),
        include_str!("../src/object_context_split_facts.rs"),
        include_str!("../src/object_context_workspace.rs"),
    ] {
        let production = src.split("\n#[cfg(test)]").next().unwrap();
        for line in production
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
        {
            assert!(
                !line.to_lowercase().contains("convert"),
                "conversion in production code: {line}"
            );
            assert!(
                !line.contains("has_symbol ="),
                "has_symbol is written: {line}"
            );
            assert!(!line.contains("symbol_id"), "symbol_id appears: {line}");
        }
    }
}
