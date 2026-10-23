//! Bounded immutable Mesh view; the scene producer supplies the actual raster projection.
use super::{ObjectSnapshot, MAX_MESSAGE_BYTES};
use crate::object_codec::{object_deserialize, ObjectTarget};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::json;

/// Only a retained native scene can construct the source target and final rectangle.
/// The host adds the identity of its actual successful presentation, never caller geometry.
#[derive(Debug, Clone)]
pub struct ImageMeshReadProjection {
    instance_id: String,
    document_id: String,
    revision: u64,
    snapshot_id: String,
    frame: u32,
    target: ObjectTarget,
    mesh_id: Option<String>,
    rect: [f64; 5],
    work_id: String,
    view_generation: u64,
}
impl ImageMeshReadProjection {
    pub(crate) fn raster(
        snapshot: &ObjectSnapshot,
        frame: u32,
        target: ObjectTarget,
        mesh_id: Option<String>,
        rect: [f64; 5],
    ) -> Self {
        Self {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            revision: snapshot.content_revision(),
            snapshot_id: snapshot.snapshot_id().into(),
            frame,
            target,
            mesh_id,
            rect,
            work_id: String::new(),
            view_generation: 0,
        }
    }
    pub fn with_presentation(
        mut self,
        work_id: String,
        view_generation: u64,
    ) -> Result<Self, DispatchErrorCode> {
        if !super::envelope_identifier(&work_id)
            || !(1..=9_007_199_254_740_991).contains(&view_generation)
        {
            return Err(DispatchErrorCode::Unavailable);
        }
        self.work_id = work_id;
        self.view_generation = view_generation;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageMeshReadFailure {
    Unpresented,
    Missing,
    Ambiguous,
    Unsupported,
    Stale,
}
impl ImageMeshReadFailure {
    fn response(self) -> (DispatchErrorCode, &'static str) {
        use DispatchErrorCode::{NotFound, StaleRevision, Unavailable};
        match self {
            Self::Unpresented => (
                Unavailable,
                "Native image mesh requires a current successful presentation.",
            ),
            Self::Missing => (
                NotFound,
                "The selected native layer has no eligible raster.",
            ),
            Self::Ambiguous => (
                Unavailable,
                "The selected native layer has multiple eligible rasters.",
            ),
            Self::Unsupported => (
                Unavailable,
                "The native raster presentation provider is unavailable.",
            ),
            Self::Stale => (StaleRevision, "The native raster presentation changed."),
        }
    }
}

#[derive(Debug)]
struct Selector {
    at_revision: u64,
    document_snapshot_id: String,
    view: View,
}
object_deserialize!(Selector {
    at_revision: u64,
    document_snapshot_id: String,
    view: View
});
#[derive(Debug)]
struct View {
    kind: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    page: Page,
}
object_deserialize!(View {
    kind: String,
    context_id: String,
    layer_uid: String,
    frame: u32,
    work_id: String,
    view_generation: u64,
    page: Page
});

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Page {
    Selection {},
    Outline {
        #[serde(rename = "meshId")]
        mesh_id: String,
        cursor: u32,
        limit: u32,
    },
    Verts {
        #[serde(rename = "meshId")]
        mesh_id: String,
        cursor: u32,
        limit: u32,
    },
    Tris {
        #[serde(rename = "meshId")]
        mesh_id: String,
        cursor: u32,
        limit: u32,
    },
    Offsets {
        #[serde(rename = "meshId")]
        mesh_id: String,
        cursor: u32,
        limit: u32,
    },
}
impl Page {
    fn parts(&self) -> Option<(&'static str, &str, usize, usize)> {
        match self {
            Self::Selection {} => None,
            Self::Outline {
                mesh_id,
                cursor,
                limit,
            } => Some(("outline", mesh_id, *cursor as usize, *limit as usize)),
            Self::Verts {
                mesh_id,
                cursor,
                limit,
            } => Some(("verts", mesh_id, *cursor as usize, *limit as usize)),
            Self::Tris {
                mesh_id,
                cursor,
                limit,
            } => Some(("tris", mesh_id, *cursor as usize, *limit as usize)),
            Self::Offsets {
                mesh_id,
                cursor,
                limit,
            } => Some(("offsets", mesh_id, *cursor as usize, *limit as usize)),
        }
    }
}

impl ObjectSnapshot {
    pub(super) fn dispatch_image_mesh_read(
        &self,
        request: &OpacityRequest,
        projection: Result<&ImageMeshReadProjection, ImageMeshReadFailure>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        let fail = |code, message| Ok(self.read_failure(request, code, message));
        let selector: Selector = match serde_json::from_value(request.payload.clone()) {
            Ok(value) => value,
            Err(_) => return fail(InvalidRequest, "Invalid closed native image mesh selector."),
        };
        let view = &selector.view;
        if selector.at_revision > 9_007_199_254_740_991
            || selector.document_snapshot_id.is_empty()
            || view.kind != "imageMesh"
            || view.context_id.is_empty()
            || view.layer_uid.is_empty()
            || !super::envelope_identifier(&view.work_id)
            || !(1..=9_007_199_254_740_991).contains(&view.view_generation)
            || view
                .page
                .parts()
                .is_some_and(|(_, id, _, limit)| id.is_empty() || !(1..=16).contains(&limit))
        {
            return fail(InvalidRequest, "Invalid native image mesh selector values.");
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return fail(StaleRevision, "Native image mesh snapshot changed.");
        }
        let projection = match projection {
            Ok(projection) => projection,
            Err(reason) => {
                let (code, message) = reason.response();
                return fail(code, message);
            }
        };
        if projection.instance_id != self.instance_id()
            || projection.document_id != self.document_id()
            || projection.revision != self.content_revision()
            || projection.snapshot_id != self.snapshot_id()
            || projection.frame != view.frame
        {
            return fail(
                StaleRevision,
                "Native image mesh source and presented frame do not match the snapshot.",
            );
        }
        if projection.work_id != view.work_id || projection.view_generation != view.view_generation
        {
            return fail(StaleRevision, "Native image mesh presentation changed.");
        }
        let target = &projection.target;
        if target.context_id() != view.context_id
            || target.layer_uid() != view.layer_uid
            || target.frame() > view.frame
            || projection.rect.iter().any(|n| !n.is_finite())
            || projection.rect[2] <= 0.
            || projection.rect[3] <= 0.
        {
            return fail(
                Unavailable,
                "Native image mesh target is not the selected presented raster.",
            );
        }
        let Some(entry) = self
            .document()
            .item_records()
            .into_iter()
            .flat_map(|items| items.raster_entries())
            .find(|entry| &entry.target == target)
        else {
            return fail(NotFound, "Native image mesh raster is absent.");
        };
        if entry.item.mesh_id() != projection.mesh_id.as_deref() {
            return fail(
                Unavailable,
                "Native image mesh binding does not match the retained raster.",
            );
        }
        let mesh = match projection.mesh_id.as_deref() {
            Some(id) => match self
                .document()
                .image_meshes()
                .and_then(|store| store.get(id))
            {
                Some(mesh) => Some(mesh),
                None => return fail(NotFound, "Native image mesh binding has no retained mesh."),
            },
            None => None,
        };
        let metadata = mesh.map(|mesh| {
            let mut value = json!({"meshId":projection.mesh_id,"outlineCount":mesh.outline.len(),
                "vertexCount":mesh.verts.len(),"triangleIndexCount":mesh.tris.len(),"offsetsPresent":mesh.offsets_authored()});
            if let Some(cols) = mesh.cols.value() { value["cols"] = json!(cols); }
            if let Some(rows) = mesh.rows.value() { value["rows"] = json!(rows); }
            value
        });
        let [x, y, width, height, rotation] = projection.rect;
        let mut result = json!({"kind":"imageMesh","atRevision":self.content_revision(),
            "documentSnapshotId":self.snapshot_id(),"contextId":view.context_id,"layerUid":view.layer_uid,
            "frame":view.frame,"workId":view.work_id,"viewGeneration":view.view_generation,
            "target":target,"rect":{"x":x,"y":y,"width":width,"height":height,"rotation":rotation},"mesh":metadata});
        let parts = view.page.parts();
        let (kind, cursor, limit, total) = if let Some((kind, mesh_id, cursor, limit)) = parts {
            if projection.mesh_id.as_deref() != Some(mesh_id) {
                return fail(
                    NotFound,
                    "Native image mesh page names another mesh binding.",
                );
            }
            let Some(mesh) = mesh else {
                return fail(NotFound, "Native raster has no mesh pages.");
            };
            let total = match kind {
                "outline" => mesh.outline.len(),
                "verts" => mesh.verts.len(),
                "tris" => mesh.tris.len(),
                "offsets" => match mesh.offsets.value() {
                    Some(offsets) => offsets.len(),
                    None => {
                        return fail(NotFound, "Native image mesh offsets are authored absent.")
                    }
                },
                _ => unreachable!(),
            };
            if cursor > total {
                return fail(
                    InvalidRequest,
                    "Native image mesh page cursor is outside the field.",
                );
            }
            (kind, cursor, limit, total)
        } else {
            ("selection", 0, 0, 0)
        };
        let mut length = limit.min(total - cursor);
        loop {
            result["page"] = if kind == "selection" {
                json!({"kind":"selection"})
            } else {
                let mesh = mesh.expect("data page requires a retained mesh");
                let end = cursor + length;
                let items = match kind {
                    "outline" => serde_json::to_value(&mesh.outline[cursor..end]),
                    "verts" => serde_json::to_value(&mesh.verts[cursor..end]),
                    "tris" => serde_json::to_value(&mesh.tris[cursor..end]),
                    "offsets" => serde_json::to_value(
                        &mesh.offsets.value().expect("authored offsets")[cursor..end],
                    ),
                    _ => unreachable!(),
                }
                .map_err(|_| Unavailable)?;
                json!({"kind":kind,"cursor":cursor,"total":total,"nextCursor":(end < total).then_some(end),"items":items})
            };
            let response = ResponseEnvelope {
                api_version: request.api_version,
                request_id: request.request_id.clone(),
                instance_id: self.instance_id().into(),
                document_id: self.document_id().into(),
                content_revision: self.content_revision(),
                ok: true,
                result: Some(result.clone()),
                error: None,
            };
            if serde_json::to_vec(&response)
                .map_err(|_| Unavailable)?
                .len()
                <= MAX_MESSAGE_BYTES
            {
                return Ok(response);
            }
            if length <= 1 {
                return fail(
                    Unavailable,
                    "Native image mesh response cannot fit the unchanged byte bound.",
                );
            }
            length -= 1;
        }
    }
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_image_mesh_read.rs"]
mod tests;
