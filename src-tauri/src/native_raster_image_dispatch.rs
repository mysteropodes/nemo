//! Image bytes resolve outside the existing owner/barrier locks, then re-enter the same admission.
use crate::application_mcp::image_source::{self, ImageReadPermission, ImageSource};
use crate::application_mcp::revision_sync::{unavailable, Delivery, RevisionSync};
use crate::native_dispatch::{NativeDispatch, NativeState};
use native_engine::commands::OpacityRequest;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use std::time::Duration;

pub(super) enum ImagePhase<'a> {
    Regular,
    Probe,
    Commit(&'a [u8]),
}
pub(super) fn is_image_import(request: &NativeApplicationRequest) -> bool {
    request.operation == "command.document.object.asset.attach"
        && request.payload.get("placement").is_some()
}
pub(super) fn invoke_image(
    application: &mut dyn NativeDispatch,
    request: &NativeApplicationRequest,
    phase: ImagePhase<'_>,
) -> Result<Option<NativeApplicationResponse>, String> {
    let owner = application
        .as_any_mut()
        .downcast_mut::<crate::native_dispatch::NativeObjectHost>()
        .ok_or("native image owner unavailable")?;
    let request: OpacityRequest = serde_json::from_value(
        serde_json::to_value(request).map_err(|_| "native image request invalid")?,
    )
    .map_err(|_| "native image request invalid")?;
    let response = match phase {
        ImagePhase::Commit(bytes) => Some(owner.image_import(&request, bytes)?),
        ImagePhase::Probe | ImagePhase::Regular => owner.image_status(&request)?,
    };
    response
        .map(|value| {
            serde_json::from_value(
                serde_json::to_value(value).map_err(|_| "native image receipt invalid")?,
            )
            .map_err(|_| "native image receipt invalid".to_owned())
        })
        .transpose()
}
impl RevisionSync {
    pub(in crate::application_mcp) async fn dispatch_scoped(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        allowed: Option<ImageReadPermission>,
    ) -> Result<Delivery, String> {
        if crate::application_mcp::revision_sync::video_dispatch::is_video_link(&request) {
            return self
                .dispatch_video_scoped(instance, native, request, external, allowed)
                .await;
        }
        if crate::application_mcp::revision_sync::import_batch_dispatch::is_import_batch_sources(
            &request,
        ) {
            return self
                .dispatch_import_batch_scoped(instance, native, request, external, allowed)
                .await;
        }
        if !is_image_import(&request) {
            return self
                .dispatch_async(instance, native, request, external)
                .await;
        }
        self.dispatch_image_using(
            instance,
            native,
            request,
            external,
            move |source| async move {
                match allowed {
                    Some(allowed) => image_source::resolve(source, allowed).await,
                    None => Err("native image scoped source port is unavailable".into()),
                }
            },
        )
        .await
    }
    // The injected port supplies only transport bytes; real publication and replay stay with the owner.
    pub(super) async fn dispatch_image_using<F, Fut>(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        resolve: F,
    ) -> Result<Delivery, String>
    where
        F: FnOnce(ImageSource) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
    {
        request.validate().map_err(|error| error.to_string())?;
        if let Some(delivery) = self.dispatch_image_step(
            instance,
            native,
            request.clone(),
            external,
            ImagePhase::Probe,
        )? {
            return Ok(delivery);
        }
        let source: ImageSource = serde_json::from_value(request.payload["source"].clone())
            .map_err(|_| "native image source descriptor invalid")?;
        let bytes = match tokio::time::timeout(Duration::from_secs(30), resolve(source)).await {
            Ok(Ok(bytes)) => bytes,
            _ => {
                if let Some(known) = self.dispatch_image_step(
                    instance,
                    native,
                    request.clone(),
                    external,
                    ImagePhase::Probe,
                )? {
                    return Ok(known);
                }
                let mut response = unavailable(
                    &request,
                    instance,
                    request.expected_revision.unwrap_or(0),
                    "native image source unavailable before publication",
                    false,
                );
                response.error.as_mut().unwrap().details = Some(
                    serde_json::json!({"disposition":"indeterminate","committed":null,"retryExecution":false}),
                );
                return Ok(Delivery {
                    response,
                    notification: None,
                });
            }
        };
        self.dispatch_image_step(
            instance,
            native,
            request,
            external,
            ImagePhase::Commit(&bytes),
        )?
        .ok_or_else(|| "native image publication has no terminal disposition".into())
    }
}

#[cfg(test)]
#[path = "native_raster_image_dispatch_tests.rs"]
mod tests;
