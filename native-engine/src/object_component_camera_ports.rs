//! The component camera port of the one component join: `sym.cameraKeys` read from the stored component records.
//!
//! Baseline `getEffectiveStrokes` samples `SMCamera.cameraMatrixAtFrame(sym.cameraKeys, ii, canvasW, canvasH)` for a placed
//! symbol (CLAUDE.md section 8, "Caméra par-Component"), so the camera travels with the instance wherever it is placed. The
//! join only asks a port for the keys; this port answers from the component table of the pinned snapshot, so the viewport
//! and the export (which both read the one production) agree. Time Remap is sampled here too: an instance's stored track
//! (`ComponentInstance::time_remap`) goes through the one `SMMotion.timeRemapValue` sampler, the same that reads a video
//! placement's track, so the viewport, the export and a break apart read one value.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::ComponentPorts;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::placement_offset::sample_remap_track;
use crate::object_document::OrderTrack;
use std::collections::HashMap;

pub(crate) struct RecordPorts {
    keys: HashMap<String, Vec<CameraKey>>,
    /// The Time Remap track of each instance, by the instance layer.
    remaps: HashMap<String, OrderTrack>,
}

impl RecordPorts {
    pub(crate) fn new(components: &ComponentSet) -> Self {
        let mut keys = HashMap::new();
        for component in &components.components {
            if let Some(stored) = component.camera_keys() {
                keys.insert(
                    component.symbol_id().to_owned(),
                    stored
                        .iter()
                        .map(|key| CameraKey {
                            frame: key.frame,
                            x: key.x,
                            y: key.y,
                            w: key.w,
                            rot: key.rot.value().copied(),
                            h_out: key.h_out.value().copied(),
                            h_in: key.h_in.value().copied(),
                            ease: key.ease.value().copied(),
                        })
                        .collect(),
                );
            }
        }
        let remaps = components
            .instances
            .iter()
            .filter_map(|instance| {
                instance
                    .time_remap()
                    .map(|track| (instance.layer_uid().to_owned(), track.clone()))
            })
            .collect();
        Self { keys, remaps }
    }
}

impl ComponentPorts for RecordPorts {
    fn time_remap(&mut self, layer_uid: &str, frame: u32) -> Result<Option<f64>, &'static str> {
        // `timeRemapValue` is `null` for a track without keys (the layer then plays by its mode).
        match self.remaps.get(layer_uid) {
            None => Ok(None),
            Some(track) if track.keys.is_empty() => Ok(None),
            Some(track) => sample_remap_track(track, f64::from(frame))
                .map(Some)
                .ok_or("time remap value is not finite"),
        }
    }
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        Ok(self.keys.get(symbol_id).map(Vec::as_slice))
    }
}
