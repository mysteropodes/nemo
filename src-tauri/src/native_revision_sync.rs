//! Private MCP revision barrier. Native state remains the sole document authority.
use crate::application_mcp::NativeApplicationError;
use crate::native_dispatch::{spawn_export_pump, NativeDispatch, NativeState};
use native_engine::{commands::OpacityRequest, protocol::OP_JOB_EXPORT_PNG_BEGIN};
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeApplicationResponse, NATIVE_API_VERSION},
    wire,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::AsyncReadExt, net::TcpStream, sync::oneshot};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RevisionBinding {
    instance_id: String,
    document_id: String,
    lifecycle_generation: u64,
    content_revision: u64,
    subscription_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RevisionEvent {
    instance_id: String,
    document_id: String,
    lifecycle_generation: u64,
    from_revision: u64,
    to_revision: u64,
    request_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum RevisionControl {
    Binding,
    Subscribe {
        binding: RevisionBinding,
    },
    Acknowledge {
        #[serde(rename = "subscriptionId")]
        subscription_id: String,
        event: RevisionEvent,
    },
    Disconnect {
        #[serde(rename = "subscriptionId")]
        subscription_id: String,
    },
}

struct PendingRevision {
    event: RevisionEvent,
    sender: Option<oneshot::Sender<()>>,
    acknowledged: bool,
    failed: bool,
}

#[derive(Default)]
struct RevisionState {
    identity: Option<(u64, String)>,
    subscriber: Option<RevisionBinding>,
    pending: Option<PendingRevision>,
}

#[derive(Clone, Default)]
pub(super) struct RevisionSync(Arc<Mutex<RevisionState>>);

pub(super) struct Delivery {
    pub(super) response: NativeApplicationResponse,
    notification: Option<(RevisionEvent, oneshot::Receiver<()>)>,
}

impl RevisionState {
    fn reconcile(&mut self, generation: u64, document: &str) {
        let identity = (generation, document.to_owned());
        if self.identity.as_ref() != Some(&identity) {
            *self = Self {
                identity: Some(identity),
                ..Self::default()
            };
        }
    }
}

impl RevisionSync {
    pub(super) fn invalidate(&self) -> Option<String> {
        let mut state = self.0.lock().ok()?;
        let subscriber = state
            .subscriber
            .take()
            .map(|binding| binding.subscription_id);
        *state = RevisionState::default(); // Drops the old waiter before a later generation can register.
        subscriber
    }

    pub(super) fn control(
        &self,
        native: &NativeState,
        request: RevisionControl,
    ) -> Result<serde_json::Value, String> {
        let authority = native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| "native revision lock unavailable")?;
        if let RevisionControl::Disconnect { subscription_id } = &request {
            if state
                .subscriber
                .as_ref()
                .map(|binding| &binding.subscription_id)
                != Some(subscription_id)
            {
                return Err("native revision subscriber is stale".into());
            }
            state.subscriber = None;
            if let Some(pending) = state.pending.as_mut() {
                pending.failed = true;
                pending.sender = None;
            }
            return Ok(serde_json::Value::Null);
        }
        let (generation, application) = authority
            .active()
            .ok_or("native application is unavailable")?;
        state.reconcile(generation, application.document_id());
        let current = RevisionBinding {
            instance_id: application.instance_id().into(),
            document_id: application.document_id().into(),
            lifecycle_generation: generation,
            content_revision: application.content_revision(),
            subscription_id: uuid::Uuid::new_v4().to_string(),
        };
        match request {
            RevisionControl::Binding => {
                serde_json::to_value(current).map_err(|error| error.to_string())
            }
            RevisionControl::Subscribe { binding } => {
                let expected = RevisionBinding {
                    subscription_id: binding.subscription_id.clone(),
                    ..current
                };
                if binding != expected
                    || binding.subscription_id.is_empty()
                    || binding.subscription_id.len() > 128
                {
                    return Err("native revision subscription binding is stale or malformed".into());
                }
                if state.subscriber.is_some() || state.pending.is_some() {
                    return Err("native revision subscription is occupied or indeterminate; release before re-entry".into());
                }
                state.subscriber = Some(binding.clone());
                serde_json::to_value(binding).map_err(|error| error.to_string())
            }
            RevisionControl::Acknowledge {
                subscription_id,
                event,
            } => {
                let binding = state
                    .subscriber
                    .as_ref()
                    .ok_or("native revision subscriber is absent")?;
                if subscription_id != binding.subscription_id
                    || event.instance_id != current.instance_id
                    || event.document_id != current.document_id
                    || event.lifecycle_generation != generation
                    || event.to_revision != current.content_revision
                {
                    return Err("native revision acknowledgment identity is stale".into());
                }
                let pending = state
                    .pending
                    .as_mut()
                    .ok_or("native revision acknowledgment has no pending event")?;
                if pending.event != event || pending.failed || pending.acknowledged {
                    return Err(
                        "native revision acknowledgment is duplicate or out of order".into(),
                    );
                }
                pending.acknowledged = true;
                if pending
                    .sender
                    .take()
                    .ok_or("native revision waiter is absent")?
                    .send(())
                    .is_err()
                {
                    pending.failed = true;
                    return Err(
                        "native revision waiter was disconnected; synchronization is indeterminate"
                            .into(),
                    );
                }
                Ok(serde_json::Value::Null)
            }
            RevisionControl::Disconnect { .. } => unreachable!(),
        }
    }

    pub(super) fn dispatch(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
    ) -> Result<Delivery, String> {
        request.validate().map_err(|error| error.to_string())?;
        let mut authority = native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| "native revision lock unavailable")?;
        let generation = match authority.active_generation() {
            Ok(generation) => generation,
            Err(_) => {
                return Ok(Delivery {
                    response: unavailable(
                        &request,
                        instance,
                        0,
                        authority.unavailable_reason(),
                        false,
                    ),
                    notification: None,
                })
            }
        };
        let application = authority.active_mut(generation)?;
        state.reconcile(generation, application.document_id());
        let before = application.content_revision();
        let potential_advance = request.operation.starts_with("command.")
            || request.operation.starts_with("history.")
            || request.operation == "transaction.commit";
        let valid_identity =
            request.instance_id == instance && request.document_id == application.document_id();
        if potential_advance && valid_identity {
            if let Some(pending) = &state.pending {
                let retained = pending.event.request_id == request.request_id;
                if retained {
                    // This identity has already committed. Only the authority may classify its body.
                    let response = invoke_dispatch(application, &request)?;
                    if !response.ok {
                        return Ok(Delivery {
                            response,
                            notification: None,
                        });
                    }
                }
                if retained || !request.cancelled_before_dispatch {
                    return Ok(Delivery { response: unavailable(&request, instance, before,
                    "native revision synchronization is pending or indeterminate; do not execute again; release before re-entry",
                    retained), notification: None });
                }
            }
            if external && state.subscriber.is_none() && !request.cancelled_before_dispatch {
                return Ok(Delivery {
                    response: unavailable(
                        &request,
                        instance,
                        before,
                        "native revision subscriber is absent; command was not dispatched",
                        false,
                    ),
                    notification: None,
                });
            }
        }
        let response = invoke_dispatch(application, &request)?;
        let after = application.content_revision();
        let notification = if external && response.ok && after > before {
            let event = RevisionEvent {
                instance_id: instance.into(),
                document_id: application.document_id().into(),
                lifecycle_generation: generation,
                from_revision: before,
                to_revision: after,
                request_id: request.request_id,
            };
            let (sender, receiver) = oneshot::channel();
            state.pending = Some(PendingRevision {
                event: event.clone(),
                sender: Some(sender),
                acknowledged: false,
                failed: false,
            });
            Some((event, receiver))
        } else {
            None
        };
        drop(state);
        drop(authority);
        if request.operation == OP_JOB_EXPORT_PNG_BEGIN {
            if let Some(job_id) = response
                .result
                .as_ref()
                .filter(|_| response.ok)
                .and_then(|result| result.get("jobId"))
                .and_then(serde_json::Value::as_str)
            {
                spawn_export_pump(Arc::clone(native), generation, job_id.to_owned());
            }
        }
        Ok(Delivery {
            response,
            notification,
        })
    }

    fn settle(&self, native: &NativeState, event: &RevisionEvent, acknowledged: bool) -> bool {
        let Ok(authority) = native.lock() else {
            return false;
        };
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        let live = authority.active().is_some_and(|(generation, application)| {
            generation == event.lifecycle_generation
                && application.document_id() == event.document_id
                && application.content_revision() == event.to_revision
        });
        let Some(pending) = state
            .pending
            .as_mut()
            .filter(|pending| pending.event == *event)
        else {
            return false;
        };
        if live && acknowledged && pending.acknowledged && !pending.failed {
            state.pending = None;
            true
        } else {
            pending.failed = true;
            pending.sender = None;
            false
        }
    }
}

fn invoke_dispatch(
    application: &mut dyn NativeDispatch,
    request: &NativeApplicationRequest,
) -> Result<NativeApplicationResponse, String> {
    let native_request: OpacityRequest =
        serde_json::from_value(serde_json::to_value(request).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    serde_json::from_value(
        serde_json::to_value(application.dispatch(native_request))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn unavailable(
    request: &NativeApplicationRequest,
    instance: &str,
    revision: u64,
    message: &str,
    committed: bool,
) -> NativeApplicationResponse {
    NativeApplicationResponse {
        api_version: NATIVE_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: instance.into(),
        document_id: request.document_id.clone(),
        content_revision: revision,
        ok: false,
        result: None,
        error: Some(NativeApplicationError {
            code: "unavailable".into(),
            message: message.into(),
            details: Some(
                serde_json::json!({"disposition": if committed { "indeterminate" } else { "notDispatched" }, "committed": committed, "retryExecution": false}),
            ),
        }),
    }
}

pub(super) async fn serve_native(
    stream: &mut TcpStream,
    instance: &str,
    native: &NativeState,
    sync: &RevisionSync,
    request: NativeApplicationRequest,
    emit: impl FnOnce(RevisionEvent) -> Result<(), String>,
    timeout: Duration,
) -> Result<(), String> {
    let mut byte = [0u8; 1];
    match stream.try_read(&mut byte) {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        _ => return Err("native connection cancelled before dispatch".into()),
    }
    let mut delivery = sync.dispatch(instance, native, request.clone(), true)?;
    if let Some((event, receiver)) = delivery.notification {
        let acknowledged = if emit(event.clone()).is_ok() {
            tokio::select! {
                result = receiver => result.is_ok(),
                _ = stream.read(&mut byte) => false,
                _ = tokio::time::sleep(timeout) => false,
            }
        } else {
            false
        };
        if !sync.settle(native, &event, acknowledged) {
            delivery.response = unavailable(&request, instance, event.to_revision,
                "native command committed but consumer synchronization is indeterminate; do not execute again; release before re-entry", true);
        }
    }
    wire::write_json(stream, &delivery.response)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(super) fn dispatch_native(
    instance: &str,
    native: &NativeState,
    request: NativeApplicationRequest,
) -> Result<NativeApplicationResponse, String> {
    RevisionSync::default()
        .dispatch(instance, native, request, false)
        .map(|delivery| delivery.response)
}

#[cfg(test)]
#[path = "native_revision_sync_tests.rs"]
mod tests;
