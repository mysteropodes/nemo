//! Compact request receipt retention for one native document lifetime.
//!
//! Entries keep a bounded-request SHA-256 fingerprint and typed terminal
//! disposition, never the request JSON, document, or generic response value.

use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value};
use std::collections::HashMap;

pub const APPLICATION_API_VERSION: u32 = 2;
pub const OP_COMMAND_APPLY: &str = "command.document.apply";
pub const OP_QUERY_OPACITY: &str = "query.document.opacity";
pub const OP_QUERY_REVISION: &str = "query.document.revision";
pub const OP_QUERY_SNAPSHOT: &str = "query.document.snapshot.acquire";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpacityRequest {
    pub(crate) api_version: u32,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expected_revision: Option<u64>,
    pub(crate) operation: String,
    pub(crate) payload: Value,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) cancelled_before_dispatch: bool,
}

impl OpacityRequest {
    pub fn query(
        request_id: impl Into<String>,
        instance_id: impl Into<String>,
        document_id: impl Into<String>,
        operation: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            api_version: APPLICATION_API_VERSION,
            request_id: request_id.into(),
            instance_id: instance_id.into(),
            document_id: document_id.into(),
            expected_revision: None,
            operation: operation.into(),
            payload,
            cancelled_before_dispatch: false,
        }
    }

    pub fn command(
        request_id: impl Into<String>,
        instance_id: impl Into<String>,
        document_id: impl Into<String>,
        expected_revision: u64,
        payload: Value,
    ) -> Self {
        let mut request = Self::query(
            request_id,
            instance_id,
            document_id,
            OP_COMMAND_APPLY,
            payload,
        );
        request.expected_revision = Some(expected_revision);
        request
    }

    pub fn cancelled(mut self) -> Self {
        self.cancelled_before_dispatch = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchErrorCode {
    InvalidRequest,
    WrongInstance,
    WrongDocument,
    StaleRevision,
    BusyConflict,
    Unavailable,
    NotFound,
    CancelledBeforeDispatch,
    Internal,
}

impl DispatchErrorCode {
    pub const ALL: [Self; 9] = [
        Self::InvalidRequest,
        Self::WrongInstance,
        Self::WrongDocument,
        Self::StaleRevision,
        Self::BusyConflict,
        Self::Unavailable,
        Self::NotFound,
        Self::CancelledBeforeDispatch,
        Self::Internal,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchError {
    pub(crate) code: DispatchErrorCode,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<Value>,
}

impl DispatchError {
    pub fn code(&self) -> DispatchErrorCode {
        self.code
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub(crate) api_version: u32,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<DispatchError>,
}

impl ResponseEnvelope {
    pub fn is_ok(&self) -> bool {
        self.ok
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn error(&self) -> Option<&DispatchError> {
        self.error.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ApplyOpacity {
    pub(crate) command: String,
    pub(crate) stable_target: StableTarget,
    pub(crate) value: Number,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct StableTarget {
    pub(crate) layer_uid: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct QueryOpacity {
    pub(crate) stable_target: StableTarget,
    #[serde(default)]
    pub(crate) at_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RevisionQuery {
    #[serde(default)]
    pub(crate) frame: Option<u32>,
    #[serde(default)]
    pub(crate) context_id: Option<String>,
    #[serde(default)]
    pub(crate) view_generation: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct SnapshotQuery {
    #[serde(default)]
    pub(crate) at_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestFingerprint {
    encoded_len: usize,
    sha256: [u8; 32],
}

impl RequestFingerprint {
    pub(crate) fn new(request: &OpacityRequest) -> Option<Self> {
        let encoded = serde_json::to_vec(request).ok()?;
        Some(Self {
            encoded_len: encoded.len(),
            sha256: sha256(&encoded),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReceiptDisposition {
    CommandSuccess {
        content_revision: u64,
        applied: bool,
    },
    OpacityQuerySuccess {
        content_revision: u64,
        at_revision: u64,
        layer_uid: String,
        value: Number,
    },
    RevisionQuerySuccess {
        content_revision: u64,
        frame: Option<u32>,
        context_id: Option<String>,
        view_generation: Option<u64>,
    },
    SnapshotQuerySuccess {
        content_revision: u64,
        at_revision: u64,
        document_snapshot_id: String,
    },
    Failure {
        content_revision: u64,
        code: DispatchErrorCode,
        message: &'static str,
    },
}

impl ReceiptDisposition {
    pub(crate) fn success(content_revision: u64, applied: bool) -> Self {
        Self::CommandSuccess {
            content_revision,
            applied,
        }
    }

    pub(crate) fn failure(
        content_revision: u64,
        code: DispatchErrorCode,
        message: &'static str,
    ) -> Self {
        Self::Failure {
            content_revision,
            code,
            message,
        }
    }

    pub(crate) fn response(
        self,
        request: &OpacityRequest,
        instance_id: &str,
        document_id: &str,
    ) -> ResponseEnvelope {
        match self {
            Self::CommandSuccess {
                content_revision,
                applied,
            } => Self::success_response(
                request,
                instance_id,
                document_id,
                content_revision,
                json!({ "applied": applied, "historyEntriesAdded": 0 }),
            ),
            Self::OpacityQuerySuccess {
                content_revision,
                at_revision,
                layer_uid,
                value,
            } => Self::success_response(
                request,
                instance_id,
                document_id,
                content_revision,
                json!({ "atRevision": at_revision, "layerUid": layer_uid, "value": value }),
            ),
            Self::RevisionQuerySuccess {
                content_revision,
                frame,
                context_id,
                view_generation,
            } => Self::success_response(
                request,
                instance_id,
                document_id,
                content_revision,
                json!({
                    "contentRevision": content_revision,
                    "frame": frame,
                    "contextId": context_id,
                    "viewGeneration": view_generation
                }),
            ),
            Self::SnapshotQuerySuccess {
                content_revision,
                at_revision,
                document_snapshot_id,
            } => Self::success_response(
                request,
                instance_id,
                document_id,
                content_revision,
                json!({
                    "atRevision": at_revision,
                    "documentSnapshotId": document_snapshot_id
                }),
            ),
            Self::Failure {
                content_revision,
                code,
                message,
            } => ResponseEnvelope {
                api_version: APPLICATION_API_VERSION,
                request_id: request.request_id.clone(),
                instance_id: instance_id.to_owned(),
                document_id: document_id.to_owned(),
                content_revision,
                ok: false,
                result: None,
                error: Some(DispatchError {
                    code,
                    message: message.to_owned(),
                    details: None,
                }),
            },
        }
    }

    fn success_response(
        request: &OpacityRequest,
        instance_id: &str,
        document_id: &str,
        content_revision: u64,
        result: Value,
    ) -> ResponseEnvelope {
        ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: instance_id.to_owned(),
            document_id: document_id.to_owned(),
            content_revision,
            ok: true,
            result: Some(result),
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReceiptLookup {
    Missing,
    Retained(ReceiptDisposition),
    ChangedBody,
}

#[derive(Debug)]
struct Entry {
    fingerprint: RequestFingerprint,
    disposition: ReceiptDisposition,
}

#[derive(Debug, Default)]
pub(crate) struct RequestReceipts {
    entries: HashMap<String, Entry>,
}

impl RequestReceipts {
    pub(crate) fn lookup(
        &self,
        request_id: &str,
        fingerprint: &RequestFingerprint,
    ) -> ReceiptLookup {
        match self.entries.get(request_id) {
            None => ReceiptLookup::Missing,
            Some(entry) if &entry.fingerprint == fingerprint => {
                ReceiptLookup::Retained(entry.disposition.clone())
            }
            Some(_) => ReceiptLookup::ChangedBody,
        }
    }

    pub(crate) fn retain(
        &mut self,
        request_id: String,
        fingerprint: RequestFingerprint,
        disposition: ReceiptDisposition,
    ) {
        self.entries.insert(
            request_id,
            Entry {
                fingerprint,
                disposition,
            },
        );
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut padded = bytes.to_vec();
    let bit_length = (bytes.len() as u64) * 8;
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());
    let mut hash = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let mut state = hash;
        for index in 0..64 {
            let sum1 =
                state[4].rotate_right(6) ^ state[4].rotate_right(11) ^ state[4].rotate_right(25);
            let choose = (state[4] & state[5]) ^ (!state[4] & state[6]);
            let first = state[7]
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let sum0 =
                state[0].rotate_right(2) ^ state[0].rotate_right(13) ^ state[0].rotate_right(22);
            let majority = (state[0] & state[1]) ^ (state[0] & state[2]) ^ (state[1] & state[2]);
            let second = sum0.wrapping_add(majority);
            state = [
                first.wrapping_add(second),
                state[0],
                state[1],
                state[2],
                state[3].wrapping_add(first),
                state[4],
                state[5],
                state[6],
            ];
        }
        for (value, added) in hash.iter_mut().zip(state) {
            *value = value.wrapping_add(added);
        }
    }
    let mut output = [0u8; 32];
    for (chunk, value) in output.chunks_exact_mut(4).zip(hash) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    output
}
