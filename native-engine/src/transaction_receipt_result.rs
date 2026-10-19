//! Compact retained receipt serialization; extracted without changing other operation results.
//! Repeat alone exposes its immutable prepared cycle/skip outcome after successful commit.
use super::{CompactResult, RetainedDisposition};
use crate::commands::{DispatchError, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION};
use serde_json::json;

impl RetainedDisposition {
    pub(crate) fn response(
        &self,
        request: &OpacityRequest,
        instance_id: &str,
        document_id: &str,
    ) -> Option<ResponseEnvelope> {
        let (content_revision, outcome) = match self {
            Self::Forwarded => return None,
            Self::Exact {
                content_revision,
                outcome,
            } => (*content_revision, outcome),
        };
        let (ok, result, error) = match outcome {
            Ok(result) => {
                let result = match result {
                    #[cfg(feature = "font_asset_wire")]
                    CompactResult::FontImported {
                        asset_id,
                        encoded_bytes,
                        encoded_sha256,
                    } => json!({
                        "applied":true,"historyEntriesAdded":1,"assetId":asset_id,
                        "mediaType":"font/ttf","encodedBytes":encoded_bytes,"encodedSha256":encoded_sha256
                    }),
                    CompactResult::Command {
                        applied,
                        history_entries_added,
                    } => json!({
                        "applied": applied,
                        "historyEntriesAdded": history_entries_added
                    }),
                    CompactResult::FrameClipboardCut {
                        applied,
                        history_entries_added,
                        clipboard,
                    } => json!({
                        "applied": applied,
                        "historyEntriesAdded": history_entries_added,
                        "clipboard": clipboard
                    }),
                    CompactResult::FrameRepeat {
                        applied,
                        history_entries_added,
                        cycled_layers,
                        skipped_component_layers,
                    } => json!({
                        "applied": applied,
                        "historyEntriesAdded": history_entries_added,
                        "cycledLayers": cycled_layers,
                        "skippedComponentLayers": skipped_component_layers
                    }),
                    CompactResult::LayerCreated { layer_uid } => json!({
                        "applied": true, "historyEntriesAdded": 1, "layerUid": layer_uid
                    }),
                    CompactResult::VectorEdit(vector) => vector.result(),
                    #[cfg(feature = "project_media_codec")]
                    CompactResult::ImageImported(image) => image.result(),
                    CompactResult::ItemEdit(item) => item.result(),
                    CompactResult::ImportBatch(batch) => batch.result(),
                    CompactResult::Transaction(record) => record.result(),
                };
                (true, Some(result), None)
            }
            Err(failure) => (
                false,
                None,
                Some(DispatchError {
                    code: failure.code,
                    message: failure.message.clone(),
                    details: failure.details.clone(),
                }),
            ),
        };
        Some(ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: instance_id.to_owned(),
            document_id: document_id.to_owned(),
            content_revision,
            ok,
            result,
            error,
        })
    }
}
