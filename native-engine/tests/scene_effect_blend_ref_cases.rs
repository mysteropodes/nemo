//! Hand-derived literal cases for the CPU blend / matte / mask reference
//! (L08-B). Every expected value below was worked out on paper from the W3C
//! formulas and the shader text, independently of the oracle generator.

use super::mask::{
    apply_masks, feathered_silhouette, mask_feather, mask_silhouette, MaskCoverage, MaskMode,
};
use super::tests::{close, px_close, GRID};
use super::*;

const T: f32 = 5e-6;

fn opaque(c: [f32; 3]) -> Rgba {
    [c[0], c[1], c[2], 1.0]
}
fn rgb(p: Rgba) -> [f32; 3] {
    [p[0], p[1], p[2]]
}
fn all_close(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len());
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(close(*g, *w, T), "[{k}] {g} vs {w}");
    }
}

#[test]
fn multiply_and_screen_on_grid_pair_one() {
    let (b, s) = (GRID[1][0], GRID[1][1]);
    assert!(px_close(blend_pixel(1, b, s), [0.125, 0.25, 0.375, 1.0], T));
    assert!(px_close(blend_pixel(2, b, s), [0.625, 0.75, 0.875, 1.0], T));
}

#[test]
fn separable_modes_on_dyadic_opaque_channels() {
    // cb = 0.25, cs = 0.75 in all channels, both opaque: the output is B(cb, cs).
    let (b, s) = (opaque([0.25; 3]), opaque([0.75; 3]));
    let want = [
        (0, 0.75),
        (1, 0.1875),
        (2, 0.8125),
        (3, 0.375),
        (4, 0.25),
        (5, 0.75),
        (6, 1.0),
        (7, 0.0),
        (8, 0.625),
        (9, 0.375),
        (10, 0.5),
        (11, 0.625),
    ];
    for (mode, v) in want {
        assert!(
            px_close(blend_pixel(mode, b, s), [v, v, v, 1.0], T),
            "mode {mode}"
        );
    }
}

#[test]
fn color_dodge_and_burn_edge_channels() {
    let (b, s) = (opaque([0.0, 0.5, 1.0]), opaque([1.0, 1.0, 0.0]));
    assert!(
        px_close(blend_pixel(6, b, s), [0.0, 1.0, 1.0, 1.0], T),
        "dodge"
    );
    assert!(
        px_close(blend_pixel(7, b, s), [0.0, 0.5, 1.0, 1.0], T),
        "burn"
    );
}

#[test]
fn alpha_zero_and_partial_alpha_compositing() {
    let cb = [0.2, 0.4, 0.6];
    // Normal, source alpha 0.5 over an opaque backdrop: 0.5*cs + 0.5*cb.
    let got = blend_pixel(0, opaque(cb), [1.0, 0.0, 0.5, 0.5]);
    assert!(px_close(got, [0.6, 0.2, 0.55, 1.0], T));
    // Backdrop alpha 0 (any mode): the source straight colour and alpha.
    for mode in [0, 1, 6, 12, 15] {
        let got = blend_pixel(mode, [0.9, 0.9, 0.9, 0.0], [0.2, 0.4, 0.6, 0.8]);
        assert!(px_close(got, [0.2, 0.4, 0.6, 0.8], T), "mode {mode}");
    }
    // Source alpha 0 (any mode): the backdrop colour and alpha.
    for mode in [0, 1, 6, 12, 15] {
        let got = blend_pixel(mode, [0.2, 0.4, 0.6, 0.5], [0.9, 0.1, 0.3, 0.0]);
        assert!(px_close(got, [0.2, 0.4, 0.6, 0.5], T), "mode {mode}");
    }
    // Both alpha 0: transparent black, never NaN.
    assert_eq!(
        blend_pixel(1, [0.5, 0.5, 0.5, 0.0], [0.4, 0.4, 0.4, 0.0]),
        [0.0, 0.0, 0.0, 0.0]
    );
    // Multiply, cb = cs = 0.5, ab = as = 0.5:
    //   blended .25; cs_mix = .5*.5+.5*.25 = .375; ao = .75;
    //   co = .5*.375 + .5*.5*.5 = .3125; straight = .3125/.75.
    let got = blend_pixel(1, [0.5, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]);
    assert!(px_close(got, [0.4166667, 0.4166667, 0.4166667, 0.75], T));
}

#[test]
fn non_separable_modes_hand_literals() {
    let gray = opaque([0.5; 3]);
    let red = opaque([1.0, 0.0, 0.0]);
    // Color: SetLum(red, 0.5) = (1.2,.2,.2) then clipped on x > 1 to (1, 2/7, 2/7).
    assert!(px_close(
        blend_pixel(14, gray, red),
        [1.0, 0.2857143, 0.2857143, 1.0],
        T
    ));
    // Luminosity: SetLum(gray, lum(red) = 0.3) is a 0.3 gray.
    assert!(px_close(
        blend_pixel(15, gray, red),
        [0.3, 0.3, 0.3, 1.0],
        T
    ));
    // Hue: SetSat(red, sat(gray) = 0) is black, SetLum(black, 0.5) a 0.5 gray.
    assert!(px_close(
        blend_pixel(12, gray, red),
        [0.5, 0.5, 0.5, 1.0],
        T
    ));
    // Saturation: SetSat((.8,.4,.2), sat((1,.5,.5)) = .5) = (.5, 1/6, 0);
    // lum(cb) = .498 so SetLum adds .2496667 and nothing clips.
    let got = blend_pixel(13, opaque([0.8, 0.4, 0.2]), opaque([1.0, 0.5, 0.5]));
    assert!(px_close(got, [0.7496667, 0.4163333, 0.2496667, 1.0], T));
    // Saturation of a gray source is 0, so the result is gray with lum(cb) = .498.
    let got = blend_pixel(13, opaque([0.8, 0.4, 0.2]), gray);
    assert!(px_close(got, [0.498, 0.498, 0.498, 1.0], T));
    // ClipColor, x > 1 branch: SetLum(blue, 0.9) = (.79,.79,1.79) -> (.8876404,.8876404,1).
    assert!(px_close(
        blend_pixel(14, opaque([0.9; 3]), opaque([0.0, 0.0, 1.0])),
        [0.8876404, 0.8876404, 1.0, 1.0],
        T
    ));
    // ClipColor, n < 0 branch: SetLum(red, 0.1) = (.8,-.2,-.2) -> (1/3, 0, 0).
    assert!(px_close(
        blend_pixel(14, opaque([0.1; 3]), red),
        [0.3333333, 0.0, 0.0, 1.0],
        T
    ));
}

#[test]
fn set_sat_handles_all_six_channel_orderings() {
    // Values 0.2 < 0.5 < 0.9 in every arrangement, target saturation 0.6:
    // the largest channel becomes 0.6, the smallest 0, the middle (.5-.2)*.6/.7.
    let perms = [
        [0.2, 0.5, 0.9],
        [0.2, 0.9, 0.5],
        [0.5, 0.2, 0.9],
        [0.5, 0.9, 0.2],
        [0.9, 0.2, 0.5],
        [0.9, 0.5, 0.2],
    ];
    for c in perms {
        let got = set_sat(c, 0.6);
        let want: Vec<f32> = c
            .iter()
            .map(|v| {
                if *v == 0.9 {
                    0.6
                } else if *v == 0.2 {
                    0.0
                } else {
                    0.2571429
                }
            })
            .collect();
        all_close(&got, &want);
    }
    // Degenerate: no spread keeps every channel at 0; a flat top keeps its two high channels.
    assert_eq!(set_sat([0.5, 0.5, 0.5], 0.6), [0.0, 0.0, 0.0]);
    all_close(&set_sat([0.2, 0.2, 0.8], 0.6), &[0.0, 0.0, 0.6]);
}

#[test]
fn colour_modes_preserve_the_documented_luminosity() {
    // Hue, Saturation and Color keep lum(cb); Luminosity takes lum(cs); ClipColor keeps l.
    for pair in [GRID[0], GRID[1], GRID[5], GRID[6]] {
        let (b, s) = (rgb(pair[0]), rgb(pair[1]));
        for mode in [12, 13, 14] {
            let out = blend_rgb(mode, b, s);
            assert!(close(lum(out), lum(b), 1e-5), "mode {mode} {pair:?}");
        }
        assert!(
            close(lum(blend_rgb(15, b, s)), lum(s), 1e-5),
            "luminosity {pair:?}"
        );
    }
}

#[test]
fn separable_modes_commute_with_channel_permutation_and_unknown_modes_are_normal() {
    let (b, s) = (GRID[0][0], GRID[0][1]);
    let (pb, ps) = ([b[2], b[1], b[0], b[3]], [s[2], s[1], s[0], s[3]]);
    for mode in 1..=11u32 {
        let a = blend_pixel(mode, b, s);
        let p = blend_pixel(mode, pb, ps);
        assert!(
            px_close([a[2], a[1], a[0], a[3]], p, 1e-6),
            "mode {mode} is per channel"
        );
    }
    for mode in [16u32, 17, 255, u32::MAX] {
        assert_eq!(blend_pixel(mode, b, s), blend_pixel(0, b, s), "mode {mode}");
    }
    assert_eq!(
        blend_pixel(0, GRID[2][0], GRID[2][1]),
        blend_rgb_normal_expected()
    );
}

fn blend_rgb_normal_expected() -> Rgba {
    // ab = 1, as = .5: .5*cs + .5*cb with cb (.9,.1,.5) and cs (.2,.9,.5).
    let w = |cb: f32, cs: f32| 0.5 * cs + 0.5 * cb;
    [w(0.9, 0.2), w(0.1, 0.9), w(0.5, 0.5), 1.0]
}

#[test]
fn matte_alpha_luma_and_inverted_literals() {
    let layer = [0.8, 0.4, 0.2, 1.0];
    // Alpha matte reads only the matte alpha, never its colour.
    assert!(px_close(
        matte_pixel(layer, [0.0, 0.0, 0.0, 0.5], 0, false),
        [0.8, 0.4, 0.2, 0.5],
        T
    ));
    assert!(px_close(
        matte_pixel(layer, [1.0, 1.0, 1.0, 0.25], 0, true),
        [0.8, 0.4, 0.2, 0.75],
        T
    ));
    // Luma uses Rec. 709 weights times the matte alpha.
    assert!(close(
        matte_pixel(layer, [1.0, 0.0, 0.0, 1.0], 1, false)[3],
        0.2126,
        T
    ));
    assert!(close(
        matte_pixel(layer, [0.0, 1.0, 0.0, 1.0], 1, false)[3],
        0.7152,
        T
    ));
    assert!(close(
        matte_pixel(layer, [0.0, 0.0, 1.0, 0.5], 1, false)[3],
        0.0361,
        T
    ));
    assert!(close(
        matte_pixel(layer, [1.0, 0.0, 0.0, 1.0], 1, true)[3],
        0.7874,
        T
    ));
    // A transparent matte source contributes zero luma whatever colour is under it.
    assert!(close(
        matte_pixel(layer, [1.0, 1.0, 1.0, 0.0], 1, false)[3],
        0.0,
        T
    ));
    assert!(close(
        matte_pixel(layer, [1.0, 1.0, 1.0, 0.0], 1, true)[3],
        1.0,
        T
    ));
    // Layer alpha multiplies: 0.5 * 0.5.
    assert!(close(
        matte_pixel([0.1, 0.2, 0.3, 0.5], [0.0, 0.0, 0.0, 0.5], 0, false)[3],
        0.25,
        T
    ));
    // Any mode other than 0 is luma.
    assert_eq!(
        matte_pixel(layer, [1.0, 0.0, 0.0, 1.0], 7, false),
        matte_pixel(layer, [1.0, 0.0, 0.0, 1.0], 1, false)
    );
}

#[test]
fn matte_chain_narrows_sequentially_and_empty_is_identity() {
    let layer = [0.8, 0.4, 0.2, 1.0];
    assert_eq!(matte_chain(layer, &[]), layer);
    let half = [1.0, 1.0, 1.0, 0.5];
    assert!(px_close(
        matte_chain(layer, &[(half, 0, false), (half, 0, false)]),
        [0.8, 0.4, 0.2, 0.25],
        T
    ));
    // alpha .5 -> inverted alpha .5 -> luma of opaque white = 1: 0.25 stays.
    let white = [1.0, 1.0, 1.0, 1.0];
    assert!(px_close(
        matte_chain([0.8, 0.4, 0.2, 0.5], &[(half, 0, true), (white, 1, false)]),
        [0.8, 0.4, 0.2, 0.25],
        T
    ));
}

#[test]
fn mask_mode_names_are_exact_and_default_to_add() {
    assert_eq!(MaskMode::from_name("add"), MaskMode::Add);
    assert_eq!(MaskMode::from_name("subtract"), MaskMode::Subtract);
    assert_eq!(MaskMode::from_name("intersect"), MaskMode::Intersect);
    for other in ["", "Subtract", "INTERSECT", "lighten", "difference", "frob"] {
        assert_eq!(MaskMode::from_name(other), MaskMode::Add, "{other:?}");
    }
}

fn cov(mode: MaskMode, c: &[f32]) -> MaskCoverage<'_> {
    MaskCoverage { mode, coverage: c }
}

#[test]
fn mask_union_subtract_and_intersect_literals() {
    // Add masks union by src-over: .5 then .5 -> .75; .25 then .5 -> .625.
    let a = [0.5, 0.25, 1.0, 0.0];
    let b = [0.5, 0.5, 0.0, 0.0];
    let got = mask_silhouette(&[cov(MaskMode::Add, &a), cov(MaskMode::Add, &b)], 4).unwrap();
    all_close(&got, &[0.75, 0.625, 1.0, 0.0]);
    // Only subtract / intersect: the silhouette starts fully visible.
    let c = [0.25, 1.0, 0.0, 0.5];
    all_close(
        &mask_silhouette(&[cov(MaskMode::Subtract, &c)], 4).unwrap(),
        &[0.75, 0.0, 1.0, 0.5],
    );
    all_close(
        &mask_silhouette(&[cov(MaskMode::Intersect, &c)], 4).unwrap(),
        &[0.25, 1.0, 0.0, 0.5],
    );
    // Add 1, subtract .25, intersect .5 at the first pixel: 1 * .75 * .5.
    let add = [1.0, 1.0];
    let sub = [0.25, 0.0];
    let int = [0.5, 1.0];
    let masks = [
        cov(MaskMode::Add, &add),
        cov(MaskMode::Subtract, &sub),
        cov(MaskMode::Intersect, &int),
    ];
    all_close(&mask_silhouette(&masks, 2).unwrap(), &[0.375, 1.0]);
    // Subtract and intersect commute; their position among the list does not matter.
    let swapped = [
        cov(MaskMode::Intersect, &int),
        cov(MaskMode::Add, &add),
        cov(MaskMode::Subtract, &sub),
    ];
    all_close(&mask_silhouette(&swapped, 2).unwrap(), &[0.375, 1.0]);
    // An add mask that covers nothing leaves nothing visible (no implicit white).
    let zero = [0.0, 0.0];
    all_close(
        &mask_silhouette(&[cov(MaskMode::Add, &zero)], 2).unwrap(),
        &[0.0, 0.0],
    );
}

#[test]
fn shared_feather_literals() {
    // Constant coverage stays constant: the Gaussian weights are normalised.
    let ones = vec![1.0f32; 36];
    let got = feathered_silhouette(&[cov(MaskMode::Add, &ones)], 6, 6, 10.0).unwrap();
    assert!(got.iter().all(|v| close(*v, 1.0, 1e-5)));
    // Threshold: feather <= 0.01 returns the silhouette bit for bit.
    let step: Vec<f32> = (0..30)
        .map(|i| if i % 10 >= 5 { 1.0 } else { 0.0 })
        .collect();
    assert_eq!(mask_feather(&step, 10, 3, 0.01).unwrap(), step);
    assert_eq!(mask_feather(&step, 10, 3, 0.0).unwrap(), step);
    assert_eq!(mask_feather(&step, 10, 3, -4.0).unwrap(), step);
    // Radius 3 gives sigma 1 at one-texel steps. A unit step edge: the pixel
    // before the edge sums the weights of taps 1..=24 over the total weight
    // 1 + 2 * 0.7533141 = sqrt(2*pi), i.e. 0.30053, and the two pixels on
    // either side of the edge sum to exactly 1 by symmetry.
    let blurred = mask_feather(&step, 10, 3, 3.0).unwrap();
    for y in 0..3 {
        let (left, right) = (blurred[y * 10 + 4], blurred[y * 10 + 5]);
        assert!(close(left, 0.30053, 2e-4), "left {left}");
        assert!(close(left + right, 1.0, 1e-5), "symmetry {}", left + right);
        assert!(
            close(blurred[y * 10], 0.0, 1e-3),
            "far pixels stay clamped to the plateau"
        );
    }
}

#[test]
fn applying_masks_multiplies_layer_alpha_and_keeps_colour() {
    let layer = [[0.1, 0.2, 0.3, 0.8], [0.4, 0.5, 0.6, 1.0]];
    let c = [0.5, 0.0];
    let got = apply_masks(&layer, 2, 1, &[cov(MaskMode::Add, &c)], 0.0).unwrap();
    assert!(px_close(got[0], [0.1, 0.2, 0.3, 0.4], T));
    assert!(px_close(got[1], [0.4, 0.5, 0.6, 0.0], T));
    // No masks at all: the layer is returned untouched, even with a feather set.
    assert_eq!(
        apply_masks(&layer, 2, 1, &[], 25.0).unwrap(),
        layer.to_vec()
    );
}
