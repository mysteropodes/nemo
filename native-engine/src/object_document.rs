//! Staged N25A records; distinct from the active opacity project family.
use serde::{Deserialize, Serialize};

pub const OBJECT_DOCUMENT_FORMAT: &str = "nemo.native-object-document";
pub const OBJECT_DOCUMENT_FORMAT_VERSION: u32 = 1;

#[path = "object_order_document.rs"]
mod order;
pub use order::OrderedObjectDocument;
pub(crate) use order::*;

#[path = "object_properties.rs"]
mod properties;
pub(crate) use properties::exposed::{
    is_colour_field, metadata as exposed_metadata, without_exposed,
};
pub(crate) use properties::{validate_property_holders, ExpressionHolder, PropertyAdmissionError};
pub use properties::{
    ExpressionControl, PropertyHolder, PropertyHolderTarget, PropertyMetadata, PropertyRecord,
};
#[cfg(test)]
#[path = "../tests/object_properties.rs"]
mod property_tests;

#[path = "project_asset_inventory.rs"]
mod assets;
pub(crate) use assets::offset as placement_offset;
pub(crate) use assets::wave as audio_wave;
#[path = "object_audio_track_record.rs"]
mod audio_tracks;
pub(crate) use assets::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
};
pub(crate) use audio_tracks::edit as audio_track_edit;
pub(crate) use audio_tracks::{AudioTrackRecord, AudioTracks};

#[path = "project_media_envelope.rs"]
mod project;
pub(crate) use project::{
    BrushPresetRecord, ProjectCommentRecord, ProjectEnvelope, ProjectMetadata,
    ProjectMetadataError, ProjectPresetCatalog, ProjectReferenceState, ProjectWorkspaceRecord,
    SymmetrySettings,
};
#[path = "project_camera_state.rs"]
mod camera;
pub(crate) use camera::{ProjectCameraKey, ProjectCameraState};

#[path = "object_layer_timing.rs"]
mod timing;
pub(crate) use timing::range::{RangeError, RangeEvaluator, ResolvedRange};
#[allow(unused_imports)]
pub(crate) use timing::TimingEdit;
pub(crate) use timing::{LayerTiming, LayerTimingError};
#[allow(unused_imports)]
pub(crate) use timing::{RetimeEdit, RetimePair, RetimePlan};
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_timing.rs"]
mod timing_tests;
// L03-J (#2109, F17): lane-3 image-mesh store record; the document carries it as an optional member.
#[path = "object_image_mesh_record.rs"]
#[allow(dead_code)]
mod object_image_mesh_record;
#[allow(unused_imports)]
pub(crate) use object_image_mesh_record::{
    ImageMesh, ImageMeshAdmissionError, ImageMeshStore, MeshIdSequence,
};
// L09-A (#2126): private lane-9 media timing aggregator; its kernels are private children there.
#[path = "object_media_timing_core.rs"]
#[allow(dead_code)]
pub(crate) mod media_timing;
// L02-A (#2117): lane-2 group/boolean/clipboard/lifecycle family aggregator.
#[path = "object_group_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_kernels;
// L02-G (#2108): the per-layer group table stored on `ObjectLayer`.
pub(crate) use object_group_kernels::object_group_record::GroupTable;
// L06-C (#2137): lane-6 raster asset family aggregator.
#[path = "object_raster_asset_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_raster_asset_kernels;
// L06-A/B/F (#2135, #2136, #2175) re-homed by L06-L (#2112): the text layout / glyph family aggregator is declared from
// the document layer because the text block record is a document member (object_raster_asset_record.rs).
#[path = "object_text_layout_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_text_layout_kernels;
// L06-L (#2112): raster items and text blocks as the additive `items` document member.
#[path = "object_raster_asset_record.rs"]
#[allow(dead_code)]
mod item_records;
#[allow(unused_imports)]
pub(crate) use item_records::{
    font_asset_issue, item_asset_issue, raster_asset_issue, text_holder_id, ItemAssetIssue,
    ItemRecords, RasterEntry, TextEntry,
};
// L04-G (#2110): lane-4 component/StoryBoard/duplicator record family aggregator.
#[path = "object_component_record_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_record_kernels;

// L01-D (#2107): editable-vector-path record body (storage DTO; admission is object_vector_codec.rs).
#[path = "object_vector_record.rs"]
#[allow(dead_code)]
mod vector_record;
pub(crate) use vector_record::{VectorBody, VECTOR_FAMILY};

// L04-I (#2110): optional lane-4 component/StoryBoard/duplicator tables (a v2 member).
#[path = "object_component_document.rs"]
#[allow(dead_code)]
mod object_component_document;
#[allow(unused_imports)]
pub(crate) use object_component_document::{
    resolve_sound_assets, ComponentTables, SoundAssetError,
};

// L03-K (#2109): widget (rig control) layer record, F19; a private child like the other document members.
#[path = "object_widget_record.rs"]
#[allow(dead_code)]
mod object_widget_record;
#[allow(unused_imports)]
pub(crate) use object_widget_record::{
    validate_widget_layers, AxisBinding, AxisUnresolved, WidgetAdmissionError, WidgetAxis,
    WidgetBindings, WidgetLayerKind, WidgetRecord, WidgetRest,
};
// L03-N (#2109): native rig record (I11, the stored `ld.rig`); its admission and link-report files are private children of it.
#[path = "object_rig_record.rs"]
#[allow(dead_code)]
pub(crate) mod object_rig_record;
// L01-E (#2107): pressure fields, paramShape record, per-layer pathFx stack record, vector extension slot
// (storage DTOs only; their admission is object_vector_ext_codec.rs under the codec).
#[path = "object_vector_pressure.rs"]
#[allow(dead_code)]
mod vector_pressure;
pub(crate) use vector_pressure::{PressureFields, STROKE_PROFILE_KINDS};
#[path = "object_parametric_record.rs"]
#[allow(dead_code)]
mod parametric_record;
pub(crate) use parametric_record::{ParamShape, PARAM_SHAPE_KINDS};
#[path = "object_path_effects_record.rs"]
#[allow(dead_code)]
mod path_effects_record;
pub(crate) use path_effects_record::{PathFxRecord, PATH_FX_TYPE_IDS};
#[path = "object_bitmap_brush_record.rs"]
pub(crate) mod bitmap_brush;
pub(crate) use bitmap_brush::{BitmapBrushFields, BitmapBrushSpec};

#[path = "object_vector_ext.rs"]
#[allow(dead_code)]
mod vector_ext;
pub(crate) use vector_ext::{BrushTexture, FillTrace, StoredGradient, StoredGradientStop, VectorExt};

// L08-J (#2114): layer compositing fields; transactions are scene_effect_layer_edit.rs / _history.rs.
#[path = "scene_effect_layer_record.rs"]
#[allow(dead_code)]
mod scene_effect_layer_record;
#[allow(unused_imports)]
pub(crate) use scene_effect_layer_record::{
    blend_write, freeze_legacy_matte_sources, remap_compositing_refs, validate_layer_compositing,
    LayerEffect, MatteRef, BLEND_MODE_NAMES, MATTE_MODE_NAMES,
};

#[path = "object_layer_organization.rs"]
mod organization;
pub(crate) use organization::validate_layer_organization;
pub use organization::{LayerFolder, LayerFolders};
#[cfg(all(test, feature = "codec"))]
#[path = "../tests/object_layer_organization.rs"]
mod layer_organization_tests;
#[path = "object_layer_policy.rs"]
mod layer_policy;
// L05-G (#2111): the stored layer range (inPoint, outPoint, timeLink, markers) and the private range kernel it owns.
#[path = "object_frame_edit_range_records.rs"]
#[allow(dead_code)]
mod range_records;
#[allow(unused_imports)]
pub(crate) use range_records::{
    marker_bounds, markers_with_bounds, split_layer_range, FrameBound, FrameEditKind, LayerRange,
    LayerRole, MarkerRecord, RangeEdit, RangeEditError, RangeRecordError, RangeSplit,
    TimeLinkRecord,
};
// #2106 size split: the object record DTOs (target, frame scope, geometry, solid fill), moved verbatim.
#[path = "object_record.rs"]
mod object_record;
pub use object_record::{
    FrameScope, FrameScopeKind, ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment,
    ObjectTarget, SolidFill,
};

#[path = "object_motion_arcs.rs"]
mod object_motion_arcs;
#[allow(unused_imports)]
pub(crate) use object_motion_arcs::{KeyLockMode, MotionArcError, MotionArcHandles, MotionArcs};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectDocument {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) layer_folders: Presence<Option<LayerFolders>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) composition_order: Presence<OrderPlan>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) image_meshes: Presence<ImageMeshStore>,
    /// Raster items and text blocks (L06-L); absent when the document holds none.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) items: Presence<ItemRecords>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) component_tables: Presence<ComponentTables>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) motion_arcs: Presence<Option<MotionArcs>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) audio_tracks: Presence<AudioTracks>,
}

#[path = "object_follow_path_record.rs"]
mod follow_path;
pub(crate) use follow_path::FollowPathRecord;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectLayer {
    pub(crate) layer_uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) three_d: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) visible: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) locked: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) key_lock: Presence<Option<KeyLockMode>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) color: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) parent_layer_uid: Presence<Option<String>>,
    /// Original content-free Null role and rest marker. Missing/null are kept on wire.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_null_layer: Presence<Option<bool>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) null_pos: Presence<Option<[serde_json::Number; 2]>>,
    /// Original ld.followPath; absent, explicit null and authored object stay distinct.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) follow_path: Presence<Option<FollowPathRecord>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) folder_id: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) shy: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) timing: Presence<LayerTiming>,
    /// Ordered non-destructive path effects (baseline `ld.pathFx`); absent differs from an empty stack.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) path_fx: Presence<Vec<PathFxRecord>>,
    /// Present exactly for a widget (rig control) layer; its presence is the baseline `isWidgetLayer`.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) widget: Presence<WidgetRecord>,
    /// Absent: the layer never had a group table. Present: an authored table (possibly empty).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) groups: Presence<GroupTable>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) blend_mode: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) matte_mode: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) matte_source_layer_uid: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mattes_more: Presence<Vec<MatteRef>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_effect_layer: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_folder_layer: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) folder_collapsed: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) effects: Presence<Vec<LayerEffect>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) effects_from: Presence<String>,
    /// Present once a layer is rigged (baseline `ld.rig`); an empty rig is present, absent never had one.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rig: Presence<object_rig_record::RigRecord>,
    /// F11 (L05-G): the stored inPoint / outPoint / timeLink / markers. Absent: the layer never had any of them.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) range: Presence<LayerRange>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) text_animators: Presence<object_text_layout_kernels::animator::TextAnimators>,
}

// Serde's derived struct decoder also accepts positional sequences. These
// schema objects must enter through visit_map; Fields retains strict duplicate
// and unknown-member checks while nested DTOs apply the same object-only rule.
macro_rules! object_deserialize {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct ObjectVisitor;
                impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a JSON object")
                    }
                    fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<$name, A::Error> {
                        #[derive(serde::Deserialize)]
                        #[serde(deny_unknown_fields, rename_all = "camelCase")]
                        struct Fields { $($(#[$attr])* $field: $ty),* }
                        let fields = Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                        Ok($name { $($field: fields.$field),* })
                    }
                }
                deserializer.deserialize_map(ObjectVisitor)
            }
        }
    };
}
pub(crate) use object_deserialize;

object_deserialize!(ObjectDocument { format: String, format_version: u32, total_frames: u32,
    layers: Vec<ObjectLayer>, objects: Vec<ObjectRecord>,
    #[serde(default)] property_holders: Presence<Vec<PropertyHolder>>,
    #[serde(default)] layer_folders: Presence<Option<LayerFolders>>,
    #[serde(default)] composition_order: Presence<OrderPlan>,
    #[serde(default)] image_meshes: Presence<ImageMeshStore>,
    #[serde(default)] items: Presence<ItemRecords>,
    #[serde(default)] component_tables: Presence<ComponentTables>,
    #[serde(default)] motion_arcs: Presence<Option<MotionArcs>>,
    #[serde(default)] audio_tracks: Presence<AudioTracks> });
object_deserialize!(ObjectLayer { layer_uid: String, #[serde(default)] name: Presence<String>,
    #[serde(default)] three_d: Presence<bool>,
    #[serde(default)] visible: Presence<bool>, #[serde(default)] locked: Presence<bool>,
    #[serde(default)] key_lock: Presence<Option<KeyLockMode>>,
    #[serde(default)] color: Presence<String>,
    #[serde(default)] parent_layer_uid: Presence<Option<String>>,
    #[serde(default)] is_null_layer: Presence<Option<bool>>,
    #[serde(default)] null_pos: Presence<Option<[serde_json::Number; 2]>>,
    #[serde(default)] follow_path: Presence<Option<FollowPathRecord>>,
    #[serde(default)] folder_id: Presence<Option<String>>,
    #[serde(default)] shy: Presence<bool>,
    #[serde(default)] timing: Presence<LayerTiming>,
    #[serde(default)] path_fx: Presence<Vec<PathFxRecord>>,
    #[serde(default)] widget: Presence<WidgetRecord>,
    #[serde(default)] groups: Presence<GroupTable>,
    #[serde(default)] blend_mode: Presence<Option<String>>, #[serde(default)] matte_mode: Presence<Option<String>>,
    #[serde(default)] matte_source_layer_uid: Presence<String>, #[serde(default)] mattes_more: Presence<Vec<MatteRef>>,
    #[serde(default)] is_effect_layer: Presence<bool>, #[serde(default)] is_folder_layer: Presence<bool>,
    #[serde(default)] folder_collapsed: Presence<bool>, #[serde(default)] effects: Presence<Vec<LayerEffect>>,
    #[serde(default)] effects_from: Presence<String>,
    #[serde(default)] rig: Presence<object_rig_record::RigRecord>,
    #[serde(default)] text_animators: Presence<object_text_layout_kernels::animator::TextAnimators>,
    #[serde(default)] range: Presence<LayerRange> });

impl ObjectDocument {
    /// Organizational folder metadata, not structural folder layers.
    pub fn layer_folders(&self) -> Option<&LayerFolders> {
        self.layer_folders.value().and_then(Option::as_ref)
    }
    /// Immutable authored holder metadata; absence is not an empty authored table.
    pub fn property_holders(&self) -> Option<&[PropertyHolder]> {
        self.property_holders.value().map(Vec::as_slice)
    }
    /// Immutable authored image-mesh table; absence is not an empty table.
    #[allow(dead_code)]
    pub(crate) fn image_meshes(&self) -> Option<&ImageMeshStore> {
        self.image_meshes.value()
    }
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    pub(crate) fn valid_target(&self, target: &ObjectTarget) -> bool {
        if self.format_version == 2 {
            !target.context_id.is_empty()
                && !target.layer_uid.is_empty()
                && !target.stroke_id.is_empty()
        } else {
            target.valid()
        }
    }
    /// Storage accepts Reference records; v2 behavior is not designed yet.
    pub(crate) fn reference_unavailable(&self, target: &ObjectTarget) -> bool {
        self.format_version == 2 && target.frame_scope.kind == FrameScopeKind::Reference
    }
    pub fn total_frames(&self) -> u32 {
        self.total_frames
    }
    pub fn objects(&self) -> &[ObjectRecord] {
        &self.objects
    }
    pub fn layers(&self) -> &[ObjectLayer] {
        &self.layers
    }
}
impl ObjectLayer {
    pub(crate) fn is_null_layer(&self) -> bool {
        self.is_null_layer.value() == Some(&Some(true))
    }
    /// Original Null rest pivot, never content bounds. The caller may supply
    /// only dimensions read from its retained project, not a drawing default.
    pub(crate) fn null_pivot_base(
        &self,
        canvas: Option<(u32, u32)>,
    ) -> Result<Option<[f64; 2]>, ()> {
        if !self.is_null_layer() {
            return Ok(None);
        }
        if let Some(Some(pos)) = self.null_pos.value() {
            let pair = [pos[0].as_f64().ok_or(())?, pos[1].as_f64().ok_or(())?];
            return if pair.iter().all(|v| v.is_finite()) {
                Ok(Some(pair))
            } else {
                Err(())
            };
        }
        let (width, height) = canvas.ok_or(())?;
        Ok(Some([f64::from(width) / 2.0, f64::from(height) / 2.0]))
    }
    /// UI row filtering only: shy never changes visibility or content ownership.
    pub fn shy(&self) -> bool {
        self.shy.value().copied().unwrap_or(false)
    }
    /// Legacy falsy memberships have no active folder, but retain wire presence.
    pub fn folder_id(&self) -> Option<&str> {
        self.folder_id
            .value()
            .and_then(Option::as_deref)
            .filter(|id| !id.is_empty())
    }
    /// Native New and Add share the existing desktop creation defaults. The
    /// palette is selected by the number of layers before insertion, not revision.
    pub(crate) fn blank_drawing(layer_uid: String, name: String, existing_layers: usize) -> Self {
        const COLORS: [&str; 8] = [
            "#5FA875", "#B49BE6", "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4",
        ];
        Self {
            layer_uid,
            name: Presence::Present(name),
            visible: Presence::Present(true),
            locked: Presence::Present(false),
            color: Presence::Present(COLORS[existing_layers % COLORS.len()].into()),
            parent_layer_uid: Presence::Present(None),
            folder_id: Presence::Missing,
            shy: Presence::Missing,
            timing: Presence::Present(LayerTiming::blank_drawing()),
            ..Self::identified(String::new())
        }
    }
    /// Identified, unnamed layer with earlier native-file visibility semantics.
    pub(crate) fn identified(layer_uid: String) -> Self {
        Self {
            layer_uid,
            three_d: Presence::Missing,
            name: Presence::Missing,
            visible: Presence::Missing,
            locked: Presence::Missing,
            key_lock: Presence::Missing,
            color: Presence::Missing,
            parent_layer_uid: Presence::Missing,
            is_null_layer: Presence::Missing,
            null_pos: Presence::Missing,
            follow_path: Presence::Missing,
            folder_id: Presence::Missing,
            shy: Presence::Missing,
            timing: Presence::Missing,
            path_fx: Presence::Missing,
            widget: Presence::Missing,
            groups: Presence::Missing,
            blend_mode: Presence::Missing,
            matte_mode: Presence::Missing,
            matte_source_layer_uid: Presence::Missing,
            mattes_more: Presence::Missing,
            is_effect_layer: Presence::Missing,
            is_folder_layer: Presence::Missing,
            folder_collapsed: Presence::Missing,
            effects: Presence::Missing,
            effects_from: Presence::Missing,
            rig: Presence::Missing,
            range: Presence::Missing,
            text_animators: Presence::Missing,
        }
    }
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    /// Native stored name, without inventing one for earlier unnamed files.
    pub fn name(&self) -> Option<&str> {
        self.name.value().map(String::as_str)
    }
    pub fn visible(&self) -> bool {
        self.visible.value().copied().unwrap_or(true)
    }
    /// The stored ordered path-effect stack; `None` when the layer has none (an empty stack is `Some(&[])`).
    pub(crate) fn path_fx(&self) -> Option<&[PathFxRecord]> {
        self.path_fx.value().map(Vec::as_slice)
    }
}

pub(crate) use timing::{
    plan_move_frames, plan_shift_frames, BulkCell, BulkMoveLayer, BulkMovePlan, BulkRetimePair,
};
