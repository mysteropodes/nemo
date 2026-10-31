//! #2256: native enter/exit component context (`query.document.object.workspace.context`).
//!
//! A pure view-state transition. The workspace (active context, open tabs, playhead and work area) is session view state,
//! like the viewport navigation: the document, its revision and its history never see it. The caller sends the workspace it
//! holds with a pinned snapshot and one action (`enter`, `exit`, `close`, `refresh`); this module rebuilds the
//! `WorkspaceState` kernel from them, applies the baseline rules (`enterSymbol` / `exitToScene` / `closeSymbolTab`: active
//! no-op, nested refused, entry resets the playhead and the work area, exit restores the saved scene view exactly) against the
//! contexts the snapshot DECLARES, and answers the new workspace plus the presented context facts. A context id that is not
//! declared is `not_found`: the engine resolves it, no arbitrary string is ever admitted as a context.
//!
//! It mutates nothing, publishes no revision, adds no history entry and keeps no state: the pin is `(atRevision,
//! documentSnapshotId)` of this snapshot, any other pin is `stale_revision`. Editing inside an entered component is a
//! separate, later owner; this answer only says WHICH context the viewport presents and at which frame.
#![cfg_attr(not(feature = "evaluation"), allow(dead_code, unused_imports))]
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Map, Value};

#[cfg(test)]
#[path = "../tests/object_workspace_context.rs"]
mod tests;

const MAX_ID_BYTES: usize = 128;
const MAX_TABS: usize = 8;
pub(crate) const OPERATION: &str = "query.document.object.workspace.context";

/// The playhead and work area of one context: scalars only, the document stays the authority for content.
#[derive(Clone, Copy)]
struct View {
    frame: i64,
    work_in: i64,
    work_out: i64,
}
struct Wire<'a> {
    active: Option<&'a str>,
    tabs: Vec<&'a str>,
    view: View,
    scene_view: Option<View>,
}
enum Action<'a> {
    Enter {
        id: &'a str,
        entry: Option<i64>,
        from: Option<&'a str>,
    },
    Exit,
    Close { id: &'a str },
    Refresh,
}

fn exact<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)))
        .then_some(object)
}
fn id(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.contains('\0'))
}
fn uint32(value: &Value) -> Option<i64> {
    value
        .as_u64()
        .filter(|n| *n <= u64::from(u32::MAX))
        .map(|n| n as i64)
}
fn view(value: &Value) -> Option<View> {
    let object = exact(value, &["frame", "workIn", "workOut"])?;
    Some(View {
        frame: uint32(&object["frame"])?,
        work_in: uint32(&object["workIn"])?,
        work_out: uint32(&object["workOut"])?,
    })
}
fn wire(value: &Value) -> Option<Wire<'_>> {
    let object = exact(value, &["activeContextId", "openTabs", "view", "sceneView"])?;
    let active = match &object["activeContextId"] {
        Value::Null => None,
        other => Some(id(other)?),
    };
    let tabs = object["openTabs"]
        .as_array()
        .filter(|t| t.len() <= MAX_TABS)?;
    let tabs = tabs.iter().map(id).collect::<Option<Vec<_>>>()?;
    let scene_view = match &object["sceneView"] {
        Value::Null => None,
        other => Some(view(other)?),
    };
    (active.is_some() == scene_view.is_some()).then_some(Wire {
        active,
        tabs,
        view: view(&object["view"])?,
        scene_view,
    })
}
fn action(value: &Value) -> Option<Action<'_>> {
    match value.get("kind")?.as_str()? {
        "enter" => {
            // `fromLayerUid` (optional) names the scene instance layer being entered: the owner resolves the component frame that
            // instance shows at the held playhead (`resolveSymbolFrameIdx`) and enters there. It never combines with an explicit entry frame.
            let object = value.as_object()?;
            let with_from = object.contains_key("fromLayerUid");
            exact(
                value,
                if with_from {
                    &["kind", "contextId", "entryFrame", "fromLayerUid"][..]
                } else {
                    &["kind", "contextId", "entryFrame"][..]
                },
            )?;
            let entry = match &object["entryFrame"] {
                Value::Null => None,
                other => Some(uint32(other)?),
            };
            let from = if with_from {
                match &object["fromLayerUid"] {
                    Value::Null => None,
                    other => Some(id(other)?),
                }
            } else {
                None
            };
            if entry.is_some() && from.is_some() {
                return None;
            }
            Some(Action::Enter {
                id: id(&object["contextId"])?,
                entry,
                from,
            })
        }
        "close" => Some(Action::Close {
            id: id(&exact(value, &["kind", "contextId"])?["contextId"])?,
        }),
        "exit" => exact(value, &["kind"]).map(|_| Action::Exit),
        "refresh" => exact(value, &["kind"]).map(|_| Action::Refresh),
        _ => None,
    }
}
fn request_parts(payload: &Value) -> Option<(Wire<'_>, Action<'_>)> {
    let object = exact(
        payload,
        &["atRevision", "documentSnapshotId", "workspace", "action"],
    )?;
    Some((wire(&object["workspace"])?, action(&object["action"])?))
}

impl ObjectSnapshot {
    /// The frame count of a context this snapshot DECLARES (`scene-root` or a component), else `None`: the one question the
    /// viewport scheduler asks before it presents a context, answered by the resolver so no caller reads component tables.
    #[cfg(feature = "evaluation")]
    pub(crate) fn declared_context_frames(&self, context_id: &str) -> Option<u32> {
        use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
        ContextSource::of_document(self.document())
            .context(context_id)
            .ok()
            .map(|context| context.total_frames())
    }

    pub(crate) fn dispatch_workspace_context(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let ReadAdmission::Refused(response) = self.read_admission(request)? {
            return Ok(response);
        }
        let refused = |code, message| Ok(self.read_failure(request, code, message));
        let Some((wire, action)) = request_parts(&request.payload) else {
            return refused(
                DispatchErrorCode::InvalidRequest,
                "Invalid workspace context payload.",
            );
        };
        let pinned = request.payload["atRevision"] == self.content_revision()
            && request.payload["documentSnapshotId"] == self.snapshot_id();
        if !pinned {
            return refused(
                DispatchErrorCode::StaleRevision,
                "Workspace context pin is not this snapshot.",
            );
        }
        match self.workspace_answer(request, wire, action) {
            Ok(response) => Ok(response),
            Err(code) => refused(
                code,
                "Workspace context is invalid, unavailable or names no such context.",
            ),
        }
    }

    /// Motion `enterComponentLayer`: the component frame the instance layer `layer_uid` shows at the scene frame `outer`, from the
    /// playback kernel the renderer itself uses (placement, speed, play mode, trim, the outer component-frame keys). A Time Remap
    /// is read as baseline `resolveSymbolFrameIdx` reads it: the instance's own keyed track sampled raw at the playhead; a track
    /// without keys falls through to the play mode. No expression can drive it: the native document refuses a `timeRemap`
    /// property record (the track belongs to the instance), and baseline `timeRemapValue` ignores expressions too. `None` leaves
    /// the entry at the first frame: no such instance of this component, or a held blank key.
    #[cfg(feature = "evaluation")]
    fn instance_entry_frame(&self, component: &str, layer_uid: &str, outer: i64) -> Option<i64> {
        use crate::evaluation::object_component_kernels::object_component_playback::{
            evaluate_component_playback, PlaybackOutcome,
        };
        use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
        use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::playback_input;
        let source = ContextSource::of_document(self.document());
        let instance = source.instance(layer_uid)?;
        if instance.symbol_id() != component {
            return None;
        }
        let host = source.context("scene-root").ok()?.layer(layer_uid)?.1;
        let total = source
            .components()
            .iter()
            .find(|c| c.symbol_id() == component)?
            .total_frames();
        let frame = u32::try_from(outer).ok()?;
        let input = playback_input(instance, host, frame).ok()?;
        let remap = instance
            .time_remap()
            .and_then(|track| crate::object_frame_packet::sample_scalar_keys_raw(&track.keys, outer as f64));
        match evaluate_component_playback(&input, f64::from(total), outer, remap) {
            PlaybackOutcome::Blank => None,
            PlaybackOutcome::Frame(resolved) => {
                let raw = resolved.raw();
                raw.is_finite()
                    .then(|| raw.floor().clamp(0.0, f64::from(total.saturating_sub(1))) as i64)
            }
        }
    }

    #[cfg(not(feature = "evaluation"))]
    fn workspace_answer(
        &self,
        _request: &OpacityRequest,
        _wire: Wire<'_>,
        _action: Action<'_>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        Err(DispatchErrorCode::Unavailable)
    }

    #[cfg(feature = "evaluation")]
    fn workspace_answer(
        &self,
        request: &OpacityRequest,
        wire: Wire<'_>,
        action: Action<'_>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use crate::evaluation::object_component_kernels::object_context_workspace::{
            ActiveContext, ContextInfo, EnterOutcome, ExitOutcome, WorkspaceState, WorkspaceView,
        };
        let document = self.document();
        let source = crate::evaluation::object_component_kernels::object_context_resolver::ContextSource::of_document(document);
        let scene_total = i64::from(document.total_frames());
        let scene_fps = self.project().map_or(24.0, |p| p.project().fps());
        let catalog: Vec<ContextInfo> = source
            .components()
            .iter()
            .map(|c| ContextInfo {
                context_id: c.symbol_id().to_owned(),
                total_frames: i64::from(c.total_frames()),
                fps: c.fps.value().and_then(serde_json::Number::as_f64),
            })
            .collect();
        let find = |context: &str| catalog.iter().find(|c| c.context_id == context);
        // The held view is clamped into the context as it is NOW: an Undo or an agent edit may have shrunk the entered
        // component (or the scene) under a playhead and work area it still holds. Leaving or closing must always work, so a
        // view beyond the frame count is brought back inside it; only a work area that is not ordered is malformed.
        let ordered = |v: &View| v.work_in <= v.work_out;
        let clamped = |v: &View, total: i64| {
            let last = (total - 1).max(0);
            View {
                frame: v.frame.min(last),
                work_in: v.work_in.min(last),
                work_out: v.work_out.min(last),
            }
        };
        let scene_state = |v: &View| WorkspaceView {
            current_frame: v.frame,
            wa_in: v.work_in,
            wa_out: v.work_out,
            active_layer_idx: 0,
            total_frames: scene_total,
            fps: scene_fps,
        };
        // The workspace the caller holds. A context that vanished (an Undo removed it) is exited first, exactly as
        // `exitToScene` does, and the action then applies to the scene workspace.
        let mut gone = false;
        let mut state = match (wire.active, &wire.scene_view) {
            (Some(active), Some(scene)) => {
                if !ordered(scene) || !ordered(&wire.view) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                let scene = clamped(scene, scene_total);
                match find(active) {
                    Some(info) => {
                        let held = clamped(&wire.view, info.total_frames);
                        WorkspaceState {
                            active: Some(ActiveContext {
                                context_id: active.to_owned(),
                                saved_scene_view: scene_state(&scene),
                            }),
                            open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                            view: WorkspaceView {
                                current_frame: held.frame,
                                wa_in: held.work_in,
                                wa_out: held.work_out,
                                active_layer_idx: 0,
                                total_frames: info.total_frames,
                                fps: info
                                    .fps
                                    .filter(|f| *f != 0.0 && !f.is_nan())
                                    .unwrap_or(scene_fps),
                            },
                        }
                    }
                    None => {
                        gone = true;
                        WorkspaceState {
                            active: None,
                            open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                            view: scene_state(&scene),
                        }
                    }
                }
            }
            _ => {
                if !ordered(&wire.view) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                WorkspaceState {
                    active: None,
                    open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                    view: scene_state(&clamped(&wire.view, scene_total)),
                }
            }
        };
        state
            .validate()
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        // The kernel clears a selection the session keeps itself (a native selection is JS view state).
        let mut selection = Vec::new();
        let outcome = match action {
            Action::Enter { id, entry, from } => {
                let entry = entry.or_else(|| from.and_then(|layer| self.instance_entry_frame(id, layer, wire.view.frame)));
                match state.enter_context(&catalog, &mut selection, id, entry) {
                    EnterOutcome::Entered => "entered",
                    EnterOutcome::AlreadyActive => "already_active",
                    EnterOutcome::RefusedNested { .. } => "refused_nested",
                    EnterOutcome::UnknownContext => return Err(DispatchErrorCode::NotFound),
                }
            }
            Action::Exit => match state.exit_context(&catalog, &mut selection) {
                ExitOutcome::Exited { .. } => "exited",
                ExitOutcome::NotInContext => "not_in_context",
            },
            Action::Close { id } => {
                if state.close_tab(&catalog, &mut selection, id).tab_removed {
                    "closed"
                } else {
                    "close_noop"
                }
            }
            Action::Refresh => {
                if gone {
                    "context_gone"
                } else {
                    "refreshed"
                }
            }
        };
        let shown = |v: &WorkspaceView| json!({"frame":v.current_frame,"workIn":v.wa_in,"workOut":v.wa_out});
        let mut tabs: Vec<String> = state
            .visible_tabs(&catalog)
            .into_iter()
            .map(|tab| tab.context_id)
            .collect();
        // The baseline strip had no cap, the wire does (MAX_TABS): a ninth tab evicts the oldest tab that is not the active one,
        // so the answer is always one the caller can send back.
        while tabs.len() > MAX_TABS {
            let active = state.active_context_id();
            match tabs.iter().position(|tab| Some(tab.as_str()) != active) {
                Some(oldest) => {
                    tabs.remove(oldest);
                }
                None => break,
            }
        }
        let result = json!({"atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "outcome":outcome,
            "contextId":state.active_context_id().unwrap_or("scene-root"),
            "totalFrames":state.view.total_frames,"fps":state.view.fps,
            "workspace":{"activeContextId":state.active_context_id(),"openTabs":tabs,
                "view":shown(&state.view),
                "sceneView":state.saved_scene_view().map(shown)}});
        let response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(result),
            error: None,
        };
        match serde_json::to_vec(&response).map(|bytes| bytes.len()) {
            Ok(0..=MAX_MESSAGE_BYTES) => Ok(response),
            Ok(_) => Err(DispatchErrorCode::Unavailable),
            Err(_) => Err(DispatchErrorCode::Internal),
        }
    }
}
