//! Structure, identity and remap tests for the workspace kernel: the workspace holds
//! no document content, context ids are stable through enter -> exit -> enter, and
//! the old -> new remap table rewrites open tabs and the active context only.

use super::pinned_tests::{catalog, ids, info, scene};
use super::{ActiveContext, ContextRemap, RemapError, RemapReport, WorkspaceState, WorkspaceView};

#[test]
fn the_workspace_holds_no_document_content() {
    // Exhaustive destructuring: adding a layers/strokes/keys field to any of these
    // structs stops this test compiling. Every field is an id, a tab list or a scalar.
    let mut ws = WorkspaceState::new(scene());
    ws.enter_context(&catalog(), &mut Vec::new(), "s_a", None);
    let WorkspaceState {
        active,
        open_tabs,
        view,
    } = ws;
    let ActiveContext {
        context_id,
        saved_scene_view,
    } = active.expect("active");
    let (id, tabs): (String, Vec<String>) = (context_id, open_tabs);
    let WorkspaceView {
        current_frame,
        wa_in,
        wa_out,
        active_layer_idx,
        total_frames,
        fps,
    } = view;
    let _: (i64, i64, i64, i64, i64, f64) = (
        current_frame,
        wa_in,
        wa_out,
        active_layer_idx,
        total_frames,
        fps,
    );
    let WorkspaceView {
        current_frame,
        wa_in,
        wa_out,
        active_layer_idx,
        total_frames,
        fps,
    } = saved_scene_view;
    let _: (i64, i64, i64, i64, i64, f64) = (
        current_frame,
        wa_in,
        wa_out,
        active_layer_idx,
        total_frames,
        fps,
    );
    assert_eq!((id.as_str(), tabs.len()), ("s_a", 1));
}

#[test]
fn context_ids_are_stable_through_enter_exit_enter() {
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for id in ["s_a", "s_b", "s_a", "s_c", "s_a"] {
        ws.enter_context(&cat, &mut sel, id, None);
        assert_eq!(ws.active_context_id(), Some(id));
        seen.push(ws.active_context_id().unwrap().to_string());
        ws.exit_context(&cat, &mut sel);
    }
    assert_eq!(seen, ids(&["s_a", "s_b", "s_a", "s_c", "s_a"]));
    // The tab list holds exactly the contexts the caller named, in first-entry order.
    assert_eq!(ws.open_tabs, ids(&["s_a", "s_b", "s_c"]));
}

fn remap(pairs: &[(&str, &str)]) -> Vec<ContextRemap> {
    pairs
        .iter()
        .map(|(o, n)| ContextRemap {
            old_id: o.to_string(),
            new_id: n.to_string(),
        })
        .collect()
}

#[test]
fn remap_rewrites_tabs_and_the_active_context_only() {
    let cat = vec![
        info("a", 24, Some(12.0)),
        info("b", 10, None),
        info("c", 8, None),
    ];
    let mut ws = WorkspaceState::with_open_tabs(scene(), ids(&["a", "b", "c"])).unwrap();
    ws.enter_context(&cat, &mut Vec::new(), "b", None);
    let live = ws.view.clone();
    let saved = ws.saved_scene_view().cloned();
    let report = ws
        .remap_contexts(&remap(&[("b", "b2"), ("a", "a2"), ("zzz", "unused")]))
        .unwrap();
    assert_eq!(
        report,
        RemapReport {
            tabs_changed: 2,
            active_changed: true
        }
    );
    assert_eq!(ws.open_tabs, ids(&["a2", "b2", "c"]));
    assert_eq!(ws.active_context_id(), Some("b2"));
    // The views carry no context id: untouched. The addressing follows the new id.
    assert_eq!(
        (ws.view.clone(), ws.saved_scene_view().cloned()),
        (live, saved)
    );
    assert_eq!(ws.frame_of("b2"), Some(0));
    assert_eq!(ws.frame_of("b"), None);
    assert_eq!(ws.validate(), Ok(()));
}

#[test]
fn remap_with_an_empty_table_or_no_matching_id_changes_nothing() {
    let mut ws = WorkspaceState::with_open_tabs(scene(), ids(&["a", "b"])).unwrap();
    let before = ws.clone();
    assert_eq!(
        ws.remap_contexts(&[]).unwrap(),
        RemapReport {
            tabs_changed: 0,
            active_changed: false
        }
    );
    assert_eq!(
        ws.remap_contexts(&remap(&[("q", "r"), ("a", "a")]))
            .unwrap(),
        RemapReport {
            tabs_changed: 0,
            active_changed: false
        }
    );
    assert_eq!(ws, before);
}

#[test]
fn a_swap_is_simultaneous_and_a_merge_is_refused_without_partial_change() {
    let mut ws = WorkspaceState::with_open_tabs(scene(), ids(&["a", "b", "c"])).unwrap();
    ws.remap_contexts(&remap(&[("a", "b"), ("b", "a")]))
        .unwrap();
    assert_eq!(ws.open_tabs, ids(&["b", "a", "c"]));
    let before = ws.clone();
    // a -> c would merge two live contexts; so would mapping onto an unmapped live id.
    assert_eq!(
        ws.remap_contexts(&remap(&[("a", "c")])),
        Err(RemapError::Collision {
            new_id: "c".to_string()
        })
    );
    assert_eq!(
        ws.remap_contexts(&remap(&[("a", "x"), ("b", "x")])),
        Err(RemapError::Collision {
            new_id: "x".to_string()
        })
    );
    assert_eq!(ws, before);
}

#[test]
fn malformed_remap_tables_are_refused() {
    let mut ws = WorkspaceState::with_open_tabs(scene(), ids(&["a"])).unwrap();
    let before = ws.clone();
    assert_eq!(
        ws.remap_contexts(&remap(&[("a", "")])),
        Err(RemapError::EmptyId)
    );
    assert_eq!(
        ws.remap_contexts(&remap(&[("", "b")])),
        Err(RemapError::EmptyId)
    );
    assert_eq!(
        ws.remap_contexts(&remap(&[("a", "b"), ("a", "c")])),
        Err(RemapError::ConflictingMapping {
            old_id: "a".to_string()
        })
    );
    // The same pair twice is not a conflict.
    assert!(ws.remap_contexts(&remap(&[("a", "b"), ("a", "b")])).is_ok());
    assert_ne!(ws, before);
    assert_eq!(ws.open_tabs, ids(&["b"]));
}
