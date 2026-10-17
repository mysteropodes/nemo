//! `shapeGeom` of `shape-bridge.js` (G22/#2107, leaf L01-B) at fbe4b8a3, with its
//! `ellipseGeomJS`/`rectGeomJS`/`lineGeomJS` fallbacks and the `geometry-wasm`
//! `line_segments`/`rect_segments`/`ellipse_segments` it tries first. Split out of
//! `object_parametric_geometry.rs`, which re-exports everything here.
//!
//! The two sources really differ and both are baseline behaviour: the wasm keeps
//! the rect anchors in drag order and always emits (zero) handles; the JavaScript
//! fallback normalises the rect to min/max and leaves line and rect handles absent.

use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    build_round_rect, build_star_polygon, check_finite, js_max, js_min, or_default, ParamGeomError,
    ParamSegment, KAPPA,
};

/// A `shapeGeom` segment: the JavaScript fallback leaves the handles of a line
/// and a rect absent, the wasm emits zero handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GeomSegment {
    pub point: [f64; 2],
    pub handle_in: Option<[f64; 2]>,
    pub handle_out: Option<[f64; 2]>,
}

/// `{ segments, closed }` returned by `shapeGeom`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShapeGeom {
    pub segments: Vec<GeomSegment>,
    pub closed: bool,
}

/// The tools `shapeGeom` knows (`state.tool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeTool {
    Line,
    Rect,
    Ellipse,
    Star,
    SpeechBubble,
}

/// Which line/rect/ellipse implementation answers (`window.GeometryWasm.ready`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeomSource {
    Wasm,
    JsFallback,
}

/// `state.starPointCount` / `state.starInnerRatio` (absent reads as the default).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct StarState {
    pub point_count: Option<f64>,
    pub inner_ratio: Option<f64>,
}

fn with_handles(segs: Vec<ParamSegment>) -> Vec<GeomSegment> {
    segs.into_iter()
        .map(|s| GeomSegment {
            point: s.point,
            handle_in: Some(s.handle_in),
            handle_out: Some(s.handle_out),
        })
        .collect()
}

fn geom_corner(point: [f64; 2], handles: bool) -> GeomSegment {
    GeomSegment {
        point,
        handle_in: handles.then_some([0.0, 0.0]),
        handle_out: handles.then_some([0.0, 0.0]),
    }
}

/// The four-anchor kappa ellipse shared by `ellipseGeomJS` and the wasm `ellipse_segments`.
fn ellipse_geom(x0: f64, y0: f64, x1: f64, y1: f64) -> ShapeGeom {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let s = |point: [f64; 2], hi: [f64; 2], ho: [f64; 2]| GeomSegment {
        point,
        handle_in: Some(hi),
        handle_out: Some(ho),
    };
    ShapeGeom {
        segments: vec![
            s([cx + rx, cy], [0.0, -ky], [0.0, ky]),
            s([cx, cy + ry], [kx, 0.0], [-kx, 0.0]),
            s([cx - rx, cy], [0.0, ky], [0.0, -ky]),
            s([cx, cy - ry], [-kx, 0.0], [kx, 0.0]),
        ],
        closed: true,
    }
}

/// `shapeGeom(tool, x0, y0, x1, y1)`. Star and speech bubble always go through
/// the builders; line, rect and ellipse use the wasm or the JavaScript fallback.
pub(crate) fn shape_geom(
    tool: ShapeTool,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    star: StarState,
    source: GeomSource,
) -> Result<ShapeGeom, ParamGeomError> {
    check_finite(&[x0, y0, x1, y1])?;
    match tool {
        ShapeTool::Star => {
            let scx = (x0 + x1) / 2.0;
            let scy = (y0 + y1) / 2.0;
            let outer_r = js_min((x1 - x0).abs(), (y1 - y0).abs()) / 2.0;
            let segs = build_star_polygon(
                scx,
                scy,
                outer_r,
                Some(or_default(star.point_count, 5.0)),
                star.inner_ratio,
                Some(0.0),
            )?;
            Ok(ShapeGeom {
                segments: with_handles(segs),
                closed: true,
            })
        }
        ShapeTool::SpeechBubble => {
            let bl = js_min(x0, x1);
            let br = js_max(x0, x1);
            let bt = js_min(y0, y1);
            let bb = js_max(y0, y1);
            let rad = js_max(4.0, js_min(br - bl, bb - bt) * 0.18);
            let segs = build_round_rect(bl, bt, br, bb, [rad, rad, rad, rad])?;
            Ok(ShapeGeom {
                segments: with_handles(segs),
                closed: true,
            })
        }
        ShapeTool::Line => {
            let handles = source == GeomSource::Wasm;
            Ok(ShapeGeom {
                segments: vec![
                    geom_corner([x0, y0], handles),
                    geom_corner([x1, y1], handles),
                ],
                closed: false,
            })
        }
        ShapeTool::Rect => Ok(match source {
            GeomSource::Wasm => ShapeGeom {
                segments: vec![
                    geom_corner([x0, y0], true),
                    geom_corner([x1, y0], true),
                    geom_corner([x1, y1], true),
                    geom_corner([x0, y1], true),
                ],
                closed: true,
            },
            GeomSource::JsFallback => {
                let (l, r) = (js_min(x0, x1), js_max(x0, x1));
                let (t, b) = (js_min(y0, y1), js_max(y0, y1));
                ShapeGeom {
                    segments: vec![
                        geom_corner([l, t], false),
                        geom_corner([r, t], false),
                        geom_corner([r, b], false),
                        geom_corner([l, b], false),
                    ],
                    closed: true,
                }
            }
        }),
        ShapeTool::Ellipse => Ok(ellipse_geom(x0, y0, x1, y1)),
    }
}
