//! The `motion` port is byte-identical to the shipped geometry-wasm flow.
//!
//! Digests were produced by the ORIGINAL `geometry-wasm/src/track.rs` and
//! `interp.rs` (included verbatim by `#[path]` in a native scratch binary,
//! debug and release identical) calling `compute_flow` + `interpolate_at` on
//! the integer-only frames below; no libm, so they reproduce on any platform.
use super::*;
use crate::request_receipts::sha256;

fn hash(x: i64, y: i64, salt: u32) -> u32 {
    let mut n = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263)
        ^ salt.wrapping_mul(2_246_822_519);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    (n ^ (n >> 16)) & 0xff
}
/// Integer value noise: hashed corners every 8 px, bilinear in integers.
fn noise(x: i64, y: i64, salt: u32) -> u8 {
    let (cx, cy) = (x.div_euclid(8), y.div_euclid(8));
    let (fx, fy) = (x.rem_euclid(8) as u32, y.rem_euclid(8) as u32);
    let a = hash(cx, cy, salt) * (8 - fx) + hash(cx + 1, cy, salt) * fx;
    let b = hash(cx, cy + 1, salt) * (8 - fx) + hash(cx + 1, cy + 1, salt) * fx;
    ((a * (8 - fy) + b * fy) / 64) as u8
}
/// An RGBA frame whose content is shifted by (dx, dy).
pub(crate) fn frame(w: usize, h: usize, dx: i64, dy: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let (sx, sy) = (x - dx, y - dy);
            out.extend_from_slice(&[noise(sx, sy, 1), noise(sx, sy, 2), noise(sx, sy, 3), 255]);
        }
    }
    out
}
fn hex(bytes: &[u8]) -> String {
    sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

const ORACLE: &[(usize, usize, i64, i64, f32, &str, [u8; 4])] = &[
    (
        96,
        64,
        3,
        -2,
        0.25,
        "ca00e2f1bcde3105283ad1c32bc9d16b026d179825ddd74b9b6df6d89fb2d3f6",
        [143, 218, 88, 255],
    ),
    (
        96,
        64,
        3,
        -2,
        0.5,
        "4586b02fec6fd5ff68f670cd186dab2954b7922546b3b1860d0dffd2b3614567",
        [135, 205, 84, 255],
    ),
    (
        96,
        64,
        3,
        -2,
        0.75,
        "443989100170cd610278845923305442425ef18c92a5cc4e4073fc9ebce3cb0a",
        [127, 195, 81, 255],
    ),
    // 600 px wide: the flow runs on a 2x reduced luma image.
    (
        600,
        200,
        6,
        4,
        0.25,
        "d06677f6d2f2d1fd911edcff74645414896885f051dcf43b1579a24c02e8eb7b",
        [169, 138, 192, 255],
    ),
    (
        600,
        200,
        6,
        4,
        0.5,
        "b4a035c65029a50030c453ba6d97df09c75711ebe5603fc32080c4185b0ec475",
        [185, 165, 199, 255],
    ),
    (
        600,
        200,
        6,
        4,
        0.75,
        "8694e5fd2785b351e1bcd8bd18c40626b64ea1e805116612f73f0cdc6fc2b348",
        [194, 179, 199, 255],
    ),
];

#[test]
fn the_port_matches_the_shipped_flow_and_warp_byte_for_byte() {
    for &(w, h, dx, dy, t, digest, mid) in ORACLE {
        let (a, b) = (frame(w, h, 0, 0), frame(w, h, dx, dy));
        let out = motion_interpolate(&a, &b, w, h, t).unwrap();
        let at = ((h / 2) * w + w / 2) * 4;
        assert_eq!(out[at..at + 4], mid, "{w}x{h} t={t}");
        assert_eq!(hex(&out), digest, "{w}x{h} t={t}");
    }
}

#[test]
fn the_warp_moves_matter_instead_of_cross_fading_it() {
    // Half way through a 4 px horizontal move, the interior matches the
    // content shifted by 2 px far better than the plain 50 % blend does.
    let (w, h) = (96, 64);
    let (a, b) = (frame(w, h, 0, 0), frame(w, h, 4, 0));
    let half = frame(w, h, 2, 0);
    let out = motion_interpolate(&a, &b, w, h, 0.5).unwrap();
    let fade: Vec<u8> = a
        .iter()
        .zip(&b)
        .map(|(&x, &y)| ((u32::from(x) * 128 + u32::from(y) * 128) >> 8) as u8)
        .collect();
    let error = |img: &[u8]| -> u64 {
        let mut sum = 0;
        for y in 16..48 {
            for x in 16..80 {
                for c in 0..3 {
                    let i = (y * w + x) * 4 + c;
                    sum += u64::from(img[i].abs_diff(half[i]));
                }
            }
        }
        sum
    };
    assert!(
        error(&out) * 3 < error(&fade),
        "warp {} fade {}",
        error(&out),
        error(&fade)
    );
}

#[test]
fn short_or_empty_buffers_are_refused_not_read() {
    let a = frame(8, 8, 0, 0);
    assert!(motion_interpolate(&a, &a[..10], 8, 8, 0.5).is_none());
    assert!(motion_interpolate(&a, &a, 0, 8, 0.5).is_none());
    assert_eq!(motion_interpolate(&a, &a, 8, 8, 0.0).unwrap(), a);
}
