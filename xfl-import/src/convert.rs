//! The XFL model as a Nemo legacy project (the `exportJSON` shape that the
//! native `legacy_project_import` reads), so a `.fla` opens as a new project.
//!
//! This first mapping flattens: every frame of the first scene is drawn into
//! plain vector strokes, with symbol instances expanded, graphic-symbol
//! timelines followed and classic tweens sampled frame by frame. A layer
//! keyframe is written only where the drawing changes. Components and Motion
//! keys come later; what is not drawn is listed in the report.

use std::collections::HashMap;
use std::rc::Rc;

use serde_json::{json, Value};

use crate::model::*;
use crate::paths::{fill_regions, hex, push_path, scale, solid, MAX_ANCHORS, ZERO};
use crate::report::ImportReport;
use crate::tween::{classic_tweens, ClassicTween};

/// Deepest symbol nesting followed (a symbol placing itself stops here).
const MAX_DEPTH: usize = 32;
/// Most frames the native reader accepts.
const MAX_FRAMES: u32 = 1_000_000;

/// A converted project and what the conversion left out.
#[derive(Debug, Clone)]
pub struct LegacyProject {
    pub project: Value,
    pub report: ImportReport,
}

struct Ctx<'a> {
    symbols: HashMap<&'a str, &'a LibrarySymbol>,
    tweens: HashMap<*const Layer, HashMap<u32, Rc<ClassicTween>>>,
    frame_counts: HashMap<&'a str, u32>,
    /// Symbols being expanded, outermost first (a symbol inside itself stops).
    expanding: Vec<&'a str>,
    report: ImportReport,
}

impl<'a> Ctx<'a> {
    fn tweens_of(&mut self, timeline: &Timeline, index: usize) -> &HashMap<u32, Rc<ClassicTween>> {
        let key: *const Layer = &timeline.layers[index];
        self.tweens.entry(key).or_insert_with(|| {
            classic_tweens(timeline, index)
                .0
                .into_iter()
                .map(|t| (t.start_frame, Rc::new(t)))
                .collect()
        })
    }
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
                let (regions, dropped) = fill_regions(&path.contours, &m);
                if dropped > 0 {
                    cx.report
                        .skip("fill hole outside any outer loop (left out)", loc);
                }
                for region in regions {
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
            // `scaleMode="none"` keeps the authored width; a hairline (weight
            // 0) is one pixel. Skew and non-uniform scale use the mean scale.
            let width = match (stroke.weight, stroke.scale_mode.as_deref()) {
                (w, _) if w <= 0.0 => 1.0,
                (w, Some("none")) => w,
                (w, _) => (w * scale(&m)).max(0.05),
            };
            let miter = stroke.miter_limit;
            for c in &path.contours {
                let mut anchors = crate::paths::anchors(c, &m);
                let mut closed = c.closed;
                if closed && anchors.len() > MAX_ANCHORS {
                    // Open a long closed stroke so it can be cut into pieces.
                    let mut back = anchors[0];
                    back.handle_out = ZERO;
                    anchors[0].handle_in = ZERO;
                    anchors.push(back);
                    closed = false;
                }
                let body = json!({"strokeColor": color, "strokeWidth": width,
                    "strokeCap": cap, "strokeJoin": join, "miterLimit": miter});
                push_path(out, &anchors, closed, body, &mut cx.report, loc);
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
        (SymbolType::Graphic, "play once") => inst.first_frame.saturating_add(offset).min(len - 1),
        (SymbolType::Graphic, _) => {
            ((u64::from(inst.first_frame) + u64::from(offset)) % u64::from(len)) as u32
        }
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
                if cx.expanding.contains(&sym.name.as_str()) {
                    cx.report.skip("symbol placed inside itself", loc);
                    continue;
                }
                if inst.symbol_type != SymbolType::Graphic {
                    cx.report
                        .skip("movie clip or button drawn at its first frame", loc);
                }
                let len = *cx
                    .frame_counts
                    .entry(sym.name.as_str())
                    .or_insert_with(|| sym.timeline.frame_count());
                let f = symbol_frame(inst, offset, len);
                cx.expanding.push(sym.name.as_str());
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
                cx.expanding.pop();
            }
            Element::BitmapInstance { .. } => cx.report.skip("bitmap instance", loc),
            Element::Text { .. } => cx.report.skip("text", loc),
            Element::Other { .. } => {}
        }
    }
}

/// The keyframe span of `layer` holding frame `f`.
fn span_at(layer: &Layer, f: u32) -> Option<&Frame> {
    // Spans are sorted by index: take the last one starting at or before f.
    let n = layer.frames.partition_point(|s| s.index <= f);
    let s = layer.frames.get(n.checked_sub(1)?)?;
    (f < s.index.saturating_add(s.duration.max(1))).then_some(s)
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

/// Whether every frame of `span` draws the same: no classic tween, and every
/// symbol instance shows one fixed frame.
fn is_static(timeline: &Timeline, index: usize, span: &Frame, cx: &mut Ctx) -> bool {
    fn fixed(elements: &[Element], cx: &Ctx) -> bool {
        elements.iter().all(|e| match e {
            Element::Group { members } => fixed(members, cx),
            Element::SymbolInstance(i) => {
                i.symbol_type != SymbolType::Graphic
                    || i.loop_mode == "single frame"
                    || cx
                        .symbols
                        .get(i.library_item.as_str())
                        .is_none_or(|s| s.timeline.frame_count() <= 1)
            }
            _ => true,
        })
    }
    !cx.tweens_of(timeline, index).contains_key(&span.index) && fixed(&span.elements, cx)
}

/// A layer shows only when it and every folder above it are visible.
fn visible(timeline: &Timeline, index: usize) -> bool {
    let mut at = Some(index);
    for _ in 0..=timeline.layers.len() {
        let Some(layer) = at.and_then(|i| timeline.layers.get(i)) else {
            return true;
        };
        if !layer.visible {
            return false;
        }
        // Only folders hide what they hold; a hidden guide or mask layer
        // does not hide the layers it guides or masks.
        at = layer.parent_layer_index.filter(|&p| {
            timeline
                .layers
                .get(p)
                .is_some_and(|l| l.kind == LayerKind::Folder)
        });
    }
    true
}

/// Convert the first scene of `doc` into a legacy Nemo project.
pub fn to_legacy_project(doc: &XflDocument) -> LegacyProject {
    let mut cx = Ctx {
        symbols: doc.symbols.iter().map(|s| (s.name.as_str(), s)).collect(),
        tweens: HashMap::new(),
        frame_counts: HashMap::new(),
        expanding: Vec::new(),
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
    let mut total = scene.frame_count().max(1);
    if total > MAX_FRAMES {
        cx.report
            .skip(format!("frames after {MAX_FRAMES}"), "DOMDocument.xml");
        total = MAX_FRAMES;
    }
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
        let mut drawn_span: Option<*const Frame> = None;
        for f in 0..total {
            // Inside a span whose drawing cannot change, the frame is held.
            let span = span_at(layer, f).map(|s| s as *const Frame);
            if previous.is_some() && span == drawn_span {
                let still = span_at(layer, f).is_none_or(|s| is_static(scene, index, s, &mut cx));
                if still {
                    frames.push(json!({"strokes": []}));
                    continue;
                }
            }
            drawn_span = span;
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
            "visible": visible(scene, index),
            "locked": layer.locked,
            "frames": frames,
        });
        if let Some(c) = layer.color.as_deref().filter(|c| c.starts_with('#')) {
            record["color"] = json!(c);
        }
        layers.push(record);
        cx.report.count("layers", 1);
    }
    if layers.is_empty() {
        // The native reader needs at least one layer.
        layers.push(json!({"name": "Layer 1", "layerUid": "xfl-layer-empty",
            "frames": [{"isKeyframe": true, "strokes": []}]}));
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
