//! T12 closed public contract of `command.document.object.asset.video.link`: import one
//! linked video as a new layer, or relink an existing drawing video placement. Shape and
//! correlation only: the host probes the canonical file and the native owner repeats every
//! rule on the original bytes. Mirrored by `engineering/application/capabilities-v2/
//! native-object-asset-video.json` and the transport schema's `ObjectVideoLink*` $defs.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::{bounded_identifier, exact};
use serde_json::Value;

pub(crate) const OP: &str = "command.document.object.asset.video.link";
/// Fits the 4096-byte request with every other field at its own maximum (1107 bytes).
pub(crate) const MAX_PATH_BYTES: usize = 2048;
const MAX_SAFE: u64 = 9_007_199_254_740_991;

fn opaque(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= 255 && !v.contains('\0'))
}

fn source(value: &Value) -> bool {
    exact(value, &["kind", "path"], &[]).is_some()
        && value["kind"] == "file"
        && value["path"]
            .as_str()
            .is_some_and(|p| !p.is_empty() && p.len() <= MAX_PATH_BYTES && !p.contains('\0'))
}

fn intent(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("newLayer") => exact(value, &["kind"], &[]).is_some(),
        Some("replace") => {
            exact(value, &["kind", "placementId"], &[]).is_some() && opaque(&value["placementId"])
        }
        _ => false,
    }
}

pub(crate) fn request(payload: &Value) -> bool {
    exact(
        payload,
        &[
            "documentSnapshotId",
            "lifecycleGeneration",
            "source",
            "intent",
        ],
        &[],
    )
    .is_some()
        && payload["documentSnapshotId"]
            .as_str()
            .is_some_and(bounded_identifier)
        && payload["lifecycleGeneration"]
            .as_u64()
            .is_some_and(|n| (1..=MAX_SAFE).contains(&n))
        && source(&payload["source"])
        && intent(&payload["intent"])
}

fn dimension(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|n| (1..=u64::from(u32::MAX)).contains(&n))
}

pub(crate) fn result(value: &Value) -> bool {
    exact(
        value,
        &[
            "applied",
            "historyEntriesAdded",
            "assetId",
            "placementId",
            "target",
            "newLayer",
            "sourceWidth",
            "sourceHeight",
            "sourceFrameCount",
        ],
        &[],
    )
    .is_some()
        && value["applied"] == true
        && value["historyEntriesAdded"] == 1
        && opaque(&value["assetId"])
        && opaque(&value["placementId"])
        && exact(&value["target"], &["contextId", "layerUid"], &[]).is_some()
        // A replace names its placement's own context, which may be a component.
        && opaque(&value["target"]["contextId"])
        && opaque(&value["target"]["layerUid"])
        && value["newLayer"].is_boolean()
        && ["sourceWidth", "sourceHeight", "sourceFrameCount"]
            .iter()
            .all(|key| dimension(&value[*key]))
}

/// A receipt advances exactly one revision and names the placement its intent addressed:
/// the request's own minted id in the scene root for a new layer, the unchanged id for a
/// replace (whose target is the placement's own, echoed by the owner).
pub(crate) fn revision_matches(
    request: &NativeApplicationRequest,
    revision: u64,
    value: &Value,
) -> bool {
    let intent = &request.payload["intent"];
    let new_layer = intent["kind"] == "newLayer";
    request
        .expected_revision
        .and_then(|before| before.checked_add(1))
        == Some(revision)
        && value["applied"] == true
        && value["newLayer"] == new_layer
        && if new_layer {
            value["placementId"].as_str()
                == Some(format!("placement-{}", request.request_id).as_str())
                && value["target"]["contextId"] == "scene-root"
        } else {
            value["placementId"] == intent["placementId"]
        }
}

fn owns(request: &NativeApplicationRequest) -> bool {
    request.operation == OP && self::request(&request.payload)
}

/// The owner answers a foreign instance with its own instance id; nothing else is excused.
pub(crate) fn wrong_instance_refusal(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    owns(request)
        && !response.ok
        && response.result.is_none()
        && response.instance_id != request.instance_id
        && response
            .error
            .as_ref()
            .is_some_and(|error| error.code == "wrong_instance")
}

pub(crate) fn owner_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> Option<bool> {
    if !owns(request) {
        return None;
    }
    let exact =
        response.instance_id == request.instance_id && response.document_id == request.document_id;
    let refused = !response.ok
        && response.result.is_none()
        && response.error.as_ref().is_some_and(|error| {
            error.code == "wrong_instance" && response.instance_id != request.instance_id
                || error.code == "wrong_document" && response.instance_id == request.instance_id
        });
    Some(exact || refused)
}

/// Router fallback: the shared request table ends here for the linked-video operation.
pub(crate) fn request_check(operation: &str, payload: &Value) -> Option<bool> {
    (operation == OP).then(|| request(payload))
}

/// Router fallback for the shared result table, paired with [`request_check`].
pub(crate) fn result_validator(operation: &str) -> Option<crate::native_contract::ResultValidator> {
    (operation == OP).then_some(|_, _, _, r| result(r))
}

#[cfg(test)]
#[path = "native_video_link_contract_tests.rs"]
mod tests;
