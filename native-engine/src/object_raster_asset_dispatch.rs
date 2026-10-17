//! Asset attach / remove commands of the one object history owner (L06-H,
//! G01 / G02 / G27): envelope validation, request replay, origin pins, one history
//! entry per applied edit, typed refusals that publish nothing.
//!
//! Two commands, both on the existing `OpacityRequest` envelope (api version 2) and
//! the existing `StageReceipts` store - there is no second ledger:
//!
//! * `command.document.object.asset.attach` - payload `{documentSnapshotId, name,
//!   mediaType}`; the file bytes are borrowed OUT OF BAND (never a DTO), exactly as
//!   `dispatch_reference_command` borrows its PNG, and are fingerprinted by length
//!   and SHA-256. The asset id is minted from the request id by the admission kernel.
//! * `command.document.object.asset.remove` - payload `{documentSnapshotId, assetId}`.
//!
//! Order: envelope shape -> wrong instance / document -> replay lookup (a changed
//! body under a retained request id is refused) -> pre-dispatch cancellation ->
//! `expectedRevision` -> snapshot pin -> sealed preparation -> optional mid-flight
//! cancellation probe -> sealed owner commit -> history. A failure at any point
//! before the commit leaves revision, history, redo and every budget untouched; a
//! successful commit that changes nothing (exact duplicate, removal of an absent
//! id) adds no history entry and keeps redo. The outcome, applied or refused, is
//! retained under the request id, so an exact retry - even after undo - returns the
//! stored response and does not execute again.
//!
//! A request that names a layer `target` (an asset placement) is refused as typed
//! `layer_policy_unavailable`: placements are L06-I and the lock/shy policy is the
//! F06 provider; this file never assumes a target unlocked and never ignores it. An
//! explicit `"target": null` is NOT the same as an absent member: it is a typed
//! `InvalidRequest` (reason `target`), never silently treated as "no target".
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{
    sha256, DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::revision::ObjectSnapshot;
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};
use serde::Deserialize;
use serde_json::{json, Value};

const ATTACH: &str = "command.document.object.asset.attach";
const REMOVE: &str = "command.document.object.asset.remove";
const MAX_MEDIA_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAFE: u64 = 9_007_199_254_740_991;

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}

/// Keeps an explicit JSON null distinct from an absent member (`Some(Value::Null)`).
fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Attach {
    document_snapshot_id: String,
    name: String,
    media_type: String,
    #[serde(default, deserialize_with = "present")]
    target: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Remove {
    document_snapshot_id: String,
    asset_id: String,
}
enum Command {
    Attach(Attach),
    Remove(Remove),
}

/// A dispatch code and, for the typed asset refusals, a stable machine reason.
type Failure = (DispatchErrorCode, Option<&'static str>);
fn plain(code: DispatchErrorCode) -> Failure {
    (code, None)
}
fn typed(code: DispatchErrorCode, reason: &'static str) -> Failure {
    (code, Some(reason))
}

fn parse(request: &OpacityRequest) -> Result<Command, Failure> {
    let payload = request.payload.clone();
    let parsed = if request.operation == ATTACH {
        serde_json::from_value(payload).map(Command::Attach)
    } else {
        serde_json::from_value(payload).map(Command::Remove)
    };
    parsed.map_err(|_| plain(DispatchErrorCode::InvalidRequest))
}

impl NativeObjectHistory {
    /// Attach (`media` required) or remove (`media` absent) one asset.
    pub fn dispatch_asset_command(
        &mut self,
        request: &OpacityRequest,
        media: Option<&[u8]>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        self.dispatch_asset_command_cancellable(request, media, &|| false)
    }

    /// `cancel` is polled once after the sealed preparation exists and before it
    /// is committed; `true` drops the preparation and refuses the command.
    pub fn dispatch_asset_command_cancellable(
        &mut self,
        request: &OpacityRequest,
        media: Option<&[u8]>,
        cancel: &dyn Fn() -> bool,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if ![ATTACH, REMOVE].contains(&request.operation.as_str())
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
            || request.expected_revision.is_none_or(|v| v > MAX_SAFE)
            || (request.operation == ATTACH) != media.is_some()
            || media.is_some_and(|bytes| bytes.len() > MAX_MEDIA_BYTES)
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
        if let Some(bytes) = media {
            fingerprint_input.payload = json!({"input":request.payload,
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
            Err(plain(DispatchErrorCode::CancelledBeforeDispatch))
        } else if request.expected_revision != Some(before) {
            Err(plain(DispatchErrorCode::StaleRevision))
        } else {
            self.apply_asset_command(request, media, before, cancel)
        };
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome
                .map(|after| CompactResult::Command {
                    applied: after != before,
                    history_entries_added: u64::from(after != before),
                })
                .map_err(|(code, reason)| CompactFailure {
                    code,
                    message: "Native asset command refused.".into(),
                    details: reason.map(|reason| json!({ "reason": reason })),
                }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }

    fn asset_origin(&self, revision: u64, snapshot_id: &str) -> Result<ObjectSnapshot, Failure> {
        let snapshot = self
            .acquire_snapshot(revision)
            .ok_or(plain(DispatchErrorCode::Unavailable))?;
        if snapshot.snapshot_id() != snapshot_id {
            return Err(plain(DispatchErrorCode::StaleRevision));
        }
        if snapshot.project().is_none() {
            return Err(typed(DispatchErrorCode::Unavailable, "bare_drawing"));
        }
        Ok(snapshot)
    }

    fn apply_asset_command(
        &mut self,
        request: &OpacityRequest,
        media: Option<&[u8]>,
        before: u64,
        cancel: &dyn Fn() -> bool,
    ) -> Result<u64, Failure> {
        let command = parse(request)?;
        let pin = match &command {
            Command::Attach(args) => &args.document_snapshot_id,
            Command::Remove(args) => &args.document_snapshot_id,
        };
        if !identifier(pin) {
            return Err(plain(DispatchErrorCode::InvalidRequest));
        }
        let origin = self.asset_origin(before, pin)?;
        let prepared = match &command {
            Command::Attach(args) => {
                match &args.target {
                    None => {}
                    Some(Value::Null) => {
                        return Err(typed(DispatchErrorCode::InvalidRequest, "target"))
                    }
                    Some(_) => {
                        return Err(typed(
                            DispatchErrorCode::Unavailable,
                            "layer_policy_unavailable",
                        ))
                    }
                }
                let bytes = media.ok_or(plain(DispatchErrorCode::InvalidRequest))?;
                origin
                    .prepare_asset_attachment(
                        &request.request_id,
                        &args.name,
                        &args.media_type,
                        bytes,
                    )
                    .map_err(|refusal| (refusal.code(), Some(refusal.reason())))?
            }
            Command::Remove(args) => origin
                .prepare_asset_removal(&args.asset_id)
                .map_err(|refusal| (refusal.code(), Some(refusal.reason())))?,
        };
        if cancel() {
            return Err(typed(
                DispatchErrorCode::CancelledBeforeDispatch,
                "cancelled",
            ));
        }
        let after = self.owner.commit_asset_edit(prepared).map_err(plain)?;
        if after != before {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_attach_dispatch.rs"]
mod object_raster_asset_attach_dispatch;
#[cfg(test)]
#[path = "../tests/object_raster_asset_attach_dispatch_reopen.rs"]
mod object_raster_asset_attach_dispatch_reopen;
#[cfg(test)]
#[path = "../tests/object_raster_asset_attach_dispatch_replay.rs"]
mod object_raster_asset_attach_dispatch_replay;
#[cfg(test)]
#[path = "../tests/object_raster_asset_attach_dispatch_saveable.rs"]
mod object_raster_asset_attach_dispatch_saveable;
