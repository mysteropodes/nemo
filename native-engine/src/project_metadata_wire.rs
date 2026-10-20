//! Strict project metadata DTO shared by native project envelope versions.
use crate::object_codec::project::ProjectWireError;
use crate::object_document::{Presence, ProjectCameraKey, ProjectMetadata};
use serde::{Deserialize, Serialize};

// Separate composition DTO retains fractional frames; layer MarkerRecord is i64.
#[derive(Serialize)]
struct CompositionMarker {
    frame: f64,
    #[serde(skip_serializing_if = "Presence::missing")]
    name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    color: Presence<String>,
}
crate::object_document::object_deserialize!(CompositionMarker {
    frame: f64,
    #[serde(default)] name: Presence<String>,
    #[serde(default)] color: Presence<String>
});
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
    #[serde(skip_serializing_if = "Presence::missing")]
    canvas_bg: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wa_in: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wa_out: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    markers: Presence<Vec<CompositionMarker>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    bpm: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    bpm_offset: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    bpm_show: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    camera_keys: Presence<Vec<ProjectCameraKey>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    camera_layer_on: Presence<bool>,
}
// Derived decoding alone admits positional arrays; reuse the drawing's
// object-only visitor with strict duplicate/unknown/required-member checks.
crate::object_document::object_deserialize!(Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
    #[serde(default)] canvas_bg: Presence<String>,
    #[serde(default)] wa_in: Presence<f64>,
    #[serde(default)] wa_out: Presence<f64>,
    #[serde(default)] markers: Presence<Vec<CompositionMarker>>,
    #[serde(default)] bpm: Presence<f64>,
    #[serde(default)] bpm_offset: Presence<f64>,
    #[serde(default)] bpm_show: Presence<bool>,
    #[serde(default)] camera_keys: Presence<Vec<ProjectCameraKey>>,
    #[serde(default)] camera_layer_on: Presence<bool>
});

impl Metadata {
    pub(super) fn into_metadata(self) -> Result<ProjectMetadata, ProjectWireError> {
        ProjectMetadata::new(self.canvas_w, self.canvas_h, self.fps)
            .and_then(|value| {
                value.with_canvas_background(self.canvas_bg.value().map(String::as_str))
            })
            .and_then(|value| {
                value.with_work_area(self.wa_in.value().copied(), self.wa_out.value().copied())
            })
            .and_then(|value| {
                value.with_composition_metadata(
                    self.markers.value().map(|values| {
                        values
                            .iter()
                            .map(|m| (m.frame, m.name.value().cloned(), m.color.value().cloned()))
                            .collect()
                    }),
                    self.bpm.value().copied(),
                    self.bpm_offset.value().copied(),
                    self.bpm_show.value().copied(),
                )
            })
            .and_then(|value| {
                value.with_camera(
                    self.camera_keys.value().cloned(),
                    self.camera_layer_on.value().copied(),
                )
            })
            .map_err(|_| ProjectWireError::Invalid)
    }
    pub(super) fn from_metadata(value: &ProjectMetadata) -> Self {
        let (canvas_w, canvas_h) = value.canvas_dimensions();
        let (wa_in, wa_out) = value.work_area();
        let (bpm, bpm_offset, bpm_show) = value.composition_bpm();
        Self {
            canvas_w,
            canvas_h,
            fps: value.fps(),
            canvas_bg: value
                .canvas_background()
                .map(|v| Presence::Present(v.to_owned()))
                .unwrap_or(Presence::Missing),
            wa_in: wa_in.map(Presence::Present).unwrap_or(Presence::Missing),
            wa_out: wa_out.map(Presence::Present).unwrap_or(Presence::Missing),
            markers: value
                .composition_markers()
                .map(|values| {
                    Presence::Present(
                        values
                            .iter()
                            .map(|(frame, name, color)| CompositionMarker {
                                frame: *frame,
                                name: name
                                    .clone()
                                    .map(Presence::Present)
                                    .unwrap_or(Presence::Missing),
                                color: color
                                    .clone()
                                    .map(Presence::Present)
                                    .unwrap_or(Presence::Missing),
                            })
                            .collect(),
                    )
                })
                .unwrap_or(Presence::Missing),
            bpm: bpm.map(Presence::Present).unwrap_or(Presence::Missing),
            bpm_offset: bpm_offset
                .map(Presence::Present)
                .unwrap_or(Presence::Missing),
            bpm_show: bpm_show.map(Presence::Present).unwrap_or(Presence::Missing),
            camera_keys: value
                .camera_keys()
                .map(|keys| Presence::Present(keys.to_vec()))
                .unwrap_or(Presence::Missing),
            camera_layer_on: value
                .camera_layer_on()
                .map(Presence::Present)
                .unwrap_or(Presence::Missing),
        }
    }
}
