//! Strict project metadata DTO shared by both native envelope versions.
use crate::object_codec::project::ProjectWireError;
use crate::object_document::{
    Presence, ProjectCameraKey, ProjectCommentRecord, ProjectMetadata, ProjectPresetCatalog,
    ProjectWorkspaceRecord, SymmetrySettings,
};
use serde::{Deserialize, Serialize};

// Nested records are object-only and closed, just like the original metadata DTO.
#[derive(Serialize)]
struct Palette {
    id: String,
    name: String,
    colors: Vec<String>,
}
crate::object_document::object_deserialize!(Palette { id:String, name:String, colors:Vec<String> });
#[derive(Serialize)]
struct ShadowColor {
    id: String,
    color: String,
}
crate::object_document::object_deserialize!(ShadowColor {
    id: String,
    color: String
});
#[derive(Serialize)]
struct Guides {
    #[serde(skip_serializing_if = "Presence::missing")]
    h: Presence<Vec<f64>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    v: Presence<Vec<f64>>,
}
crate::object_document::object_deserialize!(Guides {
    #[serde(default)] h:Presence<Vec<f64>>,
    #[serde(default)] v:Presence<Vec<f64>>
});
#[derive(Serialize)]
struct VanishingPoint {
    x: f64,
    y: f64,
    #[serde(skip_serializing_if = "Presence::missing")]
    locked: Presence<bool>,
}
crate::object_document::object_deserialize!(VanishingPoint {
    x:f64, y:f64, #[serde(default)] locked:Presence<bool>
});
#[derive(Serialize)]
struct SymmetryAxis {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}
crate::object_document::object_deserialize!(SymmetryAxis { x1:f64, y1:f64, x2:f64, y2:f64 });
#[derive(Serialize)]
struct SymmetryCenter {
    x: f64,
    y: f64,
}
crate::object_document::object_deserialize!(SymmetryCenter { x:f64, y:f64 });
fn present<T>(value: Option<T>) -> Presence<T> {
    value.map(Presence::Present).unwrap_or(Presence::Missing)
}

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
    palettes: Presence<Vec<Palette>>,
    // Input compatibility only; the native owner stores canonical palettes.
    #[serde(skip_serializing)]
    color_palette: Presence<Vec<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    active_palette_idx: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    shadow_palette: Presence<Vec<ShadowColor>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    shadow_active_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    guides: Presence<Guides>,
    #[serde(skip_serializing_if = "Presence::missing")]
    perspective_enabled: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    perspective_mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    perspective_density: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing", rename = "perspectiveVPs")]
    perspective_vps: Presence<Option<Vec<VanishingPoint>>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_enabled: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_axis: Presence<Option<SymmetryAxis>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_radial_center: Presence<Option<SymmetryCenter>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_radial_sectors: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    symmetry_extend: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    workspace: Presence<ProjectWorkspaceRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    comments: Presence<Vec<ProjectCommentRecord>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    custom_brush_presets: Presence<ProjectPresetCatalog>,
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
    #[serde(default)] palettes:Presence<Vec<Palette>>,
    #[serde(default)] color_palette:Presence<Vec<String>>,
    #[serde(default)] active_palette_idx:Presence<f64>,
    #[serde(default)] shadow_palette:Presence<Vec<ShadowColor>>,
    #[serde(default)] shadow_active_id:Presence<String>,
    #[serde(default)] guides:Presence<Guides>,
    #[serde(default)] perspective_enabled:Presence<bool>,
    #[serde(default)] perspective_mode:Presence<String>,
    #[serde(default)] perspective_density:Presence<f64>,
    #[serde(default,rename="perspectiveVPs")] perspective_vps:Presence<Option<Vec<VanishingPoint>>>,
    #[serde(default)] symmetry_enabled:Presence<bool>,
    #[serde(default)] symmetry_mode:Presence<String>,
    #[serde(default)] symmetry_axis:Presence<Option<SymmetryAxis>>,
    #[serde(default)] symmetry_radial_center:Presence<Option<SymmetryCenter>>,
    #[serde(default)] symmetry_radial_sectors:Presence<f64>,
    #[serde(default)] symmetry_extend:Presence<bool>,
    #[serde(default)] workspace:Presence<ProjectWorkspaceRecord>,
    #[serde(default)] comments:Presence<Vec<ProjectCommentRecord>>,
    #[serde(default)] custom_brush_presets:Presence<ProjectPresetCatalog>,
    #[serde(default)] camera_keys: Presence<Vec<ProjectCameraKey>>,
    #[serde(default)] camera_layer_on: Presence<bool>
});

impl Metadata {
    pub(super) fn into_metadata(self) -> Result<ProjectMetadata, ProjectWireError> {
        // Match the original import precedence inside an already admitted envelope.
        // Without an alias, retain native missing versus explicitly empty palettes.
        let palettes = self.palettes.value();
        let canonical_palettes = palettes
            .filter(|items| !items.is_empty())
            .map(|items| {
                items
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), p.colors.clone()))
                    .collect()
            })
            .or_else(|| {
                self.color_palette
                    .value()
                    .map(|colors| vec![("p0".into(), "Palette 1".into(), colors.clone())])
            })
            .or_else(|| palettes.map(|_| Vec::new()));
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
                value.with_palette_metadata(
                    canonical_palettes,
                    self.active_palette_idx.value().copied(),
                )
            })
            .and_then(|value| {
                value.with_shadow_palette_metadata(
                    self.shadow_palette.value().map(|items| {
                        items
                            .iter()
                            .map(|p| (p.id.clone(), p.color.clone()))
                            .collect()
                    }),
                    self.shadow_active_id.value().cloned(),
                )
            })
            .and_then(|value| {
                value.with_guide_metadata(
                    self.guides
                        .value()
                        .map(|g| (g.h.value().cloned(), g.v.value().cloned())),
                )
            })
            .and_then(|value| {
                value.with_perspective_metadata(
                    self.perspective_enabled.value().copied(),
                    self.perspective_mode.value().cloned(),
                    self.perspective_density.value().copied(),
                    self.perspective_vps.value().map(|items| {
                        items.as_ref().map(|items| {
                            items
                                .iter()
                                .map(|p| (p.x, p.y, p.locked.value().copied()))
                                .collect()
                        })
                    }),
                )
            })
            .and_then(|value| {
                value.with_symmetry_metadata(SymmetrySettings {
                    enabled: self.symmetry_enabled.value().copied(),
                    mode: self.symmetry_mode.value().cloned(),
                    axis: self.symmetry_axis.value().map(|axis| {
                        axis.as_ref().map(|a| (a.x1, a.y1, a.x2, a.y2))
                    }),
                    radial_center: self
                        .symmetry_radial_center
                        .value()
                        .map(|centre| centre.as_ref().map(|c| (c.x, c.y))),
                    radial_sectors: self.symmetry_radial_sectors.value().copied(),
                    extend: self.symmetry_extend.value().copied(),
                })
            })
            .and_then(|value| {
                value.with_annotations(
                    self.workspace.value().cloned(),
                    self.comments.value().cloned(),
                )
            })
            .and_then(|value| {
                value.with_custom_brush_presets(self.custom_brush_presets.value().cloned())
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
        let (workspace, comments) = value.annotations();
        let (palettes, active_palette_idx) = value.palette_metadata();
        let (shadow_palette, shadow_active_id) = value.shadow_palette_metadata();
        let (perspective_enabled, perspective_mode, perspective_density, perspective_vps) =
            value.perspective_metadata();
        let symmetry = value.symmetry_metadata();
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
            palettes: present(palettes.map(|items| {
                items
                    .iter()
                    .map(|(id, name, colors)| Palette {
                        id: id.clone(),
                        name: name.clone(),
                        colors: colors.clone(),
                    })
                    .collect()
            })),
            color_palette: Presence::Missing,
            active_palette_idx: present(active_palette_idx),
            custom_brush_presets: present(value.custom_brush_presets().cloned()),
            workspace: present(workspace.cloned()),
            comments: present(comments.map(<[ProjectCommentRecord]>::to_vec)),
            shadow_palette: present(shadow_palette.map(|items| {
                items
                    .iter()
                    .map(|(id, color)| ShadowColor {
                        id: id.clone(),
                        color: color.clone(),
                    })
                    .collect()
            })),
            shadow_active_id: present(shadow_active_id.map(str::to_owned)),
            guides: present(value.guide_metadata().map(|(h, v)| Guides {
                h: present(h.map(<[f64]>::to_vec)),
                v: present(v.map(<[f64]>::to_vec)),
            })),
            perspective_enabled: present(perspective_enabled),
            perspective_mode: present(perspective_mode.map(str::to_owned)),
            perspective_density: present(perspective_density),
            perspective_vps: present(perspective_vps.map(|items| {
                items.map(|items| {
                    items
                        .iter()
                        .map(|(x, y, locked)| VanishingPoint {
                            x: *x,
                            y: *y,
                            locked: present(*locked),
                        })
                        .collect()
                })
            })),
            symmetry_enabled: present(symmetry.enabled),
            symmetry_mode: present(symmetry.mode.clone()),
            symmetry_axis: present(symmetry.axis.map(|axis| {
                axis.map(|(x1, y1, x2, y2)| SymmetryAxis { x1, y1, x2, y2 })
            })),
            symmetry_radial_center: present(
                symmetry
                    .radial_center
                    .map(|centre| centre.map(|(x, y)| SymmetryCenter { x, y })),
            ),
            symmetry_radial_sectors: present(symmetry.radial_sectors),
            symmetry_extend: present(symmetry.extend),
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
