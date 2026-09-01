// Image vectorization (2026-09) — "un system puissant de vectorisation".
// Traces a raster (PNG/JPEG/etc, whatever `image` can decode) into flat
// color regions with cubic-bezier boundaries, Illustrator "Image Trace"
// style. Runs natively in Rust via the `vtracer` crate (visioncortex),
// NOT through geometry-wasm — this is a one-shot, potentially CPU-heavy
// batch conversion, not a per-frame render, so it belongs next to the
// other native Tauri commands (run_ffmpeg, video_decode) rather than
// bloating the real-time WebGPU engine's wasm binary. Desktop-only for
// now (gated by tauriOk() on the JS side, same convention as feedback
// submission) — no web-build equivalent yet.
//
// Output contract: raw geometry only (absolute-coordinate cubic-bezier
// control-point chains, or plain polygons), NOT an SVG string — the
// caller (vectorize-bridge.js) builds real paper.Path/CompoundPath
// objects directly from this, so there's no SVG string round-trip to
// parse or drift out of sync with Nemo's own segment format.

use base64::Engine;
use serde::{Deserialize, Serialize};
use vtracer::{ColorImage, Config};
use visioncortex::{CompoundPathElement, PathSimplifyMode};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VectorizeConfigIn {
    pub color_mode: Option<String>,   // "color" | "binary"
    pub hierarchical: Option<String>, // "stacked" | "cutout"
    pub filter_speckle: Option<usize>,
    pub color_precision: Option<i32>,
    pub layer_difference: Option<i32>,
    pub corner_threshold: Option<i32>,
    pub length_threshold: Option<f64>,
    pub max_iterations: Option<usize>,
    pub splice_threshold: Option<i32>,
    pub path_precision: Option<u32>,
}

#[derive(Serialize)]
pub struct VecContour {
    // "spline" — `points` is an absolute cubic-bezier control-point chain
    // in vtracer's own "1+3n" convention: point 0 is the contour's start,
    // then each following group of 3 is (control1, control2, nextAnchor).
    // "polygon" — `points` is a plain vertex list (straight edges only).
    // Flattened as [x0,y0, x1,y1, ...] — a typed array on the JS side is
    // cheaper to hand across the Tauri IPC boundary than nested objects.
    pub kind: &'static str,
    pub points: Vec<f64>,
}

#[derive(Serialize)]
pub struct VecShape {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    // A shape can have more than one contour when it has holes (a donut)
    // — becomes a paper.CompoundPath on the JS side instead of a plain
    // Path, exactly the CLAUDE.md §1 CompoundPath convention already used
    // for boolean-op results.
    pub contours: Vec<VecContour>,
}

#[derive(Serialize)]
pub struct VectorizeResult {
    pub width: usize,
    pub height: usize,
    pub shapes: Vec<VecShape>,
}

fn contour_from_element(el: &CompoundPathElement) -> VecContour {
    match el {
        CompoundPathElement::Spline(s) => {
            let points = s.points.iter().flat_map(|p| [p.x, p.y]).collect();
            VecContour { kind: "spline", points }
        }
        CompoundPathElement::PathF64(p) => {
            let points = p.path.iter().flat_map(|pt| [pt.x, pt.y]).collect();
            VecContour { kind: "polygon", points }
        }
        CompoundPathElement::PathI32(p) => {
            let points = p.path.iter().flat_map(|pt| [pt.x as f64, pt.y as f64]).collect();
            VecContour { kind: "polygon", points }
        }
    }
}

#[tauri::command]
pub async fn vectorize_image(image_base64: String, config: VectorizeConfigIn) -> Result<VectorizeResult, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64.as_bytes())
        .map_err(|e| e.to_string())?;
    let decoded = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (width, height) = (decoded.width() as usize, decoded.height() as usize);
    let color_img = ColorImage { pixels: decoded.into_raw(), width, height };
    vectorize_color_image(color_img, config)
}

// Pure core, no Tauri/async dependency — split out so a plain `#[test]` can
// exercise the real vtracer pipeline (color clustering + curve fitting)
// end-to-end without a native window, the one thing these tools can't
// drive from this sandbox.
fn vectorize_color_image(color_img: ColorImage, config: VectorizeConfigIn) -> Result<VectorizeResult, String> {
    let (width, height) = (color_img.width, color_img.height);
    let mut cfg = Config::default();
    // mode stays Config::default()'s PathSimplifyMode::Spline — smooth
    // curves is the entire point of a "puissant" tracer, never overridden.
    let _ = PathSimplifyMode::Spline; // keeps the import intentional/documented
    if let Some(cm) = &config.color_mode {
        cfg.color_mode = cm.parse().map_err(|e: String| e)?;
    }
    if let Some(h) = &config.hierarchical {
        cfg.hierarchical = h.parse().map_err(|e: String| e)?;
    }
    if let Some(v) = config.filter_speckle {
        cfg.filter_speckle = v;
    }
    if let Some(v) = config.color_precision {
        cfg.color_precision = v;
    }
    if let Some(v) = config.layer_difference {
        cfg.layer_difference = v;
    }
    if let Some(v) = config.corner_threshold {
        cfg.corner_threshold = v;
    }
    if let Some(v) = config.length_threshold {
        cfg.length_threshold = v;
    }
    if let Some(v) = config.max_iterations {
        cfg.max_iterations = v;
    }
    if let Some(v) = config.splice_threshold {
        cfg.splice_threshold = v;
    }
    if let Some(v) = config.path_precision {
        cfg.path_precision = Some(v);
    }

    let svg = vtracer::convert(color_img, cfg)?;
    let shapes = svg
        .paths
        .iter()
        .map(|sp| VecShape {
            r: sp.color.r,
            g: sp.color.g,
            b: sp.color.b,
            contours: sp.path.paths.iter().map(contour_from_element).collect(),
        })
        .collect();

    Ok(VectorizeResult { width, height, shapes })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Red disk (r=15, centered 20,20) with a blue square occluding its
    // right half — one shape unbroken (the visible red crescent), one
    // simple rectangle (the blue occluder). Real ground truth for the
    // Phase 0 pipeline (color clustering + spline fitting actually runs,
    // returns real cubic-bezier contours) — Phase 2's occlusion
    // COMPLETION isn't built yet, so the red shape is expected to still
    // be the cut/crescent form here, not the full circle.
    fn synthetic_occluded_disk() -> ColorImage {
        let (w, h) = (40usize, 40usize);
        let mut pixels = vec![255u8; w * h * 4]; // white background, opaque
        let (cx, cy, r) = (20.0f64, 20.0f64, 15.0f64);
        for y in 0..h {
            for x in 0..w {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                let i = (y * w + x) * 4;
                if dx * dx + dy * dy <= r * r {
                    pixels[i] = 220; pixels[i + 1] = 30; pixels[i + 2] = 30; pixels[i + 3] = 255; // red
                }
                if x >= 20 {
                    pixels[i] = 30; pixels[i + 1] = 30; pixels[i + 2] = 220; pixels[i + 3] = 255; // blue occluder on top
                }
            }
        }
        ColorImage { pixels, width: w, height: h }
    }

    #[test]
    fn vectorizes_two_distinct_color_regions() {
        let img = synthetic_occluded_disk();
        let result = vectorize_color_image(img, VectorizeConfigIn::default()).expect("vectorize_color_image failed");
        assert_eq!(result.width, 40);
        assert_eq!(result.height, 40);
        assert!(result.shapes.len() >= 2, "expected at least a red crescent + a blue rect, got {}", result.shapes.len());

        let has_red = result.shapes.iter().any(|s| s.r > 150 && s.g < 100 && s.b < 100);
        let has_blue = result.shapes.iter().any(|s| s.b > 150 && s.r < 100 && s.g < 100);
        assert!(has_red, "no red shape in output: {:?}", result.shapes.iter().map(|s| (s.r, s.g, s.b)).collect::<Vec<_>>());
        assert!(has_blue, "no blue shape in output: {:?}", result.shapes.iter().map(|s| (s.r, s.g, s.b)).collect::<Vec<_>>());

        // Every contour must be a real, closed-enough curve: spline mode
        // means we expect the "1+3n" cubic-bezier chain shape (n>=1 curve,
        // so at least 4 points) on any contour with more than a couple of
        // pixels of extent — proves the CompoundPathElement::Spline branch
        // (the one this whole bridge is built around) is actually taken,
        // not silently falling through to the polygon branch.
        let any_spline = result.shapes.iter().any(|s| s.contours.iter().any(|c| c.kind == "spline" && c.points.len() >= 8));
        assert!(any_spline, "expected at least one real spline contour with 4+ points");
    }
}
