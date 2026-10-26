//! #2358: tween resample; adapted from geometry-wasm at 46442ea9.
use crate::tween_kernels::*;

// ---- resampleP / resampleCenterline ----
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResampledOut {
    pub segments: Vec<SegOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widths: Option<Vec<f64>>,
    #[serde(default)]
    pub is_vector_brush: bool,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegOut {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

pub(super) fn resample_stroke_inner(s: &StrokeIn, n: usize) -> ResampledOut {
    let vb = is_vb(s);
    let path = build_feat_path(s);
    let sampler = PathSampler::new(&path);
    let len = sampler.total;
    if len < 1.0 || n < 2 {
        // JS falls back to returning `sd` unchanged in this edge case;
        // mirrored here by emitting the un-resampled points as-is.
        if vb {
            let cs = s.center_segments.as_ref().unwrap();
            return ResampledOut {
                segments: cs
                    .iter()
                    .map(|c| SegOut {
                        point: c.point,
                        handle_in: c.handle_in,
                        handle_out: c.handle_out,
                    })
                    .collect(),
                widths: Some(cs.iter().map(|c| c.width).collect()),
                is_vector_brush: true,
                stroke_color: None,
                fill_color: s.fill_color.clone(),
            };
        }
        return ResampledOut {
            segments: s
                .segments
                .iter()
                .map(|c| SegOut {
                    point: c.point,
                    handle_in: c.handle_in,
                    handle_out: c.handle_out,
                })
                .collect(),
            widths: None,
            is_vector_brush: false,
            stroke_color: s.stroke_color.clone(),
            fill_color: s.fill_color.clone(),
        };
    }

    let mut segments = Vec::with_capacity(n);
    let mut widths: Option<Vec<f64>> = if vb {
        Some(Vec::with_capacity(n))
    } else {
        None
    };

    // Centerline width interpolation walks the RAW anchor-to-anchor chord
    // lengths (not the smooth arc length), exactly like resampleCenterline —
    // widths are stored per anchor point, linearly interpolated between them.
    let width_lens: Vec<f64> = if vb {
        let cs = s.center_segments.as_ref().unwrap();
        let mut acc = vec![0.0];
        for i in 1..cs.len() {
            let (dx, dy) = (
                cs[i].point[0] - cs[i - 1].point[0],
                cs[i].point[1] - cs[i - 1].point[1],
            );
            acc.push(acc[i - 1] + (dx * dx + dy * dy).sqrt());
        }
        acc
    } else {
        Vec::new()
    };
    let width_total = width_lens.last().copied().unwrap_or(1.0).max(1.0);

    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let off = t * len;
        let pt = sampler.point_at(off);
        let (tx, ty) = sampler.tangent_at(off);
        let hl = len / (n - 1) as f64 / 3.0;
        segments.push(SegOut {
            point: [pt.x, pt.y],
            handle_in: if i == 0 {
                [0.0, 0.0]
            } else {
                [-tx * hl, -ty * hl]
            },
            handle_out: if i == n - 1 {
                [0.0, 0.0]
            } else {
                [tx * hl, ty * hl]
            },
        });
        if vb {
            let cs = s.center_segments.as_ref().unwrap();
            let target = t * width_total;
            let mut wi = 0;
            while wi < width_lens.len().saturating_sub(2) && width_lens[wi + 1] < target {
                wi += 1;
            }
            let span = (width_lens[wi + 1] - width_lens[wi]).max(0.0001);
            let lt = ((target - width_lens[wi]) / span).clamp(0.0, 1.0);
            widths
                .as_mut()
                .unwrap()
                .push(cs[wi].width + (cs[wi + 1].width - cs[wi].width) * lt);
        }
    }

    ResampledOut {
        segments,
        widths,
        is_vector_brush: vb,
        stroke_color: if vb { None } else { s.stroke_color.clone() },
        fill_color: s.fill_color.clone(),
    }
}
