//! Independent CPU reference of `simple_fx.wgsl`, written in Rust straight from
//! the shader text (not from the Node generator): the fourteen built-in effects
//! and the hqBloom extract, one fullscreen pass, straight RGBA8 in and out.
//!
//! It runs without a GPU, so the oracle's Node transliteration is checked
//! against a second implementation on the lead's CPU run, and the GPU tests of
//! the appointment compare the real shader with the same oracle. Textures are
//! sampled like the baseline sampler: bilinear, clamp to edge, pixel centres at
//! +0.5. Every value is `f32`; the pass quantises to RGBA8 round-to-nearest.

fn unit(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

fn quantise(value: f32) -> u8 {
    (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// WGSL `fract`: `x - floor(x)` (never negative, unlike `f32::fract`).
fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hash(uv: [f32; 2]) -> f32 {
    fract((uv[0] * 12.9898 + uv[1] * 78.233).sin() * 43758.5453)
}

struct Texture<'a> {
    bytes: &'a [u8],
    w: u32,
    h: u32,
}

impl Texture<'_> {
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let x = x.clamp(0, i64::from(self.w) - 1) as usize;
        let y = y.clamp(0, i64::from(self.h) - 1) as usize;
        let at = &self.bytes[(y * self.w as usize + x) * 4..][..4];
        [unit(at[0]), unit(at[1]), unit(at[2]), unit(at[3])]
    }

    /// `textureSample` at a normalised coordinate: linear filter, clamp to edge.
    fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        let sx = uv[0] * self.w as f32 - 0.5;
        let sy = uv[1] * self.h as f32 - 0.5;
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b) = (self.texel(x0, y0), self.texel(x0 + 1, y0));
        let (c, d) = (self.texel(x0, y0 + 1), self.texel(x0 + 1, y0 + 1));
        let mut out = [0.0f32; 4];
        for k in 0..4 {
            out[k] =
                (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
        }
        out
    }
}

/// One `simple_fx.wgsl` pass over `image` (`w` x `h`): `effect_id`, `p1..p4`
/// as in the uniform, `time` in seconds. `bbox` is not read by any built-in.
pub(super) fn simple_fx(
    image: &[u8],
    (w, h): (u32, u32),
    effect_id: f32,
    p: [f32; 4],
    time: f32,
) -> Vec<u8> {
    let tex = Texture { bytes: image, w, h };
    let (fw, fh) = (w as f32, h as f32);
    let texel = [1.0 / fw.max(1.0), 1.0 / fh.max(1.0)];
    let id = effect_id.round() as i32;
    let mut out = Vec::with_capacity(image.len());
    for y in 0..h {
        for x in 0..w {
            let uv = [(x as f32 + 0.5) / fw, (y as f32 + 0.5) / fh];
            let px = fragment(&tex, id, p, time, uv, texel);
            out.extend(px.map(quantise));
        }
    }
    out
}

fn fragment(
    tex: &Texture,
    id: i32,
    p: [f32; 4],
    time: f32,
    uv: [f32; 2],
    texel: [f32; 2],
) -> [f32; 4] {
    let (fw, fh) = (tex.w as f32, tex.h as f32);
    let [p1, p2, p3, p4] = p;
    match id {
        4 => {
            let block = p1.max(1.0);
            let grid = [fw / block, fh / block];
            let snapped = [
                (uv[0] * grid[0]).floor() / grid[0],
                (uv[1] * grid[1]).floor() / grid[1],
            ];
            tex.sample(snapped)
        }
        5 => {
            let dir = [uv[0] - 0.5, uv[1] - 0.5];
            let amt = [p1 * texel[0], p1 * texel[1]];
            let off = [dir[0] * amt[0] * 2.0, dir[1] * amt[1] * 2.0];
            let r = tex.sample([uv[0] + off[0], uv[1] + off[1]])[0];
            let b = tex.sample([uv[0] - off[0], uv[1] - off[1]])[2];
            let here = tex.sample(uv);
            [r, here[1], b, here[3]]
        }
        14 => {
            let src = tex.sample(uv);
            let mask = smoothstep(p1, (p1 + 0.35).min(1.0), luma([src[0], src[1], src[2]]));
            let warmth = (p4 * 0.5 + 0.5).clamp(0.0, 1.0);
            let cool = [0.65, 0.78, 1.0];
            let warm = [1.0, 0.78, 0.45];
            let gain = p3.max(0.0);
            let mut out = [0.0, 0.0, 0.0, src[3] * mask];
            for k in 0..3 {
                let tint = cool[k] * (1.0 - warmth) + warm[k] * warmth;
                out[k] = src[k] * src[3] * mask * gain * tint;
            }
            out
        }
        8 => {
            let c = tex.sample(uv);
            let n = tex.sample([uv[0], uv[1] - texel[1]]);
            let s = tex.sample([uv[0], uv[1] + texel[1]]);
            let w = tex.sample([uv[0] - texel[0], uv[1]]);
            let e = tex.sample([uv[0] + texel[0], uv[1]]);
            let mut out = [0.0, 0.0, 0.0, c[3]];
            for k in 0..3 {
                let ring = (n[k] + s[k] + w[k] + e[k]) * p1;
                out[k] = (c[k] * (1.0 + 4.0 * p1) - ring).clamp(0.0, 1.0);
            }
            out
        }
        9 => {
            let mut g = [0.0f32; 9];
            for (k, slot) in g.iter_mut().enumerate() {
                let (dx, dy) = ((k % 3) as f32 - 1.0, (k / 3) as f32 - 1.0);
                let s = tex.sample([uv[0] + dx * texel[0], uv[1] + dy * texel[1]]);
                *slot = luma([s[0], s[1], s[2]]);
            }
            let gx = (g[2] + 2.0 * g[5] + g[8]) - (g[0] + 2.0 * g[3] + g[6]);
            let gy = (g[6] + 2.0 * g[7] + g[8]) - (g[0] + 2.0 * g[1] + g[2]);
            let mag = ((gx * gx + gy * gy).sqrt() * p1).clamp(0.0, 1.0);
            [mag, mag, mag, tex.sample(uv)[3]]
        }
        10 => ground_shadow(tex, p, uv, texel),
        11 => {
            let src = tex.sample(uv);
            let thickness = p1.max(0.5);
            let roughness = p2.clamp(0.0, 1.0);
            let (mut min_a, mut max_a) = (1.0f32, 0.0f32);
            for i in 0..12 {
                let ang = i as f32 / 12.0 * 6.28318;
                let jitter = 1.0
                    + (hash([uv[0] * (i + 1) as f32 + time, uv[1] * (i + 1) as f32 + time]) - 0.5)
                        * roughness;
                let offs = [
                    ang.cos() * thickness * jitter * texel[0],
                    ang.sin() * thickness * jitter * texel[1],
                ];
                let s = tex.sample([uv[0] + offs[0], uv[1] + offs[1]]);
                min_a = min_a.min(s[3]);
                max_a = max_a.max(s[3]);
            }
            let edge = ((max_a - min_a) * 2.0).clamp(0.0, 1.0);
            let tint = p3.clamp(0.0, 1.0);
            let outline = edge * p4.clamp(0.0, 1.0);
            let mut out = [0.0, 0.0, 0.0, src[3].max(outline)];
            for k in 0..3 {
                out[k] = src[k] * (1.0 - outline) + tint * outline;
            }
            out
        }
        13 => {
            let cell = p1.max(2.0);
            let px = [uv[0] * fw, uv[1] * fh];
            let idx = [(px[0] / cell).floor(), (px[1] / cell).floor()];
            let pos = [
                (px[0] - idx[0] * cell) / cell - 0.5,
                (px[1] - idx[1] * cell) / cell - 0.5,
            ];
            let centre = [(idx[0] + 0.5) * cell / fw, (idx[1] + 0.5) * cell / fh];
            let s = tex.sample(centre);
            let radius = (1.0 - luma([s[0], s[1], s[2]])) * 0.5 * p2.clamp(0.0, 1.0);
            let in_dot = (pos[0] * pos[0] + pos[1] * pos[1]).sqrt() < radius;
            let value = if in_dot { 0.0 } else { 1.0 };
            [value, value, value, s[3]]
        }
        _ => {
            let src = tex.sample(uv);
            let rgb = remap(id, p, time, uv, (fw, fh), [src[0], src[1], src[2]]);
            [rgb[0], rgb[1], rgb[2], src[3]]
        }
    }
}

/// The plain per-pixel remaps (threshold, sepia, invert, grayscale, posterize,
/// scanlines, grain); any other id returns the sample unchanged.
fn remap(
    id: i32,
    p: [f32; 4],
    time: f32,
    uv: [f32; 2],
    (fw, fh): (f32, f32),
    rgb: [f32; 3],
) -> [f32; 3] {
    let [p1, p2, ..] = p;
    match id {
        12 => {
            let threshold = p1.clamp(0.0, 1.0);
            let soft = p2.max(0.0001);
            let v = smoothstep(threshold - soft, threshold + soft, luma(rgb));
            [v, v, v]
        }
        0 => {
            let rows = [
                [0.393, 0.769, 0.189],
                [0.349, 0.686, 0.168],
                [0.272, 0.534, 0.131],
            ];
            rows.map(|row| (rgb[0] * row[0] + rgb[1] * row[1] + rgb[2] * row[2]).clamp(0.0, 1.0))
        }
        1 => rgb.map(|v| 1.0 - v),
        2 => [luma(rgb); 3],
        3 => {
            let levels = p1.max(2.0);
            rgb.map(|v| (v * levels).floor() / levels)
        }
        6 => {
            let freq = p1.max(1.0);
            let darkness = p2.clamp(0.0, 1.0);
            let wave = (uv[1] * fh * freq * 3.14159).sin();
            let dim = 1.0 - darkness * (0.5 + 0.5 * wave);
            rgb.map(|v| v * dim)
        }
        7 => {
            let n = hash([uv[0] * fw + time, uv[1] * fh + time]);
            rgb.map(|v| (v + (n - 0.5) * p1).clamp(0.0, 1.0))
        }
        _ => rgb,
    }
}

fn ground_shadow(tex: &Texture, p: [f32; 4], uv: [f32; 2], texel: [f32; 2]) -> [f32; 4] {
    let (fw, fh) = (tex.w as f32, tex.h as f32);
    let [skew, ground, length, opacity] = p;
    let src = tex.sample(uv);
    let ground_y = ground * fh;
    let y = uv[1] * fh;
    let height = ((y - ground_y) / length.max(0.0001)).max(0.0);
    let sy = ground_y - height;
    let sx = uv[0] * fw - height * skew;
    let in_bounds = sy >= 0.0 && sy < fh && sx >= 0.0 && sx < fw;
    let src_uv = [sx / fw, sy / fh];
    let blur_texels = (height * 0.085 + 0.75).clamp(0.75, 14.0);
    let (mut a_sum, mut w_sum) = (0.0f32, 0.0f32);
    for i in 0..9 {
        let (ox, oy) = ((i % 3) as f32 - 1.0, (i / 3) as f32 - 1.0);
        let weight = if (ox * ox + oy * oy).sqrt() > 0.9 {
            1.0
        } else {
            2.0
        };
        let tap = [
            src_uv[0] + ox * texel[0] * blur_texels,
            src_uv[1] + oy * texel[1] * blur_texels,
        ];
        a_sum += tex.sample(tap)[3] * weight;
        w_sum += weight;
    }
    let valid = y > ground_y && length > 0.0001 && in_bounds;
    let fade = 1.0 - smoothstep(0.72, 1.0, height / ground_y.max(1.0));
    let shadow_a = if valid {
        (a_sum / w_sum.max(0.001)) * opacity.clamp(0.0, 1.0) * fade
    } else {
        0.0
    };
    if src[3] > 0.01 {
        src
    } else {
        [0.008, 0.01, 0.016, shadow_a]
    }
}

#[cfg(test)]
mod checks {
    use super::super::support::{oracle, worst_mismatch};
    use super::simple_fx;
    use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectPass;

    fn one(texel: [u8; 4], id: f32, p: [f32; 4]) -> [u8; 4] {
        let out = simple_fx(&texel, (1, 1), id, p, 0.0);
        [out[0], out[1], out[2], out[3]]
    }

    #[test]
    fn the_reference_matches_the_oracle_on_every_single_pass_fx_case() {
        // The planner supplies id, defaults and zoom scaling; the reference runs the shader
        // arithmetic; the oracle comes from the Node model. Three independent paths agree.
        let oracle = oracle();
        let mut checked = 0;
        for case in oracle.cases.iter().filter(|c| c.name.starts_with("fx-")) {
            let plan = case.plan();
            assert_eq!(plan.steps.len(), 1, "{}", case.name);
            let EffectPass::SimpleFx(fx) = &plan.steps[0].pass else {
                panic!("{} is not a simple_fx pass", case.name);
            };
            let image = oracle.image(&case.image);
            let p = [fx.params[0], fx.params[1], fx.params[2], fx.params[3]];
            let out = simple_fx(&image.bytes, (image.w, image.h), fx.effect_id, p, case.time);
            if let Some((pixel, channel, delta)) = worst_mismatch(&out, &case.expect, case.tol) {
                panic!(
                    "{}: reference pixel {pixel} channel {channel} off by {delta} (tolerance {})",
                    case.name, case.tol
                );
            }
            checked += 1;
        }
        assert_eq!(checked, 21);
    }

    #[test]
    fn the_reference_changes_pixels_where_the_oracle_says_it_does() {
        let oracle = oracle();
        for name in [
            "fx-sepia-ramp",
            "fx-halftone-cell-4",
            "fx-edge-detect-shape",
        ] {
            let case = oracle.case(name);
            assert_ne!(
                case.expect,
                oracle.image(&case.image).bytes,
                "{name} is not the identity"
            );
        }
        let grain = oracle.case("fx-grain-zero-amplitude-is-the-identity");
        assert_eq!(grain.expect, oracle.image(&grain.image).bytes);
    }

    #[test]
    fn hand_derived_pixels_of_the_per_pixel_effects() {
        let none = [0.0; 4];
        // sepia of white: (1.351, 1.203, 0.937) clamped -> (255, 255, 0.937 * 255 = 238.9 -> 239).
        assert_eq!(one([255, 255, 255, 255], 0.0, none), [255, 255, 239, 255]);
        // invert keeps alpha.
        assert_eq!(one([10, 20, 30, 40], 1.0, none), [245, 235, 225, 40]);
        // grayscale of pure red: 0.299 * 255 = 76.2 -> 76.
        assert_eq!(one([255, 0, 0, 255], 2.0, none), [76, 76, 76, 255]);
        // posterize to 4 levels: 200/255*4 = 3.14 -> 3 -> 0.75 -> 191; 128/255*4 = 2.008 -> 2 -> 0.5 -> 128.
        assert_eq!(
            one([200, 128, 255, 255], 3.0, [4.0, 0.0, 0.0, 0.0]),
            [191, 128, 255, 255]
        );
        // threshold 0.5 +- 0.08: white is above, black below.
        let cut = [0.5, 0.08, 0.0, 0.0];
        assert_eq!(one([255, 255, 255, 255], 12.0, cut), [255, 255, 255, 255]);
        assert_eq!(one([0, 0, 0, 255], 12.0, cut), [0, 0, 0, 255]);
        // An id the shader has no branch for returns the sample unchanged.
        assert_eq!(one([1, 2, 3, 4], 15.0, none), [1, 2, 3, 4]);
    }

    #[test]
    fn hand_derived_pixels_of_the_bloom_extract() {
        let id = 14.0;
        // White, threshold 0, intensity 1, coolest tint: mask 1, rgb = tint (0.65, 0.78, 1.0).
        assert_eq!(
            one([255, 255, 255, 255], id, [0.0, 0.0, 1.0, -1.0]),
            [166, 199, 255, 255]
        );
        // Warmest tint (1.0, 0.78, 0.45).
        assert_eq!(
            one([255, 255, 255, 255], id, [0.0, 0.0, 1.0, 1.0]),
            [255, 199, 115, 255]
        );
        // Black has no luma above the threshold: nothing, and no alpha.
        assert_eq!(one([0, 0, 0, 255], id, [0.0, 0.0, 1.0, 0.0]), [0, 0, 0, 0]);
        // Intensity 0 keeps the mask (alpha) but no colour.
        assert_eq!(
            one([255, 255, 255, 255], id, [0.0, 0.0, 0.0, 0.0]),
            [0, 0, 0, 255]
        );
        // A negative intensity is floored at zero.
        assert_eq!(
            one([255, 255, 255, 255], id, [0.0, 0.0, -2.0, 0.0]),
            [0, 0, 0, 255]
        );
    }
}
