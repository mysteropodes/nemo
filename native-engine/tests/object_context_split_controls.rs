//! Mutation controls for the element-splitting plan: each named deviation from the
//! baseline must be caught by the oracle rows.
//!
//! The fixture header carries, per deviation, the number of oracle result rows that
//! the equivalent edit of the baseline JavaScript changes (`# mutant <name> <rows>`,
//! measured by the generator over the layer rows AND the entry rows). Each deviation
//! here edits the layer before planning or the plan after it, on both the layer path
//! and the entry path, and replays the same oracle; it must miss at least that many
//! rows. The undeviated run misses none.

use super::entry_tests::compare_entries;
use super::tests::{compare_layers, real, Planned, Tweak, ORACLE};
use super::{plan_entry_split, plan_split, EntrySplitPlan, PieceMatte, SplitPlan};
use crate::evaluation::object_component_kernels::object_context_split_facts::{
    ElementWords, FieldState, LayerFacts,
};

fn measured(name: &str) -> usize {
    let key = format!("# mutant {name} ");
    ORACLE
        .lines()
        .find_map(|l| l.strip_prefix(key.as_str()))
        .unwrap_or_else(|| panic!("no measurement for {name}"))
        .parse()
        .unwrap()
}

type Prepare = fn(&LayerFacts) -> LayerFacts;
type Edit = fn(&mut SplitPlan, &LayerFacts);

fn keep(l: &LayerFacts) -> LayerFacts {
    l.clone()
}

fn state(l: &LayerFacts, field: &str) -> FieldState {
    l.fields
        .get(field)
        .copied()
        .unwrap_or(FieldState::Undefined)
}

/// Runs a deviation through the layer rows and the entry rows and checks the
/// number of rows it misses against the generator's measurement.
fn caught(name: &str, prepare: Prepare, edit: Edit) {
    let tweak = Tweak {
        prepare: Box::new(prepare),
        post: Box::new(move |r: Planned, layer: &LayerFacts| {
            r.map(|mut plan| {
                edit(&mut plan, layer);
                plan
            })
        }),
    };
    let planner = move |layers: &[LayerFacts], words: &ElementWords| -> EntrySplitPlan {
        let prepared: Vec<LayerFacts> = layers.iter().map(prepare).collect();
        let mut plan = plan_entry_split(&prepared, words);
        for split in &mut plan.splits {
            let index = split.layer_index;
            edit(split, &prepared[index]);
        }
        plan
    };
    let (_, wrong_layers) = compare_layers(&tweak);
    let (_, wrong_entries) = compare_entries(&planner);
    assert_missed(name, wrong_layers + wrong_entries);
}

fn assert_missed(name: &str, wrong: usize) {
    let want = measured(name);
    assert!(want > 0, "{name}: the generator measured no changed row");
    assert!(
        wrong >= want,
        "{name}: only {wrong} rows differ, the baseline mutant changes {want}"
    );
}

fn no_edit(_: &mut SplitPlan, _: &LayerFacts) {}

#[test]
fn the_undeviated_run_misses_no_row() {
    assert_eq!(compare_layers(&real()).1, 0);
    assert_eq!(compare_entries(&plan_entry_split).1, 0);
}

#[test]
fn a_hyphen_instead_of_the_em_dash_is_caught() {
    caught("hyphenName", keep, |p, _| {
        for piece in &mut p.pieces {
            piece.name = piece.name.replace('\u{2014}', "-");
        }
    });
}

#[test]
fn the_last_piece_losing_its_first_stroke_is_caught() {
    caught("overflowDropped", keep, |p, _| {
        let n = p.pieces.len();
        for f in &mut p.pieces[n - 1].frames {
            f.stroke_indices.retain(|&i| i != n - 1);
        }
    });
}

#[test]
fn element_motion_that_is_not_promoted_is_caught() {
    caught("noMotionPromotion", keep, |p, _| {
        for piece in &mut p.pieces {
            piece.promote_motion = false;
            piece.promote_motion_static = false;
        }
    });
}

#[test]
fn a_frozen_matte_kept_on_the_last_piece_only_is_caught() {
    caught("matteOnLastOnly", keep, |p, layer| {
        let n = p.pieces.len();
        let has_mode = state(layer, "matteMode") != FieldState::Undefined;
        for (e, piece) in p.pieces.iter_mut().enumerate() {
            piece.matte = PieceMatte {
                mode: e == n - 1 && has_mode,
                source_uid: false,
                more: false,
            };
        }
    });
}

#[test]
fn combine_groups_kept_without_the_member_test_are_caught() {
    caught("allGroupsKept", keep, |p, layer| {
        let n = p.pieces.len();
        p.pieces[n - 1].kept_groups = layer.groups.clone().unwrap_or_default();
    });
}

#[test]
fn a_dropped_shy_flag_is_caught() {
    caught("dropShy", keep, |p, _| {
        for piece in &mut p.pieces {
            piece.carry.retain(|c| c.field != "shy");
        }
    });
}

#[test]
fn a_texture_copy_counted_as_an_element_is_caught() {
    caught(
        "textureCopyIsElement",
        |l| {
            let mut l = l.clone();
            for s in &mut l.reference_strokes {
                s.is_brush_texture_copy = false;
            }
            l
        },
        no_edit,
    );
}

#[test]
fn splitting_a_component_instance_layer_is_caught() {
    caught(
        "splitSymbolLayer",
        |l| LayerFacts {
            has_symbol: false,
            ..l.clone()
        },
        no_edit,
    );
}

#[test]
fn a_layer_uid_given_to_a_falsy_but_defined_value_is_caught() {
    caught("layerUidOnAll", keep, |p, layer| {
        let n = p.pieces.len();
        p.pieces[n - 1].keeps_layer_uid = state(layer, "layerUid") != FieldState::Undefined;
    });
}

/// The entry plan without the single-layer guard: every layer is planned.
fn entry_without_guard(layers: &[LayerFacts], words: &ElementWords) -> EntrySplitPlan {
    let splits: Vec<SplitPlan> = layers
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, l)| plan_split(i, l, words).ok())
        .collect();
    EntrySplitPlan {
        record_undo: !splits.is_empty(),
        splits,
        skipped: None,
    }
}

#[test]
fn an_entry_plan_without_the_single_layer_guard_is_not_idempotent() {
    let (_, wrong) = compare_entries(&entry_without_guard);
    assert_missed("noSingleLayerGuard", wrong);
}
