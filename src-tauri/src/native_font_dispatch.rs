//! PRIVATE: source I/O outside the existing authority/barrier locks.
use crate::application_mcp::font_source::{self, FontBytes, FontSource};
use crate::application_mcp::revision_sync::{unavailable, Delivery, RevisionSync};
use crate::native_dispatch::{NativeDispatch, NativeState};
use native_engine::commands::OpacityRequest;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use std::time::Duration;

pub(super) enum FontPhase<'a> {
    Regular,
    Probe,
    Commit(&'a [u8]),
}

fn native_request(request: &NativeApplicationRequest) -> Result<OpacityRequest, String> {
    serde_json::from_value(serde_json::to_value(request).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn response(response: impl serde::Serialize) -> Result<NativeApplicationResponse, String> {
    serde_json::from_value(serde_json::to_value(response).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub(super) fn invoke_font(
    application: &mut dyn NativeDispatch,
    request: &NativeApplicationRequest,
    phase: FontPhase<'_>,
) -> Result<Option<NativeApplicationResponse>, String> {
    let owner = application
        .as_any_mut()
        .downcast_mut::<crate::native_dispatch::NativeObjectHost>()
        .ok_or("native font owner unavailable")?;
    let request = native_request(request)?;
    match phase {
        FontPhase::Commit(bytes) => response(owner.font_import(&request, bytes)?).map(Some),
        FontPhase::Probe | FontPhase::Regular => {
            owner.font_status(&request)?.map(response).transpose()
        }
    }
}

impl RevisionSync {
    pub(super) async fn dispatch_async(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
    ) -> Result<Delivery, String> {
        self.dispatch_font_using(instance, native, request, external, font_source::resolve)
            .await
    }
    // Injection supplies only baseline transport bytes. It cannot publish IDs,
    // bypass preflight, retain a writable owner or run under either mutex.
    pub(super) async fn dispatch_font_using<F, Fut>(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        resolve: F,
    ) -> Result<Delivery, String>
    where
        F: FnOnce(FontSource) -> Fut,
        Fut: std::future::Future<Output = Result<FontBytes, String>>,
    {
        request.validate().map_err(|e| e.to_string())?;
        if request.operation != font_source::IMPORT {
            return self.dispatch(instance, native, request, external);
        }
        if let Some(delivery) = self.dispatch_font_step(
            instance,
            native,
            request.clone(),
            external,
            false,
            FontPhase::Probe,
        )? {
            return Ok(delivery);
        }
        // Core preflight has already validated the complete closed payload.
        let source: FontSource = serde_json::from_value(request.payload["source"].clone())
            .map_err(|_| "invalid font source descriptor")?;
        let bytes = match tokio::time::timeout(Duration::from_secs(30), resolve(source)).await {
            Ok(Ok(bytes)) => bytes,
            _ => {
                if let Some(known) = self.dispatch_font_step(
                    instance,
                    native,
                    request.clone(),
                    external,
                    false,
                    FontPhase::Probe,
                )? {
                    return Ok(known);
                }
                let mut response = unavailable(
                    &request,
                    instance,
                    request.expected_revision.unwrap_or(0),
                    "font source unavailable before native publication",
                    false,
                );
                // Another request with this identity may still be resolving.
                // This attempt did not publish; do not guess global refusal.
                response.error.as_mut().unwrap().details = Some(serde_json::json!({
                    "disposition":"indeterminate","committed":null,"retryExecution":false}));
                return Ok(Delivery {
                    response,
                    notification: None,
                });
            }
        };
        self.dispatch_font_step(
            instance,
            native,
            request,
            external,
            false,
            FontPhase::Commit(bytes.as_ref()),
        )?
        .ok_or_else(|| "font commit produced no terminal disposition".into())
    }
}

#[cfg(test)]
#[path = "native_font_dispatch_tests.rs"]
mod tests;
