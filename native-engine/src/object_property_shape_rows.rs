//! A dynamic shape's own Motion rows (baseline `propsFor`: `paramShape` kind rect / ellipse / star), keyable per element.
use crate::object_document::{FrameScopeKind, ObjectDocument};

pub(crate) fn shape_rows(document: &ObjectDocument, context_id: &str, layer_uid: &str, stroke: &str) -> &'static [&'static str] {
    let kind = crate::object_snapshot::property_metadata_read::metadata_context_parts(document, context_id)
        .1
        .iter()
        .find(|r| {
            r.target.context_id == context_id
                && r.target.layer_uid == layer_uid
                && r.target.stroke_id == stroke
                && r.target.frame_scope.kind == FrameScopeKind::Authored
        })
        .and_then(|r| r.vector_ext.value())
        .and_then(|e| e.param_shape.value())
        .map(|p| p.kind.as_str());
    match kind {
        Some("rect") => &["cornerTL", "cornerTR", "cornerBR", "cornerBL"],
        Some("ellipse") => &["arcStart", "arcSweep", "arcInner"],
        Some("star") => &["starInner", "starCorner"],
        _ => &[],
    }
}
