//! The two constructors of a layer record: a blank drawing layer as New and Add create it, and an identified layer with every
//! member absent (earlier native-file semantics). Moved out of `object_document.rs` (500 line ceiling) when the guide members landed.
use crate::object_document::{LayerTiming, ObjectLayer, Presence};

impl ObjectLayer {
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
            is_guide_layer: Presence::Missing,
            guide_pos: Presence::Missing,
            guide_orientation: Presence::Missing,
            follow_path: Presence::Missing,
            folder_id: Presence::Missing,
            shy: Presence::Missing,
            timing: Presence::Missing,
            path_fx: Presence::Missing,
            widget: Presence::Missing,
            groups: Presence::Missing,
            blend_mode: Presence::Missing,
            blend_keys: Presence::Missing,
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
}
