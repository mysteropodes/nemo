//! T12: closed intent for one linked-video import or replace.
//! The request names only the host-resolved file; video facts arrive from the host probe.
use crate::commands::object_fill_owner::image_import::wire::closed_map;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

/// The request path bound of the public descriptor: it fits the 4096-byte request with
/// every other field at its own maximum (1107 bytes), so the advertised bound is reachable.
pub(crate) const MAX_PATH_BYTES: usize = 2048;
/// The stored canonical locator may be longer than the request path (symlinks resolve).
pub(crate) const MAX_LOCATOR_BYTES: usize = 4096;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum VideoSource {
    File { path: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum VideoIntent {
    NewLayer {},
    Replace {
        #[serde(rename = "placementId")]
        placement_id: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct VideoLink {
    pub(crate) document_snapshot_id: String,
    pub(crate) lifecycle_generation: u64,
    #[serde(deserialize_with = "closed_map")]
    pub(crate) source: VideoSource,
    #[serde(deserialize_with = "closed_map")]
    pub(crate) intent: VideoIntent,
}

impl VideoLink {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        // Objects only, from the original bytes: no positional form and no duplicate members.
        let mut input = serde_json::Deserializer::from_slice(bytes);
        let parsed: Self = closed_map(&mut input).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        input.end().map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let VideoSource::File { path } = &parsed.source;
        let placement_ok = match &parsed.intent {
            VideoIntent::NewLayer {} => true,
            VideoIntent::Replace { placement_id } => {
                !placement_id.trim().is_empty()
                    && placement_id.len() <= 255
                    && !placement_id.contains('\0')
            }
        };
        if parsed.lifecycle_generation == 0
            || parsed.lifecycle_generation > 9_007_199_254_740_991
            || parsed.document_snapshot_id.is_empty()
            || parsed.document_snapshot_id.len() > 128
            || path.is_empty()
            || path.len() > MAX_PATH_BYTES
            || path.contains('\0')
            || !placement_ok
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(parsed)
    }
}
