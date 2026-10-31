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
        if is_import_batch_sources(&request) {
            return self
                .dispatch_import_batch_step(
                    instance,
                    native,
                    request,
                    external,
                    object_client,
                    ImportBatchPhase::Probe,
                )?
                .ok_or_else(|| "import batch sources require async application dispatch".into());
        }
        self.dispatch_font_step(
            instance,
            native,
            request,
            external,
            object_client,
            FontPhase::Regular,
        )?
        .ok_or_else(|| "font source requires async application dispatch".into())
    }
    pub(super) fn dispatch_font_step(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        object_client: bool,
        font_phase: FontPhase<'_>,
    ) -> Result<Option<Delivery>, String> {
        let phase = if request.operation == crate::application_mcp::font_source::IMPORT {
            ResourcePhase::Font(font_phase)
        } else {
            ResourcePhase::Regular
        };
        self.dispatch_resource_step(instance, native, request, external, object_client, phase)
    }
    pub(super) fn dispatch_image_step(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        phase: ImagePhase<'_>,
    ) -> Result<Option<Delivery>, String> {
        self.dispatch_resource_step(
            instance,
            native,
            request,
            external,
            false,
            ResourcePhase::Image(phase),
        )
    }
    pub(super) fn dispatch_resource_step(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        object_client: bool,
        resource_phase: ResourcePhase<'_>,
    ) -> Result<Option<Delivery>, String> {
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
                return Ok(Some(Delivery {
                    response: unavailable(
                        &request,
                        instance,
                        0,
                        authority.unavailable_reason(),
                        false,
                    ),
                    notification: None,
                }))
            }
        };
        let application = authority.active_mut(generation)?;
        if (request.operation == crate::application_mcp::font_source::IMPORT
            || is_image_import(&request)
            || is_video_link(&request)
            || is_import_batch_sources(&request))
            && request
                .payload
                .get("lifecycleGeneration")
                .and_then(serde_json::Value::as_u64)
                != Some(generation)
        {
            return Ok(Some(Delivery {
                response: unavailable(
                    &request,
                    instance,
                    application.content_revision(),
                    if is_import_batch_sources(&request) {
                        "native import batch lifecycle generation is stale"
                    } else if is_image_import(&request) {
                        "native image lifecycle generation is stale"
                    } else if is_video_link(&request) {
                        "native video lifecycle generation is stale"
                    } else {
                        "native font lifecycle generation is stale"
                    },
                    false,
                ),
                notification: None,
            }));
        }
        // The private raw token is not the only route into this core: the
        // versioned public API and bundled MCP also carry object operations.
        // Fence those before an unrelated active native owner sees the body.
        let object_operation = request.operation == "query.document.object"
            || request.operation.starts_with("query.document.object.")
            || request.operation.starts_with("command.document.object.");
        if (object_client || object_operation) && !application.object_client_available() {
            return Ok(Some(Delivery {
                response: unavailable(
                    &request,
                    instance,
                    application.content_revision(),
                    if object_client {
                        "bounded object client has no active object owner"
                    } else {
                        "native object operation has no active object owner"
                    },
                    false,
                ),
                notification: None,
            }));
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
                        return Ok(Some(Delivery {
                            response,
                            notification: None,
                        }));
                    }
                }
                if retained || !request.cancelled_before_dispatch {
                    return Ok(Some(Delivery { response: unavailable(&request, instance, before,
                    "native revision synchronization is pending or indeterminate; do not execute again; release before re-entry",
                    retained), notification: None }));
                }
            }
            if !request.cancelled_before_dispatch
                && state.motion_reservation.as_ref().is_some_and(|reserved| {
                    reserved.matches_owner(
                        generation,
                        application.instance_id(),
                        application.document_id(),
                        before,
                    )
                })
            {
                let retained = state
                    .motion_reservation
                    .as_ref()
                    .is_some_and(|reserved| reserved.retains_request(&request.request_id));
                if retained {
                    let response = invoke_dispatch(application, &request)?;
                    if !response.ok {
                        return Ok(Some(Delivery {
                            response,
                            notification: None,
                        }));
                    }
                }
                return Ok(Some(Delivery { response: unavailable(&request, instance, before,
                    "native retained Motion effects await trusted completion; command was not dispatched",
                    retained), notification: None }));
            }
            if external && state.subscriber.is_none() && !request.cancelled_before_dispatch {
                return Ok(Some(Delivery {
                    response: unavailable(
                        &request,
                        instance,
                        before,
                        "native revision subscriber is absent; command was not dispatched",
                        false,
                    ),
                    notification: None,
                }));
            }
        }
        let response = match resource_phase {
            ResourcePhase::Font(phase) => match invoke_font(application, &request, phase)? {
                Some(response) => response,
                None => return Ok(None),
            },
            ResourcePhase::Image(phase) => match invoke_image(application, &request, phase)? {
                Some(response) => response,
                None => return Ok(None),
            },
            ResourcePhase::Video(phase) => match invoke_video(application, &request, phase)? {
                Some(response) => response,
                None => return Ok(None),
            },
            ResourcePhase::ImportBatch(phase) => {
                match invoke_import_batch(application, &request, phase)? {
                    Some(response) => response,
                    None => return Ok(None),
                }
            }
            ResourcePhase::Regular => invoke_dispatch(application, &request)?,
        };
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
            Some(state.begin_event(event))
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
        Ok(Some(Delivery {
            response,
            notification,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application_mcp::ApplicationMcp, native_dispatch::TerminalPump};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn public_object_operations_refuse_non_object_owner_before_dispatch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut state = ApplicationMcp::default();
        state.instance_id = "pump-fixture".into();
        let reservation = state.reserve_native_install().unwrap();
        let generation = reservation.generation();
        state
            .install_dispatch(generation, Box::new(TerminalPump(calls.clone())))
            .unwrap();
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut read_payload = cases["read"]["request"]["payload"].clone();
        read_payload["atRevision"] = serde_json::json!(0);
        for (operation, expected_revision, payload, external) in [
            ("query.document.object", None, read_payload, false),
            (
                "command.document.object.layer.visibility.set",
                Some(0),
                serde_json::json!({"command":"object.layer.visibility.set",
                    "layerUid":"legacy layer:alpha","visible":false}),
                true,
            ),
        ] {
            let request = NativeApplicationRequest {
                api_version: NATIVE_API_VERSION,
                request_id: format!("wrong-owner-{operation}"),
                instance_id: state.instance_id().into(),
                document_id: "document-fixture".into(),
                expected_revision,
                operation: operation.into(),
                payload,
                cancelled_before_dispatch: false,
            };
            assert!(request.validate_input().is_ok());
            let delivery = state
                .revisions
                .dispatch_admitted(state.instance_id(), &state.native, request, external, false)
                .unwrap();
            assert!(delivery.notification.is_none());
            assert!(!delivery.response.ok);
            let error = delivery.response.error.unwrap();
            assert_eq!(error.code, "unavailable");
            assert_eq!(error.details.unwrap()["disposition"], "notDispatched");
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let authority = state.native.lock().unwrap();
            assert_eq!(authority.active_generation(), Ok(generation));
            assert_eq!(authority.active().unwrap().1.content_revision(), 0);
        }
        let ordinary = NativeApplicationRequest {
            api_version: NATIVE_API_VERSION,
            request_id: "ordinary-query".into(),
            instance_id: state.instance_id().into(),
            document_id: "document-fixture".into(),
            expected_revision: None,
            operation: "query.document.revision".into(),
            payload: serde_json::json!({}),
            cancelled_before_dispatch: false,
        };
        assert_eq!(
            state
                .revisions
                .dispatch_admitted(state.instance_id(), &state.native, ordinary, false, false)
                .err()
                .unwrap(),
            "terminal pump has no document dispatcher"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
#[path = "native_property_metadata_dispatch_tests.rs"]
mod metadata_tests;

#[cfg(test)]
#[path = "native_layer_policy_dispatch_tests.rs"]
mod layer_policy_tests;

#[cfg(test)]
#[path = "native_property_track_dispatch_tests.rs"]
mod property_track_tests;

#[cfg(test)]
#[path = "native_property_read_dispatch_tests.rs"]
mod property_read_tests;

#[cfg(test)]
#[path = "native_property_edit_dispatch_tests.rs"]
mod property_edit_tests;

#[cfg(test)]
#[path = "native_property_bake_dispatch_tests.rs"]
mod property_bake_tests;

#[cfg(test)]
#[path = "native_property_expression_dispatch_tests.rs"]
mod property_expression_tests;

#[cfg(test)]
#[path = "native_image_mesh_binding_public_tests.rs"]
mod image_mesh_binding_public_tests;

#[cfg(test)]
#[path = "native_composition_dispatch_tests.rs"]
mod composition_tests;

#[cfg(test)]
#[path = "native_shadow_dispatch_tests.rs"]
mod shadow_tests;

#[cfg(test)]
#[path = "native_perspective_dispatch_tests.rs"]
mod perspective_tests;

#[cfg(test)]
#[path = "native_workspace_dispatch_tests.rs"]
mod workspace_tests;

#[cfg(test)]
#[path = "native_custom_brush_dispatch_tests.rs"]
mod custom_brush_tests;

#[cfg(test)]
#[path = "native_comments_dispatch_tests.rs"]
mod comments_tests;

#[cfg(test)]
#[path = "native_import_batch_dispatch_tests.rs"]
mod import_batch_tests;
