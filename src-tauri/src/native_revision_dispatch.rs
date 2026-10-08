//! One locked dispatch core shared by typed public and bounded raw admission.
use crate::application_mcp::revision_sync::*;

impl RevisionSync {
    pub(super) fn dispatch_admitted(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        object_client: bool,
    ) -> Result<Delivery, String> {
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
        if object_client && !application.object_client_available() {
            return Ok(Delivery {
                response: unavailable(
                    &request,
                    instance,
                    application.content_revision(),
                    "bounded object client has no active object owner",
                    false,
                ),
                notification: None,
            });
        }
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
}
