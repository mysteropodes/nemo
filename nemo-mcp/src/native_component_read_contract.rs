//! #2256: closed contract of the read-only native component model (`query.document.object.component.read`).
//! Pure shape and correlation validation of a pinned, paged, byte-bounded read; the native owner holds the model.
//! The table is mirrored by `src/js/adapters/native-object-component-read-contract.js`.
use crate::contract::{bounded_identifier, NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::{exact, revision};
use serde_json::{Map, Value};

pub(crate) const OP: &str = "query.document.object.component.read";
const MAX_PAGE: u64 = 16;
const MAX_ID: usize = 128;
const SCENE: &str = "scene-root";
const LAYER_KINDS: [&str; 6] = ["drawing", "folder", "effect", "widget", "null", "instance"];

fn context_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID && !id.contains('\0'))
}
fn page(object: &Map<String, Value>) -> bool {
    revision(object.get("cursor")).is_some()
        && object
            .get("limit")
            .and_then(Value::as_u64)
            .is_some_and(|limit| (1..=MAX_PAGE).contains(&limit))
}
fn selector(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("contexts") => exact(value, &["kind", "cursor", "limit"], &[]).is_some_and(page),
        Some("context") => exact(value, &["kind", "contextId"], &[])
            .is_some_and(|o| context_id(o.get("contextId")).is_some()),
        Some("layers" | "instances") => {
            exact(value, &["kind", "contextId", "cursor", "limit"], &[])
                .is_some_and(|o| context_id(o.get("contextId")).is_some() && page(o))
        }
        _ => false,
    }
}
pub(crate) fn request(payload: &Value) -> bool {
    exact(
        payload,
        &["atRevision", "documentSnapshotId", "selector"],
        &[],
    )
    .is_some_and(|p| {
        revision(p.get("atRevision")).is_some()
            && p["documentSnapshotId"]
                .as_str()
                .is_some_and(bounded_identifier)
            && selector(&p["selector"])
    })
}

fn uint32(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_u64)
        .is_some_and(|n| n <= u64::from(u32::MAX))
}
fn name(value: Option<&Value>) -> bool {
    value.is_some_and(|v| v.is_null() || v.is_string())
}
fn uid(value: Option<&Value>) -> bool {
    context_id(value).is_some()
}
fn context_item(value: &Value, first: bool) -> bool {
    exact(
        value,
        &[
            "contextId",
            "contextKind",
            "name",
            "totalFrames",
            "layerCount",
        ],
        &["nameTruncated"],
    )
    .is_some_and(|o| {
        let scene = o["contextId"] == SCENE;
        // Present only as `true`: the owner cut an oversized component name to fit the page. The scene has no name to cut.
        o.get("nameTruncated")
            .map_or(true, |flag| *flag == true && !scene && o["name"].is_string())
            && uid(o.get("contextId"))
            && scene == first
            && o["contextKind"] == if scene { "scene" } else { "component" }
            && name(o.get("name"))
            && (!scene || o["name"].is_null())
            && uint32(o.get("totalFrames"))
            && uint32(o.get("layerCount"))
    })
}
fn layer_item(value: &Value) -> bool {
    exact(value, &["layerUid", "name", "layerKind"], &[]).is_some_and(|o| {
        uid(o.get("layerUid"))
            && name(o.get("name"))
            && o["layerKind"]
                .as_str()
                .is_some_and(|k| LAYER_KINDS.contains(&k))
    })
}
fn instance_item(value: &Value) -> bool {
    exact(value, &["layerUid"], &[]).is_some_and(|o| uid(o.get("layerUid")))
}

/// The page frame shared by the three paged kinds: the cursor answers the request, the page is within its limit,
/// is contiguous and inside the total, and `nextCursor` names the first unread index or is null at the end.
fn paged(
    result: &Map<String, Value>,
    wanted: &Map<String, Value>,
    count_key: &str,
    item: &dyn Fn(&Value, bool) -> bool,
) -> bool {
    let (Some(count), Some(cursor), Some(items)) = (
        revision(result.get(count_key)),
        revision(result.get("cursor")),
        result.get("items").and_then(Value::as_array),
    ) else {
        return false;
    };
    let limit = wanted.get("limit").and_then(Value::as_u64).unwrap_or(0);
    let end = cursor + items.len() as u64;
    let next = match result.get("nextCursor") {
        Some(Value::Null) => end >= count,
        other => revision(other).is_some_and(|n| n == end && n < count && !items.is_empty()),
    };
    Some(cursor) == wanted.get("cursor").and_then(Value::as_u64)
        && items.len() as u64 <= limit
        && end <= count
        && next
        && items
            .iter()
            .enumerate()
            .all(|(at, entry)| item(entry, cursor == 0 && at == 0))
}
pub(crate) fn result(payload: &Value, value: &Value) -> bool {
    if !request(payload) {
        return false;
    }
    let wanted = &payload["selector"];
    let (Some(wanted), Some(kind)) = (wanted.as_object(), wanted["kind"].as_str()) else {
        return false;
    };
    fn with<'a>(extra: &[&'a str]) -> Vec<&'a str> {
        ["atRevision", "documentSnapshotId", "kind"]
            .into_iter()
            .chain(extra.iter().copied())
            .collect()
    }
    let pins = |o: &Map<String, Value>| {
        o.get("atRevision") == payload.get("atRevision")
            && o.get("documentSnapshotId") == payload.get("documentSnapshotId")
            && o.get("kind").and_then(Value::as_str) == Some(kind)
    };
    let same_context = |o: &Map<String, Value>| o.get("contextId") == wanted.get("contextId");
    match kind {
        "contexts" => exact(
            value,
            &with(&["contextCount", "cursor", "items", "nextCursor"]),
            &[],
        )
        .is_some_and(|o| pins(o) && paged(o, wanted, "contextCount", &context_item)),
        "context" => exact(
            value,
            &with(&[
                "contextId",
                "contextKind",
                "name",
                "totalFrames",
                "layerCount",
                "fps",
                "instanceCount",
                "exposedPropertyCount",
                "hasCameraKeys",
            ]),
            &[],
        )
        .is_some_and(|o| {
            let scene = o["contextId"] == SCENE;
            pins(o)
                && same_context(o)
                && o["contextKind"] == if scene { "scene" } else { "component" }
                && name(o.get("name"))
                && (o["fps"].is_null()
                    || o["fps"].as_f64().is_some_and(|f| f.is_finite() && f > 0.0))
                && uint32(o.get("totalFrames"))
                && uint32(o.get("layerCount"))
                && uint32(o.get("instanceCount"))
                && uint32(o.get("exposedPropertyCount"))
                && o["hasCameraKeys"].is_boolean()
                && (!scene
                    || (o["name"].is_null()
                        && o["fps"].is_null()
                        && o["exposedPropertyCount"] == 0))
        }),
        "layers" => exact(
            value,
            &with(&["contextId", "layerCount", "cursor", "items", "nextCursor"]),
            &[],
        )
        .is_some_and(|o| {
            pins(o) && same_context(o) && paged(o, wanted, "layerCount", &|e, _| layer_item(e))
        }),
        "instances" => exact(
            value,
            &with(&[
                "contextId",
                "instanceCount",
                "cursor",
                "items",
                "nextCursor",
            ]),
            &[],
        )
        .is_some_and(|o| {
            pins(o)
                && same_context(o)
                && wanted.get("contextId").is_some_and(|id| id != SCENE)
                && paged(o, wanted, "instanceCount", &|e, _| instance_item(e))
        }),
        _ => false,
    }
}

/// Owner and revision correlation of a successful envelope: same request, instance and document (a refusal may report
/// the replacement document), and the header revision is the revision the answer is pinned to.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if request.operation != OP {
        return true;
    }
    response.request_id == request.request_id
        && response.instance_id == request.instance_id
        && (response.document_id == request.document_id
            || (!response.ok
                && response
                    .error
                    .as_ref()
                    .is_some_and(|error| error.code == "wrong_document")))
        && (!response.ok
            || response.result.as_ref().is_some_and(|result| {
                result.get("atRevision").and_then(Value::as_u64) == Some(response.content_revision)
                    && request.payload.get("atRevision").and_then(Value::as_u64)
                        == Some(response.content_revision)
            }))
}

#[cfg(test)]
#[path = "native_component_read_contract_tests.rs"]
mod tests;
