//! Pinned literals for the workspace kernel: the baseline facts written out by hand,
//! each cross-checked against the oracle rows named in its comment.

use super::{
    ActiveContext, ContextInfo, ContextWriteback, EnterOutcome, ExitOutcome, SelectionTarget,
    TabClick, TabTarget, VisibleTab, WorkspaceError, WorkspaceState, WorkspaceView,
};
use crate::evaluation::SUPPORTED_CONTEXT_ID;

pub(crate) fn view(
    frame: i64,
    wa_in: i64,
    wa_out: i64,
    li: i64,
    total: i64,
    fps: f64,
) -> WorkspaceView {
    WorkspaceView {
        current_frame: frame,
        wa_in,
        wa_out,
        active_layer_idx: li,
        total_frames: total,
        fps,
    }
}

pub(crate) fn scene() -> WorkspaceView {
    view(37, 5, 99, 2, 120, 24.0)
}

pub(crate) fn info(id: &str, total: i64, fps: Option<f64>) -> ContextInfo {
    ContextInfo {
        context_id: id.to_string(),
        total_frames: total,
        fps,
    }
}

pub(crate) fn catalog() -> Vec<ContextInfo> {
    vec![
        info("s_a", 24, Some(12.0)),
        info("s_b", 10, None),
        info("s_c", 0, Some(24.0)),
    ]
}

pub(crate) fn selection(n: usize) -> Vec<SelectionTarget> {
    super::tests::make_sel(n, SUPPORTED_CONTEXT_ID)
}

pub(crate) fn ids(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn entry_saves_the_scene_and_resets_the_view() {
    // oracle: S basic 0 E:s_a
    let mut ws = WorkspaceState::new(scene());
    let mut sel = selection(3);
    assert_eq!(
        ws.enter_context(&catalog(), &mut sel, "s_a", None),
        EnterOutcome::Entered
    );
    assert_eq!(ws.view, view(0, 0, 23, 0, 24, 12.0));
    assert_eq!(ws.saved_scene_view(), Some(&scene()));
    assert_eq!(ws.active_context_id(), Some("s_a"));
    assert_eq!(ws.open_tabs, ids(&["s_a"]));
    assert!(sel.is_empty());
    assert!(!ws.scene_tab_active());
    // Per-context playheads: the live one, the parked scene one, nothing for others.
    assert_eq!(ws.frame_of("s_a"), Some(0));
    assert_eq!(ws.frame_of(SUPPORTED_CONTEXT_ID), Some(37));
    assert_eq!(ws.frame_of("s_b"), None);
    assert_eq!(ws.work_area_of("s_a"), Some((0, 23)));
    assert_eq!(ws.work_area_of(SUPPORTED_CONTEXT_ID), Some((5, 99)));
}

#[test]
fn exit_restores_the_scene_view_exactly_and_keeps_the_tab() {
    // oracle: S edit 1..3 (V:9,2,20,1,60,30 then X)
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = selection(3);
    ws.enter_context(&cat, &mut sel, "s_a", None);
    ws.view = view(9, 2, 20, 1, 60, 30.0);
    sel.extend(selection(2));
    let exit = ws.exit_context(&cat, &mut sel);
    assert_eq!(
        exit,
        ExitOutcome::Exited {
            context_id: "s_a".to_string(),
            writeback: Some(ContextWriteback {
                total_frames: 60,
                fps: 30.0
            }),
        }
    );
    assert_eq!(ws.view, scene());
    assert!(sel.is_empty());
    assert_eq!(ws.active, None);
    assert_eq!(ws.open_tabs, ids(&["s_a"]));
    assert!(ws.scene_tab_active());
    assert_eq!(ws.frame_of(SUPPORTED_CONTEXT_ID), Some(37));
    // Nothing active: a second exit is a no-op.
    assert_eq!(ws.exit_context(&cat, &mut sel), ExitOutcome::NotInContext);
}

#[test]
fn exit_without_the_context_has_no_writeback_but_still_names_the_thumbnail() {
    // oracle: S delete 2 X (wb=-, thumb=s_a)
    let mut cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = selection(1);
    ws.enter_context(&cat, &mut sel, "s_a", None);
    cat.retain(|c| c.context_id != "s_a");
    assert!(ws.visible_tabs(&cat).is_empty());
    assert_eq!(
        ws.exit_context(&cat, &mut sel),
        ExitOutcome::Exited {
            context_id: "s_a".to_string(),
            writeback: None
        }
    );
    assert_eq!(ws.view, scene());
}

#[test]
fn refusals_leave_state_and_selection_untouched() {
    // oracle: S nested 1 (nested), S nested 2 (already), S unknown 0 (unknown)
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = selection(3);
    assert_eq!(
        ws.enter_context(&cat, &mut sel, "ghost", None),
        EnterOutcome::UnknownContext
    );
    assert_eq!(
        ws.enter_context(&cat, &mut sel, "", None),
        EnterOutcome::UnknownContext
    );
    assert_eq!(
        (
            ws.view.clone(),
            ws.active.clone(),
            ws.open_tabs.clone(),
            sel.len()
        ),
        (scene(), None, vec![], 3)
    );
    ws.enter_context(&cat, &mut sel, "s_a", None);
    sel.extend(selection(2));
    let before = ws.clone();
    assert_eq!(
        ws.enter_context(&cat, &mut sel, "s_b", None),
        EnterOutcome::RefusedNested {
            active: "s_a".to_string()
        }
    );
    // The nested refusal wins over the unknown check, and "already" wins over both.
    assert_eq!(
        ws.enter_context(&cat, &mut sel, "ghost", None),
        EnterOutcome::RefusedNested {
            active: "s_a".to_string()
        }
    );
    assert_eq!(
        ws.enter_context(&cat, &mut sel, "s_a", Some(7)),
        EnterOutcome::AlreadyActive
    );
    assert_eq!(ws, before);
    assert_eq!(sel.len(), 2);
}

#[test]
fn fps_falls_back_to_the_scene_like_a_javascript_or() {
    // oracle: S fps rows (s_b no fps, s_d fps 0) and fpsNoFallback mutant
    for (fps, want) in [
        (None, 24.0),
        (Some(0.0), 24.0),
        (Some(-0.0), 24.0),
        (Some(f64::NAN), 24.0),
        (Some(12.0), 12.0),
        (Some(29.97), 29.97),
    ] {
        let cat = vec![info("c", 5, fps)];
        let mut ws = WorkspaceState::new(scene());
        ws.enter_context(&cat, &mut Vec::new(), "c", None);
        assert_eq!(ws.view.fps, want, "fps {fps:?}");
    }
}

#[test]
fn entry_frame_is_clamped_like_the_motion_jump() {
    // oracle: S zero rows. Math.max(0, Math.min(total - 1, target)).
    let cat = catalog();
    for (id, target, want_frame, want_out) in [
        ("s_a", 30, 23, 23),
        ("s_a", -3, 0, 23),
        ("s_a", 11, 11, 23),
        ("s_a", 23, 23, 23),
        ("s_c", 5, 0, -1),
        ("s_c", -5, 0, -1),
    ] {
        let mut ws = WorkspaceState::new(scene());
        ws.enter_context(&cat, &mut Vec::new(), id, Some(target));
        assert_eq!(
            (ws.view.current_frame, ws.view.wa_in, ws.view.wa_out),
            (want_frame, 0, want_out),
            "{id} {target}"
        );
    }
    // No entry frame is the plain Animation 2D entry at frame 0.
    let mut ws = WorkspaceState::new(scene());
    ws.enter_context(&cat, &mut Vec::new(), "s_a", None);
    assert_eq!(ws.view.current_frame, 0);
}

#[test]
fn tabs_are_appended_once_and_re_entry_starts_at_frame_zero() {
    // oracle: S dup rows. The baseline keeps no inner playhead between visits.
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = Vec::new();
    for _ in 0..3 {
        ws.enter_context(&cat, &mut sel, "s_a", None);
        assert_eq!(ws.view.current_frame, 0);
        ws.view.current_frame = 9;
        ws.exit_context(&cat, &mut sel);
        assert_eq!(ws.view, scene());
    }
    assert_eq!(ws.open_tabs, ids(&["s_a"]));
    ws.enter_context(&cat, &mut sel, "s_b", None);
    ws.exit_context(&cat, &mut sel);
    ws.enter_context(&cat, &mut sel, "s_a", None);
    assert_eq!(ws.open_tabs, ids(&["s_a", "s_b"]));
}

#[test]
fn closing_a_tab_exits_first_only_for_the_active_context() {
    // oracle: S close rows
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = selection(2);
    ws.enter_context(&cat, &mut sel, "s_a", None);
    ws.exit_context(&cat, &mut sel);
    ws.enter_context(&cat, &mut sel, "s_b", None);
    let closed = ws.close_tab(&cat, &mut sel, "s_a");
    assert_eq!(
        (closed.exit, closed.tab_removed),
        (ExitOutcome::NotInContext, true)
    );
    assert_eq!(ws.active_context_id(), Some("s_b"));
    let closed = ws.close_tab(&cat, &mut sel, "ghost");
    assert_eq!(
        (closed.exit, closed.tab_removed),
        (ExitOutcome::NotInContext, false)
    );
    let closed = ws.close_tab(&cat, &mut sel, "s_b");
    assert!(matches!(closed.exit, ExitOutcome::Exited { .. }) && closed.tab_removed);
    assert_eq!(
        (ws.view.clone(), ws.open_tabs.clone(), ws.active.clone()),
        (scene(), vec![], None)
    );
}

#[test]
fn tab_clicks_follow_the_baseline_handlers() {
    // oracle: S nested 3..8 and same-tab-click rows
    let cat = catalog();
    let mut ws = WorkspaceState::new(scene());
    let mut sel = Vec::new();
    assert_eq!(
        ws.click_tab(&cat, &mut sel, TabTarget::Scene),
        TabClick::Exited(ExitOutcome::NotInContext)
    );
    assert_eq!(
        ws.click_tab(&cat, &mut sel, TabTarget::Context("s_a")),
        TabClick::Entered(EnterOutcome::Entered)
    );
    // Another tab while inside: the nested refusal. The Scene tab is the way out.
    assert_eq!(
        ws.click_tab(&cat, &mut sel, TabTarget::Context("s_b")),
        TabClick::Entered(EnterOutcome::RefusedNested {
            active: "s_a".to_string()
        })
    );
    assert!(matches!(
        ws.click_tab(&cat, &mut sel, TabTarget::Scene),
        TabClick::Exited(ExitOutcome::Exited { .. })
    ));
}

#[test]
fn project_replace_exits_and_forgets_every_tab() {
    // oracle: S reset rows
    let cat = catalog();
    let mut ws = WorkspaceState::with_open_tabs(scene(), ids(&["s_b", "ghost"])).unwrap();
    let mut sel = selection(2);
    ws.enter_context(&cat, &mut sel, "s_a", None);
    let exit = ws.reset_for_project_replace(&cat, &mut sel);
    assert!(matches!(exit, ExitOutcome::Exited { ref context_id, .. } if context_id == "s_a"));
    assert_eq!(
        (ws.view.clone(), ws.active.clone(), ws.open_tabs.clone()),
        (scene(), None, vec![])
    );
    assert_eq!(
        ws.reset_for_project_replace(&cat, &mut sel),
        ExitOutcome::NotInContext
    );
}

#[test]
fn the_tab_bar_lists_only_existing_contexts() {
    // oracle: S stale rows (ghost tabs are open but not rendered)
    let cat = catalog();
    let mut ws =
        WorkspaceState::with_open_tabs(scene(), ids(&["ghost", "s_a", "ghost2", "s_b"])).unwrap();
    let tab = |id: &str, active: bool| VisibleTab {
        context_id: id.to_string(),
        is_active: active,
    };
    assert_eq!(
        ws.visible_tabs(&cat),
        vec![tab("s_a", false), tab("s_b", false)]
    );
    ws.enter_context(&cat, &mut Vec::new(), "s_b", None);
    assert_eq!(
        ws.visible_tabs(&cat),
        vec![tab("s_a", false), tab("s_b", true)]
    );
}

#[test]
fn restoring_tabs_rejects_duplicates_empty_ids_and_an_unopened_active_context() {
    assert_eq!(
        WorkspaceState::with_open_tabs(scene(), ids(&["a", "b", "a"])).unwrap_err(),
        WorkspaceError::DuplicateTab("a".to_string())
    );
    assert_eq!(
        WorkspaceState::with_open_tabs(scene(), ids(&["a", ""])).unwrap_err(),
        WorkspaceError::EmptyTabId
    );
    let mut ws = WorkspaceState::new(scene());
    ws.active = Some(ActiveContext {
        context_id: "x".to_string(),
        saved_scene_view: scene(),
    });
    assert_eq!(
        ws.validate(),
        Err(WorkspaceError::ActiveNotOpen("x".to_string()))
    );
}
