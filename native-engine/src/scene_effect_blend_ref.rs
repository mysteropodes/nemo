//! CPU reference pixels for the native scene compositor's blend and track
//! matte passes.
//!
//! This is an independent oracle for GPU leaves, never a second renderer: it
//! has no texture, device, scheduler or document, and no production call
//! site. Every function is a line-by-line port of one retained baseline
//! shader (`geometry-wasm/src/blend.wgsl`, `matte.wgsl` at fbe4b8a3), in `f32`
//! with the same operation order, over STRAIGHT (non-premultiplied) RGBA like
//! the baseline textures. The mask combine and its single shared feather live
//! in the `mask` child. Quantisation to Rgba8 between passes is not modelled:
//! a GPU consumer compares against this reference within its documented
//! 1/255 channel tolerance.

#[path = "scene_effect_blend_ref_mask.rs"]
pub(crate) mod mask;

/// Straight-alpha pixel: red, green, blue, alpha, each nominally in 0..=1.
pub(crate) type Rgba = [f32; 4];
type Rgb = [f32; 3];

fn multiply(cb: f32, cs: f32) -> f32 {
    cb * cs
}
fn screen(cb: f32, cs: f32) -> f32 {
    cb + cs - cb * cs
}
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return multiply(cb, 2.0 * cs);
    }
    screen(cb, 2.0 * cs - 1.0)
}
/// W3C defines Overlay(Cb, Cs) as HardLight(Cs, Cb).
fn overlay(cb: f32, cs: f32) -> f32 {
    hard_light(cs, cb)
}
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 {
        return 0.0;
    }
    if cs >= 1.0 {
        return 1.0;
    }
    f32::min(1.0, cb / (1.0 - cs))
}
fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 {
        return 1.0;
    }
    if cs <= 0.0 {
        return 0.0;
    }
    1.0 - f32::min(1.0, (1.0 - cb) / cs)
}
fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    let d = if cb <= 0.25 {
        ((16.0 * cb - 12.0) * cb + 4.0) * cb
    } else {
        cb.sqrt()
    };
    cb + (2.0 * cs - 1.0) * (d - cb)
}
fn exclusion(cb: f32, cs: f32) -> f32 {
    cb + cs - 2.0 * cb * cs
}

/// Per-channel formula selected by `switch params.mode` cases 1..=11.
fn separable(mode: u32, cb: f32, cs: f32) -> f32 {
    match mode {
        1 => multiply(cb, cs),
        2 => screen(cb, cs),
        3 => overlay(cb, cs),
        4 => f32::min(cb, cs),
        5 => f32::max(cb, cs),
        6 => color_dodge(cb, cs),
        7 => color_burn(cb, cs),
        8 => hard_light(cb, cs),
        9 => soft_light(cb, cs),
        10 => (cb - cs).abs(),
        _ => exclusion(cb, cs),
    }
}

/// Rec. 601-style weights of blend.wgsl `lum`, summed left to right.
fn lum(c: Rgb) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}
fn clip_color(c_in: Rgb) -> Rgb {
    let mut c = c_in;
    let l = lum(c);
    let n = f32::min(c[0], f32::min(c[1], c[2]));
    let x = f32::max(c[0], f32::max(c[1], c[2]));
    if n < 0.0 {
        let s = l / f32::max(l - n, 1e-6);
        c = [l + (c[0] - l) * s, l + (c[1] - l) * s, l + (c[2] - l) * s];
    }
    if x > 1.0 {
        let s = (1.0 - l) / f32::max(x - l, 1e-6);
        c = [l + (c[0] - l) * s, l + (c[1] - l) * s, l + (c[2] - l) * s];
    }
    c
}
fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: Rgb) -> f32 {
    f32::max(c[0], f32::max(c[1], c[2])) - f32::min(c[0], f32::min(c[1], c[2]))
}
/// W3C SetSat, walking the same six channel orderings as blend.wgsl.
fn set_sat(c: Rgb, s: f32) -> Rgb {
    let (mut r, mut g, mut b) = (c[0], c[1], c[2]);
    if r <= g {
        if g <= b {
            if b > r {
                g = (g - r) * s / (b - r);
                b = s;
            } else {
                g = 0.0;
                b = 0.0;
            }
            r = 0.0;
        } else if r <= b {
            if g > r {
                b = (b - r) * s / (g - r);
                g = s;
            } else {
                b = 0.0;
                g = 0.0;
            }
            r = 0.0;
        } else {
            if g > b {
                r = (r - b) * s / (g - b);
                g = s;
            } else {
                r = 0.0;
                g = 0.0;
            }
            b = 0.0;
        }
    } else if r <= b {
        if b > g {
            r = (r - g) * s / (b - g);
            b = s;
        } else {
            r = 0.0;
            b = 0.0;
        }
        g = 0.0;
    } else if g <= b {
        if r > g {
            b = (b - g) * s / (r - g);
            r = s;
        } else {
            b = 0.0;
            r = 0.0;
        }
        g = 0.0;
    } else {
        if r > b {
            g = (g - b) * s / (r - b);
            r = s;
        } else {
            g = 0.0;
            r = 0.0;
        }
        b = 0.0;
    }
    [r, g, b]
}

/// `B(Cb, Cs)` of `blend.wgsl` for mode indices 0..=15. Mode 0 and every
/// unrecognised index take the shader `default` arm: the plain source colour.
pub(crate) fn blend_rgb(mode: u32, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
    match mode {
        1..=11 => [
            separable(mode, cb[0], cs[0]),
            separable(mode, cb[1], cs[1]),
            separable(mode, cb[2], cs[2]),
        ],
        12 => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        13 => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        14 => set_lum(cs, lum(cb)),
        15 => set_lum(cb, lum(cs)),
        _ => cs,
    }
}

/// The compositing tail of `fs_main`: W3C generalized formula with Porter-Duff
/// SrcOver, `blended` already mixed by the caller, divided back to straight
/// colour. `ao <= 1e-6` yields transparent black.
pub(crate) fn composite_tail(backdrop: Rgba, source: Rgba, blended: [f32; 3]) -> Rgba {
    let ab = backdrop[3];
    let as_ = source[3];
    let ao = as_ + ab * (1.0 - as_);
    let mut out = [0.0, 0.0, 0.0, ao];
    for k in 0..3 {
        let cs_mix = (1.0 - ab) * source[k] + ab * blended[k];
        let co = as_ * cs_mix + ab * (1.0 - as_) * backdrop[k];
        out[k] = if ao > 1e-6 {
            co / f32::max(ao, 1e-6)
        } else {
            0.0
        };
    }
    out
}

/// One pixel of the `blend.wgsl` pass: `source` over `backdrop` with mix
/// `mode` (the `u32` uniform, 0..=15 in the baseline vocabulary).
pub(crate) fn blend_pixel(mode: u32, backdrop: Rgba, source: Rgba) -> Rgba {
    let cb = [backdrop[0], backdrop[1], backdrop[2]];
    let cs = [source[0], source[1], source[2]];
    composite_tail(backdrop, source, blend_rgb(mode, cb, cs))
}

/// One pixel of the `matte.wgsl` pass: the layer alpha is multiplied by the
/// matte alpha (`mode` 0) or by its Rec. 709 luma weighted by its own alpha
/// (any other `mode`), optionally inverted; the layer colour is untouched.
pub(crate) fn matte_pixel(layer: Rgba, matte: Rgba, mode: u32, invert: bool) -> Rgba {
    let mut m = if mode == 0 {
        matte[3]
    } else {
        (matte[0] * 0.2126 + matte[1] * 0.7152 + matte[2] * 0.0722) * matte[3]
    };
    if invert {
        m = 1.0 - m;
    }
    [layer[0], layer[1], layer[2], layer[3] * m]
}

/// Multi-matte: each `(matte, mode, invert)` narrows what the previous one
/// left, in order, as the ping-pong loop of `composite_scene` does.
pub(crate) fn matte_chain(layer: Rgba, mattes: &[(Rgba, u32, bool)]) -> Rgba {
    mattes.iter().fold(layer, |cur, (matte, mode, invert)| {
        matte_pixel(cur, *matte, *mode, *invert)
    })
}

/// Whole-grid form of [`blend_pixel`]; `None` when the grids differ in size.
pub(crate) fn blend_grid(mode: u32, backdrop: &[Rgba], source: &[Rgba]) -> Option<Vec<Rgba>> {
    if backdrop.len() != source.len() {
        return None;
    }
    Some(
        backdrop
            .iter()
            .zip(source)
            .map(|(b, s)| blend_pixel(mode, *b, *s))
            .collect(),
    )
}

#[cfg(test)]
#[path = "../tests/scene_effect_blend_ref.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/scene_effect_blend_ref_cases.rs"]
mod cases;

#[cfg(test)]
#[path = "../tests/scene_effect_blend_ref_controls.rs"]
mod controls;
