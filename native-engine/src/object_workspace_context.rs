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
    Enter { id: &'a str, entry: Option<i64> },
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
            let object = exact(value, &["kind", "contextId", "entryFrame"])?;
            let entry = match &object["entryFrame"] {
                Value::Null => None,
                other => Some(uint32(other)?),
            };
            Some(Action::Enter {
                id: id(&object["contextId"])?,
                entry,
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
        let bounded =
            |v: &View, total: i64| v.frame < total && v.work_in <= v.work_out && v.work_out < total;
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
                if !bounded(scene, scene_total) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                match find(active) {
                    Some(info) if bounded(&wire.view, info.total_frames) => WorkspaceState {
                        active: Some(ActiveContext {
                            context_id: active.to_owned(),
                            saved_scene_view: scene_state(scene),
                        }),
                        open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                        view: WorkspaceView {
                            current_frame: wire.view.frame,
                            wa_in: wire.view.work_in,
                            wa_out: wire.view.work_out,
                            active_layer_idx: 0,
                            total_frames: info.total_frames,
                            fps: info
                                .fps
                                .filter(|f| *f != 0.0 && !f.is_nan())
                                .unwrap_or(scene_fps),
                        },
                    },
                    Some(_) => return Err(DispatchErrorCode::InvalidRequest),
                    None => {
                        gone = true;
                        WorkspaceState {
                            active: None,
                            open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                            view: scene_state(scene),
                        }
                    }
                }
            }
            _ => {
                if !bounded(&wire.view, scene_total) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                WorkspaceState {
                    active: None,
                    open_tabs: wire.tabs.iter().map(|t| (*t).to_owned()).collect(),
                    view: scene_state(&wire.view),
                }
            }
        };
        state
            .validate()
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        // The kernel clears a selection the session keeps itself (a native selection is JS view state).
        let mut selection = Vec::new();
        let outcome = match action {
            Action::Enter { id, entry } => {
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
        let tabs: Vec<String> = state
            .visible_tabs(&catalog)
            .into_iter()
            .map(|tab| tab.context_id)
            .collect();
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
