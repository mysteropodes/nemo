//! The XFL model as a Nemo legacy project (the `exportJSON` shape that the
//! native `legacy_project_import` reads), so a `.fla` opens as a new project.
//!
//! This first mapping flattens: every frame of the first scene is drawn into
//! plain vector strokes, with symbol instances expanded, graphic-symbol
//! timelines followed and classic tweens sampled frame by frame. A layer
//! keyframe is written only where the drawing changes. Components and Motion
//! keys come later; what is not drawn is listed in the report.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::model::*;
use crate::paths::{fill_regions, segments_json, Anchor, MAX_ANCHORS};
use crate::report::ImportReport;
use crate::tween::{classic_tweens, ClassicTween};

/// Deepest symbol nesting followed (a symbol placing itself stops here).
const MAX_DEPTH: usize = 32;

/// A converted project and what the conversion left out.
#[derive(Debug, Clone)]
pub struct LegacyProject {
    pub project: Value,
    pub report: ImportReport,
}

struct Ctx<'a> {
    symbols: HashMap<&'a str, &'a LibrarySymbol>,
    tweens: HashMap<*const Layer, HashMap<u32, ClassicTween>>,
    report: ImportReport,
}

impl<'a> Ctx<'a> {
    fn tweens_of(&mut self, timeline: &Timeline, index: usize) -> &HashMap<u32, ClassicTween> {
        let key: *const Layer = &timeline.layers[index];
        self.tweens.entry(key).or_insert_with(|| {
            classic_tweens(timeline, index)
                .0
                .into_iter()
                .map(|t| (t.start_frame, t))
                .collect()
        })
    }
}

fn hex(c: Rgba, alpha: f64) -> String {
    Rgba {
        a: c.a * alpha,
        ..c
    }
    .hex8()
}

/// One colour for a paint; gradients fall back to their middle stop.
fn solid(paint: &Paint, alpha: f64, report: &mut ImportReport, loc: &str) -> Option<String> {
    match paint {
        Paint::Solid { color } => Some(hex(*color, alpha)),
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
            report.skip("gradient drawn as its middle colour", loc);
            stops.get(stops.len() / 2).map(|s| hex(s.color, alpha))
        }
        Paint::Bitmap { .. } => {
            report.skip("bitmap fill", loc);
            None
        }
    }
}

/// Average linear scale of `m`, for stroke widths.
fn scale(m: &Matrix) -> f64 {
    (m.a * m.d - m.b * m.c).abs().sqrt()
}

fn push_path(
    out: &mut Vec<Value>,
    anchors: &[Anchor],
    closed: bool,
    mut body: Value,
    report: &mut ImportReport,
    loc: &str,
) {
    if anchors.len() < 2 {
        return;
    }
    if anchors.len() > MAX_ANCHORS {
        report.skip(format!("path with more than {MAX_ANCHORS} anchors"), loc);
        return;
    }
    body["segments"] = segments_json(anchors);
    body["closed"] = json!(closed);
    out.push(body);
}

fn draw_shape(
    out: &mut Vec<Value>,
    shape: &Shape,
    m: &Matrix,
    alpha: f64,
    cx: &mut Ctx,
    loc: &str,
) {
    let m = m.then_after(&shape.matrix);
    for path in &shape.paths {
        if let Some(fill) = &path.fill {
            if let Some(color) = solid(fill, alpha, &mut cx.report, loc) {
                for region in fill_regions(&path.contours, &m) {
                    let body = json!({"fillColor": color, "hasRealStroke": false});
                    push_path(out, &region, true, body, &mut cx.report, loc);
                }
            }
        }
        if let Some(stroke) = &path.stroke {
            let Some(color) = solid(&stroke.paint, alpha, &mut cx.report, loc) else {
                continue;
            };
            let cap = match stroke.caps.as_str() {
                "none" => "butt",
                "square" => "square",
                _ => "round",
            };
            let join = match stroke.joints.as_str() {
                "miter" => "miter",
                "bevel" => "bevel",
                _ => "round",
            };
            // Flash's hairline (weight 0) is one screen pixel wide.
            let width = (stroke.weight * scale(&m)).max(0.05);
            let miter = stroke.miter_limit;
            for c in &path.contours {
                let anchors = crate::paths::anchors(c, &m);
                let body = json!({"strokeColor": color, "strokeWidth": width,
                    "strokeCap": cap, "strokeJoin": join, "miterLimit": miter});
                push_path(out, &anchors, c.closed, body, &mut cx.report, loc);
            }
        }
    }
}

/// Frame of a graphic symbol's timeline shown `offset` frames into the
/// parent keyframe span.
fn symbol_frame(inst: &SymbolInstance, offset: u32, len: u32) -> u32 {
    let len = len.max(1);
    match (inst.symbol_type, inst.loop_mode.as_str()) {
        (SymbolType::Graphic, "single frame") => inst.first_frame.min(len - 1),
        (SymbolType::Graphic, "play once") => (inst.first_frame + offset).min(len - 1),
        (SymbolType::Graphic, _) => (inst.first_frame + offset) % len,
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_elements(
    out: &mut Vec<Value>,
    elements: &[Element],
    m: &Matrix,
    alpha: f64,
    offset: u32,
    depth: usize,
    cx: &mut Ctx,
    loc: &str,
) {
    for el in elements {
        match el {
            Element::Shape(s) => draw_shape(out, s, m, alpha, cx, loc),
            Element::Group { members } => {
                draw_elements(out, members, m, alpha, offset, depth, cx, loc)
            }
            Element::SymbolInstance(inst) => {
                let Some(sym) = cx.symbols.get(inst.library_item.as_str()).copied() else {
                    cx.report.skip("instance of a missing library symbol", loc);
                    continue;
                };
                if depth >= MAX_DEPTH {
                    cx.report.skip("symbol nesting deeper than 32", loc);
                    continue;
                }
                if inst.symbol_type != SymbolType::Graphic {
                    cx.report
                        .skip("movie clip or button drawn at its first frame", loc);
                }
                let f = symbol_frame(inst, offset, sym.timeline.frame_count());
                let m = m.then_after(&inst.matrix);
                let at = format!("{loc} > {}", sym.name);
                draw_timeline(
                    out,
                    &sym.timeline,
                    f,
                    &m,
                    alpha * inst.alpha,
                    depth + 1,
                    cx,
                    &at,
                );
            }
            Element::BitmapInstance { .. } => cx.report.skip("bitmap instance", loc),
            Element::Text { .. } => cx.report.skip("text", loc),
            Element::Other { .. } => {}
        }
    }
}

/// The keyframe span of `layer` holding frame `f`.
fn span_at(layer: &Layer, f: u32) -> Option<&Frame> {
    layer
        .frames
        .iter()
        .find(|s| s.index <= f && f < s.index.saturating_add(s.duration.max(1)))
}

/// Draw one layer of `timeline` at frame `f`.
#[allow(clippy::too_many_arguments)]
fn draw_layer(
    out: &mut Vec<Value>,
    timeline: &Timeline,
    index: usize,
    f: u32,
    m: &Matrix,
    alpha: f64,
    depth: usize,
    cx: &mut Ctx,
    loc: &str,
) {
    let layer = &timeline.layers[index];
    let Some(span) = span_at(layer, f) else {
        return;
    };
    let offset = f - span.index;
    let tween = cx.tweens_of(timeline, index).get(&span.index).cloned();
    match (tween, span.elements.as_slice()) {
        (Some(t), [Element::SymbolInstance(inst)]) => {
            let (d, a) = t.sample(f);
            let inst = SymbolInstance {
                matrix: d.to_matrix(t.pivot),
                alpha: a,
                ..inst.clone()
            };
            let el = [Element::SymbolInstance(inst)];
            draw_elements(out, &el, m, alpha, offset, depth, cx, loc);
        }
        _ => draw_elements(out, &span.elements, m, alpha, offset, depth, cx, loc),
    }
}

/// Every drawable layer of a symbol timeline, bottom first.
#[allow(clippy::too_many_arguments)]
fn draw_timeline(
    out: &mut Vec<Value>,
    timeline: &Timeline,
    f: u32,
    m: &Matrix,
    alpha: f64,
    depth: usize,
    cx: &mut Ctx,
    loc: &str,
) {
    for index in (0..timeline.layers.len()).rev() {
        let layer = &timeline.layers[index];
        match layer.kind {
            LayerKind::Normal => {}
            LayerKind::Mask => {
                cx.report
                    .skip("mask layer (masked layers drawn unmasked)", loc);
                continue;
            }
            _ => continue,
        }
        draw_layer(out, timeline, index, f, m, alpha, depth, cx, loc);
    }
}

/// Convert the first scene of `doc` into a legacy Nemo project.
pub fn to_legacy_project(doc: &XflDocument) -> LegacyProject {
    let mut cx = Ctx {
        symbols: doc.symbols.iter().map(|s| (s.name.as_str(), s)).collect(),
        tweens: HashMap::new(),
        report: ImportReport::default(),
    };
    if doc.timelines.len() > 1 {
        cx.report.skip("scenes after the first", "DOMDocument.xml");
    }
    let empty = Timeline {
        name: String::new(),
        layers: Vec::new(),
    };
    let scene = doc.timelines.first().unwrap_or(&empty);
    let total = scene.frame_count().max(1);
    let mut layers = Vec::new();
    // XFL lists the top layer first; a Nemo project lists the bottom first.
    for index in (0..scene.layers.len()).rev() {
        let layer = &scene.layers[index];
        match layer.kind {
            LayerKind::Normal => {}
            LayerKind::Folder => {
                cx.report
                    .skip("layer folders (their layers are kept, flat)", &layer.name);
                continue;
            }
            LayerKind::Mask => {
                cx.report
                    .skip("mask layer (masked layers drawn unmasked)", &layer.name);
                continue;
            }
            LayerKind::Guide => {
                cx.report.count("guideLayersOmitted", 1);
                continue;
            }
            LayerKind::Other(ref k) => {
                cx.report.skip(format!("layer type {k}"), &layer.name);
                continue;
            }
        }
        let mut frames = Vec::with_capacity(total as usize);
        let mut previous: Option<Vec<Value>> = None;
        for f in 0..total {
            let loc = format!("{} frame {}", layer.name, f + 1);
            let mut strokes = Vec::new();
            draw_layer(
                &mut strokes,
                scene,
                index,
                f,
                &Matrix::IDENTITY,
                1.0,
                0,
                &mut cx,
                &loc,
            );
            if previous.as_ref() == Some(&strokes) {
                frames.push(json!({"strokes": []}));
                continue;
            }
            let ids: Vec<Value> = strokes
                .iter()
                .enumerate()
                .map(|(n, s)| {
                    let mut s = s.clone();
                    s["strokeId"] = json!(format!("xfl-{index}-{f}-{n}"));
                    s
                })
                .collect();
            cx.report.count("keyframes", 1);
            cx.report.count("paths", ids.len() as u64);
            frames.push(json!({"isKeyframe": true, "strokes": ids}));
            previous = Some(strokes);
        }
        let mut record = json!({
            "name": layer.name,
            "layerUid": format!("xfl-layer-{index}"),
            "visible": layer.visible,
            "locked": layer.locked,
            "frames": frames,
        });
        if let Some(c) = layer.color.as_deref().filter(|c| c.starts_with('#')) {
            record["color"] = json!(c);
        }
        layers.push(record);
        cx.report.count("layers", 1);
    }
    let fps = if doc.frame_rate > 0.0 {
        doc.frame_rate
    } else {
        24.0
    };
    let project = json!({
        "totalFrames": total,
        "fps": fps,
        "canvasW": doc.width.round().max(1.0),
        "canvasH": doc.height.round().max(1.0),
        "canvasBg": hex(doc.background, 1.0)[..7].to_string(),
        "layers": layers,
    });
    LegacyProject {
        project,
        report: cx.report,
    }
}
