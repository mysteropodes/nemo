//! Immutable project value at the document floor, not an owner or wire codec.
use crate::object_document::ObjectDocument;
use std::sync::Arc;

#[path = "project_settings.rs"]
mod project_settings;
pub(crate) use project_settings::{ProjectMetadata, ProjectMetadataError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectReferenceError {
    Id,
    Name,
    Opacity,
    Offset,
    EncodedBudget,
    Unavailable,
}

/// Persistent data only: no decoded pixels, source path or runtime handle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StillReference {
    id: String,
    name: String,
    visible: bool,
    opacity: f64,
    offset_frames: i64,
    encoded: Arc<Vec<u8>>,
}
impl StillReference {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn opacity(&self) -> f64 {
        self.opacity
    }
    pub(crate) fn offset_frames(&self) -> i64 {
        self.offset_frames
    }
    pub(crate) fn encoded_png(&self) -> &[u8] {
        &self.encoded
    }
    pub(crate) fn encoded_allocation(&self) -> &Arc<Vec<u8>> {
        &self.encoded
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectReferenceState {
    NoMedia,
    StillImage(Arc<StillReference>),
}
impl ProjectReferenceState {
    fn validate_settings(
        name: &str,
        opacity: f64,
        offset_frames: i64,
    ) -> Result<(), ProjectReferenceError> {
        if name.is_empty()
            || name.len() > 255
            || matches!(name, "." | "..")
            || name.contains(['/', '\\', '\0'])
        {
            return Err(ProjectReferenceError::Name);
        }
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(ProjectReferenceError::Opacity);
        }
        if offset_frames.unsigned_abs() > 9_007_199_254_740_991 {
            return Err(ProjectReferenceError::Offset);
        }
        Ok(())
    }
    /// Changes settings only: the encoded Arc/ID are never replaced or copied.
    pub(crate) fn with_settings(
        &self,
        name: String,
        visible: bool,
        opacity: f64,
        offset_frames: i64,
    ) -> Result<Self, ProjectReferenceError> {
        Self::validate_settings(&name, opacity, offset_frames)?;
        let Self::StillImage(data) = self else {
            return Err(ProjectReferenceError::Unavailable);
        };
        let mut next = (**data).clone();
        next.name = name;
        next.visible = visible;
        next.opacity = opacity;
        next.offset_frames = offset_frames;
        if &next == data.as_ref() {
            return Ok(self.clone());
        }
        Ok(Self::StillImage(Arc::new(next)))
    }
    /// PRIVATE PRECONDITION: encoded bytes have ALREADY passed PNG admission.
    /// Retention/settings only; wire import must decode BEFORE this factory.
    pub(crate) fn admitted_still(
        id: String,
        name: String,
        visible: bool,
        opacity: f64,
        offset_frames: i64,
        encoded: Vec<u8>,
        encoded_limit: usize,
    ) -> Result<Self, ProjectReferenceError> {
        if id.is_empty() || id.len() > 255 || id.contains('\0') {
            return Err(ProjectReferenceError::Id);
        }
        Self::validate_settings(&name, opacity, offset_frames)?;
        if encoded_limit == 0
            || encoded_limit > 16 * 1024 * 1024
            || encoded.is_empty()
            || encoded.len() > encoded_limit
        {
            return Err(ProjectReferenceError::EncodedBudget);
        }
        Ok(Self::StillImage(Arc::new(StillReference {
            id,
            name,
            visible,
            opacity,
            offset_frames,
            encoded: Arc::new(encoded),
        })))
    }
}

impl ProjectReferenceState {
    /// Codec + same-owner quota admission precede this retention-only factory.
    pub(crate) fn admitted_still_shared(
        id: String,
        name: String,
        visible: bool,
        opacity: f64,
        offset_frames: i64,
        encoded: Arc<Vec<u8>>,
        encoded_limit: usize,
    ) -> Result<Self, ProjectReferenceError> {
        if id.is_empty() || id.len() > 255 || id.contains('\0') {
            return Err(ProjectReferenceError::Id);
        }
        Self::validate_settings(&name, opacity, offset_frames)?;
        if encoded_limit == 0
            || encoded_limit > 16 * 1024 * 1024
            || encoded.is_empty()
            || encoded.capacity() > encoded_limit
        {
            return Err(ProjectReferenceError::EncodedBudget);
        }
        Ok(Self::StillImage(Arc::new(StillReference {
            id,
            name,
            visible,
            opacity,
            offset_frames,
            encoded,
        })))
    }
}

#[path = "project_envelope_value.rs"]
mod value;
pub(crate) use value::ProjectEnvelope;

#[cfg(test)]
mod tests {
    use super::ProjectReferenceError;
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
                ObjectLayer::identified("legacy layer:alpha".into()),
                ObjectLayer::identified("0".into()),
            ],
            objects,
            property_holders: Presence::Missing,
            layer_folders: Presence::Missing,
            composition_order: if version == 1 {
                Presence::Missing
            } else {
                Presence::Present(order)
            },
            image_meshes: Presence::Missing,
            items: Presence::Missing,
            component_tables: Presence::Missing,
            motion_arcs: Presence::Missing,
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
            let project = ProjectEnvelope::no_media(metadata.clone(), bare.clone());
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

    const PNG_HEX: &str = "89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
    fn png() -> Vec<u8> {
        PNG_HEX
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    fn still(
        id: &str,
        name: &str,
        opacity: f64,
        offset: i64,
        bytes: Vec<u8>,
        cap: usize,
    ) -> Result<ProjectReferenceState, ProjectReferenceError> {
        ProjectReferenceState::admitted_still(
            id.into(),
            name.into(),
            true,
            opacity,
            offset,
            bytes,
            cap,
        )
    }

    #[test]
    fn attached_value_shares_original_drawing_and_encoded_payload_across_clone_drop() {
        for version in [1, 2] {
            let original = ProjectEnvelope::no_media(
                ProjectMetadata::new(320, 180, 29.97).unwrap(),
                drawing(version),
            );
            let before = serde_json::to_vec(original.drawing()).unwrap();
            let source = png();
            let source_ptr = source.as_ptr();
            let attached = original.with_reference(
                still("media-1", "plate.png", 0.5, -7, source, 16 * 1024 * 1024).unwrap(),
            );
            assert!(std::ptr::eq(original.drawing(), attached.drawing()));
            assert_eq!(original.reference_state(), ProjectReferenceState::NoMedia);
            assert_eq!(attached.project(), original.project());
            assert_eq!(serde_json::to_vec(attached.drawing()).unwrap(), before);
            let ProjectReferenceState::StillImage(data) = attached.reference_state() else {
                panic!()
            };
            assert_eq!(data.id(), "media-1");
            assert_eq!(data.name(), "plate.png");
            assert!(data.visible());
            assert_eq!(data.opacity(), 0.5);
            assert_eq!(data.offset_frames(), -7);
            assert_eq!(data.encoded_png(), png());
            assert_eq!(data.encoded_png().as_ptr(), source_ptr);
            let clone = attached.clone();
            let ProjectReferenceState::StillImage(retained) = clone.reference_state() else {
                panic!()
            };
            assert!(std::sync::Arc::ptr_eq(&data, &retained));
            drop(data);
            drop(attached);
            drop(original);
            assert_eq!(retained.encoded_png().as_ptr(), source_ptr);
            assert_eq!(retained.encoded_png(), png());
            assert_eq!(serde_json::to_vec(clone.drawing()).unwrap(), before);
            assert_eq!(clone.drawing().composition_order.missing(), version == 1);
            assert_eq!(
                clone.drawing().objects()[1].target().scope_kind(),
                &crate::object_document::FrameScopeKind::Reference
            );
        }
    }

    #[test]
    fn id_and_basename_utf8_budgets_are_exact_and_hidden_names_remain_allowed() {
        for good in ["a".repeat(255), "a".repeat(253) + "é", ".hidden.png".into()] {
            assert!(still(&good, &good, 1., 0, png(), 1024).is_ok());
        }
        for bad in [
            String::new(),
            "a".repeat(256),
            "a".repeat(254) + "é",
            "x\0y".into(),
        ] {
            assert_eq!(
                still(&bad, "plate.png", 1., 0, png(), 1024),
                Err(ProjectReferenceError::Id)
            );
        }
        for bad in ["", ".", "..", "dir/plate.png", "dir\\plate.png", "x\0y"] {
            assert_eq!(
                still("media-1", bad, 1., 0, png(), 1024),
                Err(ProjectReferenceError::Name)
            );
        }
        for bad in ["a".repeat(256), "a".repeat(254) + "é"] {
            assert_eq!(
                still("media-1", &bad, 1., 0, png(), 1024),
                Err(ProjectReferenceError::Name)
            );
        }
    }

    #[test]
    fn visibility_opacity_and_signed_safe_offsets_preserve_supplied_values() {
        const SAFE: i64 = 9_007_199_254_740_991;
        for (opacity, offset, visible) in [(0., -SAFE, false), (1., SAFE, true), (-0., 0, false)] {
            let state = ProjectReferenceState::admitted_still(
                "media-1".into(),
                "plate.png".into(),
                visible,
                opacity,
                offset,
                png(),
                1024,
            )
            .unwrap();
            let ProjectReferenceState::StillImage(data) = state else {
                panic!()
            };
            assert_eq!(
                (data.visible(), data.opacity(), data.offset_frames()),
                (visible, opacity, offset)
            );
        }
        for opacity in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
            assert_eq!(
                still("media-1", "plate.png", opacity, 0, png(), 1024),
                Err(ProjectReferenceError::Opacity)
            );
        }
        for offset in [-SAFE - 1, SAFE + 1, i64::MIN, i64::MAX] {
            assert_eq!(
                still("media-1", "plate.png", 0.5, offset, png(), 1024),
                Err(ProjectReferenceError::Offset)
            );
        }
    }

    #[test]
    fn encoded_retention_caps_refuse_without_changing_an_existing_value() {
        const MAX: usize = 16 * 1024 * 1024;
        let original =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), drawing(2));
        let old = original.clone();
        let bytes = png();
        assert!(still("media-1", "plate.png", 1., 0, bytes.clone(), bytes.len()).is_ok());
        for (bytes, cap) in [
            (bytes.clone(), bytes.len() - 1),
            (bytes.clone(), 0),
            (bytes, MAX + 1),
            (Vec::new(), MAX),
            (png(), usize::MAX),
            (vec![0; MAX + 1], MAX),
        ] {
            assert_eq!(
                still("media-1", "plate.png", 1., 0, bytes, cap),
                Err(ProjectReferenceError::EncodedBudget)
            );
            assert_eq!(original, old);
        }
        // Opaque model budget fixture ONLY, not an assertion of PNG validity.
        let bytes = vec![0; MAX];
        let pointer = bytes.as_ptr();
        let ProjectReferenceState::StillImage(data) =
            still("media-1", "plate.png", 1., 0, bytes, MAX).unwrap()
        else {
            panic!()
        };
        assert_eq!(data.encoded_png().len(), MAX);
        assert_eq!(data.encoded_png().as_ptr(), pointer);
    }
}
