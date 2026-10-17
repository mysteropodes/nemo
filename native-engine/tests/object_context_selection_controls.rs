//! Negative and mutation controls of the selection and remap kernels (L04-J, F14).
//!
//! Each deviation is a plausible wrong rule replayed over the oracle rows the VERBATIM baseline
//! produced (`H` hit-test rows, `S`/`M` import rows). The honest kernel misses none; every
//! deviation misses at least the number of rows stated next to it, so no control can pass
//! vacuously.
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::{rows, world};

/// Rows `[H, layer, symbol, visible, locked, hit]` the rule gets wrong.
fn hit_misses(rule: impl Fn(bool, bool, bool) -> bool) -> usize {
    rows("H")
        .iter()
        .filter(|row| {
            let (is_instance, visible, locked) = (row[2] != "-", row[3] == "1", row[4] == "1");
            rule(is_instance, visible, locked) != (row[5] == "1")
        })
        .count()
}

#[test]
fn the_honest_hit_rule_misses_no_baseline_row() {
    assert_eq!(
        hit_misses(|is_instance, visible, _| is_instance && visible),
        0
    );
    let w = world("D");
    let source = w.source();
    for row in rows("H") {
        assert_eq!(
            instance_selectable(&source, row[1]),
            row[5] == "1",
            "{row:?}"
        );
    }
}

#[test]
fn letting_a_lock_block_the_hit_misses_every_locked_instance() {
    // All 7 fixture instances and the clone's instance are locked and still hit by the baseline.
    let misses = hit_misses(|is_instance, visible, locked| is_instance && visible && !locked);
    assert_eq!(misses, 8);
}

#[test]
fn ignoring_visibility_hits_the_hidden_instance() {
    let misses = hit_misses(|is_instance, _, _| is_instance);
    assert_eq!(misses, 1);
    let w = world("D");
    assert_eq!(rows("H").iter().filter(|row| row[3] == "0").count(), 1);
    assert!(!instance_selectable(&w.source(), "ly_comp_hidden"));
}

#[test]
fn hit_testing_every_layer_as_a_component_would_select_plain_layers_as_instances() {
    let misses = hit_misses(|_, visible, _| visible);
    assert_eq!(misses, 2, "the two visible plain layers");
}

#[test]
fn only_the_active_context_guard_refuses_a_stale_selection() {
    let w = world("D");
    let source = w.source();
    let stale = SelectionTarget {
        context_id: "sym_comp".to_string(),
        layer_uid: "ly_comp_ia".to_string(),
        stroke_id: Some("s_comp_a_1".to_string()),
    };
    // The same target resolves when the context really is active; the guard is the difference.
    assert!(resolve_selection(&source, Some("sym_comp"), &stale).is_ok());
    assert!(matches!(
        resolve_selection(&source, None, &stale),
        Err(SelectionError::NotInActiveContext { .. })
    ));
    assert!(matches!(
        resolve_selection(&source, Some("sym_comp_b"), &stale),
        Err(SelectionError::NotInActiveContext { .. })
    ));
}

/// The wrong way to apply a swap: one entry after the other on the running id.
fn sequential(entries: &[ContextRemap], id: &str) -> String {
    let mut current = id.to_string();
    for entry in entries {
        if entry.old_id == current {
            current = entry.new_id.clone();
        }
    }
    current
}

#[test]
fn applying_a_swap_entry_by_entry_would_undo_it() {
    let entries = vec![
        ContextRemap {
            old_id: "sym_comp".to_string(),
            new_id: "sym_comp_b".to_string(),
        },
        ContextRemap {
            old_id: "sym_comp_b".to_string(),
            new_id: "sym_comp".to_string(),
        },
    ];
    let index = RemapIndex::new(&entries, &["sym_comp", "sym_comp_b"]).unwrap();
    assert_eq!(index.new_id("sym_comp"), "sym_comp_b");
    assert_eq!(sequential(&entries, "sym_comp"), "sym_comp");
    assert_eq!(index.new_id("sym_comp_b"), "sym_comp");
    assert_eq!(sequential(&entries, "sym_comp_b"), "sym_comp_b");
}

#[test]
fn rewriting_layer_ids_with_the_context_breaks_every_imported_row() {
    let new = rows("M")[0][2];
    let w = world("D2");
    let source = w.source();
    let copy = source.context(new).unwrap();
    let (mut honest_misses, mut deviant_misses, mut total) = (0, 0, 0);
    for row in rows("S").iter().filter(|row| row[1] == "sym_comp") {
        let frame: u32 = row[3].parse().unwrap();
        total += 1;
        // Honest: the context changes, the layer uid and the stroke id are the baseline's copies.
        let honest = ElementIdentity::new(new, row[2], row[4]).unwrap();
        if copy.element_at(&honest, frame).is_err() {
            honest_misses += 1;
        }
        // Deviant: the layer uid is rewritten along with the context.
        let prefixed = format!("{new}:{}", row[2]);
        let deviant = ElementIdentity::new(new, &prefixed, row[4]).unwrap();
        if copy.element_at(&deviant, frame).is_err() {
            deviant_misses += 1;
        }
    }
    assert_eq!((honest_misses, deviant_misses, total), (0, 13, 13));
}

#[test]
fn dropping_the_table_leaves_every_old_identity_unresolved_after_the_old_context_is_gone() {
    let new = rows("M")[0][2];
    let mut w = world("D2");
    w.set
        .components
        .retain(|component| component.symbol_id != "sym_comp");
    let source = w.source();
    let index = RemapIndex::new(
        &[ContextRemap {
            old_id: "sym_comp".to_string(),
            new_id: new.to_string(),
        }],
        &["sym_comp", "sym_comp_b"],
    )
    .unwrap();
    let (mut without, mut with, mut total) = (0, 0, 0);
    for row in rows("S").iter().filter(|row| row[1] == "sym_comp") {
        let frame: u32 = row[3].parse().unwrap();
        let old = ElementIdentity::new("sym_comp", row[2], row[4]).unwrap();
        total += 1;
        let direct = source
            .context(old.context_id())
            .and_then(|c| c.element_at(&old, frame));
        if direct.is_err() {
            without += 1;
        }
        if index.resolve_old(&source, &old, frame).is_ok() {
            with += 1;
        }
    }
    assert_eq!((without, with, total), (13, 13, 13));
}
