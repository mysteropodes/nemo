//! Resolve only the existing file/font sources outside both native locks.
use crate::application_mcp::font_source::{self, FontBytes, FontSource};
use crate::application_mcp::image_source::{self, ImageReadPermission, ImageSource};
use crate::application_mcp::revision_sync::{unavailable, Delivery, ResourcePhase, RevisionSync};
use crate::native_dispatch::{NativeDispatch, NativeState};
use native_engine::{commands::OpacityRequest, history::ImportAssetSource};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde::Deserialize;
use std::{future::Future, time::Duration};

pub(super) fn is_import_batch_sources(request: &NativeApplicationRequest) -> bool {
    request.operation == "command.document.object.import.batch"
        && request
            .payload
            .as_object()
            .is_some_and(|payload| payload.contains_key("sources"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceDescriptor {
    source_id: String,
    name: String,
    media_type: String,
    source: serde_json::Value,
}

pub(super) enum BatchSource {
    Image(ImageSource),
    Font(FontSource),
}

pub(super) enum SourceBytes {
    Image(Vec<u8>),
    Font(FontBytes),
}
impl AsRef<[u8]> for SourceBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Image(bytes) => bytes,
            Self::Font(bytes) => bytes.as_ref(),
        }
    }
}

pub(super) struct ResolvedSource {
    source_id: String,
    name: String,
    media_type: String,
    bytes: SourceBytes,
}

pub(super) enum ImportBatchPhase<'a> {
    Probe,
    Commit(&'a [ResolvedSource]),
}

pub(super) fn invoke_import_batch(
    application: &mut dyn NativeDispatch,
    request: &NativeApplicationRequest,
    phase: ImportBatchPhase<'_>,
) -> Result<Option<NativeApplicationResponse>, String> {
    let owner = application
        .as_any_mut()
        .downcast_mut::<crate::native_dispatch::NativeObjectHost>()
        .ok_or("native import batch owner unavailable")?;
    let request: OpacityRequest = serde_json::from_value(
        serde_json::to_value(request).map_err(|_| "native import batch request invalid")?,
    )
    .map_err(|_| "native import batch request invalid")?;
    let response = match phase {
        ImportBatchPhase::Probe => owner.import_batch_sources_status(&request)?,
        ImportBatchPhase::Commit(resolved) => {
            let sources: Vec<_> = resolved
                .iter()
                .map(|item| ImportAssetSource {
                    source_id: &item.source_id,
                    name: &item.name,
                    media_type: &item.media_type,
                    bytes: item.bytes.as_ref(),
                })
                .collect();
            Some(owner.import_batch_sources(&request, &sources)?)
        }
    };
    response
        .map(|response| {
            serde_json::from_value(
                serde_json::to_value(response)
                    .map_err(|_| "native import batch receipt invalid")?,
            )
            .map_err(|_| "native import batch receipt invalid".to_owned())
        })
        .transpose()
}

async fn resolve_source(
    source: BatchSource,
    allowed: Option<ImageReadPermission>,
) -> Result<SourceBytes, String> {
    match source {
        BatchSource::Image(source) => {
            let allowed = allowed.ok_or("native import file source permission is unavailable")?;
            image_source::resolve(source, allowed)
                .await
                .map(SourceBytes::Image)
        }
        BatchSource::Font(source) => font_source::resolve(source).await.map(SourceBytes::Font),
    }
}

impl RevisionSync {
    pub(super) fn dispatch_import_batch_step(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        object_client: bool,
        phase: ImportBatchPhase<'_>,
    ) -> Result<Option<Delivery>, String> {
        self.dispatch_resource_step(
            instance,
            native,
            request,
            external,
            object_client,
            ResourcePhase::ImportBatch(phase),
        )
    }

    pub(super) async fn dispatch_import_batch_scoped(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        allowed: Option<ImageReadPermission>,
    ) -> Result<Delivery, String> {
        self.dispatch_import_batch_using(instance, native, request, external, move |source| {
            resolve_source(source, allowed.clone())
        })
        .await
    }

    // The boundary supplies bytes only: no owner or prepared candidate crosses I/O.
    pub(super) async fn dispatch_import_batch_using<F, Fut>(
        &self,
        instance: &str,
        native: &NativeState,
        request: NativeApplicationRequest,
        external: bool,
        mut resolve: F,
    ) -> Result<Delivery, String>
    where
        F: FnMut(BatchSource) -> Fut,
        Fut: Future<Output = Result<SourceBytes, String>>,
    {
        request.validate().map_err(|error| error.to_string())?;
        if let Some(delivery) = self.dispatch_import_batch_step(
            instance,
            native,
            request.clone(),
            external,
            false,
            ImportBatchPhase::Probe,
        )? {
            return Ok(delivery);
        }
        // Engine preflight has admitted the full closed request, including each source.
        let descriptors: Vec<SourceDescriptor> =
            serde_json::from_value(request.payload["sources"].clone())
                .map_err(|_| "native import source descriptors invalid")?;
        if descriptors.is_empty() {
            return Err("native import sources must be nonempty".into());
        }
        let resolved = tokio::time::timeout(Duration::from_secs(30), async {
            let mut resolved = Vec::with_capacity(descriptors.len());
            for descriptor in descriptors {
                let SourceDescriptor {
                    source_id,
                    name,
                    media_type,
                    source,
                } = descriptor;
                let source = match media_type.as_str() {
                    "image/png" => BatchSource::Image(
                        serde_json::from_value(source)
                            .map_err(|_| "native import file source invalid")?,
                    ),
                    "font/ttf" => BatchSource::Font(
                        serde_json::from_value(source)
                            .map_err(|_| "native import font source invalid")?,
                    ),
                    _ => return Err("native import source media type unavailable".to_owned()),
                };
                let bytes = resolve(source).await?;
                // Pair each result with its own original descriptor, in exact request order.
                resolved.push(ResolvedSource {
                    source_id,
                    name,
                    media_type,
                    bytes,
                });
            }
            Ok(resolved)
        })
        .await;
        let resolved = match resolved {
            Ok(Ok(resolved)) => resolved,
            _ => {
                if let Some(known) = self.dispatch_import_batch_step(
                    instance,
                    native,
                    request.clone(),
                    external,
                    false,
                    ImportBatchPhase::Probe,
                )? {
                    return Ok(known);
                }
                let mut response = unavailable(
                    &request,
                    instance,
                    request.expected_revision.unwrap_or(0),
                    "native import source unavailable before publication",
                    false,
                );
                // Timeout does not prove blocking I/O stopped, or another attempt did not commit.
                response.error.as_mut().unwrap().details = Some(serde_json::json!({
                    "disposition":"indeterminate", "committed":null, "retryExecution":false
                }));
                return Ok(Delivery {
                    response,
                    notification: None,
                });
            }
        };
        self.dispatch_import_batch_step(
            instance,
            native,
            request,
            external,
            false,
            ImportBatchPhase::Commit(&resolved),
        )?
        .ok_or_else(|| "native import batch publication has no terminal disposition".into())
    }
}
