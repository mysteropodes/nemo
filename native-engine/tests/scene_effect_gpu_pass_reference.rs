//! Independent CPU references for the three single-input shaders, written in
//! Rust straight from the formulas of `color_adjust.wgsl`, `vignette.wgsl` and
//! `blur.wgsl` (not from the Node generator). Texels are straight RGBA8; every
//! pass quantises to RGBA8 round-to-nearest, as the Rgba8Unorm targets do.
//! The blur reference supports the integer sample step (radius <= 24) only.

/// `(sample_step, 49 tap weights)` of blur.wgsl for `radius_px`.
pub(super) fn kernel(radius_px: f32) -> (f32, Vec<f32>) {
    let step = (radius_px / 24.0).max(1.0);
    let sigma = (radius_px / 3.0 / step).max(0.0001);
    let two_sigma2 = 2.0 * sigma * sigma;
    let weights = (-24i32..=24)
        .map(|i| {
            let offset = i as f32;
            (-(offset * offset) / two_sigma2).exp()
        })
        .collect();
    (step, weights)
}

fn quantise(value: f32) -> u8 {
    (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn unit(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

pub(super) fn color_adjust(image: &[u8], brightness: f32, contrast: f32) -> Vec<u8> {
    let factor = 1.0 + contrast.clamp(-1.0, 1.0);
    image
        .chunks_exact(4)
        .flat_map(|texel| {
            let alpha = unit(texel[3]);
            let divisor = alpha.max(0.0001);
            let mut out = [0u8; 4];
            for channel in 0..3 {
                let straight = unit(texel[channel]) / divisor;
                let graded = ((straight - 0.5) * factor + 0.5 + brightness).clamp(0.0, 1.0);
                out[channel] = quantise(graded * alpha);
            }
            out[3] = texel[3];
            out
        })
        .collect()
}

pub(super) fn vignette(image: &[u8], (w, h): (u32, u32), strength: f32, radius: f32) -> Vec<u8> {
    let start = radius.clamp(0.0, 0.99);
    let strength = strength.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(image.len());
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            let d = ((u - 0.5) * (u - 0.5) + (v - 0.5) * (v - 0.5)).sqrt() / 0.7071;
            let falloff = ((d - start) / (1.0 - start).max(0.001)).clamp(0.0, 1.0);
            let darken = 1.0 - falloff * strength;
            let texel = &image[((y * w + x) * 4) as usize..][..4];
            out.extend([
                quantise(unit(texel[0]) * darken),
                quantise(unit(texel[1]) * darken),
                quantise(unit(texel[2]) * darken),
                texel[3],
            ]);
        }
    }
    out
}

fn blur_axis(image: &[u8], (w, h): (u32, u32), weights: &[f32], horizontal: bool) -> Vec<u8> {
    let (w, h) = (w as i32, h as i32);
    let mut out = Vec::with_capacity(image.len());
    for y in 0..h {
        for x in 0..w {
            let (mut color, mut alpha_sum, mut wsum) = ([0.0f32; 3], 0.0f32, 0.0f32);
            for (k, weight) in weights.iter().enumerate() {
                let shift = k as i32 - 24;
                let (sx, sy) = if horizontal {
                    (x + shift, y)
                } else {
                    (x, y + shift)
                };
                let (sx, sy) = (sx.clamp(0, w - 1), sy.clamp(0, h - 1));
                let texel = &image[((sy * w + sx) * 4) as usize..][..4];
                let alpha = unit(texel[3]);
                for channel in 0..3 {
                    color[channel] += unit(texel[channel]) * alpha * weight;
                }
                alpha_sum += alpha * weight;
                wsum += weight;
            }
            let divisor = alpha_sum.max(0.0001);
            out.extend([
                quantise(color[0] / divisor),
                quantise(color[1] / divisor),
                quantise(color[2] / divisor),
                quantise(alpha_sum / wsum.max(0.0001)),
            ]);
        }
    }
    out
}

pub(super) fn blur(image: &[u8], size: (u32, u32), radius_px: f32) -> Vec<u8> {
    let (step, weights) = kernel(radius_px);
    assert_eq!(step, 1.0, "the reference covers integer sample steps only");
    let horizontal = blur_axis(image, size, &weights, true);
    blur_axis(&horizontal, size, &weights, false)
}
