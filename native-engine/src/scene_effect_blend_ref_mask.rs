//! CPU reference for the per-layer vector-mask combine and its single shared
//! feather (`composite_scene` in `geometry-wasm/src/engine.rs` at fbe4b8a3,
//! documented on `LayerIn::masks` and `LayerIn::mask_feather`).
//!
//! Inputs are per-pixel COVERAGE (0..=1 alpha of each mask's rendered white
//! silhouette), so no geometry is rasterised here. Semantics ported:
//! every `add` mask is unioned by src-over (anything not `subtract` or
//! `intersect` is an add); with no add mask the silhouette starts fully
//! visible; `subtract` and `intersect` then narrow it one mask at a time
//! through the matte alpha math (invert for subtract); ONE feather, applied to
//! the final combined silhouette only, is `blur.wgsl`'s separable Gaussian and
//! runs only above 0.01; the silhouette finally multiplies the layer alpha.

use crate::object_render_scene::scene_effect_kernels::scene_effect_blend_ref::{matte_pixel, Rgba};

/// Mask mode vocabulary of `MaskIn::mode`; the wire default is `add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskMode {
    Add,
    Subtract,
    Intersect,
}

impl MaskMode {
    /// `subtract` and `intersect` are exact, case-sensitive names; any other
    /// text (including the default `add`) combines as a union member.
    pub(crate) fn from_name(name: &str) -> MaskMode {
        match name {
            "subtract" => MaskMode::Subtract,
            "intersect" => MaskMode::Intersect,
            _ => MaskMode::Add,
        }
    }
}

/// One mask: its mode and the coverage of its silhouette at every pixel.
pub(crate) struct MaskCoverage<'a> {
    pub(crate) mode: MaskMode,
    pub(crate) coverage: &'a [f32],
}

/// Feather values at or below this threshold skip the blur entirely.
const FEATHER_THRESHOLD: f32 = 0.01;
/// Taps on each side of the centre in `blur.wgsl` (`KERNEL_HALF`).
const KERNEL_HALF: i32 = 24;

/// Combined silhouette alpha before feather. `None` when a coverage list does
/// not have exactly `len` entries.
pub(crate) fn mask_silhouette(masks: &[MaskCoverage<'_>], len: usize) -> Option<Vec<f32>> {
    if masks.iter().any(|m| m.coverage.len() != len) {
        return None;
    }
    let adds: Vec<&MaskCoverage<'_>> = masks.iter().filter(|m| m.mode == MaskMode::Add).collect();
    let mut acc = vec![1.0f32; len];
    if !adds.is_empty() {
        for (p, slot) in acc.iter_mut().enumerate() {
            let mut a = 0.0f32;
            for m in &adds {
                a += m.coverage[p] * (1.0 - a);
            }
            *slot = a;
        }
    }
    for m in masks.iter().filter(|m| m.mode != MaskMode::Add) {
        let invert = m.mode == MaskMode::Subtract;
        for (p, slot) in acc.iter_mut().enumerate() {
            *slot = matte_pixel(
                [1.0, 1.0, 1.0, *slot],
                [1.0, 1.0, 1.0, m.coverage[p]],
                0,
                invert,
            )[3];
        }
    }
    Some(acc)
}

/// One separable blur direction of `blur.wgsl` over an alpha-only image:
/// bilinear taps at `i * step` texels, clamp-to-edge, Gaussian weights.
/// Accumulates in `f64`; the GPU path is `f32` with hardware filtering.
fn blur_1d(src: &[f32], w: usize, h: usize, dx: f64, dy: f64, radius: f64) -> Vec<f32> {
    let step = f64::max(radius / 24.0, 1.0);
    let sigma = f64::max(radius / 3.0 / step, 0.0001);
    let two_sigma2 = 2.0 * sigma * sigma;
    let at = |x: i64, y: i64| -> f64 {
        let xc = x.clamp(0, w as i64 - 1) as usize;
        let yc = y.clamp(0, h as i64 - 1) as usize;
        src[yc * w + xc] as f64
    };
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut alpha_sum = 0.0f64;
            let mut wsum = 0.0f64;
            for i in -KERNEL_HALF..=KERNEL_HALF {
                let off = i as f64;
                let weight = (-(off * off) / two_sigma2).exp();
                let px = x as f64 + dx * off * step;
                let py = y as f64 + dy * off * step;
                let (x0, y0) = (px.floor(), py.floor());
                let (fx, fy) = (px - x0, py - y0);
                let (x0, y0) = (x0 as i64, y0 as i64);
                let a = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
                    + at(x0 + 1, y0) * fx * (1.0 - fy)
                    + at(x0, y0 + 1) * (1.0 - fx) * fy
                    + at(x0 + 1, y0 + 1) * fx * fy;
                alpha_sum += a * weight;
                wsum += weight;
            }
            out[y * w + x] = (alpha_sum / f64::max(wsum, 0.0001)) as f32;
        }
    }
    out
}

/// The single shared feather on a combined silhouette: unchanged at or below
/// 0.01, otherwise horizontal then vertical Gaussian. `None` on a size mismatch.
pub(crate) fn mask_feather(sil: &[f32], w: usize, h: usize, feather: f32) -> Option<Vec<f32>> {
    if sil.len() != w * h {
        return None;
    }
    if !(feather > FEATHER_THRESHOLD) || w == 0 || h == 0 {
        return Some(sil.to_vec());
    }
    let radius = feather as f64;
    let horizontal = blur_1d(sil, w, h, 1.0, 0.0, radius);
    Some(blur_1d(&horizontal, w, h, 0.0, 1.0, radius))
}

/// Silhouette after the shared feather, as it multiplies the layer.
pub(crate) fn feathered_silhouette(
    masks: &[MaskCoverage<'_>],
    w: usize,
    h: usize,
    feather: f32,
) -> Option<Vec<f32>> {
    mask_feather(&mask_silhouette(masks, w * h)?, w, h, feather)
}

/// Apply a layer's masks: no masks leaves the layer untouched; otherwise the
/// feathered silhouette multiplies each pixel's alpha (colour unchanged).
pub(crate) fn apply_masks(
    layer: &[Rgba],
    w: usize,
    h: usize,
    masks: &[MaskCoverage<'_>],
    feather: f32,
) -> Option<Vec<Rgba>> {
    if layer.len() != w * h {
        return None;
    }
    if masks.is_empty() {
        return Some(layer.to_vec());
    }
    let sil = feathered_silhouette(masks, w, h, feather)?;
    Some(
        layer
            .iter()
            .zip(&sil)
            .map(|(px, s)| matte_pixel(*px, [1.0, 1.0, 1.0, *s], 0, false))
            .collect(),
    )
}
