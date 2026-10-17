//! Atomic admission and semantic JSON round-trip for staged native objects.
#[cfg(feature = "commands")]
pub(crate) use crate::object_document::object_deserialize;
// The codec admission port exposes the immutable types consumed by reads.
pub use crate::object_document::{
    FrameScopeKind, ObjectDocument, ObjectRecord, ObjectTarget, PropertyHolderTarget,
};
#[cfg(feature = "font_asset_admission")]
#[path = "font_asset_admission.rs"]
mod font_assets;
#[cfg(feature = "font_asset_wire")]
pub(crate) use font_assets::admitted_font_source;
#[cfg(feature = "font_asset_admission")]
pub(crate) use font_assets::{
    admit_reserved_font, validate_font, AdmittedFont, FontAssetError, FontAssetLimits,
    FontAssetMetadata,
};

#[cfg(feature = "font_asset_wire")]
pub(crate) use crate::object_document::AssetRecord;
#[cfg(all(feature = "font_asset_wire", not(feature = "history")))]
pub(crate) use crate::object_document::{AssetId, AssetKind};
use crate::object_document::{
    ImageMeshStore, ObjectLayer, OBJECT_DOCUMENT_FORMAT, OBJECT_DOCUMENT_FORMAT_VERSION,
};
pub(crate) use crate::object_document::{Presence, PropertyHolder};
use serde::Deserialize;
use std::collections::HashSet;

#[cfg(all(test, feature = "project_media_codec"))]
pub(crate) use crate::reference_image_asset::decode_png_pixels;
#[cfg(feature = "project_media_codec")]
pub(crate) use crate::reference_image_asset::{
    decode_png_pixels_with_buffers, decode_png_pixels_with_workspace, validate_png_with_buffers,
    validate_png_with_workspace, DecodedPngPixels, ImageAssetError, ImageAssetLimits, PngWorkspace,
};

#[path = "project_media_codec.rs"]
mod project;
// Literal settings wire oracles exercise the real private codec without history.
#[cfg(all(test, feature = "project_media_codec"))]
pub(crate) mod project_settings_test_codec {
    pub(crate) use super::project::{decode_project, encode_project, ProjectWireLimits};
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use crate::object_document::{ProjectEnvelope, ProjectReferenceState};
// L06-H (lane #2112): the lane-6 document kernels the read-queries preparation consumes.
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use crate::object_document::object_raster_asset_kernels::object_raster_asset_admission as raster_asset_admission;
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use crate::object_document::object_raster_asset_kernels::object_raster_asset_admission::png_port as raster_asset_png_port;
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use crate::object_document::object_raster_asset_kernels::object_raster_asset_use as raster_asset_use;
// L06-E (lane #2112): the asset value types the read-queries pixel handle names; read-queries may not import object_document.
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use crate::object_document::{AssetId, AssetInventory, AssetKind};
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use project::{
    decode_allocated_project, decode_validated_project, encode_project as encode_project_envelope,
    prepare_project_wire, DecodedProjectWire, PreparedProjectWire, ProjectWireError,
    ProjectWireLimits,
};

/// Literal wire and lifetime harness for the private managed codec consumer.
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
pub(crate) mod encoded_wire_test {
    use super::ProjectWireLimits;
    pub(crate) fn counted<T>(calls: &std::cell::Cell<usize>, operation: impl FnOnce() -> T) -> T {
        calls.set(calls.get() + 1);
        operation()
    }
    pub(crate) const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAGklEQVR4nGMQVDL+7xKa1sBQ3jGTYdXuM/8BN4oHrU/zhk0AAAAASUVORK5CYII=";
    pub(crate) fn wire() -> serde_json::Value {
        serde_json::json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":{"kind":"still-image","assetId":"media-1","name":"plate.png","visible":true,"opacity":0.5,"offsetFrames":0},
        "assets":[{"id":"media-1","mediaType":"image/png","encoding":"base64","data":PNG}]})
    }
    pub(crate) fn limits() -> ProjectWireLimits {
        ProjectWireLimits::new(4096, 2048)
            .unwrap()
            .with_png_limits(1024, 64, 4096, 4, 4)
            .unwrap()
    }
    pub(crate) struct EncodedDropProbe<'a, R> {
        pub(crate) permit: R,
        pub(crate) weak: &'a std::cell::RefCell<std::sync::Weak<Vec<u8>>>,
    }
    impl<R> Drop for EncodedDropProbe<'_, R> {
        fn drop(&mut self) {
            assert_eq!(self.weak.borrow().strong_count(), 0);
        }
    }
}

#[path = "object_order_codec.rs"]
mod order;
// L01-D (#2107): admission of the editable-vector-path family.
#[path = "object_vector_codec.rs"]
#[allow(dead_code)]
mod object_vector_codec;
// L04-I (#2110): admission of the componentTables member (v2 only).
#[path = "object_component_codec.rs"]
mod object_component_codec;
pub(crate) use object_component_codec::validate_component_tables;
// L02-G (#2108): admission of the per-layer group table.
#[path = "object_group_codec.rs"]
mod object_group_codec;
// L01-E (#2107): admission of the vector extension slot and of the per-layer pathFx stack.
#[path = "object_vector_ext_codec.rs"]
#[allow(dead_code)]
mod object_vector_ext_codec;
// L05-G (#2111): admission of the per-layer range record (inPoint, outPoint, timeLink, markers).
#[path = "object_frame_edit_range_codec.rs"]
#[allow(dead_code)]
mod object_frame_edit_range_codec;
// L06-L (#2112): admission of the lane-6 `items` document member (raster items and text blocks).
#[path = "object_raster_asset_codec.rs"]
#[allow(dead_code)]
mod object_raster_asset_codec;
pub use crate::object_document::OrderedObjectDocument;
#[allow(unused_imports)]
pub(crate) use object_raster_asset_codec::{validate_item_assets, validate_item_parts};
pub use order::{decode_ordered_project, encode_ordered_project, migrate_ordered_project};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectCodecErrorKind {
    Parse,
    Invalid,
    Unsupported,
    DuplicateId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectCodecError {
    kind: ObjectCodecErrorKind,
    message: String,
}
impl ObjectCodecError {
    pub fn kind(&self) -> ObjectCodecErrorKind {
        self.kind
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub(crate) fn new(kind: ObjectCodecErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for ObjectCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ObjectCodecError {}

/// Deserialize directly into strict structs: a Value intermediate would erase
/// duplicate JSON member keys before admission could reject them.
pub fn decode_project(bytes: &[u8]) -> Result<ObjectDocument, ObjectCodecError> {
    let wire: LegacyDocument = parse(bytes)?;
    let document = ObjectDocument {
        format: wire.format,
        format_version: wire.format_version,
        total_frames: wire.total_frames,
        layers: wire.layers,
        objects: wire.objects,
        property_holders: wire.property_holders,
        layer_folders: wire.layer_folders,
        composition_order: Presence::Missing,
        image_meshes: wire.image_meshes,
        items: wire.items,
        component_tables: Presence::Missing,
    };
    validate(&document)?;
    Ok(document)
}

// Canonical v1 still rejects compositionOrder as an unknown member, even null.
struct LegacyDocument {
    format: String,
    format_version: u32,
    total_frames: u32,
    layers: Vec<ObjectLayer>,
    objects: Vec<ObjectRecord>,
    property_holders: Presence<Vec<PropertyHolder>>,
    layer_folders: Presence<Option<crate::object_document::LayerFolders>>,
    image_meshes: Presence<ImageMeshStore>,
    items: Presence<crate::object_document::ItemRecords>,
}
crate::object_document::object_deserialize!(LegacyDocument {
    format: String, format_version: u32, total_frames: u32,
    layers: Vec<ObjectLayer>, objects: Vec<ObjectRecord>,
    #[serde(default)] property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(default)] layer_folders: Presence<Option<crate::object_document::LayerFolders>>,
    #[serde(default)] image_meshes: Presence<ImageMeshStore>,
    #[serde(default)] items: Presence<crate::object_document::ItemRecords>
});

/// Explicit core storage admission; both attempts read the original raw bytes.
pub fn decode_storage_project(bytes: &[u8]) -> Result<ObjectDocument, ObjectCodecError> {
    match decode_project(bytes) {
        Ok(document) => Ok(document),
        Err(legacy_error) => match decode_ordered_project(bytes) {
            Ok(document) => Ok(document.into_storage()),
            Err(_) => Err(legacy_error),
        },
    }
}

pub fn encode_storage_project(document: &ObjectDocument) -> Result<Vec<u8>, ObjectCodecError> {
    validate_storage(document)?;
    serde_json::to_vec(document)
        .map_err(|e| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, e.to_string()))
}

pub(crate) fn validate_storage(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
    if document.format_version == 1 {
        return validate(document);
    }
    let plan = document.composition_order.value().ok_or_else(|| {
        ObjectCodecError::new(
            ObjectCodecErrorKind::Unsupported,
            "v2 requires explicit compositionOrder",
        )
    })?;
    order::validate_storage_order(document, plan)?;
    validate_component_tables(&document.component_tables, &document.layers)
}

pub(crate) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ObjectCodecError> {
    serde_json::from_slice(bytes).map_err(|error| {
        let kind = if error.to_string().contains("unknown field") {
            ObjectCodecErrorKind::Unsupported
        } else if error.is_syntax() || error.is_eof() {
            ObjectCodecErrorKind::Parse
        } else {
            ObjectCodecErrorKind::Invalid
        };
        ObjectCodecError::new(kind, error.to_string())
    })
}

pub fn encode_project(document: &ObjectDocument) -> Result<Vec<u8>, ObjectCodecError> {
    validate(document)?;
    serde_json::to_vec(document)
        .map_err(|error| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, error.to_string()))
}

pub(crate) fn validate(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
    use ObjectCodecErrorKind::Unsupported;
    let reject = |kind, message: &str| ObjectCodecError::new(kind, message);
    if document.format != OBJECT_DOCUMENT_FORMAT
        || document.format_version != OBJECT_DOCUMENT_FORMAT_VERSION
        || !document.composition_order.missing()
        || !document.component_tables.missing()
    {
        return Err(reject(
            Unsupported,
            "unsupported object format or formatVersion",
        ));
    }
    validate_content(
        document.total_frames,
        &document.layers,
        &document.objects,
        false,
    )?;
    validate_layer_organization(&document.layers, &document.layer_folders)?;
    validate_property_holders(
        &document.property_holders,
        &document.layers,
        &document.objects,
        None,
    )?;
    validate_image_meshes(&document.image_meshes)?;
    object_raster_asset_codec::validate_items(document)
}

/// L03-J: typed admission of the optional image-mesh table; absent is valid, present is checked whole.
pub(crate) fn validate_image_meshes(
    store: &Presence<ImageMeshStore>,
) -> Result<(), ObjectCodecError> {
    match store.value() {
        None => Ok(()),
        Some(store) => store.validate().map_err(|error| {
            let kind = if error.is_duplicate() {
                ObjectCodecErrorKind::DuplicateId
            } else {
                ObjectCodecErrorKind::Invalid
            };
            ObjectCodecError::new(kind, error.message())
        }),
    }
}

pub(crate) fn validate_layer_organization(
    layers: &[ObjectLayer],
    folders: &Presence<Option<crate::object_document::LayerFolders>>,
) -> Result<(), ObjectCodecError> {
    crate::object_document::validate_layer_organization(layers, folders)
        .map_err(|message| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message))
}

pub(crate) fn validate_property_holders(
    holders: &Presence<Vec<PropertyHolder>>,
    layers: &[ObjectLayer],
    records: &[ObjectRecord],
    plan: Option<&crate::object_document::OrderPlan>,
) -> Result<(), ObjectCodecError> {
    use crate::object_document::PropertyAdmissionError;
    crate::object_document::validate_property_holders(holders, layers, records, plan).map_err(
        |error| {
            let (kind, message) = match error {
                PropertyAdmissionError::Invalid(message) => {
                    (ObjectCodecErrorKind::Invalid, message)
                }
                PropertyAdmissionError::Unsupported(message) => {
                    (ObjectCodecErrorKind::Unsupported, message)
                }
                PropertyAdmissionError::DuplicateId(message) => {
                    (ObjectCodecErrorKind::DuplicateId, message)
                }
            };
            ObjectCodecError::new(kind, message)
        },
    )
}

pub(crate) fn validate_content(
    total_frames: u32,
    declared_layers: &[ObjectLayer],
    objects: &[ObjectRecord],
    complete_contexts: bool,
) -> Result<(), ObjectCodecError> {
    use ObjectCodecErrorKind::{DuplicateId, Invalid, Unsupported};
    let reject = |kind, message: &str| ObjectCodecError::new(kind, message);
    if total_frames == 0 || declared_layers.is_empty() {
        return Err(reject(
            Invalid,
            "positive totalFrames and identified layers are required",
        ));
    }
    let mut layers = HashSet::new();
    for layer in declared_layers {
        // Drawing Fill enforces its own lock guard. Generic Motion actions
        // retain their permissive baseline semantics; parenting remains separate.
        if layer.parent_layer_uid.value().is_some_and(Option::is_some) {
            return Err(reject(Unsupported, "layer parenting is unavailable"));
        }
        if let Some(color) = layer.color.value() {
            if color.len() != 7
                || !color.starts_with('#')
                || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(reject(
                    Invalid,
                    "layer color must be a six-digit hexadecimal color",
                ));
            }
        }
        if let Some(timing) = layer.timing.value() {
            timing.validate(total_frames).map_err(|error| {
                let (kind, message) = match error {
                    crate::object_document::LayerTimingError::Invalid => {
                        (Invalid, "invalid sparse layer timing")
                    }
                    crate::object_document::LayerTimingError::Unsupported => (
                        Unsupported,
                        "unsupported layer timing format, family or context",
                    ),
                };
                reject(kind, message)
            })?;
        }
        object_vector_ext_codec::validate_layer_path_fx(layer)?;
        object_frame_edit_range_codec::validate_layer_range(layer)?;
        if layer.layer_uid.is_empty() {
            return Err(reject(Invalid, "empty layerUid"));
        }
        if !layers.insert(layer.layer_uid.as_str()) {
            return Err(reject(DuplicateId, "duplicate layerUid"));
        }
        if let Some(table) = layer.groups.value() {
            object_group_codec::validate_group_table(
                &layer.layer_uid,
                table,
                objects,
                complete_contexts,
            )?;
        }
    }
    crate::object_document::validate_widget_layers(declared_layers, objects)
        .map_err(|error| reject(Invalid, error.message()))?;
    crate::object_document::object_rig_record::validate_rig_layers(declared_layers, objects)
        .map_err(|error| {
            let kind = if error.is_duplicate() {
                DuplicateId
            } else if error.is_unsupported() {
                Unsupported
            } else {
                Invalid
            };
            reject(kind, error.message())
        })?;
    // L08-J (#2114): layer compositing fields (blend, mattes, effects stack, kind flags).
    crate::object_document::validate_layer_compositing(declared_layers, objects)
        .map_err(|error| reject(Invalid, error.message()))?;
    let mut targets = HashSet::new();
    for record in objects {
        let vector_family = record.family == crate::object_document::VECTOR_FAMILY;
        if record.schema_version != 1
            || (record.family != "closed-solid-fill-cubic-path" && !vector_family)
        {
            return Err(reject(Unsupported, "unsupported object schema or family"));
        }
        let valid_target = if complete_contexts {
            !record.target.context_id.is_empty()
                && !record.target.layer_uid.is_empty()
                && !record.target.stroke_id.is_empty()
        } else {
            record.target.valid()
        };
        if !valid_target || record.target.frame_scope.frame >= total_frames {
            return Err(reject(Invalid, "invalid scoped object target or frame"));
        }
        if !layers.contains(record.target.layer_uid.as_str()) {
            return Err(reject(Invalid, "object layer reference is unresolved"));
        }
        if !targets.insert(&record.target) {
            return Err(reject(DuplicateId, "duplicate scoped object key"));
        }
        object_vector_ext_codec::validate_vector_ext(record)?;
        if vector_family {
            object_vector_codec::validate_vector_record(record)?;
            continue;
        }
        let geometry = &record.geometry;
        if geometry.kind != "cubic-path"
            || !geometry.closed
            || geometry.coordinate_space != "document-world"
            || geometry.handle_space != "relative"
        {
            return Err(reject(Unsupported, "unsupported cubic geometry"));
        }
        if !(2..=256).contains(&geometry.segments.len()) {
            return Err(reject(Invalid, "cubic geometry requires 2..256 segments"));
        }
        for segment in &geometry.segments {
            for point in [&segment.point, &segment.handle_in, &segment.handle_out] {
                if ![&point.x, &point.y]
                    .into_iter()
                    .all(|n| n.as_f64().is_some_and(f64::is_finite))
                {
                    return Err(reject(
                        Invalid,
                        "cubic coordinates must be finite f64 values",
                    ));
                }
            }
        }
        if !record.vector.missing() {
            return Err(reject(
                Unsupported,
                "vector body requires the editable-vector-path family",
            ));
        }
        let fill = match record.solid_fill() {
            Some(fill) => fill,
            None => return Err(reject(Invalid, "legacy family requires a solid fill")),
        };
        if fill.kind != "solid" {
            return Err(reject(Unsupported, "only solid fill is supported"));
        }
        if !fill.valid_channels() {
            return Err(reject(
                Invalid,
                "solid fill channels must be finite and within 0..1",
            ));
        }
    }
    Ok(())
}
