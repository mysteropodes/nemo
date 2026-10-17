//! Mutation controls for the workspace kernel: each named deviation from the
//! baseline must be caught by the oracle rows.
//!
//! The fixture header carries, per deviation, the number of oracle rows that the
//! equivalent edit of the baseline JavaScript changes (`# mutant <name> <rows>`,
//! measured by the generator). Each deviation here wraps the REAL kernel call with
//! the one behavior change and replays the same oracle; it must miss at least that
//! many rows. The undeviated wrapper must miss none, which proves the wrapping
//! mechanics change nothing by themselves.

use super::tests::{replay, Driver, Env, Real, ORACLE};
use super::{
    ActiveContext, CloseOutcome, EnterOutcome, ExitOutcome, TabClick, TabTarget, WorkspaceView,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Deviation {
    Passthrough,
    /// `openSymbolTabs.push` without the `indexOf < 0` guard.
    DuplicateTab,
    /// Entry no longer clears the selection.
    SelectionKeptOnEnter,
    /// Exit no longer clears the selection.
    SelectionKeptOnExit,
    /// The nested guard is gone: a second context replaces the first one.
    NestedAllowed,
    /// Entry keeps the scene work area instead of `[0, total - 1]`.
    WorkAreaKept,
    /// Entry keeps the scene playhead instead of frame 0.
    FrameKept,
    /// Entry takes `sym.fps` without the `|| state.fps` fallback.
    FpsWithoutFallback,
    /// Exit keeps the inner playhead instead of the saved scene one.
    ExitKeepsInnerFrame,
    /// Closing a tab never removes it.
    CloseKeepsTab,
    /// Exit reports no write-back of the frame count and fps.
    NoWriteback,
}

struct Mutant(Deviation);

impl Driver for Mutant {
    fn enter(&self, env: &mut Env, id: &str, frame: Option<i64>) -> EnterOutcome {
        let had_tab = env.ws.open_tabs.iter().any(|t| t == id);
        let sel_before = env.sel.clone();
        let scene_view = env.ws.view.clone();
        let nested_from = env.ws.active_context_id().map(|s| s.to_string());
        if self.0 == Deviation::NestedAllowed {
            if let Some(from) = nested_from {
                if from != id {
                    return nested_entry(env, id, from);
                }
            }
        }
        let outcome = Real.enter(env, id, frame);
        if outcome != EnterOutcome::Entered {
            return outcome;
        }
        match self.0 {
            Deviation::DuplicateTab if had_tab => env.ws.open_tabs.push(id.to_string()),
            Deviation::SelectionKeptOnEnter => env.sel = sel_before,
            Deviation::WorkAreaKept => {
                env.ws.view.wa_in = scene_view.wa_in;
                env.ws.view.wa_out = scene_view.wa_out;
            }
            Deviation::FrameKept if frame.is_none() => {
                env.ws.view.current_frame = scene_view.current_frame;
            }
            Deviation::FpsWithoutFallback => {
                let own = env
                    .catalog
                    .iter()
                    .find(|c| c.context_id == id)
                    .and_then(|c| c.fps);
                env.ws.view.fps = own.unwrap_or(f64::NAN);
            }
            _ => {}
        }
        outcome
    }

    fn exit(&self, env: &mut Env) -> ExitOutcome {
        let sel_before = env.sel.clone();
        let inner_frame = env.ws.view.current_frame;
        let outcome = Real.exit(env);
        let exited = match &outcome {
            ExitOutcome::Exited { context_id, .. } => Some(context_id.clone()),
            ExitOutcome::NotInContext => None,
        };
        let context_id = match exited {
            Some(id) => id,
            None => return outcome,
        };
        match self.0 {
            Deviation::SelectionKeptOnExit => env.sel = sel_before,
            Deviation::ExitKeepsInnerFrame => env.ws.view.current_frame = inner_frame,
            Deviation::NoWriteback => {
                return ExitOutcome::Exited {
                    context_id,
                    writeback: None,
                };
            }
            _ => {}
        }
        outcome
    }

    fn close(&self, env: &mut Env, id: &str) -> CloseOutcome {
        // `closeSymbolTab` exits through `exitToScene`, so the exit deviations apply here too.
        let tabs_before = env.ws.open_tabs.clone();
        let exit = if env.ws.active_context_id() == Some(id) {
            self.exit(env)
        } else {
            ExitOutcome::NotInContext
        };
        let tab_removed = match env.ws.open_tabs.iter().position(|t| t == id) {
            Some(at) => {
                env.ws.open_tabs.remove(at);
                true
            }
            None => false,
        };
        if self.0 == Deviation::CloseKeepsTab {
            env.ws.open_tabs = tabs_before;
            return CloseOutcome {
                exit,
                tab_removed: false,
            };
        }
        CloseOutcome { exit, tab_removed }
    }

    fn click(&self, env: &mut Env, target: TabTarget<'_>) -> TabClick {
        match target {
            TabTarget::Scene => TabClick::Exited(self.exit(env)),
            TabTarget::Context(id) => TabClick::Entered(self.enter(env, id, None)),
        }
    }

    fn reset(&self, env: &mut Env) -> ExitOutcome {
        let exit = self.exit(env);
        env.ws.open_tabs.clear();
        exit
    }
}

/// The baseline without its nested guard: the second context overwrites the saved
/// scene view with the first one's live view. The outcome label stays "nested",
/// as the generator derives it from the state before the call.
fn nested_entry(env: &mut Env, id: &str, from: String) -> EnterOutcome {
    if let Some(info) = env.catalog.iter().find(|c| c.context_id == id) {
        env.sel.clear();
        let saved: WorkspaceView = env.ws.view.clone();
        let last = info.total_frames - 1;
        let fps = match info.fps {
            Some(f) if f != 0.0 && !f.is_nan() => f,
            _ => saved.fps,
        };
        env.ws.view = WorkspaceView {
            current_frame: 0,
            wa_in: 0,
            wa_out: last,
            active_layer_idx: 0,
            total_frames: info.total_frames,
            fps,
        };
        if !env.ws.open_tabs.iter().any(|t| t == id) {
            env.ws.open_tabs.push(id.to_string());
        }
        env.ws.active = Some(ActiveContext {
            context_id: id.to_string(),
            saved_scene_view: saved,
        });
    }
    EnterOutcome::RefusedNested { active: from }
}

fn assert_caught(name: &str, d: Deviation) {
    let (total, wrong) = replay(&Mutant(d));
    let want = measured(name);
    assert!(want > 0, "{name}: the generator measured no changed row");
    assert!(
        wrong >= want,
        "{name}: only {wrong} of {total} rows differ, the baseline mutant changes {want}"
    );
}

#[test]
fn the_undeviated_wrapper_misses_no_row() {
    let (total, wrong) = replay(&Mutant(Deviation::Passthrough));
    assert_eq!((total, wrong), (1292, 0));
}

#[test]
fn a_duplicated_tab_is_caught() {
    assert_caught("dupTab", Deviation::DuplicateTab);
}

#[test]
fn a_selection_kept_across_entry_is_caught() {
    assert_caught("selKeptEnter", Deviation::SelectionKeptOnEnter);
}

#[test]
fn a_selection_kept_across_exit_is_caught() {
    assert_caught("selKeptExit", Deviation::SelectionKeptOnExit);
}

#[test]
fn nested_entry_being_allowed_is_caught() {
    assert_caught("nestedAllowed", Deviation::NestedAllowed);
}

#[test]
fn a_work_area_kept_across_entry_is_caught() {
    assert_caught("workAreaKept", Deviation::WorkAreaKept);
}

#[test]
fn a_playhead_kept_across_entry_is_caught() {
    assert_caught("frameKept", Deviation::FrameKept);
}

#[test]
fn a_missing_fps_fallback_is_caught() {
    assert_caught("fpsNoFallback", Deviation::FpsWithoutFallback);
}

#[test]
fn an_exit_that_keeps_the_inner_playhead_is_caught() {
    assert_caught("exitNoRestoreFrame", Deviation::ExitKeepsInnerFrame);
}

#[test]
fn a_close_that_keeps_the_tab_is_caught() {
    assert_caught("closeKeepsTab", Deviation::CloseKeepsTab);
}

#[test]
fn a_missing_writeback_is_caught() {
    assert_caught("noWriteback", Deviation::NoWriteback);
}
