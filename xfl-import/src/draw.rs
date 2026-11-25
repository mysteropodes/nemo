//! Flattening: a layer drawn frame by frame into plain vector strokes, with
//! symbol instances expanded, graphic-symbol timelines followed and classic
//! tweens sampled. Used for every layer that is not a component instance and
//! for the layers inside a component.

use serde_json::{json, Value};

use crate::convert::Ctx;
use crate::model::*;
use crate::paths::{fill_regions, push_path, scale, solid, MAX_ANCHORS, ZERO};

/// Deepest symbol nesting followed (a symbol placing itself stops here).
const MAX_DEPTH: usize = 32;

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
                (w, _) if w <= 0.0 => {
                    cx.fixed_width = true;
                    1.0
                }
                (w, Some("none")) => {
                    cx.fixed_width = true;
                    w
                }
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
pub(crate) fn symbol_frame(inst: &SymbolInstance, offset: u32, len: u32) -> u32 {
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
                let len = cx.frame_count(sym);
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
pub(crate) fn span_at(layer: &Layer, f: u32) -> Option<&Frame> {
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

/// Frames of layer `index` drawn into strokes, a keyframe where the drawing
/// changes. Stroke ids start with `tag`.
pub(crate) fn flatten_frames(
    timeline: &Timeline,
    index: usize,
    total: u32,
    tag: &str,
    cx: &mut Ctx,
) -> Vec<Value> {
    let layer = &timeline.layers[index];
    let mut frames = Vec::with_capacity(total as usize);
    let mut previous: Option<Vec<Value>> = None;
    let mut drawn_span: Option<*const Frame> = None;
    for f in 0..total {
        // Inside a span whose drawing cannot change, the frame is held.
        let span = span_at(layer, f).map(|s| s as *const Frame);
        if previous.is_some() && span == drawn_span {
            let still = span_at(layer, f).is_none_or(|s| is_static(timeline, index, s, cx));
            if still {
                frames.push(json!({"strokes": []}));
                continue;
            }
        }
        drawn_span = span;
        let loc = format!("{} frame {}", layer.name, f + 1);
        let mut strokes = Vec::new();
        let m = Matrix::IDENTITY;
        draw_layer(&mut strokes, timeline, index, f, &m, 1.0, 0, cx, &loc);
        if previous.as_ref() == Some(&strokes) {
            frames.push(json!({"strokes": []}));
            continue;
        }
        let ids: Vec<Value> = strokes
            .iter()
            .enumerate()
            .map(|(n, s)| {
                let mut s = s.clone();
                s["strokeId"] = json!(format!("{tag}-{index}-{f}-{n}"));
                s
            })
            .collect();
        cx.report.count("keyframes", 1);
        cx.report.count("paths", ids.len() as u64);
        frames.push(json!({"isKeyframe": true, "strokes": ids}));
        previous = Some(strokes);
    }
    frames
}
