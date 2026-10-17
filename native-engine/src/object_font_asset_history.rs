//! PRIVATE draft. Scalar commands and borrowed host bytes, original owner only.
//! Host must reject raw JSON duplicates and fence lifecycle before/after I/O.
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::{AssetId, AssetKind, AssetRecord};
use crate::request_receipts::{
    sha256, DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::revision::ObjectSnapshot;
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

const IMPORT: &str = "command.document.object.asset.font.import";
const QUERY: &str = "query.document.object.asset.font";
const MAX_SAFE: u64 = 9_007_199_254_740_991;
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
fn hex(bytes: &[u8]) -> String {
    sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Import {
    command: String,
    document_snapshot_id: String,
    lifecycle_generation: u64,
    source: Source,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Source {
    Bundled {
        #[serde(rename = "fontKey")]
        font_key: String,
    },
    Google {
        family: String,
        weight: u32,
        italic: bool,
    },
}
impl Source {
    fn name(&self) -> Result<String, DispatchErrorCode> {
        match self {
            Self::Bundled { font_key }
                if [
                    "Roboto-Regular",
                    "Roboto-Bold",
                    "DejaVuSans-Regular",
                    "DejaVuSans-Bold",
                    "DejaVuSerif-Regular",
                    "DejaVuSerif-Bold",
                    "Manrope-Regular",
                    "Inter-Regular",
                ]
                .contains(&font_key.as_str()) =>
            {
                Ok(font_key.clone())
            }
            Self::Google {
                family,
                weight,
                italic,
            } if !family.trim().is_empty()
                && family.len() <= 255
                && !family.contains('\0')
                && (1..=1000).contains(weight) =>
            {
                let _ = italic; // Descriptor stays in the exact request fingerprint.
                Ok(family.clone())
            }
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Read {
    List {
        #[serde(rename = "atRevision")]
        at: u64,
        #[serde(rename = "documentSnapshotId")]
        snapshot: String,
        offset: usize,
        limit: usize,
    },
    Inspect {
        #[serde(rename = "atRevision")]
        at: u64,
        #[serde(rename = "documentSnapshotId")]
        snapshot: String,
        #[serde(rename = "assetId")]
        asset: String,
    },
    Read {
        #[serde(rename = "atRevision")]
        at: u64,
        #[serde(rename = "documentSnapshotId")]
        snapshot: String,
        #[serde(rename = "assetId")]
        asset: String,
        offset: usize,
        length: usize,
    },
    Receipt {
        #[serde(rename = "forRequestId")]
        request: String,
    },
}

fn reply(
    owner: &NativeObjectHistory,
    request: &OpacityRequest,
    revision: u64,
    result: Value,
) -> Result<ResponseEnvelope, DispatchErrorCode> {
    let response = ResponseEnvelope {
        api_version: 2,
        request_id: request.request_id.clone(),
        instance_id: owner.instance_id().into(),
        document_id: owner.document_id().into(),
        content_revision: revision,
        ok: true,
        result: Some(result),
        error: None,
    };
    if serde_json::to_vec(&response).map_or(true, |v| v.len() > 4096) {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(response)
}
fn row(record: &AssetRecord) -> Result<Value, DispatchErrorCode> {
    let mut value = json!({"assetId":record.id().as_str(),"name":record.name()});
    if record.source().locator().is_some() {
        value["sourceKind"] = "linked".into();
        value["available"] = false.into();
        value["reason"] = "linked_font_unavailable".into();
    } else {
        if !record.source().has_font_admission() || record.source().media_type() != Some("font/ttf")
        {
            return Err(DispatchErrorCode::Unavailable);
        }
        let bytes = record
            .source()
            .encoded_allocation()
            .ok_or(DispatchErrorCode::Unavailable)?;
        value["sourceKind"] = "embedded".into();
        value["available"] = true.into();
        value["mediaType"] = "font/ttf".into();
        value["encodedBytes"] = bytes.len().into();
        value["encodedSha256"] = hex(bytes).into();
    }
    Ok(value)
}

impl NativeObjectHistory {
    fn font_envelope(
        &self,
        request: &OpacityRequest,
        operation: &str,
    ) -> Result<(), DispatchErrorCode> {
        if request.api_version != 2
            || request.operation != operation
            || !request.payload.is_object()
            || [
                &request.request_id,
                &request.instance_id,
                &request.document_id,
            ]
            .iter()
            .any(|v| !id(v))
            || serde_json::to_vec(request).map_or(true, |v| v.len() > 4096)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        Ok(())
    }
    fn font_args(&self, request: &OpacityRequest) -> Result<Import, DispatchErrorCode> {
        self.font_envelope(request, IMPORT)?;
        if request.expected_revision.is_none_or(|r| r > MAX_SAFE) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let args: Import = serde_json::from_value(request.payload.clone())
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if args.command != "object.asset.font.import"
            || !id(&args.document_snapshot_id)
            || args.lifecycle_generation == 0
            || args.lifecycle_generation > MAX_SAFE
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        args.source.name()?;
        Ok(args)
    }
    fn font_origin(
        &self,
        request: &OpacityRequest,
        args: &Import,
    ) -> Result<ObjectSnapshot, DispatchErrorCode> {
        if request.cancelled_before_dispatch {
            return Err(DispatchErrorCode::CancelledBeforeDispatch);
        }
        if request.expected_revision != Some(self.content_revision()) {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let snapshot = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        if snapshot.snapshot_id() != args.document_snapshot_id {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if snapshot.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        Ok(snapshot)
    }
    fn retain_font_outcome(
        &mut self,
        request: &OpacityRequest,
        outcome: Result<CompactResult, DispatchErrorCode>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome.map_err(|code| CompactFailure {
                code,
                message: "Native font import refused.".into(),
                details: None,
            }),
        };
        let digest =
            hex(&serde_json::to_vec(request).map_err(|_| DispatchErrorCode::InvalidRequest)?);
        self.receipts.retain_font(
            request.request_id.clone(),
            fingerprint,
            disposition.clone(),
            digest,
        );
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }
    /// Host checks this BEFORE source I/O. None means only "may resolve bytes".
    /// Actual lifecycle/close admission remains the original host's obligation.
    pub fn font_import_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        let args = self.font_args(request)?;
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(value) => {
                return value
                    .response(request, self.instance_id(), self.document_id())
                    .map(Some)
                    .ok_or(DispatchErrorCode::Internal)
            }
            ReceiptLookup::ChangedBody => return Err(DispatchErrorCode::InvalidRequest),
            ReceiptLookup::Missing => {}
        }
        if let Err(error) = self.font_origin(request, &args) {
            return self.retain_font_outcome(request, Err(error)).map(Some);
        }
        Ok(None)
    }
    /// Trusted resolved bytes are out-of-band INTERNAL input, never a raw DTO.
    /// Replay/status and complete origin checks repeat under this owner borrow.
    pub fn dispatch_font_import(
        &mut self,
        request: &OpacityRequest,
        bytes: &[u8],
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let Some(response) = self.font_import_status(request)? {
            return Ok(response);
        }
        let args = self.font_args(request)?;
        let outcome = (|| {
            let origin = self.font_origin(request, &args)?;
            let name = args.source.name()?;
            let seed =
                serde_json::to_vec(&(self.instance_id(), self.document_id(), &request.request_id))
                    .map_err(|_| DispatchErrorCode::Internal)?;
            let base = format!("native-font-{}", hex(&seed));
            let records = origin
                .project()
                .unwrap()
                .asset_inventory()
                .map(|a| a.records())
                .unwrap_or(&[]);
            let id = (0..=1024)
                .map(|n| {
                    if n == 0 {
                        base.clone()
                    } else {
                        format!("{base}-{n}")
                    }
                })
                .find(|candidate| !records.iter().any(|r| r.id().as_str() == candidate))
                .ok_or(DispatchErrorCode::Unavailable)?;
            let record = origin.admit_font_asset(id.clone(), name, bytes)?;
            let allocation = record
                .source()
                .encoded_allocation()
                .ok_or(DispatchErrorCode::Unavailable)?;
            let count = allocation.len();
            let digest = hex(allocation);
            let before = self.content_revision();
            let after = self.owner.commit_font_asset(&origin, record)?;
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
            Ok(CompactResult::FontImported {
                asset_id: id,
                encoded_bytes: count,
                encoded_sha256: digest,
            })
        })();
        self.retain_font_outcome(request, outcome)
    }
    /// Original immutable catalog/bytes only. No I/O, mutation or ACK.
    pub fn query_font_asset(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        self.font_envelope(request, QUERY)?;
        if request.expected_revision.is_some() || request.cancelled_before_dispatch {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let args: Read = serde_json::from_value(request.payload.clone())
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if let Read::Receipt { request: original } = &args {
            if !id(original) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let value = match self.receipts.font_receipt(original) {
                Some((
                    RetainedDisposition::Exact {
                        content_revision,
                        outcome:
                            Ok(CompactResult::FontImported {
                                asset_id,
                                encoded_bytes,
                                encoded_sha256,
                            }),
                    },
                    digest,
                )) => {
                    json!({"status":"committed","forRequestId":original,"requestFingerprintSha256":digest,"contentRevision":content_revision,"assetId":asset_id,"mediaType":"font/ttf","encodedBytes":encoded_bytes,"encodedSha256":encoded_sha256})
                }
                Some((
                    RetainedDisposition::Exact {
                        content_revision,
                        outcome: Err(error),
                    },
                    digest,
                )) => {
                    json!({"status":"refused","forRequestId":original,"requestFingerprintSha256":digest,"contentRevision":content_revision,"code":error.code})
                }
                _ => json!({"status":"not_found","forRequestId":original}),
            };
            return reply(self, request, self.content_revision(), value);
        }
        let (at, snapshot_id) = match &args {
            Read::List { at, snapshot, .. }
            | Read::Inspect { at, snapshot, .. }
            | Read::Read { at, snapshot, .. } => (*at, snapshot),
            _ => unreachable!(),
        };
        if at > MAX_SAFE || !id(snapshot_id) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let snapshot = self
            .acquire_snapshot(at)
            .filter(|s| s.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let project = snapshot.project().ok_or(DispatchErrorCode::Unavailable)?;
        let records = project
            .asset_inventory()
            .map(|a| a.records())
            .unwrap_or(&[]);
        if let Read::List { offset, limit, .. } = args {
            let fonts: Vec<_> = records
                .iter()
                .filter(|r| r.kind() == AssetKind::Font)
                .collect();
            if limit != 1 || offset > fonts.len() {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let values = fonts
                .get(offset)
                .map(|r| row(r))
                .transpose()?
                .into_iter()
                .collect::<Vec<_>>();
            let next = (offset + values.len() < fonts.len()).then_some(offset + values.len());
            return reply(
                self,
                request,
                at,
                json!({"atRevision":at,"documentSnapshotId":snapshot_id,"offset":offset,"nextOffset":next,"totalFontRecords":fonts.len(),"rows":values}),
            );
        }
        let asset = match &args {
            Read::Inspect { asset, .. } | Read::Read { asset, .. } => asset,
            _ => unreachable!(),
        };
        AssetId::new(asset.clone()).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let record = records
            .iter()
            .find(|r| r.id().as_str() == asset && r.kind() == AssetKind::Font)
            .ok_or(DispatchErrorCode::NotFound)?;
        if matches!(&args, Read::Inspect { .. }) {
            return reply(
                self,
                request,
                at,
                json!({"atRevision":at,"documentSnapshotId":snapshot_id,"font":row(record)?}),
            );
        }
        let Read::Read { offset, length, .. } = args else {
            unreachable!()
        };
        if !(1..=1536).contains(&length) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if !record.source().has_font_admission() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let bytes = record
            .source()
            .encoded_allocation()
            .ok_or(DispatchErrorCode::Unavailable)?;
        if offset >= bytes.len() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let mut count = length.min(bytes.len() - offset);
        loop {
            let value = json!({"atRevision":at,"documentSnapshotId":snapshot_id,"assetId":asset,"offset":offset,
                "returnedLength":count,"totalLength":bytes.len(),"encodedSha256":hex(bytes),
                "base64":base64::engine::general_purpose::STANDARD.encode(&bytes[offset..offset+count])});
            if let Ok(response) = reply(self, request, at, value) {
                return Ok(response);
            }
            // Explicit bounded short-read contract, not a transport fallback.
            if count == 1 {
                return Err(DispatchErrorCode::Unavailable);
            }
            count = (count / 2).max(1);
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_font_asset_history.rs"]
mod tests;
