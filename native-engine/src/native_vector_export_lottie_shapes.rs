//! Path values, keyed properties and paint helpers of the native Lottie writer (#2357): `lottieShapeValue`,
//! `lottieCamPoint`/`lottieCamVector`, `lottieHexToRGBA`, `lottieBmCode` and the dab outline
//! (`src/js/export.js` @1065e9d8).
use crate::export_job::native_vector_export::native_vector_export_lottie::LottieExportError;
use crate::render_geometry::ClosedCubicPath;
use kurbo::{BezPath, PathEl, Point};
use serde_json::{json, Number, Value};

pub(crate) fn number(value: f64) -> Result<Value, LottieExportError> {
    Number::from_f64(value)
        .map(Value::Number)
        .ok_or(LottieExportError::NonFinite)
}
pub(crate) fn pair(value: [f64; 2]) -> Result<Value, LottieExportError> {
    Ok(Value::Array(vec![number(value[0])?, number(value[1])?]))
}
pub(crate) fn float(value: &Number) -> Result<f64, LottieExportError> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(LottieExportError::NonFinite)
}

/// `lottieShapeValue`: `i`, `o`, `v` and `c` from the drawn path (one subpath of cubic segments, as
/// `build_path` writes it; a closed path's closing curve ends on its first point), through the
/// frame's camera matrix `[a, b, c, d, e, f]` when one is stored (`lottieCamPoint`/`lottieCamVector`).
pub(crate) fn shape_value(
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
pub(crate) fn r3(value: f64) -> f64 {
    (value * 1000.0 + 0.5).floor() / 1000.0
}
/// A produced dab as the baseline stores it (`serP`): its own anchors and relative handles through `_r3`.
pub(crate) fn dab_shape(
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
pub(crate) fn keyed(
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
pub(crate) fn rgb(r: &Number, g: &Number, b: &Number) -> Result<Value, LottieExportError> {
    Ok(Value::Array(vec![
        number(float(r)?)?,
        number(float(g)?)?,
        number(float(b)?)?,
        json!(1),
    ]))
}

/// `lottieBmCode`: the blend-mode order lottie-web expects; anything else is Normal.
pub(crate) fn blend_code(mode: Option<&str>) -> u32 {
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

pub(crate) fn identity_ks() -> Value {
    json!({"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}})
}
