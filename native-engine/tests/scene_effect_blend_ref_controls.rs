//! Mutation, negative and guard controls for the CPU blend / matte / mask
//! reference (L08-B).
//!
//! Each mutant below is a deliberately wrong variant of one piece of the
//! reference (a swapped mode index, swapped multiply/screen roles, a flipped
//! invert flag, the wrong premultiply order, a swapped mask mode, a per-mask
//! feather...). It is run against the SAME oracle literals as the real
//! reference and must disagree with a named, literal subset of them; the
//! unmutated harness variant must agree with all of them, so a mismatch count
//! can only come from the mutation. The expected failing sets were evaluated
//! over the oracle rows by `tools/verify_controls_claims.js` before being
//! written here.

use super::mask::{mask_feather, MaskMode};
use super::tests::{close, mask_rows, px, px_close, rows, MaskRow, GRID, TOL};
use super::*;

/// Modes (first-seen order) whose oracle rows `f` fails to reproduce.
fn blend_failing(f: impl Fn(u32, Rgba, Rgba) -> Rgba) -> Vec<u32> {
    let mut modes: Vec<u32> = Vec::new();
    for r in rows("blend") {
        let mode = r[1].parse::<u32>().unwrap();
        let got = f(mode, px(&r[3..7]), px(&r[7..11]));
        if !px_close(got, px(&r[11..15]), TOL) && !modes.contains(&mode) {
            modes.push(mode);
        }
    }
    modes
}

/// `(mode, invert)` combinations (first-seen order) that `f` fails on.
fn matte_failing(f: impl Fn(Rgba, Rgba, u32, bool) -> Rgba) -> Vec<(u32, bool)> {
    let mut combos: Vec<(u32, bool)> = Vec::new();
    for r in rows("matte") {
        let (mode, invert) = (r[1].parse::<u32>().unwrap(), r[2] == "1");
        let got = f(px(&r[4..8]), px(&r[8..12]), mode, invert);
        if !px_close(got, px(&r[12..16]), TOL) && !combos.contains(&(mode, invert)) {
            combos.push((mode, invert));
        }
    }
    combos
}

/// Chain ids that `f` fails to reproduce.
fn chain_failing(f: impl Fn(Rgba, &[(Rgba, u32, bool)]) -> Rgba) -> Vec<&'static str> {
    let mut ids = Vec::new();
    for r in rows("chain") {
        let n = r[6].parse::<usize>().unwrap();
        let mattes: Vec<(Rgba, u32, bool)> = (0..n)
            .map(|k| {
                let at = 7 + k * 6;
                (
                    px(&r[at..at + 4]),
                    r[at + 4].parse::<u32>().unwrap(),
                    r[at + 5] == "1",
                )
            })
            .collect();
        let end = 7 + n * 6;
        if !px_close(f(px(&r[2..6]), &mattes), px(&r[end..end + 4]), TOL) {
            ids.push(r[1]);
        }
    }
    ids
}

#[test]
fn unmutated_harness_variants_reproduce_every_oracle_literal() {
    assert!(blend_failing(blend_pixel).is_empty());
    assert!(blend_failing(|m, b, s| tail_mutant(TailMut::Faithful, m, b, s)).is_empty());
    assert!(matte_failing(matte_pixel).is_empty());
    assert!(chain_failing(matte_chain).is_empty());
    assert!(mask_failing(MaskMut::None).is_empty());
}

#[test]
fn oracle_tolerance_is_far_tighter_than_one_rgba8_step() {
    assert!(TOL * 255.0 < 1e-3, "TOL must stay far below 1/255");
}

#[test]
fn every_pair_of_blend_modes_is_distinguishable_on_the_oracle_grid() {
    let blend = rows("blend");
    for a in 0..16u32 {
        for b in (a + 1)..16u32 {
            let ra: Vec<&Vec<&str>> = blend
                .iter()
                .filter(|r| r[1].parse::<u32>().unwrap() == a)
                .collect();
            let rb: Vec<&Vec<&str>> = blend
                .iter()
                .filter(|r| r[1].parse::<u32>().unwrap() == b)
                .collect();
            assert_eq!(ra.len(), rb.len());
            let differ = ra
                .iter()
                .zip(&rb)
                .any(|(x, y)| !px_close(px(&x[11..15]), px(&y[11..15]), TOL));
            assert!(differ, "modes {a} and {b} give identical oracle literals");
        }
    }
}

#[test]
fn swapped_mode_indices_are_detected_on_both_sides_of_each_swap() {
    // Multiply/screen, overlay/hard light (argument order), darken/lighten, dodge/burn,
    // difference/exclusion, hue/saturation, color/luminosity, normal/multiply.
    for (a, b) in [
        (1u32, 2u32),
        (3, 8),
        (4, 5),
        (6, 7),
        (10, 11),
        (12, 13),
        (14, 15),
        (0, 1),
    ] {
        let failing = blend_failing(|m, bk, s| {
            let swapped = if m == a {
                b
            } else if m == b {
                a
            } else {
                m
            };
            blend_pixel(swapped, bk, s)
        });
        assert!(
            failing.contains(&a) && failing.contains(&b),
            "swap {a}/{b} flagged {failing:?}"
        );
        assert_eq!(
            failing.len(),
            2,
            "swap {a}/{b} touches only its own two modes"
        );
    }
}

#[test]
fn an_off_by_one_mode_index_fails_every_defined_mode() {
    let failing = blend_failing(|m, b, s| blend_pixel(m + 1, b, s));
    assert_eq!(failing, (0..16u32).collect::<Vec<u32>>());
}

#[test]
fn unknown_mode_rows_equal_the_normal_row_literal() {
    let blend = rows("blend");
    let normal = blend.iter().find(|r| r[1] == "0" && r[2] == "0").unwrap();
    for mode in ["16", "255"] {
        let row = blend.iter().find(|r| r[1] == mode).unwrap();
        assert_eq!(&row[11..15], &normal[11..15], "mode {mode}");
        assert_eq!(&row[3..11], &normal[3..11]);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum TailMut {
    Faithful,
    NoUnpremultiply,
    SwappedMixWeights,
    PremultipliedInputs,
}

/// The compositing tail with one deliberate defect; `Faithful` is the
/// harness baseline and must equal the real tail on every oracle row.
fn tail_mutant(kind: TailMut, mode: u32, backdrop: Rgba, source: Rgba) -> Rgba {
    let (ab, as_) = (backdrop[3], source[3]);
    let mut cb = [backdrop[0], backdrop[1], backdrop[2]];
    let mut cs = [source[0], source[1], source[2]];
    if kind == TailMut::PremultipliedInputs {
        for k in 0..3 {
            cb[k] *= ab;
            cs[k] *= as_;
        }
    }
    let blended = blend_rgb(mode, cb, cs);
    let ao = as_ + ab * (1.0 - as_);
    let mut out = [0.0, 0.0, 0.0, ao];
    for k in 0..3 {
        let mix = if kind == TailMut::SwappedMixWeights {
            ab * cs[k] + (1.0 - ab) * blended[k]
        } else {
            (1.0 - ab) * cs[k] + ab * blended[k]
        };
        let co = as_ * mix + ab * (1.0 - as_) * cb[k];
        out[k] = if kind == TailMut::NoUnpremultiply {
            co
        } else if ao > 1e-6 {
            co / f32::max(ao, 1e-6)
        } else {
            0.0
        };
    }
    out
}

#[test]
fn wrong_premultiply_order_and_mix_weights_change_oracle_literals() {
    // Dropping the divide back to straight colour only shows where ao < 1 (pair 4).
    let f = blend_failing(|m, b, s| tail_mutant(TailMut::NoUnpremultiply, m, b, s));
    assert!(f.contains(&0) && f.contains(&1), "{f:?}");
    // Premultiplying the inputs before the blend changes Normal and Multiply rows.
    let f = blend_failing(|m, b, s| tail_mutant(TailMut::PremultipliedInputs, m, b, s));
    assert!(f.contains(&0) && f.contains(&1), "{f:?}");
    // Swapping the (1-ab)/ab weights of Cs_mix is invisible for Normal (blended == Cs)
    // and visible for every mode whose blend differs from the source.
    let f = blend_failing(|m, b, s| tail_mutant(TailMut::SwappedMixWeights, m, b, s));
    assert!(f.contains(&1) && !f.contains(&0), "{f:?}");
}

#[test]
fn flipped_invert_swapped_mode_and_wrong_luma_weights_change_matte_literals() {
    let all = vec![(0, false), (0, true), (1, false), (1, true)];
    let flipped = matte_failing(|l, m, mode, inv| matte_pixel(l, m, mode, !inv));
    assert_eq!(flipped.len(), 4, "{flipped:?}");
    for combo in &all {
        assert!(flipped.contains(combo));
    }
    let swapped_mode = matte_failing(|l, m, mode, inv| matte_pixel(l, m, 1 - mode, inv));
    assert_eq!(swapped_mode.len(), 4, "{swapped_mode:?}");
    // Rec. 601 instead of Rec. 709: only the luma rows move.
    let w601 = matte_failing(|l, m, mode, inv| {
        if mode == 0 {
            return matte_pixel(l, m, mode, inv);
        }
        let luma = (m[0] * 0.299 + m[1] * 0.587 + m[2] * 0.114) * m[3];
        let v = if inv { 1.0 - luma } else { luma };
        [l[0], l[1], l[2], l[3] * v]
    });
    assert_eq!(w601, vec![(1, false), (1, true)]);
    // Luma that forgets the matte's own alpha: again only the luma rows.
    let no_alpha = matte_failing(|l, m, mode, inv| {
        if mode == 0 {
            return matte_pixel(l, m, mode, inv);
        }
        let luma = m[0] * 0.2126 + m[1] * 0.7152 + m[2] * 0.0722;
        let v = if inv { 1.0 - luma } else { luma };
        [l[0], l[1], l[2], l[3] * v]
    });
    assert_eq!(no_alpha, vec![(1, false), (1, true)]);
    // Replacing the layer alpha instead of multiplying it, and pre-multiplying colour.
    let replace =
        matte_failing(|l, m, mode, inv| matte_pixel([l[0], l[1], l[2], 1.0], m, mode, inv));
    assert_eq!(replace.len(), 4, "{replace:?}");
    let premul = matte_failing(|l, m, mode, inv| {
        let got = matte_pixel(l, m, mode, inv);
        let a = if l[3] > 0.0 { got[3] / l[3] } else { 0.0 };
        [got[0] * a, got[1] * a, got[2] * a, got[3]]
    });
    assert_eq!(premul.len(), 4, "{premul:?}");
}

#[test]
fn truncated_matte_chains_are_detected() {
    assert_eq!(
        chain_failing(|l, ms| matte_chain(l, &ms[..1])),
        vec!["0", "1", "2"]
    );
    assert_eq!(
        chain_failing(|l, ms| matte_chain(l, &ms[ms.len() - 1..])),
        vec!["0", "1", "2"]
    );
    assert_eq!(
        chain_failing(|l, ms| matte_chain(l, ms)),
        Vec::<&str>::new()
    );
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum MaskMut {
    None,
    SubtractAsIntersect,
    IntersectAsSubtract,
    EmptyAddStartsBlack,
    AddAsMax,
    FeatherSkipped,
    FeatherPerMask,
}

/// Layer alphas after the mask pipeline with one deliberate defect. `None`
/// is the harness baseline: same operations as the reference, so it must
/// equal every oracle literal.
fn mutated_alphas(row: &MaskRow, kind: MaskMut) -> Vec<f32> {
    if row.masks.is_empty() {
        return row.layer.clone();
    }
    let n = row.w * row.h;
    let covs: Vec<Vec<f32>> = row
        .masks
        .iter()
        .map(|(_, c)| {
            if kind == MaskMut::FeatherPerMask {
                mask_feather(c, row.w, row.h, row.feather).unwrap()
            } else {
                c.clone()
            }
        })
        .collect();
    let any_add = row.masks.iter().any(|(m, _)| *m == MaskMode::Add);
    let start = if any_add || kind == MaskMut::EmptyAddStartsBlack {
        0.0f32
    } else {
        1.0
    };
    let mut acc = vec![start; n];
    for ((mode, _), c) in row.masks.iter().zip(&covs) {
        if *mode == MaskMode::Add {
            for p in 0..n {
                acc[p] = if kind == MaskMut::AddAsMax {
                    acc[p].max(c[p])
                } else {
                    acc[p] + c[p] * (1.0 - acc[p])
                };
            }
        }
    }
    for ((mode, _), c) in row.masks.iter().zip(&covs) {
        if *mode != MaskMode::Add {
            let invert = match (*mode, kind) {
                (MaskMode::Subtract, MaskMut::SubtractAsIntersect) => false,
                (MaskMode::Intersect, MaskMut::IntersectAsSubtract) => true,
                (m, _) => m == MaskMode::Subtract,
            };
            for p in 0..n {
                acc[p] *= if invert { 1.0 - c[p] } else { c[p] };
            }
        }
    }
    let sil = if kind == MaskMut::FeatherSkipped || kind == MaskMut::FeatherPerMask {
        acc
    } else {
        mask_feather(&acc, row.w, row.h, row.feather).unwrap()
    };
    row.layer.iter().zip(&sil).map(|(a, s)| a * s).collect()
}

fn mask_failing(kind: MaskMut) -> Vec<&'static str> {
    mask_rows()
        .into_iter()
        .filter(|row| {
            let got = mutated_alphas(row, kind);
            got.len() != row.out.len() || got.iter().zip(&row.out).any(|(g, e)| !close(*g, *e, TOL))
        })
        .map(|row| row.name)
        .collect()
}

#[test]
fn swapped_mask_modes_and_a_black_start_change_named_mask_literals() {
    assert_eq!(
        mask_failing(MaskMut::SubtractAsIntersect),
        vec![
            "subtract_only",
            "add_subtract_intersect",
            "feather_shared_after_combine"
        ]
    );
    assert_eq!(
        mask_failing(MaskMut::IntersectAsSubtract),
        vec![
            "intersect_only",
            "add_subtract_intersect",
            "intersect_before_add_is_order_free"
        ]
    );
    // With no add mask the silhouette must start fully visible, not empty.
    assert_eq!(
        mask_failing(MaskMut::EmptyAddStartsBlack),
        vec!["subtract_only", "intersect_only"]
    );
    // Add masks union by src-over, not by max.
    assert_eq!(mask_failing(MaskMut::AddAsMax), vec!["two_add_union"]);
}

#[test]
fn skipping_or_misplacing_the_shared_feather_changes_named_mask_literals() {
    assert_eq!(
        mask_failing(MaskMut::FeatherSkipped),
        vec![
            "feather_2_block",
            "feather_shared_after_combine",
            "feather_30_fractional_step"
        ]
    );
    // One feather on the FINAL silhouette, not one per mask before combining.
    assert_eq!(
        mask_failing(MaskMut::FeatherPerMask),
        vec!["feather_shared_after_combine"]
    );
}

#[test]
fn the_feather_threshold_rows_are_continuous_and_a_real_blur_is_not() {
    let masks = mask_rows();
    let find = |name: &str| masks.iter().find(|r| r.name == name).unwrap();
    let single = find("single_add");
    let (at, above) = (
        find("feather_at_threshold_is_off"),
        find("feather_just_above_threshold"),
    );
    assert!(at.feather <= 0.01 && above.feather > 0.01);
    for (p, v) in single.masks[0].1.iter().enumerate() {
        assert!(close(at.sil[p], *v, TOL), "off at the threshold: pixel {p}");
        assert!(
            close(above.sil[p], *v, TOL),
            "a sub-texel radius is the identity: pixel {p}"
        );
    }
    // A real feather leaves values strictly between 0 and 1 where the mask was binary.
    let block = find("feather_2_block");
    assert!(block.sil.iter().any(|v| *v > 0.05 && *v < 0.95));
}

#[test]
fn the_oracle_grid_covers_all_six_channel_orderings_on_both_sides() {
    for side in 0..2 {
        let mut seen: Vec<[usize; 3]> = Vec::new();
        for pair in GRID.iter().take(9) {
            let c = pair[side];
            let mut order = [0usize, 1, 2];
            order.sort_by(|a, b| c[*a].partial_cmp(&c[*b]).unwrap());
            if !seen.contains(&order) {
                seen.push(order);
            }
        }
        assert_eq!(seen.len(), 6, "side {side}: {seen:?}");
    }
}

#[test]
fn a_corrupted_oracle_literal_is_detected_on_every_channel() {
    // Nudge each channel of each embedded blend literal by 1/1024 (a quarter
    // of one Rgba8 step): the reference must then disagree with it.
    let mut checked = 0;
    for r in rows("blend") {
        let got = blend_pixel(r[1].parse::<u32>().unwrap(), px(&r[3..7]), px(&r[7..11]));
        assert!(
            px_close(got, px(&r[11..15]), TOL),
            "uncorrupted row passes: {r:?}"
        );
        for k in 0..4 {
            let mut corrupted = px(&r[11..15]);
            corrupted[k] += 1.0 / 1024.0;
            assert!(!px_close(got, corrupted, TOL), "channel {k} of {r:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 162 * 4);
}
