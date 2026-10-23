//! Closed bounded Mesh view for the existing common object query.
use super::{exact, frame, revision};
use serde_json::Value;

// The image-mesh branch carries one presented raster and at most sixteen field
// entries. It never admits a serialized document or an unbounded mesh object.
pub(super) fn image_mesh_view(value: &Value) -> bool {
    value
        .get("view")
        .and_then(|view| view.get("kind"))
        .and_then(Value::as_str)
        == Some("imageMesh")
}
fn opaque(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}
pub(super) fn image_mesh_request(value: &Value) -> bool {
    let Some(payload) = exact(value, &["atRevision", "documentSnapshotId", "view"], &[]) else {
        return false;
    };
    let Some(view) = exact(
        &payload["view"],
        &[
            "kind",
            "contextId",
            "layerUid",
            "frame",
            "workId",
            "viewGeneration",
            "page",
        ],
        &[],
    ) else {
        return false;
    };
    revision(payload.get("atRevision")).is_some()
        && opaque(payload.get("documentSnapshotId"))
        && view["kind"] == "imageMesh"
        && opaque(view.get("contextId"))
        && opaque(view.get("layerUid"))
        && frame(view.get("frame")).is_some()
        && view
            .get("workId")
            .and_then(Value::as_str)
            .is_some_and(crate::contract::bounded_identifier)
        && revision(view.get("viewGeneration")).is_some_and(|generation| generation > 0)
        && image_mesh_page_request(&view["page"])
}
fn image_mesh_page_request(value: &Value) -> bool {
    if value.get("kind").and_then(Value::as_str) == Some("selection") {
        return exact(value, &["kind"], &[]).is_some();
    }
    exact(value, &["kind", "meshId", "cursor", "limit"], &[]).is_some_and(|page| {
        matches!(
            page["kind"].as_str(),
            Some("outline" | "verts" | "tris" | "offsets")
        ) && opaque(page.get("meshId"))
            && frame(page.get("cursor")).is_some()
            && page
                .get("limit")
                .and_then(Value::as_u64)
                .is_some_and(|limit| (1..=16).contains(&limit))
    })
}
struct MeshCounts {
    outline: u64,
    vertices: u64,
    triangles: u64,
    offsets: bool,
}
fn image_mesh_counts(value: &Value) -> Option<MeshCounts> {
    let mesh = exact(
        value,
        &[
            "meshId",
            "outlineCount",
            "vertexCount",
            "triangleIndexCount",
            "offsetsPresent",
        ],
        &["cols", "rows"],
    )?;
    let counts = MeshCounts {
        outline: revision(mesh.get("outlineCount"))?,
        vertices: revision(mesh.get("vertexCount"))?,
        triangles: revision(mesh.get("triangleIndexCount"))?,
        offsets: mesh.get("offsetsPresent")?.as_bool()?,
    };
    (opaque(mesh.get("meshId"))
        && counts.outline <= counts.vertices
        && counts.triangles % 3 == 0
        && ["cols", "rows"].iter().all(|key| {
            mesh.get(*key).is_none_or(|value| {
                value
                    .as_u64()
                    .is_some_and(|density| (1..=64).contains(&density))
            })
        }))
    .then_some(counts)
}
fn image_mesh_target(value: &Value, view: &Value) -> bool {
    exact(
        value,
        &["contextId", "layerUid", "frameScope", "strokeId"],
        &[],
    )
    .is_some_and(|target| {
        target.get("contextId") == view.get("contextId")
            && target.get("layerUid") == view.get("layerUid")
            && opaque(target.get("strokeId"))
            && exact(&target["frameScope"], &["kind", "frame"], &[]).is_some_and(|scope| {
                matches!(scope["kind"].as_str(), Some("authored" | "reference"))
                    && frame(scope.get("frame"))
                        .is_some_and(|source| source <= view["frame"].as_u64().unwrap())
            })
    })
}
fn image_mesh_rect(value: &Value) -> bool {
    exact(value, &["x", "y", "width", "height", "rotation"], &[]).is_some_and(|rect| {
        rect.values()
            .all(|value| value.as_f64().is_some_and(f64::is_finite))
            && ["width", "height"]
                .iter()
                .all(|key| rect[*key].as_f64().is_some_and(|size| size > 0.0))
    })
}
fn image_mesh_page_result(
    requested: &Value,
    page: &Value,
    mesh: &Value,
    counts: &MeshCounts,
) -> bool {
    let Some(page) = exact(
        page,
        &["kind", "cursor", "total", "nextCursor", "items"],
        &[],
    ) else {
        return false;
    };
    let (Some(cursor), Some(total), Some(items)) = (
        frame(page.get("cursor")),
        revision(page.get("total")),
        page["items"].as_array(),
    ) else {
        return false;
    };
    if page.get("kind") != requested.get("kind")
        || page.get("cursor") != requested.get("cursor")
        || mesh.get("meshId") != requested.get("meshId")
        || items.len() > 16
        || items.len() as u64 > requested["limit"].as_u64().unwrap()
    {
        return false;
    }
    let expected = match page["kind"].as_str() {
        Some("outline") => counts.outline,
        Some("verts") => counts.vertices,
        Some("tris") => counts.triangles,
        Some("offsets") if counts.offsets => counts.vertices,
        _ => return false,
    };
    let end = cursor + items.len() as u64;
    let next_matches = if end == total {
        page["nextCursor"].is_null()
    } else {
        !items.is_empty() && frame(page.get("nextCursor")) == Some(end)
    };
    total == expected
        && end <= total
        && next_matches
        && items.iter().all(|value| {
            if page["kind"] == "tris" {
                frame(Some(value)).is_some_and(|index| index < counts.vertices)
            } else {
                value.as_array().is_some_and(|pair| {
                    pair.len() == 2
                        && pair
                            .iter()
                            .all(|number| number.as_f64().is_some_and(f64::is_finite))
                })
            }
        })
}
pub(super) fn image_mesh_result(document_id: &str, payload: &Value, result: &Value) -> bool {
    if !image_mesh_request(payload) {
        return false;
    }
    let view = &payload["view"];
    let Some(value) = exact(
        result,
        &[
            "kind",
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "layerUid",
            "frame",
            "workId",
            "viewGeneration",
            "target",
            "rect",
            "mesh",
            "page",
        ],
        &[],
    ) else {
        return false;
    };
    if value["kind"] != "imageMesh"
        || ["atRevision", "documentSnapshotId"]
            .iter()
            .any(|key| value.get(*key) != payload.get(*key))
        || value["documentSnapshotId"].as_str()
            != Some(
                format!(
                    "native-object:{document_id}:{}",
                    payload["atRevision"].as_u64().unwrap()
                )
                .as_str(),
            )
        || ["contextId", "layerUid", "frame", "workId", "viewGeneration"]
            .iter()
            .any(|key| value.get(*key) != view.get(*key))
        || !image_mesh_target(&value["target"], view)
        || !image_mesh_rect(&value["rect"])
    {
        return false;
    }
    let counts = if value["mesh"].is_null() {
        None
    } else {
        match image_mesh_counts(&value["mesh"]) {
            Some(counts) => Some(counts),
            None => return false,
        }
    };
    if view["page"]["kind"] == "selection" {
        return exact(&value["page"], &["kind"], &[]).is_some()
            && value["page"]["kind"] == "selection";
    }
    counts.is_some_and(|counts| {
        image_mesh_page_result(&view["page"], &value["page"], &value["mesh"], &counts)
    })
}
