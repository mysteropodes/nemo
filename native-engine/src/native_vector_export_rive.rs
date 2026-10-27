//! Native Rive export plan over pinned paint plans (#2357 G44).
//!
//! Mirrors the baseline exporter (`exportRive` and its helpers in `src/js/rive-export.js` @1065e9d8), which builds the
//! animation in a running Rive Editor over its MCP tools. This module is the pure half: from one paint plan per frame of
//! the range it assigns stable lanes per layer (a stroke keeps its lane by stroke id), splits each lane into runs of
//! frames with the same anchor count, and plans each run as the baseline does: one static cubic shape (a held stretch,
//! or a single frame), a translated straight shape (a rigid move), or a morphing straight shape keyed vertex by vertex.
//! It also plans the opacity holds and fades. `driver` (the impure half) issues the editor calls and binds the ids the
//! editor returns.
//!
//! Paths are the geometry the compositor draws, as for the Lottie writer, so motion and poses are the evaluated ones.
//! Paint comes from the run's first stroke, as the baseline reads `firstSd`. Colours are the hex the baseline stores
//! (`colorHex8`: six digits when opaque, eight otherwise), so `riveColorHex` keeps a translucent colour's own alpha and
//! gives an opaque one the stroke opacity. A seeded brush-texture anchor is written as the baseline writes it: its dabs
//! stay out (`isBrushTextureCopy`), and its own path wobbles by the preset's nib, scatter and spacing (`riveApplyWobble`,
//! seeded by its stroke id), painted at its remembered opacity. A stored camera becomes, as in the baseline, a component
//! artboard holding the drawing that the main artboard instances and moves by the inverse camera on every frame.
//! Duplicator copies are the dicts `applyLayerDuplicator` returns (clone-outer, source-inner, each keeping its source's
//! stroke id, so repeats take the `+` lanes), with the copy's factored opacity and hue-turned fill and stroke. A placed
//! component is its `symbolId` branch's dicts on the host layer: inner strokes in inner order under camera then
//! symMatrix, inner layer opacity folded in, stroke widths as stored.
//! Dropped as the baseline drops them: zero-opacity strokes without a texture. Refused, until later increments admit
//! them with oracle rows: rasters (the baseline throws on them), text, bitmap brush, video, legacy fill records and a
//! layer opacity factor other than one (the baseline reads no layer opacity).
use crate::object_render_scene::{ordered_entries, ObjectPaintPlan, PaintEntry};
use kurbo::BezPath;
use serde_json::{Number, Value};
use std::collections::HashMap;

#[path = "native_vector_export_rive_driver.rs"]
pub(crate) mod driver;
#[path = "native_vector_export_rive_shapes.rs"]
pub(crate) mod shapes;
use shapes::{
    apply_wobble, cubic_vertices, float, geometry, hash_seed, paints, point_command, rgba,
    straight, translation, wobble_params,
};
pub(crate) use shapes::{color_hex, num};

/// The range and the document facts the baseline reads from `state`; the artboard name carries the export clock.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RiveSpec {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) fps: Number,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) background: String,
    pub(crate) artboard_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RiveExportError {
    InvalidRange,
    WrongFrame {
        index: usize,
    },
    MixedSnapshot,
    Unsupported {
        source_object_index: usize,
        reason: &'static str,
    },
    UnsupportedLayer {
        source_layer_index: usize,
        reason: &'static str,
    },
    NonFinite,
    /// The stored camera of a frame could not be read.
    CameraUnavailable {
        frame: u32,
    },
    /// The editor refused a call or its reply lacks what the baseline reads from it.
    Editor {
        reason: &'static str,
    },
}

const VECTOR_FAMILY: &str = "editable-vector-path";

/// One shown stroke as the baseline reads its stored dictionary: anchors, relative handles and paint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stroke {
    id: String,
    v: Vec<[f64; 2]>,
    i: Vec<[f64; 2]>,
    o: Vec<[f64; 2]>,
    closed: bool,
    fill: Option<[f64; 4]>,
    stroke: Option<[f64; 4]>,
    width: f64,
    real: bool,
    opacity: f64,
    /// `brushTexturePreset` and `preTextureOpacity` of a seeded brush-texture anchor.
    texture: Option<String>,
    pre_texture_opacity: Option<f64>,
}

/// `riveSignature`: two frames of a lane are the same stroke when geometry, closure, fill, a real stroke's colour,
/// width and opacity agree.
fn same(a: &Stroke, b: &Stroke) -> bool {
    a.v == b.v
        && a.i == b.i
        && a.o == b.o
        && a.closed == b.closed
        && a.fill == b.fill
        && (if a.real { a.stroke } else { None }) == (if b.real { b.stroke } else { None })
        && a.width == b.width
        && a.opacity == b.opacity
}

/// One drawn stroke dict, or `None` when the baseline filter drops it. A macro, not a function: the
/// record type belongs to the evaluation layer, which this export layer may read but not name.
macro_rules! stroke_of {
    ($record:expr, $path:expr, $index:expr) => {{
        let (record, path, index): (_, &BezPath, usize) = ($record, $path, $index);
        let stroke: Result<Option<Stroke>, RiveExportError> = 'stroke: {
            if record.family != VECTOR_FAMILY {
                break 'stroke Err(RiveExportError::Unsupported {
                    source_object_index: index,
                    reason: "legacy fill records have no admitted Rive representation yet",
                });
            }
            let texture = record
                .vector_ext
                .value()
                .and_then(|e| e.brush_texture.value());
            let preset = texture
                .and_then(|t| t.brush_texture_preset.value())
                .filter(|k| !k.is_empty())
                .cloned();
            let pre_texture_opacity = match texture.and_then(|t| t.pre_texture_opacity.value()) {
                Some(n) => Some(float(n)?),
                None => None,
            };
            let body = record.vector().ok_or(RiveExportError::Unsupported {
                source_object_index: index,
                reason: "vector body missing",
            })?;
            // `sd.opacity !== 0 || sd.brushTexturePreset`: a camouflaged brush-texture anchor stays.
            let opacity = float(&body.opacity)?;
            if opacity == 0.0 && preset.is_none() {
                break 'stroke Ok(None);
            }
            let stroke = match body.stroke.value().and_then(|paint| paint.as_ref()) {
                Some(p) => Some(rgba(&p.r, &p.g, &p.b, &p.a)?),
                None => None,
            };
            let fill = match record.solid_fill() {
                Some(p) => Some(rgba(&p.r, &p.g, &p.b, &p.a)?),
                None => None,
            };
            let (v, i, o, closed) = geometry(path, index)?;
            Ok(Some(Stroke {
                id: record.target.stroke_id.clone(),
                v,
                i,
                o,
                closed,
                fill,
                stroke,
                width: float(&body.stroke_width)?,
                real: body.has_real_stroke,
                opacity,
                texture: preset,
                pre_texture_opacity,
            }))
        };
        stroke
    }};
}

/// The strokes one frame shows, per source layer, in stack order: the post-filter `getEffectiveStrokesRendered` list.
fn strokes_by_layer(
    plan: &ObjectPaintPlan,
    layers: usize,
) -> Result<Vec<Vec<Stroke>>, RiveExportError> {
    let mut out: Vec<Vec<Stroke>> = (0..layers).map(|_| Vec::new()).collect();
    for entry in ordered_entries(plan) {
        let refuse = |source_object_index, reason| {
            Err(RiveExportError::Unsupported {
                source_object_index,
                reason,
            })
        };
        let command = match entry {
            PaintEntry::Contour(command) => command,
            PaintEntry::Raster(at) => {
                return refuse(
                    plan.raster_sources().get(at).map_or(0, |r| r.source_index),
                    "rasters have no admitted Rive representation",
                )
            }
            PaintEntry::Text(text) => {
                return refuse(
                    text.source_index,
                    "text has no admitted Rive representation yet",
                )
            }
            // The `symbolId` branch's dicts: inner strokes under camera then symMatrix, inner order.
            PaintEntry::Component(component) => {
                let host = component.host_layer_index;
                if let Some(stroke) = stroke_of!(&component.record, &component.path, host)? {
                    let opacity = plan.packet().layer_opacity(host).map_err(|_| {
                        RiveExportError::UnsupportedLayer {
                            source_layer_index: host,
                            reason: "component host opacity is unavailable",
                        }
                    })?;
                    push(&mut out, host, opacity, stroke)?;
                }
                continue;
            }
            // `applyLayerDuplicator`'s dicts: clone-outer/source-inner, each keeping its source's stroke id.
            PaintEntry::Duplicator(copy) => {
                if let Some(stroke) = stroke_of!(&copy.record, &copy.path, copy.source_index)? {
                    let host = plan.packet().layer_opacity(copy.layer_index).map_err(|_| {
                        RiveExportError::UnsupportedLayer {
                            source_layer_index: copy.layer_index,
                            reason: "duplicator host opacity is unavailable",
                        }
                    })?;
                    push(&mut out, copy.layer_index, host, stroke)?;
                }
                continue;
            }
            // `!sd.isBrushTextureCopy`: the dabs stay out; the anchor is written with its own wobble.
            PaintEntry::Dab(_) => continue,
            PaintEntry::Video(index) => {
                return Err(RiveExportError::UnsupportedLayer {
                    source_layer_index: plan.video_slots()[index].layer_index,
                    reason: "video has no Rive representation",
                })
            }
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintEntry::Brush(_) => {
                return refuse(
                    plan.first_brush_source().unwrap_or(0),
                    "bitmap brush paint has no admitted Rive representation yet",
                )
            }
        };
        let source = command.source();
        let layer = source.source_layer_index();
        if let Some(stroke) = stroke_of!(source.record(), command.path(), source.source_object_index())? {
            push(&mut out, layer, command.layer_opacity(), stroke)?;
        }
    }
    Ok(out)
}

fn push(
    out: &mut [Vec<Stroke>],
    layer: usize,
    layer_opacity: f64,
    stroke: Stroke,
) -> Result<(), RiveExportError> {
    if layer_opacity != 1.0 {
        return Err(RiveExportError::UnsupportedLayer {
            source_layer_index: layer,
            reason: "a layer opacity factor has no admitted Rive representation yet",
        });
    }
    out.get_mut(layer)
        .ok_or(RiveExportError::InvalidRange)?
        .push(stroke);
    Ok(())
}

/// What the editor must do with one planned shape after it exists.
pub(crate) enum Body {
    /// A one-vertex placeholder path whose vertices are then replaced by the cubic ones (`addVertices`).
    Cubic(Vec<Value>),
    /// The shape's node moves by these deltas, one per frame of `start..=end`.
    Translate { start: u32, deltas: Vec<[f64; 2]> },
    /// Each vertex is keyed per frame of `start..=end`.
    Morph {
        start: u32,
        points: Vec<Vec<[f64; 2]>>,
    },
}
pub(crate) struct Pending {
    pub(crate) name: String,
    pub(crate) closed: bool,
    /// (relative frame, value, interpolation).
    pub(crate) opacity_keys: Vec<(u32, f64, &'static str)>,
    pub(crate) body: Body,
}
/// A frame shape's definition; its parent is the lane's container, bound by name once the containers exist.
pub(crate) struct ShapeDef {
    pub(crate) name: String,
    pub(crate) container: String,
    pub(crate) paints: Value,
    pub(crate) commands: Value,
}
pub(crate) struct RivePlan {
    pub(crate) containers: Vec<String>,
    pub(crate) shapes: Vec<ShapeDef>,
    pub(crate) pending: Vec<Pending>,
    /// The stored camera's `{x, y, w, rot}` per frame of the range, when the camera layer is on with keys (`camActive`).
    pub(crate) camera: Option<Vec<[f64; 4]>>,
}

/// The whole range as the baseline plans it, layers from the top of the stack down.
pub(crate) fn plan_rive(
    plans: &[ObjectPaintPlan],
    spec: &RiveSpec,
) -> Result<RivePlan, RiveExportError> {
    if spec.start > spec.end || plans.len() as u64 != u64::from(spec.end - spec.start) + 1 {
        return Err(RiveExportError::InvalidRange);
    }
    let pinned = plans[0].packet().snapshot().snapshot_id();
    for (index, plan) in plans.iter().enumerate() {
        if plan.packet().frame() != spec.start + index as u32 {
            return Err(RiveExportError::WrongFrame { index });
        }
        if plan.packet().snapshot().snapshot_id() != pinned {
            return Err(RiveExportError::MixedSnapshot);
        }
    }
    let layers = plans[0].packet().source_layers();
    let frames = plans
        .iter()
        .map(|plan| strokes_by_layer(plan, layers.len()))
        .collect::<Result<Vec<_>, _>>()?;
    let (first, last) = (spec.start, spec.end);
    let snapshot = plans[0].packet().snapshot();
    let poses = (first..=last)
        .map(|f| {
            snapshot
                .sampled_camera_pose(f64::from(f))
                .map(|pose| pose.map(|p| [p.x, p.y, p.w, p.rot]))
                .map_err(|_| RiveExportError::CameraUnavailable { frame: f })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let packet = plans[0].packet();
    let mut out = RivePlan {
        containers: vec![],
        shapes: vec![],
        pending: vec![],
        camera: poses
            .iter()
            .all(Option::is_some)
            .then(|| poses.into_iter().flatten().collect()),
    };
    let mut lanes_by_layer = vec![];
    for (li, layer) in layers.iter().enumerate().rev() {
        // Pass 0: a stroke keeps its lane by id across the range; a repeated id on one frame takes a further lane.
        let (mut lane_of, mut count) = (HashMap::<String, usize>::new(), 0usize);
        let mut rows: Vec<Vec<Option<&Stroke>>> = vec![];
        for shown in &frames {
            let mut row: Vec<Option<&Stroke>> = vec![];
            for sd in &shown[li] {
                let mut key = format!("id:{}", sd.id);
                while lane_of
                    .get(&key)
                    .is_some_and(|&lane| row.get(lane).is_some_and(Option::is_some))
                {
                    key.push('+');
                }
                let lane = *lane_of.entry(key).or_insert_with(|| {
                    count += 1;
                    count - 1
                });
                if row.len() <= lane {
                    row.resize(lane + 1, None);
                }
                row[lane] = Some(sd);
            }
            rows.push(row);
        }
        let name = layer
            .name
            .value()
            .map_or("undefined".to_string(), |n| n.clone());
        for slot in 0..count {
            out.containers.push(format!("{name} / shape{slot} [{li}]"));
        }
        lanes_by_layer.push((li, name, count, rows));
    }
    for (li, name, count, rows) in &lanes_by_layer {
        for slot in 0..*count {
            let container = format!("{name} / shape{slot} [{li}]");
            let at = |f: u32| rows[(f - first) as usize].get(slot).copied().flatten();
            // `riveCountRuns`: a run ends where the lane is empty or its anchor count changes.
            let mut runs = vec![];
            let (mut run_start, mut run_count): (Option<u32>, Option<usize>) = (None, None);
            for f in first..=last + 1 {
                let sd = if f <= last { at(f) } else { None };
                let cnt = sd.map(|s| s.v.len());
                if cnt != run_count {
                    if let (Some(s), Some(_)) = (run_start, run_count) {
                        runs.push((s, f - 1));
                    }
                    run_start = sd.map(|_| f);
                    run_count = cnt;
                }
            }
            for (start, end) in runs {
                let first_sd = at(start).ok_or(RiveExportError::InvalidRange)?;
                let all_same =
                    (start + 1..=end).all(|f| at(f).is_some_and(|sd| same(sd, first_sd)));
                let morph = end > start && !all_same;
                let shape_name = format!("n{}", out.pending.len());
                // Opacity holds bracket the run; a fade (over 0.01 from the first frame) keys every frame linearly.
                let mut keys = vec![];
                if start > first {
                    keys.push((0, 0.0, "hold"));
                }
                // `resolveBrushTexturePreset`: back from the run's start, then forward through the range.
                let key = (first..=start)
                    .rev()
                    .chain(start..=last)
                    .find_map(|f| at(f).and_then(|s| s.texture.clone()));
                let brush = key.and_then(|k| packet.brush_wobble_inputs(&k));
                let wobble = brush.map(|(nib, scatter, spacing)| {
                    let (amp, freq) = wobble_params(nib, scatter, spacing, first_sd.width);
                    (amp, freq, hash_seed(&first_sd.id))
                });
                let shake = |v: &[[f64; 2]], closed: bool| match wobble {
                    Some((amp, freq, seed)) => apply_wobble(v, amp, freq, seed, closed),
                    None => v.to_vec(),
                };
                let varies = brush.is_none()
                    && (start + 1..=end).any(|f| {
                        (at(f).map_or(1.0, |s| s.opacity) - first_sd.opacity).abs() > 0.01
                    });
                if varies {
                    keys.extend(
                        (start..=end)
                            .map(|f| (f - first, at(f).map_or(1.0, |s| s.opacity), "linear")),
                    );
                } else {
                    keys.push((start - first, 1.0, "hold"));
                }
                if end < last {
                    keys.push((end - first + 1, 0.0, "hold"));
                }
                let override_opacity = if brush.is_some() {
                    Some(first_sd.pre_texture_opacity.unwrap_or(1.0))
                } else {
                    varies.then_some(1.0)
                };
                let anchors = shake(&first_sd.v, first_sd.closed);
                let paints = paints(first_sd, first_sd.real, override_opacity);
                let (commands, body) = if !morph {
                    (
                        Value::Array(vec![
                            point_command("moveTo", anchors[0]),
                            point_command("lineTo", anchors[0]),
                        ]),
                        Body::Cubic(cubic_vertices(&anchors, first_sd)),
                    )
                } else {
                    let raw: Vec<&Vec<[f64; 2]>> = (start..=end)
                        .map(|f| at(f).map(|s| &s.v).ok_or(RiveExportError::InvalidRange))
                        .collect::<Result<_, _>>()?;
                    match translation(&raw) {
                        Some(deltas) => (
                            straight(&anchors, first_sd.closed),
                            Body::Translate { start, deltas },
                        ),
                        None => (
                            straight(&anchors, first_sd.closed),
                            Body::Morph {
                                start,
                                points: raw
                                    .into_iter()
                                    .map(|v| shake(v, first_sd.closed))
                                    .collect(),
                            },
                        ),
                    }
                };
                out.shapes.push(ShapeDef {
                    name: shape_name.clone(),
                    container: container.clone(),
                    paints,
                    commands,
                });
                out.pending.push(Pending {
                    name: shape_name,
                    closed: first_sd.closed,
                    opacity_keys: keys,
                    body,
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/native_vector_export_rive.rs"]
mod tests;
