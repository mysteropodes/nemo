//! The component camera port of the one component join: `sym.cameraKeys` read from the stored component records.
//!
//! Baseline `getEffectiveStrokes` samples `SMCamera.cameraMatrixAtFrame(sym.cameraKeys, ii, canvasW, canvasH)` for a placed
//! symbol (CLAUDE.md section 8, "Caméra par-Component"), so the camera travels with the instance wherever it is placed. The
//! join only asks a port for the keys; this port answers from the component table of the pinned snapshot, so the viewport
//! and the export (which both read the one production) agree. Time Remap is not sampled here: its port stays unwired and an
//! instance that has one stays a typed unavailable slot, exactly as with `NoPorts`.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::ComponentPorts;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use std::collections::HashMap;

pub(crate) struct RecordPorts {
    keys: HashMap<String, Vec<CameraKey>>,
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
        Self { keys }
    }
}

impl ComponentPorts for RecordPorts {
    fn time_remap(&mut self, _layer_uid: &str, _frame: u32) -> Result<Option<f64>, &'static str> {
        Err("time remap sampler is not wired")
    }
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        Ok(self.keys.get(symbol_id).map(Vec::as_slice))
    }
}
