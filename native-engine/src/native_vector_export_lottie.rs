//! Native Lottie writer over pinned paint plans (#2357 G43, increment 1).
//!
//! Mirrors the baseline frame-baked Bodymovin converter (`lottieBuild`, `lottiePathLayer`,
//! `lottieShapeValue`, `lottieHexToRGBA` and `lottieBmCode` in `src/js/export.js` @1065e9d8): a
//! background solid, then, layer by layer from the back, one shape layer per stroke slot and
//! contiguous frame run whose path is keyed on every frame of the run; the array is first-on-top.
//! Paint and width come from the run's first stroke, as the baseline reads `first`.
//!
//! Each plan is one frame of the range, all pinned to one snapshot. A path is the geometry the
//! compositor draws (`ObjectPaintCommand::path`), so motion and poses are the evaluated ones; an
//! open path's first handle-in and last handle-out do not reach the drawn path and are emitted as
//! zero, which no player draws. A stored camera bakes into every anchor and handle of the frame,
//! as `lottieCamMatrix` does (the camera matrix of the frame, its linear part for handles). Dropped
//! as the baseline drops them: rasters, masks (never paint commands) and zero-opacity strokes. As
//! `getEffectiveStrokesRendered` returns them, a placed component is its inner strokes on the host
//! layer under camera then symMatrix, and a duplicator its copies (factored opacity, hue-turned
//! paints). Refused, until later increments admit them with oracle rows: text (the baseline's vector
//! text glyphs are filled paths it writes as shapes), bitmap brush, video, legacy fill records and a layer opacity factor other than one (the baseline
//! writes every layer at full opacity, `ks.o` 100). A seeded brush-texture anchor's dabs are written as
//! the dabs the baseline stores for it: each a fill-only shape at the dab opacity, its colour through
//! the hex Paper stores (`toCSS(true)`) and its outline through the three-decimal rounding `serP`
//! stores (`_r3`), right after the anchor's own slot.
use crate::object_render_scene::{ordered_entries, ObjectPaintPlan, PaintEntry};
use kurbo::BezPath;
use serde_json::{json, Number, Value};

/// The range and the document facts the baseline reads from `state`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LottieSpec {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) fps: Number,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) background: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LottieExportError {
    /// `start > end`, or the plans are not exactly one per frame of the range.
    InvalidRange,
    /// The plan at this position is not pinned to its frame of the range.
    WrongFrame { index: usize },
    /// The plans are not pinned to one snapshot.
    MixedSnapshot,
    /// The stored camera of a frame could not be read.
    CameraUnavailable { frame: u32 },
    Unsupported {
        source_object_index: usize,
        reason: &'static str,
    },
    UnsupportedLayer {
        source_layer_index: usize,
        reason: &'static str,
    },
    /// A coordinate or paint value is not a finite number.
    NonFinite,
}

/// The vector record family the exporter admits (the export layer reads record fields, not document types).
const VECTOR_FAMILY: &str = "editable-vector-path";

/// One stroke shown on one frame: its keyed path value and its paint, as `lottiePathLayer` reads them.
struct Shown {
    shape: Value,
    /// `st` colour, width, line cap and line join, present exactly for `hasRealStroke`.
    stroke: Option<(Value, f64, u32, u32)>,
    /// `fl` colour, present exactly when the stroke has a fill.
    fill: Option<Value>,
    opacity: f64,
}

#[path = "native_vector_export_lottie_shapes.rs"]
mod shapes;
use shapes::{number, float, shape_value, dab_shape, rgb, blend_code, identity_ks};


/// `lottiePathLayer` for one slot run: the path keyed per frame, then stroke, fill and the identity transform.
fn path_layer(
    name: String,
    run: &[&Shown],
    start: u32,
    bm: u32,
) -> Result<Value, LottieExportError> {
    let first = run[0];
    let keys = run
        .iter()
        .enumerate()
        .map(|(k, shown)| json!({"t": start + k as u32, "s": [shown.shape.clone()], "i": {"x":[1],"y":[1]}, "o": {"x":[0],"y":[0]}}))
        .collect::<Vec<_>>();
    let opacity = number(first.opacity * 100.0)?;
    let mut items = vec![json!({"ty":"sh","ks":{"a":1,"k":keys}})];
    if let Some((colour, width, cap, join)) = &first.stroke {
        // `first.strokeWidth || 2`.
        let width = if *width == 0.0 {
            json!(2)
        } else {
            number(*width)?
        };
        items.push(json!({"ty":"st","c":{"a":0,"k":colour},"o":{"a":0,"k":opacity},"w":{"a":0,"k":width},"lc":cap,"lj":join}));
    }
    if let Some(colour) = &first.fill {
        items.push(json!({"ty":"fl","c":{"a":0,"k":colour},"o":{"a":0,"k":opacity}}));
    }
    items.push(json!({"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}));
    Ok(
        json!({"ddd":0,"ty":4,"nm":name,"sr":1,"ks":identity_ks(),"ao":0,"shapes":[{"ty":"gr","it":items}],
        "ip":start,"op":start + run.len() as u32,"st":0,"bm":bm}),
    )
}

/// One drawn stroke dict as `lottieBuild` shows it, or `None` when its filter drops it. A macro, not a
/// function: the record type belongs to the evaluation layer, which this export layer may read but not name.
macro_rules! shown_of {
    ($record:expr, $path:expr, $index:expr, $camera:expr) => {{
        let (record, path, index, camera): (_, &BezPath, usize, _) = ($record, $path, $index, $camera);
        let shown: Result<Option<Shown>, LottieExportError> = 'shown: {
                if record.family != VECTOR_FAMILY {
                    break 'shown Err(LottieExportError::Unsupported {
                        source_object_index: index,
                        reason: "legacy fill records have no admitted Lottie representation yet",
                    });
                }
                let body = record.vector().ok_or(LottieExportError::Unsupported {
                    source_object_index: index,
                    reason: "vector body missing",
                })?;
                // `sd.opacity !== 0`: a fully transparent stroke is not exported.
                let opacity = float(&body.opacity)?;
                if opacity == 0.0 {
                    break 'shown Ok(None);
                }
                let stroke = if body.has_real_stroke {
                    // A missing stroke colour is `lottieHexToRGBA(undefined)`: white.
                    let colour = match body.stroke.value().and_then(|paint| paint.as_ref()) {
                        Some(paint) => rgb(&paint.r, &paint.g, &paint.b)?,
                        None => json!([1, 1, 1, 1]),
                    };
                    let cap = match body.stroke_cap.as_str() {
                        "round" => 2,
                        "square" => 3,
                        _ => 1,
                    };
                    let join = match body.stroke_join.as_str() {
                        "round" => 2,
                        "bevel" => 3,
                        _ => 1,
                    };
                    Some((colour, float(&body.stroke_width)?, cap, join))
                } else {
                    None
                };
                let fill = match record.solid_fill() {
                    Some(paint) => Some(rgb(&paint.r, &paint.g, &paint.b)?),
                    None => None,
                };
                Ok(Some(Shown {
                    shape: shape_value(path, camera, index)?,
                    stroke,
                    fill,
                    opacity,
                }))
        };
        shown
    }};
}

fn host_opacity(plan: &ObjectPaintPlan, host: usize) -> Result<f64, LottieExportError> {
    plan.packet()
        .layer_opacity(host)
        .map_err(|_| LottieExportError::UnsupportedLayer {
            source_layer_index: host,
            reason: "the host layer opacity is unavailable",
        })
}

fn push(
    out: &mut [Vec<Shown>],
    layer: usize,
    layer_opacity: f64,
    shown: Shown,
) -> Result<(), LottieExportError> {
    if layer_opacity != 1.0 {
        return Err(LottieExportError::UnsupportedLayer {
            source_layer_index: layer,
            reason: "a layer opacity factor has no admitted Lottie representation yet",
        });
    }
    out.get_mut(layer)
        .ok_or(LottieExportError::InvalidRange)?
        .push(shown);
    Ok(())
}

/// Every layer's shown strokes on one frame, in paint order.
fn shown_by_layer(
    plan: &ObjectPaintPlan,
    layers: usize,
) -> Result<Vec<Vec<Shown>>, LottieExportError> {
    let frame = plan.packet().frame();
    let camera = plan
        .packet()
        .stored_camera_matrix()
        .map_err(|_| LottieExportError::CameraUnavailable { frame })?;
    let mut out: Vec<Vec<Shown>> = (0..layers).map(|_| Vec::new()).collect();
    for entry in ordered_entries(plan) {
        let refuse = |source_object_index, reason| {
            Err(LottieExportError::Unsupported {
                source_object_index,
                reason,
            })
        };
        let command = match entry {
            PaintEntry::Contour(command) => command,
            // `lottieBuild` drops rasters (and with them bitmap-brush companions).
            PaintEntry::Raster(_) => continue,
            // Native text is the baseline's vector text (`buildVectorTextGroup`): its glyphs are plain
            // filled paths that `lottieBuild` writes as shapes. Until that row has an oracle, refuse.
            PaintEntry::Text(text) => {
                return refuse(
                    text.source_index,
                    "text has no admitted Lottie representation yet",
                )
            }
            // `getEffectiveStrokesRendered`: a placed component's `symbolId`-branch dicts on its host, and
            // `applyLayerDuplicator`'s copies on theirs, drawn like any stroke.
            PaintEntry::Component(component) => {
                let host = component.host_layer_index;
                if let Some(shown) = shown_of!(&component.record, &component.path, host, camera)? {
                    push(&mut out, host, host_opacity(plan, host)?, shown)?;
                }
                continue;
            }
            PaintEntry::Duplicator(copy) => {
                let host = copy.layer_index;
                if let Some(shown) = shown_of!(&copy.record, &copy.path, copy.source_index, camera)? {
                    push(&mut out, host, host_opacity(plan, host)?, shown)?;
                }
                continue;
            }
            PaintEntry::Dab(dab) => {
                // `sd.opacity !== 0`, as for any stroke.
                if dab.opacity == 0.0 {
                    continue;
                }
                if dab.layer_opacity != 1.0 {
                    return Err(LottieExportError::UnsupportedLayer {
                        source_layer_index: dab.layer_index,
                        reason: "a layer opacity factor has no admitted Lottie representation yet",
                    });
                }
                let [r, g, b, _] = dab.svg_rgba.ok_or(LottieExportError::NonFinite)?;
                let hex = |c: f64| number((c.clamp(0.0, 1.0) * 255.0).round() / 255.0);
                let shown = Shown {
                    shape: dab_shape(dab.svg_contour.as_ref(), dab.closed, dab.source_index, camera)?,
                    stroke: None,
                    fill: Some(Value::Array(vec![hex(r)?, hex(g)?, hex(b)?, json!(1)])),
                    opacity: dab.opacity,
                };
                out.get_mut(dab.layer_index)
                    .ok_or(LottieExportError::InvalidRange)?
                    .push(shown);
                continue;
            }
            PaintEntry::Video(index) => {
                return Err(LottieExportError::UnsupportedLayer {
                    source_layer_index: plan.video_slots()[index].layer_index,
                    reason: "video has no Lottie representation",
                })
            }
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintEntry::Brush(_) => {
                return refuse(
                    plan.first_brush_source().unwrap_or(0),
                    "bitmap brush paint has no admitted Lottie representation yet",
                )
            }
        };
        let source = command.source();
        let layer = source.source_layer_index();
        if let Some(shown) = shown_of!(source.record(), command.path(), source.source_object_index(), camera)? {
            push(&mut out, layer, command.layer_opacity(), shown)?;
        }
    }
    Ok(out)
}

/// The whole range as one Lottie document (`lottieBuild(start, end)`).
pub(crate) fn build_lottie(
    plans: &[ObjectPaintPlan],
    spec: &LottieSpec,
) -> Result<Value, LottieExportError> {
    if spec.start > spec.end || plans.len() as u64 != u64::from(spec.end - spec.start) + 1 {
        return Err(LottieExportError::InvalidRange);
    }
    let pinned = plans[0].packet().snapshot().snapshot_id();
    for (index, plan) in plans.iter().enumerate() {
        if plan.packet().frame() != spec.start + index as u32 {
            return Err(LottieExportError::WrongFrame { index });
        }
        if plan.packet().snapshot().snapshot_id() != pinned {
            return Err(LottieExportError::MixedSnapshot);
        }
    }
    let layers = plans[0].packet().source_layers();
    let frames = plans
        .iter()
        .map(|plan| shown_by_layer(plan, layers.len()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ind = 1u32;
    let mut out = vec![
        json!({"ddd":0,"ty":1,"nm":"Background","sr":1,"sc":spec.background,"sw":spec.width,"sh":spec.height,
        "ks":identity_ks(),"ao":0,"ip":spec.start,"op":spec.end + 1,"st":0,"bm":0,"ind":ind}),
    ];
    ind += 1;
    for (li, layer) in layers.iter().enumerate().rev() {
        let bm = blend_code(layer.blend_mode.value().and_then(|mode| mode.as_deref()));
        let name = layer.name.value().map_or("undefined", String::as_str);
        let slots = frames
            .iter()
            .map(|frame| frame[li].len())
            .max()
            .unwrap_or(0);
        for slot in 0..slots {
            // Contiguous frames where this slot exists, each one Lottie layer.
            let mut run: Vec<&Shown> = Vec::new();
            let mut run_start = spec.start;
            for (k, frame) in frames.iter().enumerate() {
                match frame[li].get(slot) {
                    Some(shown) => {
                        if run.is_empty() {
                            run_start = spec.start + k as u32;
                        }
                        run.push(shown);
                    }
                    None if !run.is_empty() => {
                        let mut layer =
                            path_layer(format!("{name} / shape{slot}"), &run, run_start, bm)?;
                        layer["ind"] = json!(ind);
                        ind += 1;
                        out.push(layer);
                        run.clear();
                    }
                    None => {}
                }
            }
            if !run.is_empty() {
                let mut layer = path_layer(format!("{name} / shape{slot}"), &run, run_start, bm)?;
                layer["ind"] = json!(ind);
                ind += 1;
                out.push(layer);
            }
        }
    }
    out.reverse();
    Ok(
        json!({"v":"5.9.0","fr":spec.fps,"ip":spec.start,"op":spec.end + 1,"w":spec.width,"h":spec.height,
        "nm":"Nemo Export","ddd":0,"assets":[],"layers":out}),
    )
}

#[cfg(test)]
#[path = "../tests/native_vector_export_lottie.rs"]
mod tests;
