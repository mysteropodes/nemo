//! Canonical typed-envelope limits; no transport, owner or capability substitution.
use crate::contract::NativeApplicationRequest;
use crate::wire::{decode_native_request, native_request_byte_limit};
use serde_json::json;
fn request(vector: bool) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"typed","instanceId":"instance","documentId":"document","expectedRevision":7,
        "operation":if vector {"command.document.object.vector.edit"} else {"command.document.object.property.edit"},
        "payload":if vector {json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":0},"layerUid":"layer","strokeId":"stroke"}})}else{json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"layer"},"property":"opacity","action":{"kind":"removeKey","frame":-0.5}})}})).unwrap()
}
fn sized(
    mut request: NativeApplicationRequest,
    vector: bool,
    bytes: usize,
) -> NativeApplicationRequest {
    let field = if vector { "stableTarget" } else { "target" };
    request.payload[field]["layerUid"] = json!("雪");
    let size = serde_json::to_vec(&request).unwrap().len();
    request.payload[field]["layerUid"] = json!(format!(
        "雪{}",
        "x".repeat(bytes.checked_sub(size).unwrap())
    ));
    assert_eq!(serde_json::to_vec(&request).unwrap().len(), bytes);
    request
}
#[test]
fn typed_full_request_and_original_decoder_share_inclusive_vector_only_limits() {
    for (vector, limit) in [(true, 65536), (false, 4096)] {
        let at = sized(request(vector), vector, limit);
        assert!(at.validate_input().is_ok());
        assert!(decode_native_request(&serde_json::to_vec(&at).unwrap()).is_ok());
        let over = sized(request(vector), vector, limit + 1);
        let error = over.validate_input().unwrap_err().to_string();
        assert!(error.contains(&format!("{limit} encoded bytes")), "{error}");
        assert!(decode_native_request(&serde_json::to_vec(&over).unwrap()).is_err());
    }
}
#[test]
fn request_allowance_is_exact_operation_not_prefix_family_or_arbitrary_nested_name() {
    assert_eq!(
        native_request_byte_limit("command.document.object.vector.edit"),
        65536
    );
    for op in [
        "command.document.object.vector.edit.extra",
        "command.document.object.vector",
        "command.document.object.parametric.edit",
        "command.document.object.path.effects.edit",
        "command.document.object.property.edit",
        "command.document.object.property.track.edit",
        "command.document.object.layer.policy.edit",
        "command.document.object.font.import",
        "command.document.object.item.edit",
        "command.document.object.asset.attach",
        "command.document.object.rig.edit",
        "command.document.object.component.edit",
        "command.document.object.frames.insert",
        "command.document.object.import.svg",
        "query.document.object",
        "query.document.object.property.read",
        "",
    ] {
        assert_eq!(native_request_byte_limit(op), 4096, "{op}");
    }
    let mut bad = request(true);
    bad.payload["extra"] = json!({"operation":"command.document.object.vector.edit"});
    assert!(
        bad.validate_input().is_err(),
        "larger byte budget never relaxes closed DTO admission"
    );
    let mut bad = request(true);
    bad.expected_revision = None;
    assert!(
        bad.validate_input().is_err(),
        "existing native revision admission remains required"
    );
}
