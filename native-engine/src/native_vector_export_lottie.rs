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
//! as the baseline drops them: rasters, masks (never paint commands) and zero-opacity strokes.
//! Refused, until later increments admit them with oracle rows: text, components, duplicators,
//! bitmap brush, video, legacy fill records and a layer opacity factor other than one (the baseline
//! writes every layer at full opacity, `ks.o` 100). A seeded brush-texture anchor's dabs are written as
//! the dabs the baseline stores for it: each a fill-only shape at the dab opacity, its colour through
//! the hex Paper stores (`toCSS(true)`) and its outline through the three-decimal rounding `serP`
//! stores (`_r3`), right after the anchor's own slot.
use crate::object_render_scene::{ordered_entries, ObjectPaintPlan, PaintEntry};
use crate::render_geometry::ClosedCubicPath;
use kurbo::{BezPath, PathEl, Point};
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

fn number(value: f64) -> Result<Value, LottieExportError> {
    Number::from_f64(value)
        .map(Value::Number)
        .ok_or(LottieExportError::NonFinite)
}
fn pair(value: [f64; 2]) -> Result<Value, LottieExportError> {
    Ok(Value::Array(vec![number(value[0])?, number(value[1])?]))
}
fn float(value: &Number) -> Result<f64, LottieExportError> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(LottieExportError::NonFinite)
}

/// `lottieShapeValue`: `i`, `o`, `v` and `c` from the drawn path (one subpath of cubic segments, as
/// `build_path` writes it; a closed path's closing curve ends on its first point), through the
/// frame's camera matrix `[a, b, c, d, e, f]` when one is stored (`lottieCamPoint`/`lottieCamVector`).
fn shape_value(
    path: &BezPath,
    camera: Option<[f64; 6]>,
    source_object_index: usize,
) -> Result<Value, LottieExportError> {
    let unsupported = |reason| LottieExportError::Unsupported {
        source_object_index,
        reason,
    };
    let (mut v, mut i, mut o): (Vec<[f64; 2]>, Vec<[f64; 2]>, Vec<[f64; 2]>) =
        (vec![], vec![], vec![]);
    let mut closed = false;
    let rel = |c: Point, p: [f64; 2]| [c.x - p[0], c.y - p[1]];
    for element in path.elements() {
        if closed {
            return Err(unsupported(
                "a drawn path with several subpaths has no single Lottie shape",
            ));
        }
        match *element {
            PathEl::MoveTo(p) if v.is_empty() => {
                v.push([p.x, p.y]);
                i.push([0.0, 0.0]);
                o.push([0.0, 0.0]);
            }
            PathEl::LineTo(p) if !v.is_empty() => {
                v.push([p.x, p.y]);
                i.push([0.0, 0.0]);
                o.push([0.0, 0.0]);
            }
            PathEl::CurveTo(c1, c2, p) if !v.is_empty() => {
                let last = v.len() - 1;
                o[last] = rel(c1, v[last]);
                let to = [p.x, p.y];
                v.push(to);
                i.push(rel(c2, to));
                o.push([0.0, 0.0]);
            }
            PathEl::ClosePath if v.len() > 1 => {
                // The closing curve's end repeats the first anchor: its handle-in belongs to anchor 0.
                let first = v[0];
                let last = v.len() - 1;
                if v[last] == first {
                    v.pop();
                    o.pop();
                    i[0] = i.pop().unwrap_or([0.0, 0.0]);
                }
                closed = true;
            }
            _ => return Err(unsupported("the drawn path is not one cubic subpath")),
        }
    }
    if v.len() < 2 {
        return Err(unsupported("the drawn path has fewer than two anchors"));
    }
    keyed(v, i, o, closed, camera)
}
/// `_r3` (app.js): the baseline stores outline coordinates rounded to three decimals, halves toward +∞.
fn r3(value: f64) -> f64 {
    (value * 1000.0 + 0.5).floor() / 1000.0
}
/// A produced dab as the baseline stores it (`serP`): its own anchors and relative handles through `_r3`.
fn dab_shape(
    contour: Option<&ClosedCubicPath>,
    closed: bool,
    source_object_index: usize,
    camera: Option<[f64; 6]>,
) -> Result<Value, LottieExportError> {
    let contour = contour.ok_or(LottieExportError::Unsupported {
        source_object_index,
        reason: "a brush-texture dab has no single outline",
    })?;
    let rounded = |p: [f64; 2]| [r3(p[0]), r3(p[1])];
    let segments = contour.segments();
    keyed(
        segments.iter().map(|s| rounded(s.point)).collect(),
        segments.iter().map(|s| rounded(s.handle_in)).collect(),
        segments.iter().map(|s| rounded(s.handle_out)).collect(),
        closed,
        camera,
    )
}
/// The keyed shape value through the frame's camera matrix (`lottieCamPoint`/`lottieCamVector`).
fn keyed(
    mut v: Vec<[f64; 2]>,
    mut i: Vec<[f64; 2]>,
    mut o: Vec<[f64; 2]>,
    closed: bool,
    camera: Option<[f64; 6]>,
) -> Result<Value, LottieExportError> {
    if let Some([a, b, c, d, e, f]) = camera {
        let linear = |p: [f64; 2]| [a * p[0] + c * p[1], b * p[0] + d * p[1]];
        v.iter_mut().for_each(|p| {
            *p = {
                let l = linear(*p);
                [l[0] + e, l[1] + f]
            }
        });
        i.iter_mut()
            .chain(o.iter_mut())
            .for_each(|h| *h = linear(*h));
    }
    let list = |values: &[[f64; 2]]| {
        values
            .iter()
            .map(|value| pair(*value))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(json!({"i": list(&i)?, "o": list(&o)?, "v": list(&v)?, "c": closed}))
}

/// `lottieHexToRGBA(hex, 1)`: the colour's channels with alpha one.
fn rgb(r: &Number, g: &Number, b: &Number) -> Result<Value, LottieExportError> {
    Ok(Value::Array(vec![
        number(float(r)?)?,
        number(float(g)?)?,
        number(float(b)?)?,
        json!(1),
    ]))
}

/// `lottieBmCode`: the blend-mode order lottie-web expects; anything else is Normal.
fn blend_code(mode: Option<&str>) -> u32 {
    const MODES: [&str; 15] = [
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "colorDodge",
        "colorBurn",
        "hardLight",
        "softLight",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
    ];
    mode.and_then(|mode| MODES.iter().position(|known| *known == mode))
        .map_or(0, |at| at as u32 + 1)
}

fn identity_ks() -> Value {
    json!({"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}})
}

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
            PaintEntry::Text(text) => {
                return refuse(
                    text.source_index,
                    "text has no admitted Lottie representation yet",
                )
            }
            PaintEntry::Component(component) => {
                return Err(LottieExportError::UnsupportedLayer {
                    source_layer_index: component.host_layer_index,
                    reason: "a placed component has no admitted Lottie representation yet",
                })
            }
            PaintEntry::Duplicator(copy) => {
                return refuse(
                    copy.source_index,
                    "duplicator copies have no admitted Lottie representation yet",
                )
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
        let record = source.record();
        let index = source.source_object_index();
        if record.family != VECTOR_FAMILY {
            return refuse(
                index,
                "legacy fill records have no admitted Lottie representation yet",
            );
        }
        let body = record.vector().ok_or(LottieExportError::Unsupported {
            source_object_index: index,
            reason: "vector body missing",
        })?;
        // `sd.opacity !== 0`: a fully transparent stroke is not exported.
        let opacity = float(&body.opacity)?;
        if opacity == 0.0 {
            continue;
        }
        if command.layer_opacity() != 1.0 {
            return Err(LottieExportError::UnsupportedLayer {
                source_layer_index: source.source_layer_index(),
                reason: "a layer opacity factor has no admitted Lottie representation yet",
            });
        }
        let layer = out
            .get_mut(source.source_layer_index())
            .ok_or(LottieExportError::InvalidRange)?;
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
        layer.push(Shown {
            shape: shape_value(command.path(), camera, index)?,
            stroke,
            fill,
            opacity,
        });
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
