//! Selection and hit addressing over the nested-context resolver (L04-J, F14).
//!
//! `H` rows are the results of the VERBATIM baseline `hitTestComponentLayers` run over a
//! one-layer state per scene layer (a component instance is hit exactly when it is an instance
//! layer and visible; `locked` never matters). Everything else is hand-typed from the baseline
//! fixture ids (`sym_comp`, `ly_comp_ia`, `s_comp_a_1`, ...).
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::{rows, world};
use crate::evaluation::object_component_kernels::object_context_workspace::{
    EnterOutcome, WorkspaceState, WorkspaceView,
};
use crate::object_document::Presence;

fn selection(context: &str, layer: &str, stroke: Option<&str>) -> SelectionTarget {
    SelectionTarget {
        context_id: context.to_string(),
        layer_uid: layer.to_string(),
        stroke_id: stroke.map(str::to_string),
    }
}

fn instance(layer: &str, symbol: &str) -> SelectionRef {
    SelectionRef::Instance(InstanceRef {
        layer_uid: layer.to_string(),
        symbol_id: symbol.to_string(),
    })
}

fn element(context: &str, layer: &str, stroke: &str) -> ElementIdentity {
    ElementIdentity::new(context, layer, stroke).unwrap()
}

fn view(fps: f64) -> WorkspaceView {
    WorkspaceView {
        current_frame: 0,
        wa_in: 0,
        wa_out: 23,
        active_layer_idx: 0,
        total_frames: 24,
        fps,
    }
}

#[test]
fn hit_rows_match_the_baseline_component_hit_test() {
    let w = world("D");
    let source = w.source();
    let mut hits = 0;
    for row in rows("H") {
        let (layer, symbol, visible, hit) = (row[1], row[2], row[3] == "1", row[5] == "1");
        assert_eq!(instance_selectable(&source, layer), hit, "{row:?}");
        let address = address_hit(&source, None, layer, None);
        if hit {
            hits += 1;
            assert_eq!(
                address,
                Ok(HitAddress {
                    selection: instance(layer, symbol),
                    inner: None,
                    enter: Some(symbol.to_string()),
                })
            );
        } else if symbol != "-" {
            // A hidden instance is not hit, and the refusal is typed.
            assert!(!visible);
            assert_eq!(address, Err(SelectionError::HiddenInstance));
        } else {
            let plain = SelectionRef::Layer {
                context_id: "scene-root".to_string(),
                layer_uid: layer.to_string(),
            };
            assert_eq!(
                address,
                Ok(HitAddress {
                    selection: plain,
                    inner: None,
                    enter: None
                })
            );
        }
    }
    assert_eq!(hits, 8);
}

#[test]
fn locked_never_blocks_an_instance_hit() {
    let h = rows("H");
    let locked_hits = h.iter().filter(|r| r[4] == "1" && r[5] == "1").count();
    assert_eq!(
        locked_hits, 8,
        "every baseline instance layer is locked and still hit"
    );
    let w = world("D");
    let source = w.source();
    assert!(instance_selectable(&source, "ly_comp_loop"));
    assert!(!instance_selectable(&source, "ly_plain_locked"));
    assert!(!instance_selectable(&source, "ly_comp_hidden"));
    assert!(!instance_selectable(&source, "ly_missing"));
    assert!(!instance_selectable(&source, ""));
    // An inner layer id is not an instance layer of the scene.
    assert!(!instance_selectable(&source, "ly_comp_ia"));
}

#[test]
fn a_click_on_an_instance_selects_the_instance_and_names_the_inner_element() {
    let w = world("D");
    let source = w.source();
    let a = address_hit(&source, None, "ly_comp_loop", Some("s_comp_a_1")).unwrap();
    assert_eq!(a.selection, instance("ly_comp_loop", "sym_comp"));
    assert_eq!(
        a.inner,
        Some(element("sym_comp", "ly_comp_ia", "s_comp_a_1"))
    );
    assert_eq!(a.enter.as_deref(), Some("sym_comp"));
    // The look-alike stroke under a click on the clone's instance is the clone's element.
    let b = address_hit(&source, None, "ly_compb_loop", Some("s_comp_a_1")).unwrap();
    assert_eq!(b.selection, instance("ly_compb_loop", "sym_comp_b"));
    assert_eq!(
        b.inner,
        Some(element("sym_comp_b", "ly_comp_ia", "s_comp_a_1"))
    );
    assert_ne!(a.inner, b.inner);
    // A stroke the component does not have: the instance is still what is selected.
    let c = address_hit(&source, None, "ly_comp_loop", Some("Shape 1")).unwrap();
    assert_eq!(c.selection, instance("ly_comp_loop", "sym_comp"));
    assert_eq!(c.inner, None);
    assert_eq!(c.enter.as_deref(), Some("sym_comp"));
}

#[test]
fn a_double_click_enters_and_the_next_click_addresses_the_inner_stroke() {
    let w = world("D");
    let source = w.source();
    let first = address_hit(&source, None, "ly_comp_loop", Some("s_comp_a_1")).unwrap();
    let mut ws = WorkspaceState::new(view(24.0));
    let catalog = workspace_catalog(&source);
    let mut live = Vec::new();
    let entered = ws.enter_context(&catalog, &mut live, first.enter.as_deref().unwrap(), None);
    assert_eq!(entered, EnterOutcome::Entered);
    let inside = address_hit(
        &source,
        ws.active_context_id(),
        "ly_comp_ia",
        Some("s_comp_a_1"),
    )
    .unwrap();
    // The selection addresses the component's inner stroke, not the outer instance layer.
    assert_eq!(
        inside.selection,
        SelectionRef::Element(first.inner.clone().unwrap())
    );
    assert_eq!((inside.inner, inside.enter), (None, None));
    // Inside, the scene's layers (instances included) are not layers of the context.
    assert_eq!(
        address_hit(&source, ws.active_context_id(), "ly_comp_loop", None),
        Err(SelectionError::Resolve(ResolveError::UnknownLayer))
    );
    let layer_only = address_hit(&source, ws.active_context_id(), "ly_comp_ib", None).unwrap();
    assert_eq!(
        layer_only.selection,
        SelectionRef::Layer {
            context_id: "sym_comp".to_string(),
            layer_uid: "ly_comp_ib".to_string()
        }
    );
}

#[test]
fn a_stored_selection_resolves_only_in_the_active_context() {
    let w = world("D");
    let source = w.source();
    let stale = |selected: &str, active: &str| {
        Err(SelectionError::NotInActiveContext {
            selected: selected.to_string(),
            active: active.to_string(),
        })
    };
    let inner = selection("sym_comp", "ly_comp_ia", Some("s_comp_a_1"));
    assert_eq!(
        resolve_selection(&source, Some("sym_comp"), &inner),
        Ok(SelectionRef::Element(element(
            "sym_comp",
            "ly_comp_ia",
            "s_comp_a_1"
        )))
    );
    assert_eq!(
        resolve_selection(&source, None, &inner),
        stale("sym_comp", "scene-root")
    );
    assert_eq!(
        resolve_selection(&source, Some("sym_comp_b"), &inner),
        stale("sym_comp", "sym_comp_b")
    );
    assert_eq!(
        resolve_selection(&source, None, &selection("scene-root", "ly_plain", None)),
        Ok(SelectionRef::Layer {
            context_id: "scene-root".to_string(),
            layer_uid: "ly_plain".to_string()
        })
    );
    assert_eq!(
        resolve_selection(
            &source,
            None,
            &selection("scene-root", "ly_comp_loop", None)
        ),
        Ok(instance("ly_comp_loop", "sym_comp"))
    );
    assert_eq!(
        resolve_selection(
            &source,
            None,
            &selection("scene-root", "ly_comp_loop", Some("s_comp_a_1"))
        ),
        Err(SelectionError::InstanceStrokeNotAddressable {
            symbol_id: "sym_comp".to_string()
        })
    );
    let refusals = [
        (
            selection("sym_x", "ly_comp_ia", None),
            None,
            ResolveError::UnknownContext,
        ),
        (
            selection("", "ly_comp_ia", None),
            Some("sym_comp"),
            ResolveError::EmptyId,
        ),
        (
            selection("sym_comp", "", None),
            Some("sym_comp"),
            ResolveError::EmptyId,
        ),
        (
            selection("sym_comp", "ly_missing", None),
            Some("sym_comp"),
            ResolveError::UnknownLayer,
        ),
        (
            selection("sym_comp", "ly_comp_ia", Some("")),
            Some("sym_comp"),
            ResolveError::EmptyId,
        ),
        (
            selection("sym_comp", "ly_comp_ia", Some("Shape 1")),
            Some("sym_comp"),
            ResolveError::UnknownStroke,
        ),
        (
            selection("sym_comp", "ly_comp_ib", Some("s_comp_a_0")),
            Some("sym_comp"),
            ResolveError::UnknownStroke,
        ),
        (
            selection("scene-root", "ly_missing", None),
            None,
            ResolveError::UnknownLayer,
        ),
    ];
    for (target, active, expected) in refusals {
        assert_eq!(
            resolve_selection(&source, active, &target),
            Err(SelectionError::Resolve(expected)),
            "{target:?}"
        );
    }
}

#[test]
fn the_workspace_catalog_lists_the_components_with_their_frames_and_fps() {
    let mut w = world("D");
    let source = w.source();
    let catalog = workspace_catalog(&source);
    let expected = |id: &str, fps: Option<f64>| ContextInfo {
        context_id: id.to_string(),
        total_frames: 12,
        fps,
    };
    assert_eq!(
        catalog,
        vec![
            expected("sym_comp", Some(24.0)),
            expected("sym_comp_b", Some(24.0))
        ]
    );
    // The scene is not entered by id, and an undeclared id is the workspace's unknown no-op.
    let mut ws = WorkspaceState::new(view(30.0));
    let mut live = Vec::new();
    for id in ["scene-root", "sym_x", ""] {
        assert_eq!(
            ws.enter_context(&catalog, &mut live, id, None),
            EnterOutcome::UnknownContext
        );
    }
    assert_eq!(
        ws.enter_context(&catalog, &mut live, "sym_comp", None),
        EnterOutcome::Entered
    );
    assert_eq!(ws.view.total_frames, 12);
    assert_eq!(ws.view.fps, 24.0);
    // A component without fps carries none, so entry falls back to the scene's fps.
    w.set.components[0].fps = Presence::Missing;
    let catalog = workspace_catalog(&w.source());
    assert_eq!(catalog[0].fps, None);
    assert_eq!(catalog[1].fps, Some(24.0));
    let mut ws = WorkspaceState::new(view(30.0));
    assert_eq!(
        ws.enter_context(&catalog, &mut live, "sym_comp", None),
        EnterOutcome::Entered
    );
    assert_eq!(ws.view.fps, 30.0);
}

#[test]
fn an_ambiguous_inner_stroke_is_not_guessed() {
    let mut w = world("D");
    // The same stroke id in a second inner layer of the component.
    let mut twin = w.set.components[0].objects[0].clone();
    twin.target.layer_uid = "ly_comp_ib".to_string();
    w.set.components[0].objects.push(twin);
    let source = w.source();
    let hit = address_hit(&source, None, "ly_comp_loop", Some("s_comp_a_0")).unwrap();
    assert_eq!(hit.selection, instance("ly_comp_loop", "sym_comp"));
    assert_eq!(hit.inner, None);
    assert_eq!(hit.enter.as_deref(), Some("sym_comp"));
    // The unique one is still found.
    let unique = address_hit(&source, None, "ly_comp_loop", Some("s_comp_a_2")).unwrap();
    assert_eq!(
        unique.inner,
        Some(element("sym_comp", "ly_comp_ia", "s_comp_a_2"))
    );
}

#[test]
fn addressing_a_hit_rejects_unknown_active_contexts_and_empty_ids() {
    let w = world("D");
    let source = w.source();
    let refusals = [
        (Some("sym_x"), "ly_comp_ia", ResolveError::UnknownContext),
        (Some(""), "ly_comp_ia", ResolveError::EmptyId),
        (None, "", ResolveError::EmptyId),
        (None, "ly_missing", ResolveError::UnknownLayer),
        (Some("sym_comp"), "ly_plain", ResolveError::UnknownLayer),
    ];
    for (active, layer, expected) in refusals {
        assert_eq!(
            address_hit(&source, active, layer, None),
            Err(SelectionError::Resolve(expected)),
            "{active:?} {layer:?}"
        );
    }
}
