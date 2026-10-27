//! Shape geometry and paint of the native Rive exporter (#2357 G44): `lottieShapeValue`, `riveColorHex`,
//! `riveBuildPaints`, the placeholder, straight and cubic vertex commands and `riveDetectTranslation`
//! (`src/js/rive-export.js` and `src/js/export.js` @1065e9d8), bit for bit.
use crate::export_job::native_vector_export::native_vector_export_rive::{RiveExportError, Stroke};
use kurbo::{BezPath, PathEl, Point};
use serde_json::{json, Number, Value};

pub(crate) fn float(n: &Number) -> Result<f64, RiveExportError> {
    n.as_f64()
        .filter(|v| v.is_finite())
        .ok_or(RiveExportError::NonFinite)
}
pub(crate) fn num(v: f64) -> Value {
    Number::from_f64(v).map_or(Value::Null, Value::Number)
}
pub(crate) fn rgba(
    r: &Number,
    g: &Number,
    b: &Number,
    a: &Number,
) -> Result<[f64; 4], RiveExportError> {
    Ok([float(r)?, float(g)?, float(b)?, float(a)?])
}

/// `lottieShapeValue(sd, null)` over the drawn path: anchors with relative handles (an open path's outer handles,
/// which the drawn path does not carry, are zero).
pub(crate) fn geometry(
    path: &BezPath,
    index: usize,
) -> Result<(Vec<[f64; 2]>, Vec<[f64; 2]>, Vec<[f64; 2]>, bool), RiveExportError> {
    let unsupported = |reason| RiveExportError::Unsupported {
        source_object_index: index,
        reason,
    };
    let (mut v, mut i, mut o): (Vec<[f64; 2]>, Vec<[f64; 2]>, Vec<[f64; 2]>) =
        (vec![], vec![], vec![]);
    let mut closed = false;
    let rel = |c: Point, p: [f64; 2]| [c.x - p[0], c.y - p[1]];
    for element in path.elements() {
        if closed {
            return Err(unsupported(
                "a drawn path with several subpaths has no single Rive shape",
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
                let (first, last) = (v[0], v.len() - 1);
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
    if v.iter()
        .chain(&i)
        .chain(&o)
        .flatten()
        .any(|c| !c.is_finite())
    {
        return Err(RiveExportError::NonFinite);
    }
    Ok((v, i, o, closed))
}

/// `riveColorHex(hex, opacity)` over the hex the baseline stores: `#AARRGGBB`, the alpha the colour's own when it is
/// translucent (an eight-digit hex), else `Math.round(opacity * 255)` unclamped.
pub(crate) fn color_hex(paint: Option<[f64; 4]>, opacity: f64) -> String {
    let Some([r, g, b, a]) = paint else {
        return "#00000000".to_string();
    };
    let byte = |c: f64| format!("{:02x}", (c.clamp(0.0, 1.0) * 255.0).round() as u32);
    let alpha = if a < 1.0 {
        byte(a)
    } else {
        let n = opacity * 255.0;
        let n = if n < 0.0 {
            (n + 0.5).floor()
        } else {
            n.round()
        };
        let text = if n < 0.0 {
            format!("-{:x}", (-n) as u64)
        } else {
            format!("{:x}", n as u64)
        };
        format!("{text:0>2}")
    };
    format!("#{alpha}{}{}{}", byte(r), byte(g), byte(b)).to_uppercase()
}

/// `riveBuildPaints(sd, hasRealStroke, opacityOverride)`: the stroke first, then the fill.
pub(crate) fn paints(sd: &Stroke, real: bool, opacity: Option<f64>) -> Value {
    let op = opacity.unwrap_or(sd.opacity);
    let mut out = vec![];
    if real {
        let width = if sd.width == 0.0 { 2.0 } else { sd.width };
        out.push(json!({"paintType":"stroke","color":color_hex(sd.stroke, op),"width":num(width)}));
    }
    if sd.fill.is_some() {
        out.push(json!({"paintType":"fill","color":color_hex(sd.fill, op)}));
    }
    Value::Array(out)
}
pub(crate) fn point_command(kind: &str, p: [f64; 2]) -> Value {
    json!({"commandType":kind,"x":num(p[0]),"y":num(p[1])})
}
/// `riveBuildStraightCommands(points, closed)`.
pub(crate) fn straight(points: &[[f64; 2]], closed: bool) -> Value {
    let mut out: Vec<Value> = points
        .iter()
        .enumerate()
        .map(|(k, &p)| point_command(if k == 0 { "moveTo" } else { "lineTo" }, p))
        .collect();
    if closed && !out.is_empty() {
        out.push(json!({"commandType":"close"}));
    }
    Value::Array(out)
}
/// `riveCubicVerticesFromShapeVal`: absolute handles, the closure carried by the path's own flag.
pub(crate) fn cubic_vertices(sd: &Stroke) -> Vec<Value> {
    (0..sd.v.len())
        .map(|k| {
            let (v, i, o) = (sd.v[k], sd.i[k], sd.o[k]);
            json!({"x":num(v[0]),"y":num(v[1]),"type":"cubic","inX":num(v[0] + i[0]),"inY":num(v[1] + i[1]),
                "outX":num(v[0] + o[0]),"outY":num(v[1] + o[1])})
        })
        .collect()
}
/// `riveDetectTranslation`: every anchor moves with anchor 0 within 0.05 on each frame of the run.
pub(crate) fn translation(points: &[&Vec<[f64; 2]>]) -> Option<Vec<[f64; 2]>> {
    let base = points[0];
    let mut deltas = vec![[0.0, 0.0]];
    for pts in &points[1..] {
        if pts.len() != base.len() {
            return None;
        }
        let (dx, dy) = (pts[0][0] - base[0][0], pts[0][1] - base[0][1]);
        if (1..base.len()).any(|k| {
            ((pts[k][0] - base[k][0]) - dx).abs() > 0.05
                || ((pts[k][1] - base[k][1]) - dy).abs() > 0.05
        }) {
            return None;
        }
        deltas.push([dx, dy]);
    }
    Some(deltas)
}
