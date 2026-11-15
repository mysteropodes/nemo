//! `DOMShape` → fill loops and stroke paths.
//!
//! Each `<Edge>` carries a left fill (`fillStyle0`), a right fill
//! (`fillStyle1`) and a stroke style. Every drawing command becomes one
//! directed piece; right-fill pieces keep their direction, left-fill pieces
//! are reversed, so each fill's pieces chain into closed loops with a
//! consistent winding (outer boundaries one way, holes the other). Chaining
//! happens at command granularity on exact fixed-point vertices, which
//! tolerates edges listed in any order.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::edges::{decode_fixed, Fx, FxCmd};
use crate::model::*;
use crate::report::ImportReport;
use crate::xml::Node;

pub(crate) fn parse_color(node: &Node, color_attr: &str, alpha_attr: &str) -> Option<Rgba> {
    let raw = node.attr(color_attr).unwrap_or("#000000");
    let hex = raw.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        3 => {
            let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
            (d(0)?, d(1)?, d(2)?)
        }
        _ => return None,
    };
    let a = node.attr_f64(alpha_attr).unwrap_or(1.0).clamp(0.0, 1.0);
    Some(Rgba { r, g, b, a })
}

pub(crate) fn parse_matrix(parent: &Node) -> Matrix {
    let Some(m) = parent.child("matrix").and_then(|w| w.child("Matrix")) else {
        return Matrix::IDENTITY;
    };
    Matrix {
        a: m.attr_f64("a").unwrap_or(1.0),
        b: m.attr_f64("b").unwrap_or(0.0),
        c: m.attr_f64("c").unwrap_or(0.0),
        d: m.attr_f64("d").unwrap_or(1.0),
        tx: m.attr_f64("tx").unwrap_or(0.0),
        ty: m.attr_f64("ty").unwrap_or(0.0),
    }
}

fn parse_spread(node: &Node) -> Spread {
    match node.attr("spreadMethod") {
        Some("reflect") => Spread::Reflect,
        Some("repeat") => Spread::Repeat,
        _ => Spread::Pad,
    }
}

fn parse_stops(node: &Node, report: &mut ImportReport, loc: &str) -> Vec<GradientStop> {
    node.children_named("GradientEntry")
        .map(|e| GradientStop {
            ratio: e.attr_f64("ratio").unwrap_or(0.0).clamp(0.0, 1.0),
            color: parse_color(e, "color", "alpha").unwrap_or_else(|| {
                report.skip("unreadable gradient colour (black used)", loc);
                Rgba::BLACK
            }),
        })
        .collect()
}

/// Parse the paint inside a `<FillStyle>` or a stroke's `<fill>`.
pub(crate) fn parse_paint(holder: &Node, report: &mut ImportReport, loc: &str) -> Option<Paint> {
    for p in &holder.children {
        match p.name.as_str() {
            "SolidColor" => {
                let color = parse_color(p, "color", "alpha").unwrap_or_else(|| {
                    report.skip("unreadable colour (black used)", loc);
                    Rgba::BLACK
                });
                return Some(Paint::Solid { color });
            }
            "LinearGradient" => {
                return Some(Paint::Linear {
                    matrix: parse_matrix(p),
                    stops: parse_stops(p, report, loc),
                    spread: parse_spread(p),
                })
            }
            "RadialGradient" => {
                return Some(Paint::Radial {
                    matrix: parse_matrix(p),
                    stops: parse_stops(p, report, loc),
                    spread: parse_spread(p),
                    focal_ratio: p.attr_f64("focalPointRatio").unwrap_or(0.0),
                })
            }
            "BitmapFill" => {
                report.skip("bitmap fill", loc);
                return Some(Paint::Bitmap {
                    bitmap_path: p.attr("bitmapPath").unwrap_or("").to_string(),
                    matrix: parse_matrix(p),
                });
            }
            _ => {}
        }
    }
    None
}

fn parse_stroke(style: &Node, report: &mut ImportReport, loc: &str) -> Option<Stroke> {
    let s = style.children.first()?;
    if s.name != "SolidStroke" {
        report.skip(format!("{} drawn as a solid stroke", s.name), loc);
    }
    if s.attr("solidStyle") == Some("hairline") {
        report.skip("hairline stroke (1 px used)", loc);
    }
    if s.children.iter().any(|c| c.name == "VariablePointWidth")
        || s.attr("variablePointWidth").is_some()
    {
        report.skip("variable-width stroke (constant width used)", loc);
    }
    let paint = s
        .child("fill")
        .and_then(|f| parse_paint(f, report, loc))
        .unwrap_or(Paint::Solid { color: Rgba::BLACK });
    Some(Stroke {
        paint,
        weight: s.attr_f64("weight").unwrap_or(1.0).max(0.0),
        caps: s.attr("caps").unwrap_or("round").to_string(),
        joints: s.attr("joints").unwrap_or("round").to_string(),
        miter_limit: s.attr_f64("miterLimit").unwrap_or(3.0),
        scale_mode: s.attr("scaleMode").map(str::to_string),
        style: s.name.clone(),
    })
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    from: Fx,
    ctrl: Option<Fx>,
    to: Fx,
}

impl Piece {
    fn reversed(self) -> Piece {
        Piece {
            from: self.to,
            ctrl: self.ctrl,
            to: self.from,
        }
    }
    fn segment(self) -> Segment {
        match self.ctrl {
            None => Segment::Line { to: self.to.px() },
            Some(c) => Segment::Quad {
                ctrl: c.px(),
                to: self.to.px(),
            },
        }
    }
}

/// Chain directed pieces into loops. Returns (contours, open_count).
fn assemble_loops(pieces: &[Piece]) -> (Vec<Contour>, usize) {
    let mut by_start: HashMap<Fx, VecDeque<usize>> = HashMap::new();
    for (i, p) in pieces.iter().enumerate() {
        by_start.entry(p.from).or_default().push_back(i);
    }
    let mut used = vec![false; pieces.len()];
    let mut contours = Vec::new();
    let mut open = 0;
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        let origin = pieces[first].from;
        let mut segments = Vec::new();
        let mut cur = first;
        let closed = loop {
            used[cur] = true;
            segments.push(pieces[cur].segment());
            let end = pieces[cur].to;
            if end == origin {
                break true;
            }
            let next = by_start.get_mut(&end).and_then(|q| {
                while let Some(&i) = q.front() {
                    if used[i] {
                        q.pop_front();
                    } else {
                        return Some(i);
                    }
                }
                None
            });
            match next {
                Some(n) => cur = n,
                None => break false,
            }
        };
        if !closed {
            open += 1;
        }
        contours.push(Contour {
            start: origin.px(),
            segments,
            closed,
        });
    }
    (contours, open)
}

/// Join consecutive stroke pieces into polylines, keeping drawing order.
fn assemble_strokes(pieces: &[Piece]) -> Vec<Contour> {
    let mut out: Vec<(Fx, Fx, Vec<Segment>)> = Vec::new();
    for p in pieces {
        match out.last_mut() {
            Some((_, end, segs)) if *end == p.from => {
                segs.push(p.segment());
                *end = p.to;
            }
            _ => out.push((p.from, p.to, vec![p.segment()])),
        }
    }
    out.into_iter()
        .map(|(start, end, segments)| Contour {
            start: start.px(),
            closed: start == end
                && (segments.len() > 1 || matches!(segments[0], Segment::Quad { .. })),
            segments,
        })
        .collect()
}

pub(crate) fn read_shape(node: &Node, report: &mut ImportReport, loc: &str) -> Shape {
    let mut fills: BTreeMap<u32, Paint> = BTreeMap::new();
    for fs in node.list("fills").filter(|n| n.name == "FillStyle") {
        if let (Some(i), Some(p)) = (fs.attr_u32("index"), parse_paint(fs, report, loc)) {
            fills.insert(i, p);
        }
    }
    let mut strokes: BTreeMap<u32, Stroke> = BTreeMap::new();
    for ss in node.list("strokes").filter(|n| n.name == "StrokeStyle") {
        if let (Some(i), Some(s)) = (ss.attr_u32("index"), parse_stroke(ss, report, loc)) {
            strokes.insert(i, s);
        }
    }
    let mut fill_pieces: BTreeMap<u32, Vec<Piece>> = BTreeMap::new();
    let mut stroke_pieces: BTreeMap<u32, Vec<Piece>> = BTreeMap::new();
    for edge in node.list("edges").filter(|n| n.name == "Edge") {
        let Some(data) = edge.attr("edges") else {
            if edge.attr("cubics").is_some() {
                report.skip("edge stored only as cubics (not read)", loc);
            }
            continue;
        };
        let cmds = match decode_fixed(data) {
            Ok(c) => c,
            Err(_) => {
                report.skip("unreadable shape edge", loc);
                continue;
            }
        };
        let f0 = edge.attr_u32("fillStyle0").unwrap_or(0);
        let f1 = edge.attr_u32("fillStyle1").unwrap_or(0);
        let s = edge.attr_u32("strokeStyle").unwrap_or(0);
        let mut pen = Fx(0, 0);
        for cmd in cmds {
            let piece = match cmd {
                FxCmd::Move(p) => {
                    pen = p;
                    continue;
                }
                FxCmd::Line(p) => Piece {
                    from: pen,
                    ctrl: None,
                    to: p,
                },
                FxCmd::Quad(c, p) => Piece {
                    from: pen,
                    ctrl: Some(c),
                    to: p,
                },
            };
            pen = piece.to;
            if piece.from == piece.to && piece.ctrl.is_none_or(|c| c == piece.from) {
                continue;
            }
            if f0 > 0 && f0 == f1 {
                // Same fill on both sides: an interior edge, not a boundary.
                if s > 0 {
                    stroke_pieces.entry(s).or_default().push(piece);
                }
                continue;
            }
            if f1 > 0 {
                fill_pieces.entry(f1).or_default().push(piece);
            }
            if f0 > 0 {
                fill_pieces.entry(f0).or_default().push(piece.reversed());
            }
            if s > 0 {
                stroke_pieces.entry(s).or_default().push(piece);
            }
        }
    }
    let mut paths = Vec::new();
    for (index, pieces) in &fill_pieces {
        let Some(paint) = fills.get(index) else {
            report.skip("edge references a missing fill style", loc);
            continue;
        };
        let (contours, open) = assemble_loops(pieces);
        for _ in 0..open {
            report.skip("fill contour that does not close (kept open)", loc);
        }
        report.count("fillContours", contours.len() as u64);
        paths.push(ShapePath {
            fill: Some(paint.clone()),
            stroke: None,
            contours,
        });
    }
    for (index, pieces) in &stroke_pieces {
        let Some(stroke) = strokes.get(index) else {
            report.skip("edge references a missing stroke style", loc);
            continue;
        };
        let contours = assemble_strokes(pieces);
        report.count("strokePaths", contours.len() as u64);
        paths.push(ShapePath {
            fill: None,
            stroke: Some(stroke.clone()),
            contours,
        });
    }
    report.count("shapes", 1);
    Shape {
        matrix: parse_matrix(node),
        is_drawing_object: node.attr_bool("isDrawingObject").unwrap_or(false),
        paths,
    }
}

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// Ellipse as 8 quadratic arcs (the construction Flash itself uses).
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Contour {
    let step = std::f64::consts::FRAC_PI_4;
    let k = 1.0 / (step / 2.0).cos();
    let segments = (1..=8)
        .map(|i| {
            let a = step * i as f64;
            let m = a - step / 2.0;
            Segment::Quad {
                ctrl: pt(cx + rx * k * m.cos(), cy + ry * k * m.sin()),
                to: pt(cx + rx * a.cos(), cy + ry * a.sin()),
            }
        })
        .collect();
    Contour {
        start: pt(cx + rx, cy),
        segments,
        closed: true,
    }
}

/// `DOMRectangleObject` / `DOMOvalObject` (Animate primitives): geometry is
/// stored as attributes, not edges. Rounded corners, arcs and inner radii are
/// reported and drawn as the plain rectangle or full ellipse.
pub(crate) fn read_primitive(node: &Node, report: &mut ImportReport, loc: &str) -> Shape {
    let x = node.attr_f64("x").unwrap_or(0.0);
    let y = node.attr_f64("y").unwrap_or(0.0);
    let w = node.attr_f64("objectWidth").unwrap_or(0.0).max(0.0);
    let h = node.attr_f64("objectHeight").unwrap_or(0.0).max(0.0);
    let contour = if node.name == "DOMOvalObject" {
        let partial = node.attr_f64("startAngle").is_some_and(|v| v != 0.0)
            || node
                .attr_f64("endAngle")
                .is_some_and(|v| v != 0.0 && v != 360.0)
            || node.attr_f64("innerRadius").is_some_and(|v| v != 0.0);
        if partial {
            report.skip(
                "oval primitive arc or inner radius (full ellipse used)",
                loc,
            );
        }
        ellipse(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0)
    } else {
        let rounded = [
            "topLeftRadius",
            "topRightRadius",
            "bottomLeftRadius",
            "bottomRightRadius",
        ]
        .iter()
        .any(|k| node.attr_f64(k).is_some_and(|v| v != 0.0));
        if rounded {
            report.skip(
                "rectangle primitive corner radius (square corners used)",
                loc,
            );
        }
        Contour {
            start: pt(x, y),
            segments: vec![
                Segment::Line { to: pt(x + w, y) },
                Segment::Line {
                    to: pt(x + w, y + h),
                },
                Segment::Line { to: pt(x, y + h) },
                Segment::Line { to: pt(x, y) },
            ],
            closed: true,
        }
    };
    let mut paths = Vec::new();
    if let Some(paint) = node.child("fill").and_then(|f| parse_paint(f, report, loc)) {
        paths.push(ShapePath {
            fill: Some(paint),
            stroke: None,
            contours: vec![contour.clone()],
        });
    }
    if let Some(stroke) = node
        .child("stroke")
        .and_then(|s| parse_stroke(s, report, loc))
    {
        paths.push(ShapePath {
            fill: None,
            stroke: Some(stroke),
            contours: vec![contour],
        });
    }
    report.count("shapes", 1);
    report.count("primitives", 1);
    Shape {
        matrix: parse_matrix(node),
        is_drawing_object: true,
        paths,
    }
}
