//! Canonical typed host-request fingerprints and bounded identity admission.

use super::{bounded_id, host_error, HostResult, NativeReleaseRequest, NativeReplacementRequest};
use nemo_mcp::contract::NATIVE_API_VERSION;
use std::io::{self, Write};

const MAX_REPLACEMENT_FINGERPRINT_BYTES: usize = 4 * 1_048_576;

struct BoundedFingerprint(Vec<u8>);

impl Write for BoundedFingerprint {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_REPLACEMENT_FINGERPRINT_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other(
                "replacement request fingerprint exceeds 4 MiB",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn release_fingerprint(request: &NativeReleaseRequest) -> HostResult<Vec<u8>> {
    if request.api_version != NATIVE_API_VERSION
        || !bounded_id(&request.request_id)
        || !bounded_id(&request.instance_id)
        || !bounded_id(&request.document_id)
        || request.expected_revision > 9_007_199_254_740_991
    {
        return Err(host_error(
            "invalid_request",
            "invalid native release request identity",
        ));
    }
    serde_json::to_vec(request).map_err(|_| {
        host_error(
            "invalid_request",
            "native release request is not serializable",
        )
    })
}

pub(crate) fn replacement_fingerprint(request: &NativeReplacementRequest) -> HostResult<Vec<u8>> {
    if request.api_version != NATIVE_API_VERSION
        || !bounded_id(&request.request_id)
        || !bounded_id(&request.instance_id)
        || !bounded_id(&request.document_id)
        || request.expected_revision > 9_007_199_254_740_991
    {
        return Err(host_error(
            "invalid_request",
            "invalid native replacement request identity",
        ));
    }
    // Struct field order and serde_json::Value's ordered maps make one typed
    // representation for the complete accepted request, including resources.
    let mut fingerprint = BoundedFingerprint(Vec::new());
    serde_json::to_writer(&mut fingerprint, request).map_err(|_| {
        host_error(
            "invalid_request",
            "native replacement request is invalid or exceeds 4 MiB",
        )
    })?;
    Ok(fingerprint.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fingerprint_covers_entire_typed_request_and_canonicalizes_object_keys() {
        let base = json!({
            "apiVersion":NATIVE_API_VERSION, "requestId":"replace-a", "instanceId":"instance-a",
            "documentId":"document-a", "expectedRevision":0,
            "projection":{"z":2,"a":1},
            "resources":[{"resourceId":"geometry-a","resourceVersion":"v1","layers":[
                {"layerUid":"layer-a","bounds":[0,0,10,10],"transform":[1,0,0,1,0,0],
                 "paint":{"red":255,"green":0,"blue":0}}
            ]}]
        });
        let typed: NativeReplacementRequest = serde_json::from_value(base.clone()).unwrap();
        let fingerprint = replacement_fingerprint(&typed).unwrap();
        let reordered: NativeReplacementRequest = serde_json::from_str(&format!(
            "{{\"resources\":{},\"projection\":{{\"a\":1,\"z\":2}},\"expectedRevision\":0,\"documentId\":\"document-a\",\"instanceId\":\"instance-a\",\"requestId\":\"replace-a\",\"apiVersion\":{}}}",
            base["resources"], NATIVE_API_VERSION)).unwrap();
        assert_eq!(fingerprint, replacement_fingerprint(&reordered).unwrap());
        for (field, value) in [
            ("requestId", json!("replace-b")),
            ("instanceId", json!("instance-b")),
            ("documentId", json!("document-b")),
            ("expectedRevision", json!(1)),
            ("projection", json!({"a":1,"z":3})),
            (
                "resources",
                json!([{"resourceId":"geometry-b","resourceVersion":"v1","layers":base["resources"][0]["layers"]}]),
            ),
        ] {
            let mut changed = base.clone();
            changed[field] = value;
            let typed: NativeReplacementRequest = serde_json::from_value(changed).unwrap();
            assert_ne!(
                fingerprint,
                replacement_fingerprint(&typed).unwrap(),
                "{field} must participate"
            );
        }
        for (field, value) in [
            ("bounds", json!([0, 0, 11, 10])),
            ("paint", json!({"red":254,"green":0,"blue":0})),
        ] {
            let mut changed = base.clone();
            changed["resources"][0]["layers"][0][field] = value;
            let typed: NativeReplacementRequest = serde_json::from_value(changed).unwrap();
            assert_ne!(
                fingerprint,
                replacement_fingerprint(&typed).unwrap(),
                "nested {field} must participate"
            );
        }
        let mut invalid = base;
        invalid["requestId"] = json!("bad id");
        let typed: NativeReplacementRequest = serde_json::from_value(invalid).unwrap();
        assert_eq!(
            replacement_fingerprint(&typed).unwrap_err().code,
            "invalid_request"
        );
        let mut oversized = json!({
            "apiVersion":NATIVE_API_VERSION, "requestId":"large", "instanceId":"instance-a",
            "documentId":"document-a", "expectedRevision":0,
            "projection":{"payload":"x".repeat(MAX_REPLACEMENT_FINGERPRINT_BYTES)},
            "resources":[]
        });
        let typed: NativeReplacementRequest = serde_json::from_value(oversized.take()).unwrap();
        assert_eq!(
            replacement_fingerprint(&typed).unwrap_err().code,
            "invalid_request"
        );
    }
}
