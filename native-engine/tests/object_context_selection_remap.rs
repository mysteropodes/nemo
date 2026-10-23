//! Context id remap over stable identities (L04-J, F14).
//!
//! `M`/`MI` rows are the id map of the VERBATIM baseline `importForeignSymbolTree`; `D2` is the
//! world after it ran (the imported component is `state.symbols` read back and written in the
//! native form). The baseline mints one new symbol id and copies the inner layers verbatim, so a
//! remap rewrites CONTEXT ids and never layer uids or stroke ids; everything else (tables,
//! simultaneous swaps, refusals) is the contract of "stable identities, no partial change",
//! hand-typed below.
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::{
    fill_hex, rows, world,
};
use crate::evaluation::object_component_kernels::object_context_workspace::{
    EnterOutcome, WorkspaceState, WorkspaceView,
};
use serde_json::json;

fn table(pairs: &[(&str, &str)]) -> Vec<ContextRemap> {
    pairs
        .iter()
        .map(|(old, new)| ContextRemap {
            old_id: old.to_string(),
            new_id: new.to_string(),
        })
        .collect()
}

fn imported() -> &'static str {
    rows("M")[0][2]
}

fn element(context: &str, layer: &str, stroke: &str) -> ElementIdentity {
    ElementIdentity::new(context, layer, stroke).unwrap()
}

#[test]
fn the_baseline_import_mints_one_context_and_keeps_every_inner_id() {
    let new = imported();
    assert_eq!(new, "sym_1788000123456_5000");
    assert_eq!(rows("MI"), vec![vec!["MI", "same", "null"]]);
    let w = world("D2");
    let source = w.source();
    assert_eq!(
        source.declared_contexts(),
        vec!["scene-root", "sym_comp", "sym_comp_b", new]
    );
    let (old, copy) = (
        source.context("sym_comp").unwrap(),
        source.context(new).unwrap(),
    );
    assert_eq!(copy.layers().len(), old.layers().len());
    assert_eq!(copy.objects().len(), 13);
    let mut compared = 0;
    for row in rows("S").iter().filter(|row| row[1] == "sym_comp") {
        let frame: u32 = row[3].parse().unwrap();
        let ra = old
            .element_at(&element("sym_comp", row[2], row[4]), frame)
            .unwrap();
        let rb = copy
            .element_at(&element(new, row[2], row[4]), frame)
            .unwrap();
        assert_eq!(rb.layer.layer_uid(), ra.layer.layer_uid());
        assert_eq!(fill_hex(rb.record), row[5]);
        // The records differ in their context id and nothing else.
        let mut original = serde_json::to_value(ra.record).unwrap();
        original["target"]["contextId"] = json!(new);
        assert_eq!(original, serde_json::to_value(rb.record).unwrap());
        assert!(!std::ptr::eq(ra.record, rb.record));
        compared += 1;
    }
    assert_eq!(compared, 13);
}

#[test]
fn old_identities_resolve_through_the_table_once_the_old_context_is_gone() {
    let new = imported();
    let mut w = world("D2");
    w.set
        .components
        .retain(|component| component.symbol_id != "sym_comp");
    let source = w.source();
    let index = RemapIndex::new(&table(&[("sym_comp", new)]), &["sym_comp", "sym_comp_b"]).unwrap();
    let mut resolved = 0;
    for row in rows("S").iter().filter(|row| row[1] == "sym_comp") {
        let frame: u32 = row[3].parse().unwrap();
        let old = element("sym_comp", row[2], row[4]);
        // Without the table the old id names nothing any more.
        assert_eq!(
            source.context(old.context_id()).err(),
            Some(ResolveError::UnknownContext)
        );
        let via_table = index.resolve_old(&source, &old, frame).unwrap();
        assert_eq!(via_table.record.target.context_id, new);
        assert_eq!(via_table.record.target.layer_uid, row[2]);
        assert_eq!(via_table.record.target.stroke_id, row[4]);
        assert_eq!(fill_hex(via_table.record), row[5]);
        // A new id is the table's value and resolves directly, to the very same record.
        let direct = source
            .context(new)
            .unwrap()
            .element_at(&index.remap_identity(&old), frame)
            .unwrap();
        assert!(std::ptr::eq(via_table.record, direct.record));
        resolved += 1;
    }
    assert_eq!(resolved, 13);
    // An id that is not in the table is preserved and resolves where it always did.
    let kept = element("sym_comp_b", "ly_comp_ia", "s_comp_a_1");
    assert_eq!(index.remap_identity(&kept), kept);
    assert_eq!(
        index
            .resolve_old(&source, &kept, 1)
            .unwrap()
            .record
            .target
            .context_id,
        "sym_comp_b"
    );
}

#[test]
fn the_table_wins_over_an_old_context_that_still_exists() {
    let new = imported();
    let w = world("D2");
    let source = w.source();
    let index = RemapIndex::new(&table(&[("sym_comp", new)]), &["sym_comp", "sym_comp_b"]).unwrap();
    let old = element("sym_comp", "ly_comp_ia", "s_comp_a_1");
    let direct = source
        .context("sym_comp")
        .unwrap()
        .element_at(&old, 1)
        .unwrap();
    let mapped = index.resolve_old(&source, &old, 1).unwrap();
    assert_eq!(direct.record.target.context_id, "sym_comp");
    assert_eq!(mapped.record.target.context_id, new);
    assert!(!std::ptr::eq(direct.record, mapped.record));
}

#[test]
fn a_remap_rewrites_the_context_and_nothing_else() {
    let index = RemapIndex::new(
        &table(&[("sym_comp", "sym_new")]),
        &["sym_comp", "sym_comp_b"],
    )
    .unwrap();
    let selected = SelectionTarget {
        context_id: "sym_comp".to_string(),
        layer_uid: "ly_comp_ia".to_string(),
        stroke_id: Some("s_comp_a_1".to_string()),
    };
    assert_eq!(
        index.remap_selection(&selected),
        SelectionTarget {
            context_id: "sym_new".to_string(),
            ..selected.clone()
        }
    );
    let layer_level = SelectionTarget {
        stroke_id: None,
        ..selected.clone()
    };
    assert_eq!(index.remap_selection(&layer_level).stroke_id, None);
    let other = SelectionTarget {
        context_id: "sym_comp_b".to_string(),
        ..selected.clone()
    };
    assert_eq!(index.remap_selection(&other), other);
    let scene = SelectionTarget {
        context_id: "scene-root".to_string(),
        ..selected
    };
    assert_eq!(index.remap_selection(&scene), scene);
    assert_eq!(index.new_id("sym_comp"), "sym_new");
    assert_eq!(index.new_id("sym_comp_b"), "sym_comp_b");
    assert_eq!(index.new_id("scene-root"), "scene-root");
    let id = element("sym_comp", "ly_comp_ia", "s_comp_a_1");
    let moved = index.remap_identity(&id);
    assert_eq!(
        (moved.context_id(), moved.layer_uid(), moved.stroke_id()),
        ("sym_new", "ly_comp_ia", "s_comp_a_1")
    );
}

#[test]
fn a_swap_is_simultaneous_and_applying_it_twice_restores_every_identity() {
    let index = RemapIndex::new(
        &table(&[("sym_comp", "sym_comp_b"), ("sym_comp_b", "sym_comp")]),
        &["sym_comp", "sym_comp_b"],
    )
    .unwrap();
    assert_eq!(index.new_id("sym_comp"), "sym_comp_b");
    assert_eq!(index.new_id("sym_comp_b"), "sym_comp");
    for context in ["sym_comp", "sym_comp_b"] {
        let id = element(context, "ly_comp_ia", "s_comp_a_3");
        let once = index.remap_identity(&id);
        assert_ne!(once, id);
        assert_eq!(index.remap_identity(&once), id);
    }
    // A chain is not followed transitively: A -> B and B -> C move each id exactly once.
    let chain = RemapIndex::new(&table(&[("a", "b"), ("b", "c")]), &["a", "b"]).unwrap();
    assert_eq!(
        (chain.new_id("a"), chain.new_id("b"), chain.new_id("c")),
        ("b", "c", "c")
    );
}

#[test]
fn invalid_tables_are_refused_whole() {
    use RemapRefusal::*;
    let live = ["a", "b"];
    let conflicting = |old: &str| ConflictingMapping {
        old_id: old.to_string(),
    };
    let collision = |new: &str| Collision {
        new_id: new.to_string(),
    };
    let cases: [(Vec<ContextRemap>, Result<(), RemapRefusal>); 10] = [
        (table(&[("", "x")]), Err(EmptyId)),
        (table(&[("x", "")]), Err(EmptyId)),
        (table(&[("scene-root", "x")]), Err(SceneRoot)),
        (table(&[("x", "scene-root")]), Err(SceneRoot)),
        (table(&[("a", "x"), ("a", "y")]), Err(conflicting("a"))),
        (table(&[("a", "x"), ("a", "x")]), Ok(())),
        (table(&[("a", "z"), ("b", "z")]), Err(collision("z"))),
        // Mapping onto a live context that is not itself renamed would merge two contexts.
        (table(&[("a", "b")]), Err(collision("b"))),
        (table(&[]), Ok(())),
        (table(&[("c", "d")]), Ok(())),
    ];
    for (entries, expected) in cases {
        assert_eq!(
            RemapIndex::new(&entries, &live).map(|_| ()),
            expected,
            "{entries:?}"
        );
    }
    // A table naming a context that is not live is still a table: the id simply never matches.
    let index = RemapIndex::new(&table(&[("c", "d")]), &live).unwrap();
    assert_eq!(index.new_id("a"), "a");
    assert_eq!(index.new_id("c"), "d");
}

#[test]
fn the_workspace_and_the_selection_remap_agree_on_the_new_context_id() {
    let w = world("D");
    let source = w.source();
    let catalog = workspace_catalog(&source);
    let mut ws = WorkspaceState::new(WorkspaceView {
        current_frame: 0,
        wa_in: 0,
        wa_out: 23,
        active_layer_idx: 0,
        total_frames: 24,
        fps: 24.0,
    });
    let mut live = Vec::new();
    assert_eq!(
        ws.enter_context(&catalog, &mut live, "sym_comp", None),
        EnterOutcome::Entered
    );
    let pairs = table(&[("sym_comp", "sym_new")]);
    let index = RemapIndex::new(&pairs, &["sym_comp", "sym_comp_b"]).unwrap();
    let report = ws.remap_contexts(&pairs).unwrap();
    assert!(report.active_changed);
    assert_eq!(report.tabs_changed, 1);
    assert_eq!(ws.active_context_id(), Some(index.new_id("sym_comp")));
    assert_eq!(ws.open_tabs, vec!["sym_new".to_string()]);
    // The identity a selection kept before the remap follows the same id.
    let before = element("sym_comp", "ly_comp_ia", "s_comp_a_0");
    assert_eq!(
        index.remap_identity(&before).context_id(),
        ws.active_context_id().unwrap()
    );
}
