//! Immutable project value at the document floor, not an owner or wire codec.
use crate::object_document::ObjectDocument;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectMetadataError {
    InvalidCanvas,
    InvalidFps,
}

/// Supplied metadata only. This model never invents values for a bare drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectMetadata {
    canvas_width: u32,
    canvas_height: u32,
    fps: f64,
}
impl ProjectMetadata {
    pub(crate) fn new(
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<Self, ProjectMetadataError> {
        if canvas_width == 0 || canvas_height == 0 {
            return Err(ProjectMetadataError::InvalidCanvas);
        }
        if !fps.is_finite() || fps <= 0.0 {
            return Err(ProjectMetadataError::InvalidFps);
        }
        Ok(Self {
            canvas_width,
            canvas_height,
            fps,
        })
    }
    pub(crate) fn canvas_dimensions(&self) -> (u32, u32) {
        (self.canvas_width, self.canvas_height)
    }
    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }
}

/// The later attached-media variant extends this state in the SAME envelope.
/// No asset, source path, runtime handle or decoded/GPU data exists here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectReferenceState {
    NoMedia,
}

/// Later ProjectRevisionValue::Project stores this value in the single owner.
/// Bare drawing compatibility remains separate; that integration is not present.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectEnvelope {
    project: ProjectMetadata,
    drawing: Arc<ObjectDocument>,
    reference: ProjectReferenceState,
}
impl ProjectEnvelope {
    /// The input drawing has ALREADY passed the existing strict storage codec.
    /// Metadata validation is embodied by ProjectMetadata; no second admission,
    /// migration, history, decoder or wire interpretation runs at this floor.
    pub(crate) fn no_media(project: ProjectMetadata, drawing: ObjectDocument) -> Self {
        Self {
            project,
            drawing: Arc::new(drawing),
            reference: ProjectReferenceState::NoMedia,
        }
    }
    pub(crate) fn project(&self) -> &ProjectMetadata {
        &self.project
    }
    pub(crate) fn drawing(&self) -> &ObjectDocument {
        &self.drawing
    }
    pub(crate) fn reference_state(&self) -> ProjectReferenceState {
        self.reference
    }
}

#[cfg(test)]
mod tests {
    use crate::object_document::{
        ObjectDocument, ObjectLayer, ObjectRecord, OrderPlan, Presence, ProjectEnvelope,
        ProjectMetadata, ProjectMetadataError, ProjectReferenceState, OBJECT_DOCUMENT_FORMAT,
    };
    use serde_json::{json, Value};

    // Document-floor fixtures only: codec regression suites establish raw-wire
    // admission separately. No reverse dependency on a decoder or an owner.
    fn drawing(version: u32) -> ObjectDocument {
        let records: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let objects: Vec<ObjectRecord> =
            serde_json::from_value(records["records"].clone()).unwrap();
        let order: OrderPlan = serde_json::from_value(json!({
            "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{"static":[0]}},
                {"layerUid":"0","naturalPosition":1,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"
        })).unwrap();
        ObjectDocument {
            format: OBJECT_DOCUMENT_FORMAT.into(),
            format_version: version,
            total_frames: 21,
            layers: vec![
                ObjectLayer {
                    layer_uid: "legacy layer:alpha".into(),
                },
                ObjectLayer {
                    layer_uid: "0".into(),
                },
            ],
            objects,
            composition_order: if version == 1 {
                Presence::Missing
            } else {
                Presence::Present(order)
            },
        }
    }

    #[test]
    fn project_metadata_is_supplied_exactly_without_defaults_or_unit_conversion() {
        for (width, height, fps) in [(320, 180, 24.), (1920, 1080, 29.97), (1, 1, 0.5)] {
            let project = ProjectMetadata::new(width, height, fps).unwrap();
            assert_eq!(project.canvas_dimensions(), (width, height));
            assert_eq!(project.fps(), fps);
        }
        assert_ne!(
            ProjectMetadata::new(320, 180, 24.).unwrap(),
            ProjectMetadata::new(320, 180, 30.).unwrap()
        );
    }

    #[test]
    fn invalid_supplied_metadata_refuses_instead_of_guessing_a_project() {
        for (width, height) in [(0, 180), (320, 0), (0, 0)] {
            assert_eq!(
                ProjectMetadata::new(width, height, 24.).unwrap_err(),
                ProjectMetadataError::InvalidCanvas
            );
        }
        for fps in [0., -0., -1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                ProjectMetadata::new(320, 180, fps).unwrap_err(),
                ProjectMetadataError::InvalidFps
            );
        }
    }

    #[test]
    fn no_media_projects_preserve_bare_v1_v2_order_and_reference_records() {
        for version in [1, 2] {
            let bare = drawing(version);
            let before = serde_json::to_vec(&bare).unwrap();
            let metadata = ProjectMetadata::new(320, 180, 24.).unwrap();
            let project = ProjectEnvelope::no_media(metadata, bare.clone());
            assert_eq!(project.project(), &metadata);
            assert_eq!(project.reference_state(), ProjectReferenceState::NoMedia);
            assert_eq!(project.drawing().format_version(), version);
            assert_eq!(project.drawing().total_frames(), 21);
            assert_eq!(serde_json::to_vec(project.drawing()).unwrap(), before);
            assert_eq!(serde_json::to_vec(&bare).unwrap(), before);
            assert_eq!(
                project.drawing().objects()[1].target().scope_kind(),
                &crate::object_document::FrameScopeKind::Reference
            );
            assert_eq!(
                project.drawing().objects()[0].target().layer_uid(),
                "legacy layer:alpha"
            );
            assert_eq!(project.drawing().objects()[1].target().layer_uid(), "0");
            assert_eq!(project.drawing().composition_order.missing(), version == 1);
        }
    }

    #[test]
    fn clone_retains_one_immutable_drawing_and_survives_other_value_drop() {
        let project =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), drawing(2));
        let clone = project.clone();
        let before = serde_json::to_vec(project.drawing()).unwrap();
        assert!(std::ptr::eq(project.drawing(), clone.drawing()));
        drop(project);
        assert_eq!(serde_json::to_vec(clone.drawing()).unwrap(), before);
        assert_eq!(clone.reference_state(), ProjectReferenceState::NoMedia);
    }

    #[test]
    fn supplied_metadata_does_not_alias_or_reinterpret_the_drawing() {
        let bare = drawing(2);
        let before = serde_json::to_vec(&bare).unwrap();
        let a =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), bare.clone());
        let b = ProjectEnvelope::no_media(ProjectMetadata::new(640, 360, 60.).unwrap(), bare);
        assert_eq!(serde_json::to_vec(a.drawing()).unwrap(), before);
        assert_eq!(serde_json::to_vec(b.drawing()).unwrap(), before);
        assert_ne!(a.project(), b.project());
        assert_eq!(a.reference_state(), ProjectReferenceState::NoMedia);
        assert_eq!(b.reference_state(), ProjectReferenceState::NoMedia);
    }
}
