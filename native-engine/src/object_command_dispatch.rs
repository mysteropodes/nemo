//! Staged actual-owner command dispatch. Public capability admission stays closed.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
    APPLICATION_API_VERSION,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};

const OP_OBJECT_FILL: &str = "command.document.object.fill.set";
const OP_OBJECT_UNDO: &str = "command.document.object.undo";
const OP_OBJECT_REDO: &str = "command.document.object.redo";
const MAX_MESSAGE_BYTES: usize = 4096;
const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

impl NativeObjectHistory {
    /// Dispatch only the distinct staged fill operation in this actual owner.
    /// `None` leaves other operations to their existing host route. This typed
    /// entry needs original-byte ingress before deserialization; it cannot
    /// detect duplicate raw members already discarded into a Value.
    pub fn try_dispatch_object_fill(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if request.operation != OP_OBJECT_FILL {
            return Ok(None);
        }
        self.try_dispatch_object_command(request)
    }

    /// Dispatch staged fill and closed undo/redo commands through one owner and
    /// receipt store. Typed input still requires the unchanged raw ingress gate.
    pub fn try_dispatch_object_command(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if !matches!(
            request.operation.as_str(),
            OP_OBJECT_FILL | OP_OBJECT_UNDO | OP_OBJECT_REDO
        ) {
            return Ok(None);
        }
        if [
            request.request_id.as_str(),
            request.instance_id.as_str(),
            request.document_id.as_str(),
            self.instance_id(),
            self.document_id(),
        ]
        .iter()
        .any(|id| !identifier(id))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let reject = |owner: &Self, code, message| {
            owner.object_reply(request, &owner.object_failure(code, message))
        };
        if request.api_version != APPLICATION_API_VERSION
            || !request.payload.is_object()
            || !matches!(
                serde_json::to_vec(request).map(|bytes| bytes.len()),
                Ok(0..=MAX_MESSAGE_BYTES)
            )
            || request
                .expected_revision
                .is_some_and(|revision| revision > MAX_SAFE_REVISION)
        {
            return reject(
                self,
                DispatchErrorCode::InvalidRequest,
                "Invalid bounded object command request.",
            )
            .map(Some);
        }
        if request.instance_id != self.instance_id() {
            return reject(
                self,
                DispatchErrorCode::WrongInstance,
                "Object command targets a different instance.",
            )
            .map(Some);
        }
        if request.document_id != self.document_id() {
            return reject(
                self,
                DispatchErrorCode::WrongDocument,
                "Object command targets a replaced document.",
            )
            .map(Some);
        }
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(disposition) => {
                return self.object_reply(request, disposition).map(Some)
            }
            ReceiptLookup::ChangedBody => {
                return reject(
                    self,
                    DispatchErrorCode::InvalidRequest,
                    "Object command requestId was reused with a changed body.",
                )
                .map(Some)
            }
            ReceiptLookup::Missing => {}
        }
        let outcome = if request.cancelled_before_dispatch {
            Err((
                DispatchErrorCode::CancelledBeforeDispatch,
                "Object command was cancelled before dispatch.",
            ))
        } else if request.expected_revision.is_none() {
            Err((
                DispatchErrorCode::InvalidRequest,
                "Object command requires expectedRevision.",
            ))
        } else if request.expected_revision != Some(self.content_revision()) {
            Err((
                DispatchErrorCode::StaleRevision,
                "Object command expected revision is stale.",
            ))
        } else {
            if request.operation == OP_OBJECT_FILL {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                match self.prepare_fill_json(&payload) {
                    Ok(candidate) => {
                        let before = self.content_revision();
                        self.commit_fill(candidate)
                            .map(|revision| CompactResult::Command {
                                applied: revision != before,
                                history_entries_added: u64::from(revision != before),
                            })
                            .map_err(|code| (code, "Object fill could not commit."))
                    }
                    Err(code) => Err((code, "Object fill payload is invalid or target is absent.")),
                }
            } else {
                let undo = request.operation == OP_OBJECT_UNDO;
                let command = if undo { "object.undo" } else { "object.redo" };
                if request.payload != serde_json::json!({"command":command}) {
                    Err((
                        DispatchErrorCode::InvalidRequest,
                        "Object history payload is invalid.",
                    ))
                } else {
                    let revision = request
                        .expected_revision
                        .ok_or(DispatchErrorCode::InvalidRequest)?;
                    let moved = if undo {
                        self.undo(revision)
                    } else {
                        self.redo(revision)
                    };
                    moved
                        .map(|_| CompactResult::Command {
                            applied: true,
                            history_entries_added: 0,
                        })
                        .map_err(|code| (code, "Object history could not move."))
                }
            }
        };
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome.map_err(|(code, message)| CompactFailure {
                code,
                message: message.into(),
                details: None,
            }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        self.object_reply(request, &disposition).map(Some)
    }

    fn object_failure(&self, code: DispatchErrorCode, message: &str) -> RetainedDisposition {
        RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: Err(CompactFailure {
                code,
                message: message.into(),
                details: None,
            }),
        }
    }

    fn object_reply(
        &self,
        request: &OpacityRequest,
        disposition: &RetainedDisposition,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let response = disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)?;
        if !matches!(
            serde_json::to_vec(&response).map(|bytes| bytes.len()),
            Ok(0..=MAX_MESSAGE_BYTES)
        ) {
            return Err(DispatchErrorCode::Internal);
        }
        Ok(response)
    }
}
