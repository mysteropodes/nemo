//! T12: linked-video facts are probed outside the owner/barrier locks, then re-enter the
//! same admission. The probe phase (replay, changed body, stale, cancel, replace target)
//! runs before any file access, so an exact replay never touches the file again.
use crate::application_mcp::image_source::ImageReadPermission;
use crate::application_mcp::revision_sync::{unavailable, Delivery, ResourcePhase, RevisionSync};
use crate::native_dispatch::{NativeDispatch, NativeState};
use native_engine::commands::{LinkedVideoFacts, OpacityRequest};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use std::time::Duration;

#[path = "native_video_link_source.rs"]
mod video_source;
use video_source::VideoSource;

pub(super) const VIDEO_LINK: &str = "command.document.object.asset.video.link";
const PROBE_LIMIT: Duration = Duration::from_secs(30);

pub(super) enum VideoPhase<'a> {
    Regular,
    Probe,
    Commit(&'a LinkedVideoFacts),
}
pub(super) fn is_video_link(request: &NativeApplicationRequest) -> bool {
    request.operation == VIDEO_LINK
}
pub(super) fn invoke_video(
    application: &mut dyn NativeDispatch,
    request: &NativeApplicationRequest,
    phase: VideoPhase<'_>,
) -> Result<Option<NativeApplicationResponse>, String> {
    let owner = application
        .as_any_mut()
        .downcast_mut::<crate::native_dispatch::NativeObjectHost>()
        .ok_or("native video owner unavailable")?;
    let request: OpacityRequest = serde_json::from_value(
        serde_json::to_value(request).map_err(|_| "native video request invalid")?,
    )
    .map_err(|_| "native video request invalid")?;
    let response = match phase {
        VideoPhase::Commit(facts) => Some(owner.video_link(&request, facts)?),
        VideoPhase::Probe | VideoPhase::Regular => owner.video_link_status(&request)?,
    };
    response
        .map(|value| {
            serde_json::from_value(
                serde_json::to_value(value).map_err(|_| "native video receipt invalid")?,
            )
            .map_err(|_| "native video receipt invalid".to_owned())
        })
        .transpose()
}

fn not_published(
    request: &NativeApplicationRequest,
    instance: &str,
    message: &str,
    indeterminate: bool,
) -> Delivery {
    let mut response = unavailable(
        request,
        instance,
        request.expected_revision.unwrap_or(0),
        message,
        false,
    );
    if indeterminate {
        response.error.as_mut().unwrap().details = Some(
            serde_json::json!({"disposition":"indeterminate","committed":null,"retryExecution":false}),
        );
    }
    Delivery {
        response,
        notification: None,
    }
}

impl RevisionSync {
    pub(super) async fn dispatch_video_scoped(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        allowed: Option<ImageReadPermission>,
    ) -> Result<Delivery, String> {
        self.dispatch_video_using(
            instance,
            native,
            request,
            external,
            move |source| async move {
                match allowed {
                    Some(allowed) => video_source::resolve(source, allowed).await,
                    None => Err("native video scoped source port is unavailable".into()),
                }
            },
        )
        .await
    }
    // The injected port supplies only probed facts; publication and replay stay with the owner.
    pub(super) async fn dispatch_video_using<F, Fut>(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        resolve: F,
    ) -> Result<Delivery, String>
    where
        F: FnOnce(VideoSource) -> Fut,
        Fut: std::future::Future<Output = Result<LinkedVideoFacts, String>>,
    {
        self.dispatch_video_within(instance, native, request, external, PROBE_LIMIT, resolve)
            .await
    }
    pub(super) async fn dispatch_video_within<F, Fut>(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        limit: Duration,
        resolve: F,
    ) -> Result<Delivery, String>
    where
        F: FnOnce(VideoSource) -> Fut,
        Fut: std::future::Future<Output = Result<LinkedVideoFacts, String>>,
    {
        request.validate().map_err(|error| error.to_string())?;
        let step = |request: NativeApplicationRequest, phase: VideoPhase<'_>| {
            self.dispatch_resource_step(
                instance,
                native,
                request,
                external,
                false,
                ResourcePhase::Video(phase),
            )
        };
        if let Some(delivery) = step(request.clone(), VideoPhase::Probe)? {
            return Ok(delivery);
        }
        let source: VideoSource = serde_json::from_value(request.payload["source"].clone())
            .map_err(|_| "native video source descriptor invalid")?;
        let probed = tokio::time::timeout(limit, resolve(source)).await;
        let facts = match probed {
            Ok(Ok(facts)) => facts,
            failed => {
                // A concurrent identical request may have committed meanwhile.
                if let Some(known) = step(request.clone(), VideoPhase::Probe)? {
                    return Ok(known);
                }
                return Ok(match failed {
                    Err(_) => not_published(
                        &request,
                        instance,
                        "native video source probe timed out before publication",
                        true,
                    ),
                    _ => not_published(
                        &request,
                        instance,
                        "native video source refused before publication",
                        false,
                    ),
                });
            }
        };
        step(request, VideoPhase::Commit(&facts))?
            .ok_or_else(|| "native video publication has no terminal disposition".into())
    }
}

#[cfg(test)]
#[path = "native_video_link_dispatch_tests.rs"]
mod tests;
