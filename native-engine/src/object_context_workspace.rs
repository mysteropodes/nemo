//! Workspace context-switching kernel (L04-H, D33 + F14).
//!
//! The observable effect of `enterSymbol`, `exitToScene`, `closeSymbolTab` and the
//! symbol tab bar (`renderSymbolTabs`) of the baseline (`src/js/app.js`,
//! `src/js/timeline.js` at `fbe4b8a3`), reduced to data. A Component is entered as a
//! real CONTEXT of the one document (a tab), not as a second editor document: the
//! workspace keeps no layers, strokes, camera keys, markers or tweens. What the
//! baseline snapshots into `_sceneSnapshot` and swaps into `state.layers` is
//! document content that already lives in the document under the context id; the
//! kernel keeps only the playhead/work-area view and the tab list.
//!
//! Baseline facts kept (each one is a row of the oracle):
//! - entering the active context is a no-op; entering while another context is
//!   active is refused (nested components are unsupported); an unknown context is
//!   a no-op. Checked in that order;
//! - entry clears the selection, saves the scene view, resets the playhead to 0
//!   (or to the caller's clamped entry frame, the Motion `enterComponentLayer`
//!   jump), the work area to `[0, total - 1]` and the active layer to 0, takes
//!   `sym.fps || state.fps` and appends the tab once;
//! - exit clears the selection, restores the saved scene view exactly and keeps the
//!   tab open; it reports the live frame count and fps the baseline writes back to
//!   `state.symbols[id]` (only while the context still exists) and the context id
//!   whose StoryBoard thumbnail must be invalidated;
//! - closing a tab exits first when it is the active one, then drops the tab;
//! - the tab bar lists the open tabs whose context still exists; the Scene tab is
//!   active when no context is.
//!
//! Stable identities: the kernel never mints, renames or invents a context id. A
//! remap table (old -> new) rewrites the open tabs and the active context id only;
//! every unmapped id is preserved and a table that would merge two live contexts is
//! refused before anything changes. Persistence of the tab list and the real
//! Tauri tab strip are consumer ports (the baseline does not persist tabs at all).

use crate::evaluation::SUPPORTED_CONTEXT_ID;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "../tests/object_context_workspace_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_context_workspace_pinned.rs"]
mod pinned_tests;
#[cfg(test)]
#[path = "../tests/object_context_workspace_remap.rs"]
mod remap_tests;
#[cfg(test)]
#[path = "../tests/object_context_workspace.rs"]
mod tests;

/// The playhead/work-area view of the current context: `state.currentFrame`,
/// `state.waIn`, `state.waOut`, `state.activeLayerIdx`, `state.totalFrames`,
/// `state.fps`. Scalars only; the document stays the authority for content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkspaceView {
    pub current_frame: i64,
    pub wa_in: i64,
    pub wa_out: i64,
    pub active_layer_idx: i64,
    pub total_frames: i64,
    pub fps: f64,
}

/// A context (`state.symbols[id]`) as entry needs it. `fps` is `None` when the
/// context carries no fps; `0` and `NaN` are JavaScript-falsy and fall back too.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextInfo {
    pub context_id: String,
    pub total_frames: i64,
    pub fps: Option<f64>,
}

/// One selected item, addressed by native identities (never a frame-local one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectionTarget {
    pub context_id: String,
    pub layer_uid: String,
    pub stroke_id: Option<String>,
}

/// The entered context and the scene view saved by entry (`_sceneSnapshot`
/// reduced to the view; the scene content is not copied).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveContext {
    pub context_id: String,
    pub saved_scene_view: WorkspaceView,
}

/// The whole workspace state: the active context (if any), the open tab list and
/// the live view. Nothing in it duplicates document content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkspaceState {
    pub active: Option<ActiveContext>,
    pub open_tabs: Vec<String>,
    pub view: WorkspaceView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EnterOutcome {
    Entered,
    AlreadyActive,
    RefusedNested { active: String },
    UnknownContext,
}

/// The frame count and fps the baseline writes back to the exited context.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextWriteback {
    pub total_frames: i64,
    pub fps: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExitOutcome {
    NotInContext,
    /// `writeback` is `None` when the context no longer exists in the catalog
    /// (the baseline `if(sym)`); the thumbnail of `context_id` is invalidated
    /// either way.
    Exited {
        context_id: String,
        writeback: Option<ContextWriteback>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CloseOutcome {
    pub exit: ExitOutcome,
    pub tab_removed: bool,
}

/// What a tab-bar click addresses: the Scene tab or a context tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabTarget<'a> {
    Scene,
    Context(&'a str),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TabClick {
    Entered(EnterOutcome),
    Exited(ExitOutcome),
}

/// One rendered symbol tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VisibleTab {
    pub context_id: String,
    pub is_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextRemap {
    pub old_id: String,
    pub new_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemapReport {
    pub tabs_changed: usize,
    pub active_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemapError {
    EmptyId,
    /// The same old id is mapped to two different new ids.
    ConflictingMapping {
        old_id: String,
    },
    /// Two distinct live contexts would end up with the same id.
    Collision {
        new_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceError {
    EmptyTabId,
    DuplicateTab(String),
    ActiveNotOpen(String),
}

fn find<'a>(catalog: &'a [ContextInfo], id: &str) -> Option<&'a ContextInfo> {
    if id.is_empty() {
        return None;
    }
    catalog.iter().find(|c| c.context_id == id)
}

/// `sym.fps || state.fps`.
fn fps_or(candidate: Option<f64>, fallback: f64) -> f64 {
    match candidate {
        Some(f) if f != 0.0 && !f.is_nan() => f,
        _ => fallback,
    }
}

impl WorkspaceState {
    /// A scene workspace with no open tab.
    pub(crate) fn new(view: WorkspaceView) -> WorkspaceState {
        WorkspaceState {
            active: None,
            open_tabs: Vec::new(),
            view,
        }
    }

    /// A scene workspace restored with an open tab list (consumer port). Tabs must
    /// be non-empty and unique, as the baseline's append-once rule keeps them.
    pub(crate) fn with_open_tabs(
        view: WorkspaceView,
        open_tabs: Vec<String>,
    ) -> Result<WorkspaceState, WorkspaceError> {
        let state = WorkspaceState {
            active: None,
            open_tabs,
            view,
        };
        state.validate()?;
        Ok(state)
    }

    pub(crate) fn validate(&self) -> Result<(), WorkspaceError> {
        let mut seen = BTreeSet::new();
        for tab in &self.open_tabs {
            if tab.is_empty() {
                return Err(WorkspaceError::EmptyTabId);
            }
            if !seen.insert(tab.as_str()) {
                return Err(WorkspaceError::DuplicateTab(tab.clone()));
            }
        }
        if let Some(active) = &self.active {
            if !seen.contains(active.context_id.as_str()) {
                return Err(WorkspaceError::ActiveNotOpen(active.context_id.clone()));
            }
        }
        Ok(())
    }

    pub(crate) fn active_context_id(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.context_id.as_str())
    }

    pub(crate) fn saved_scene_view(&self) -> Option<&WorkspaceView> {
        self.active.as_ref().map(|a| &a.saved_scene_view)
    }

    /// The Scene tab is active exactly when no context is. (The StoryBoard montage
    /// view, `activeMontageViewId`, also clears it; that view belongs to L10.)
    pub(crate) fn scene_tab_active(&self) -> bool {
        self.active.is_none()
    }

    /// `enterSymbol`. `entry_frame` is the Motion `enterComponentLayer` jump
    /// (`Math.max(0, Math.min(totalFrames - 1, target))`); `None` is the plain
    /// Animation 2D entry at frame 0. Every non-`Entered` outcome leaves the state
    /// and the selection untouched.
    pub(crate) fn enter_context(
        &mut self,
        catalog: &[ContextInfo],
        selection: &mut Vec<SelectionTarget>,
        context_id: &str,
        entry_frame: Option<i64>,
    ) -> EnterOutcome {
        if let Some(active) = &self.active {
            if active.context_id == context_id {
                return EnterOutcome::AlreadyActive;
            }
            return EnterOutcome::RefusedNested {
                active: active.context_id.clone(),
            };
        }
        let info = match find(catalog, context_id) {
            Some(info) => info,
            None => return EnterOutcome::UnknownContext,
        };
        selection.clear();
        let saved = self.view.clone();
        let last = info.total_frames.saturating_sub(1);
        let frame = match entry_frame {
            Some(target) => target.min(last).max(0),
            None => 0,
        };
        self.view = WorkspaceView {
            current_frame: frame,
            wa_in: 0,
            wa_out: last,
            active_layer_idx: 0,
            total_frames: info.total_frames,
            fps: fps_or(info.fps, saved.fps),
        };
        if !self.open_tabs.iter().any(|t| t == context_id) {
            self.open_tabs.push(context_id.to_string());
        }
        self.active = Some(ActiveContext {
            context_id: context_id.to_string(),
            saved_scene_view: saved,
        });
        EnterOutcome::Entered
    }

    /// `exitToScene`.
    pub(crate) fn exit_context(
        &mut self,
        catalog: &[ContextInfo],
        selection: &mut Vec<SelectionTarget>,
    ) -> ExitOutcome {
        let active = match self.active.take() {
            Some(active) => active,
            None => return ExitOutcome::NotInContext,
        };
        selection.clear();
        let writeback = find(catalog, &active.context_id).map(|_| ContextWriteback {
            total_frames: self.view.total_frames,
            fps: self.view.fps,
        });
        self.view = active.saved_scene_view;
        ExitOutcome::Exited {
            context_id: active.context_id,
            writeback,
        }
    }

    /// A click on the Scene tab (exit) or on a context tab (`enterSymbol`, so a
    /// click on another tab while a context is active is the nested refusal).
    pub(crate) fn click_tab(
        &mut self,
        catalog: &[ContextInfo],
        selection: &mut Vec<SelectionTarget>,
        target: TabTarget<'_>,
    ) -> TabClick {
        match target {
            TabTarget::Scene => TabClick::Exited(self.exit_context(catalog, selection)),
            TabTarget::Context(id) => {
                TabClick::Entered(self.enter_context(catalog, selection, id, None))
            }
        }
    }

    /// `closeSymbolTab`: exit first when the tab is the active context, then drop
    /// the first matching tab.
    pub(crate) fn close_tab(
        &mut self,
        catalog: &[ContextInfo],
        selection: &mut Vec<SelectionTarget>,
        context_id: &str,
    ) -> CloseOutcome {
        let exit = if self.active_context_id() == Some(context_id) {
            self.exit_context(catalog, selection)
        } else {
            ExitOutcome::NotInContext
        };
        let tab_removed = match self.open_tabs.iter().position(|t| t == context_id) {
            Some(at) => {
                self.open_tabs.remove(at);
                true
            }
            None => false,
        };
        CloseOutcome { exit, tab_removed }
    }

    /// Project open/new: exit the active context, then forget every tab.
    pub(crate) fn reset_for_project_replace(
        &mut self,
        catalog: &[ContextInfo],
        selection: &mut Vec<SelectionTarget>,
    ) -> ExitOutcome {
        let exit = self.exit_context(catalog, selection);
        self.open_tabs.clear();
        exit
    }

    /// `renderSymbolTabs`: the open tabs whose context still exists, in order.
    pub(crate) fn visible_tabs(&self, catalog: &[ContextInfo]) -> Vec<VisibleTab> {
        self.open_tabs
            .iter()
            .filter(|t| find(catalog, t).is_some())
            .map(|t| VisibleTab {
                context_id: t.clone(),
                is_active: self.active_context_id() == Some(t.as_str()),
            })
            .collect()
    }

    /// The playhead of a context: the live one for the active context (or for the
    /// scene while none is active), the saved scene one for the scene while a
    /// context is active, `None` for any other context.
    pub(crate) fn frame_of(&self, context_id: &str) -> Option<i64> {
        self.view_of(context_id).map(|v| v.current_frame)
    }

    /// The work area `(in, out)` of a context, with the same addressing.
    pub(crate) fn work_area_of(&self, context_id: &str) -> Option<(i64, i64)> {
        self.view_of(context_id).map(|v| (v.wa_in, v.wa_out))
    }

    fn view_of(&self, context_id: &str) -> Option<&WorkspaceView> {
        match &self.active {
            Some(active) if active.context_id == context_id => Some(&self.view),
            Some(active) if context_id == SUPPORTED_CONTEXT_ID => Some(&active.saved_scene_view),
            None if context_id == SUPPORTED_CONTEXT_ID => Some(&self.view),
            _ => None,
        }
    }

    /// Rewrites the open tabs and the active context id through an old -> new table.
    /// Unmapped ids are preserved; the saved scene view and the live view carry no
    /// context id and are untouched. All-or-nothing: the table is validated first.
    pub(crate) fn remap_contexts(
        &mut self,
        table: &[ContextRemap],
    ) -> Result<RemapReport, RemapError> {
        let mut map: BTreeMap<&str, &str> = BTreeMap::new();
        for entry in table {
            if entry.old_id.is_empty() || entry.new_id.is_empty() {
                return Err(RemapError::EmptyId);
            }
            if let Some(previous) = map.insert(entry.old_id.as_str(), entry.new_id.as_str()) {
                if previous != entry.new_id.as_str() {
                    return Err(RemapError::ConflictingMapping {
                        old_id: entry.old_id.clone(),
                    });
                }
            }
        }
        let target = |id: &str| -> String { map.get(id).copied().unwrap_or(id).to_string() };
        let mut originals: Vec<String> = self.open_tabs.clone();
        if let Some(active) = &self.active {
            if !originals.contains(&active.context_id) {
                originals.push(active.context_id.clone());
            }
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for original in &originals {
            let renamed = target(original.as_str());
            if !seen.insert(renamed.clone()) {
                return Err(RemapError::Collision { new_id: renamed });
            }
        }
        let mut tabs_changed = 0;
        for tab in self.open_tabs.iter_mut() {
            let renamed = target(tab.as_str());
            if renamed != *tab {
                tabs_changed += 1;
                *tab = renamed;
            }
        }
        let mut active_changed = false;
        if let Some(active) = self.active.as_mut() {
            let renamed = target(active.context_id.as_str());
            if renamed != active.context_id {
                active_changed = true;
                active.context_id = renamed;
            }
        }
        Ok(RemapReport {
            tabs_changed,
            active_changed,
        })
    }
}
