//! Oracle-driven tests for the CPU blend / matte / mask reference (L08-B).
//!
//! `fixtures/scene-effect-blend-ref/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_scene_effect_blend_ref_oracle.js` against baseline commit
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc. The blend and matte rows run the
//! REAL `blend.wgsl` / `matte.wgsl` text (helper functions, the `switch`
//! cases parsed from the shader, the compositing tail) through a WGSL-subset
//! translator with `f32` rounding after every operation; the mask rows reuse
//! the translated matte function exactly as `engine.rs` reuses `matte_pass`,
//! and the feather is `blur.wgsl` with each formula line asserted to exist.
//! The `#` header pins the SHA-256 of every extracted source. Hand-derived
//! literals live in `scene_effect_blend_ref_cases.rs`.
//!
//! Row shapes (whitespace separated, `rgba` = four numbers):
//! `blend <mode> <pair> <backdrop rgba> <source rgba> <out rgba>`,
//! `matte <mode> <invert> <pair> <layer rgba> <matte rgba> <out rgba>`,
//! `chain <id> <layer rgba> <n> (<matte rgba> <mode> <invert>)* <out rgba>`,
//! `mask <name> <w> <h> <feather> <n> (<mode> <coverage,..>)* <layer
//! alphas> <silhouette> <out alphas>`.

use super::mask::{apply_masks, feathered_silhouette, MaskCoverage, MaskMode};
use super::*;

pub(super) const ORACLE: &str = include_str!("fixtures/scene-effect-blend-ref/oracle.txt");
pub(super) const TOL: f32 = 2e-6;

pub(super) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|t| t[0] == tag)
        .collect()
}
pub(super) fn num(s: &str) -> f32 {
    s.parse::<f32>().expect("oracle number")
}
pub(super) fn px(t: &[&str]) -> Rgba {
    [num(t[0]), num(t[1]), num(t[2]), num(t[3])]
}
pub(super) fn list(s: &str) -> Vec<f32> {
    s.split(',').map(num).collect()
}
pub(super) fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}
pub(super) fn px_close(a: Rgba, b: Rgba, tol: f32) -> bool {
    (0..4).all(|k| close(a[k], b[k], tol))
}

/// The ten literal backdrop/source pairs of the 3x3 grid plus the both-alpha-0
/// extra (index 9). Mirrors `GRID` / `EXTRA` in the generator on purpose: a
/// change to either side makes `oracle_blend_rows_use_the_literal_grid` fail.
pub(super) const GRID: [[Rgba; 2]; 10] = [
    [[0.8, 0.4, 0.2, 1.0], [0.3, 0.6, 0.9, 1.0]],
    [[0.25, 0.5, 0.75, 1.0], [0.5, 0.5, 0.5, 1.0]],
    [[0.9, 0.1, 0.5, 1.0], [0.2, 0.9, 0.5, 0.5]],
    [[0.4, 0.1, 0.6, 0.5], [0.5, 0.2, 0.8, 1.0]],
    [[0.7, 0.2, 0.4, 0.5], [0.4, 0.8, 0.1, 0.5]],
    [[0.5, 0.5, 0.5, 1.0], [1.0, 0.0, 0.0, 1.0]],
    [[0.0, 1.0, 0.5, 1.0], [1.0, 0.0, 0.5, 1.0]],
    [[0.2, 0.4, 0.6, 0.0], [0.9, 0.5, 0.1, 1.0]],
    [[0.6, 0.9, 0.2, 1.0], [0.3, 0.6, 0.9, 0.0]],
    [[0.5, 0.5, 0.5, 0.0], [0.4, 0.4, 0.4, 0.0]],
];

fn sha_line(label: &str) -> &'static str {
    ORACLE
        .lines()
        .find(|l| l.starts_with("# sha256 ") && l.contains(label))
        .expect(label)
}

#[test]
fn oracle_header_pins_the_baseline_sources() {
    assert!(ORACLE.contains("# baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    for (label, hex) in [
        (
            "blend.wgsl (whole)",
            "b928fda02ee59558fffc226e9edfe3fbe24949b2d2e3d097b428987b17691ded",
        ),
        (
            "matte.wgsl (whole)",
            "aa7aceaed9ff236b86a5f835762013cd19bc4ed82e12d583ea7ac074916aecfa",
        ),
        (
            "blur.wgsl (whole)",
            "05f6337233ca1219964113f1c012877d84cfed2e715a1e4c987ae606eb342145",
        ),
        (
            "geometry-wasm/src/engine.rs (whole)",
            "1a5e3729dc57922e9c91aa8d4782572f920bc291de331332f4b03b2a27f1cc64",
        ),
    ] {
        assert!(sha_line(label).ends_with(&format!("= {hex}")), "{label}");
    }
    assert!(ORACLE.contains("# rows: blend 162, matte 36, chain 3, mask 12"));
    for label in [
        "blend.wgsl (whole)",
        "blend.wgsl helper functions",
        "matte.wgsl (whole)",
        "matte.wgsl fs_main",
        "blur.wgsl (whole)",
        "engine.rs mask-combine region",
        "engine.rs mask_feather doc",
        "engine.rs MaskIn + default_mask_mode",
    ] {
        let hex = sha_line(label).rsplit("= ").next().unwrap();
        assert_eq!(hex.len(), 64, "{label}");
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()), "{label}");
    }
}

#[test]
fn oracle_blend_rows_use_the_literal_grid() {
    let blend = rows("blend");
    assert_eq!(blend.len(), 162);
    for r in &blend {
        let pair = r[2].parse::<usize>().unwrap();
        assert_eq!(px(&r[3..7]), GRID[pair][0], "backdrop of {r:?}");
        assert_eq!(px(&r[7..11]), GRID[pair][1], "source of {r:?}");
    }
    for mode in 0..16u32 {
        let n = blend
            .iter()
            .filter(|r| r[1].parse::<u32>().unwrap() == mode)
            .count();
        assert_eq!(n, 10, "mode {mode} covers the 9 grid pairs and the extra");
    }
    let unknown: Vec<u32> = blend
        .iter()
        .map(|r| r[1].parse::<u32>().unwrap())
        .filter(|m| *m > 15)
        .collect();
    assert_eq!(unknown, vec![16, 255]);
}

#[test]
fn oracle_blend_rows_match_every_mode_on_the_grid() {
    for r in rows("blend") {
        let mode = r[1].parse::<u32>().unwrap();
        let got = blend_pixel(mode, px(&r[3..7]), px(&r[7..11]));
        assert!(
            px_close(got, px(&r[11..15]), TOL),
            "mode {mode} pair {}: {got:?} vs {:?}",
            r[2],
            &r[11..15]
        );
    }
}

#[test]
fn oracle_blend_grid_helper_matches_per_pixel_rows() {
    let backdrop: Vec<Rgba> = GRID.iter().map(|p| p[0]).collect();
    let source: Vec<Rgba> = GRID.iter().map(|p| p[1]).collect();
    for mode in 0..16u32 {
        let grid = blend_grid(mode, &backdrop, &source).unwrap();
        let expect: Vec<Rgba> = rows("blend")
            .iter()
            .filter(|r| r[1].parse::<u32>().unwrap() == mode)
            .map(|r| px(&r[11..15]))
            .collect();
        assert_eq!(grid.len(), 10);
        for (k, (g, e)) in grid.iter().zip(&expect).enumerate() {
            assert!(px_close(*g, *e, TOL), "mode {mode} pixel {k}");
        }
    }
    assert!(
        blend_grid(1, &backdrop, &source[..9]).is_none(),
        "mismatched grids are refused"
    );
}

#[test]
fn oracle_matte_rows_match_alpha_luma_and_inverted() {
    let matte = rows("matte");
    assert_eq!(matte.len(), 36);
    for (mode, invert) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let n = matte
            .iter()
            .filter(|r| r[1] == mode.to_string() && r[2] == invert.to_string())
            .count();
        assert_eq!(n, 9, "mode {mode} invert {invert}");
    }
    for r in matte {
        let (mode, invert) = (r[1].parse::<u32>().unwrap(), r[2] == "1");
        let got = matte_pixel(px(&r[4..8]), px(&r[8..12]), mode, invert);
        assert!(px_close(got, px(&r[12..16]), TOL), "matte {r:?}: {got:?}");
    }
}

#[test]
fn oracle_chain_rows_match_sequential_multi_matte() {
    let chains = rows("chain");
    assert_eq!(chains.len(), 3);
    for r in chains {
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
        let got = matte_chain(px(&r[2..6]), &mattes);
        assert!(
            px_close(got, px(&r[end..end + 4]), TOL),
            "chain {}: {got:?}",
            r[1]
        );
        assert_eq!(r.len(), end + 4);
    }
}

pub(super) struct MaskRow {
    pub(super) name: &'static str,
    pub(super) w: usize,
    pub(super) h: usize,
    pub(super) feather: f32,
    pub(super) masks: Vec<(MaskMode, Vec<f32>)>,
    pub(super) layer: Vec<f32>,
    pub(super) sil: Vec<f32>,
    pub(super) out: Vec<f32>,
}

pub(super) fn mask_rows() -> Vec<MaskRow> {
    rows("mask")
        .into_iter()
        .map(|r| {
            let n = r[5].parse::<usize>().unwrap();
            let masks = (0..n)
                .map(|k| (MaskMode::from_name(r[6 + 2 * k]), list(r[7 + 2 * k])))
                .collect();
            let at = 6 + 2 * n;
            MaskRow {
                name: r[1],
                w: r[2].parse().unwrap(),
                h: r[3].parse().unwrap(),
                feather: num(r[4]),
                masks,
                layer: list(r[at]),
                sil: list(r[at + 1]),
                out: list(r[at + 2]),
            }
        })
        .collect()
}

pub(super) fn coverage(row: &MaskRow) -> Vec<MaskCoverage<'_>> {
    row.masks
        .iter()
        .map(|(mode, c)| MaskCoverage {
            mode: *mode,
            coverage: c,
        })
        .collect()
}

#[test]
fn oracle_mask_rows_match_combine_feather_and_alpha_multiply() {
    let masks = mask_rows();
    assert_eq!(masks.len(), 12);
    for row in &masks {
        let n = row.w * row.h;
        let layer: Vec<Rgba> = row.layer.iter().map(|a| [0.5, 0.5, 0.5, *a]).collect();
        let cov = coverage(row);
        if !cov.is_empty() {
            let sil = feathered_silhouette(&cov, row.w, row.h, row.feather).unwrap();
            assert_eq!(sil.len(), n);
            for (p, (g, e)) in sil.iter().zip(&row.sil).enumerate() {
                assert!(
                    close(*g, *e, TOL),
                    "{} silhouette[{p}] {g} vs {e}",
                    row.name
                );
            }
        }
        let got = apply_masks(&layer, row.w, row.h, &cov, row.feather).unwrap();
        for (p, (g, e)) in got.iter().zip(&row.out).enumerate() {
            assert!(
                close(g[3], *e, TOL),
                "{} alpha[{p}] {} vs {e}",
                row.name,
                g[3]
            );
            assert_eq!(
                &g[..3],
                &[0.5, 0.5, 0.5],
                "{} colour[{p}] is untouched",
                row.name
            );
        }
    }
}

#[test]
fn mask_inputs_with_the_wrong_size_are_refused() {
    let one = [1.0f32; 4];
    let short = [MaskCoverage {
        mode: MaskMode::Add,
        coverage: &one[..3],
    }];
    assert!(feathered_silhouette(&short, 2, 2, 0.0).is_none());
    let ok = [MaskCoverage {
        mode: MaskMode::Add,
        coverage: &one,
    }];
    assert!(apply_masks(&[[0.0; 4]; 3], 2, 2, &ok, 0.0).is_none());
    assert!(super::mask::mask_feather(&one[..3], 2, 2, 5.0).is_none());
}
