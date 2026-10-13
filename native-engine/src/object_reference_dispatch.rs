//! File-backed reference commands share the existing history receipt store.
use crate::commands::ReferenceEdit;
use crate::history::NativeObjectHistory;
use crate::request_receipts::{
    sha256, DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};
use serde::Deserialize;

const ATTACH: &str = "command.document.reference.attach";
const SETTINGS: &str = "command.document.reference.settings";
const REMOVE: &str = "command.document.reference.remove";
const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    id: String,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Settings {
    name: String,
    visible: bool,
    opacity: f64,
    offset_frames: i64,
}

impl NativeObjectHistory {
    /// Host ingress must preserve original JSON duplicate rejection. File bytes
    /// are borrowed separately and fingerprinted here; never retained as a DTO.
    pub fn dispatch_reference_command(
        &mut self,
        request: &OpacityRequest,
        png: Option<&[u8]>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if ![ATTACH, SETTINGS, REMOVE].contains(&request.operation.as_str())
            || request.api_version != 2
            || [
                &request.request_id,
                &request.instance_id,
                &request.document_id,
            ]
            .iter()
            .any(|id| !identifier(id))
            || !request.payload.is_object()
            || serde_json::to_vec(request).map_or(true, |v| v.len() > 4096)
            || request
                .expected_revision
                .is_none_or(|v| v > 9_007_199_254_740_991)
            || (request.operation == ATTACH) != png.is_some()
            || png.is_some_and(|bytes| bytes.len() > MAX_PNG_BYTES)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        let mut fingerprint_input = request.clone();
        if let Some(bytes) = png {
            fingerprint_input.payload = serde_json::json!({"input":request.payload,
                "encodedBytes":bytes.len(), "encodedSha256":sha256(bytes)});
        }
        let fingerprint =
            RequestFingerprint::new(&fingerprint_input).ok_or(DispatchErrorCode::InvalidRequest)?;
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(disposition) => {
                return disposition
                    .response(request, self.instance_id(), self.document_id())
                    .ok_or(DispatchErrorCode::Internal)
            }
            ReceiptLookup::ChangedBody => return Err(DispatchErrorCode::InvalidRequest),
            ReceiptLookup::Missing => {}
        }
        let before = self.content_revision();
        let outcome = if request.cancelled_before_dispatch {
            Err(DispatchErrorCode::CancelledBeforeDispatch)
        } else if request.expected_revision != Some(before) {
            Err(DispatchErrorCode::StaleRevision)
        } else {
            self.apply_reference_command(request, png)
        };
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome
                .map(|after| CompactResult::Command {
                    applied: after != before,
                    history_entries_added: u64::from(after != before),
                })
                .map_err(|code| CompactFailure {
                    code,
                    message: "Native reference command refused.".into(),
                    details: None,
                }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }

    fn apply_reference_command(
        &mut self,
        request: &OpacityRequest,
        png: Option<&[u8]>,
    ) -> Result<u64, DispatchErrorCode> {
        if request.operation == ATTACH {
            let args: Attachment = serde_json::from_value(request.payload.clone())
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
            return self.attach_reference_png(
                self.content_revision(),
                args.id,
                args.name,
                png.ok_or(DispatchErrorCode::InvalidRequest)?,
            );
        }
        let edit = if request.operation == SETTINGS {
            let args: Settings = serde_json::from_value(request.payload.clone())
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
            ReferenceEdit::Settings {
                name: args.name,
                visible: args.visible,
                opacity: args.opacity,
                offset_frames: args.offset_frames,
            }
        } else {
            if request.payload != serde_json::json!({}) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            ReferenceEdit::Remove
        };
        let prepared = self.prepare_reference_edit(edit)?;
        self.commit_reference_edit(prepared)
    }
}

#[cfg(test)]
#[path = "../tests/object_reference_dispatch.rs"]
mod tests;
