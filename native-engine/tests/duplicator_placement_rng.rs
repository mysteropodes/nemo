//! Oracle tests for the duplicator random stream (L04-B, I07).
//!
//! `fixtures/duplicator-placement-rng/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-B leaf, which cuts `seededRng` out of
//! `src/js/tools.js` and `_duplicatorClonePlacement` out of `src/js/app.js` at
//! baseline `fbe4b8a3` and runs them verbatim in a Node `vm`. Before it writes a
//! row the generator runs the verbatim `_duplicatorClonePlacement` with every
//! channel random and checks that its ten outputs equal the formulas over the
//! ten draws `rx, ry, rrr, rsx, rsy, rop, rz, rrx, rry, rhue`, in that order.
//!
//! Numbers are IEEE 754 bit patterns in hex, `u` is undefined and `n` null.
//! Rows: `rng SEEDBITS => d0..d9` (`seededRng(seed)` for any JS number) and
//! `cl DUPSEED K => SEED32 d0..d9` (`((dup.seed || 0) + k * 7919) >>> 0`, then
//! the ten draws of that stream). Every draw is `u32 / 2^32`, so all
//! comparisons are bit for bit.

use super::{
    clone_draws, clone_seed, draws_for_seed, to_uint32, CloneDraws, JsNumber, SeededRng,
    CLONE_STRIDE, DRAWS_PER_CLONE,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-rng/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn js(tok: &str) -> JsNumber {
    match tok {
        "u" => JsNumber::Undefined,
        "n" => JsNumber::Null,
        h => JsNumber::Value(hex(h)),
    }
}

fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| l.split_whitespace().next() == Some(kind))
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect()
}

fn ten(tokens: &[&str]) -> [f64; DRAWS_PER_CLONE] {
    let mut d = [0.0; DRAWS_PER_CLONE];
    for (slot, tok) in d.iter_mut().zip(tokens) {
        *slot = hex(tok);
    }
    d
}

/// One `cl` row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CloneRow {
    pub seed: JsNumber,
    pub k: u32,
    pub seed_u32: u32,
    pub draws: [f64; DRAWS_PER_CLONE],
}

pub(crate) fn clone_rows() -> Vec<CloneRow> {
    rows("cl")
        .iter()
        .map(|t| CloneRow {
            seed: js(t[0]),
            k: t[1].parse().expect("k"),
            seed_u32: t[3].parse().expect("seed"),
            draws: ten(&t[4..]),
        })
        .collect()
}

/// Whether two draw arrays are equal bit for bit.
pub(crate) fn same_draws(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn seeded_stream_rows_match_baseline() {
    let all = rows("rng");
    assert_eq!(all.len(), 30);
    for t in all {
        let mut rng = SeededRng::from_js_seed(hex(t[0]));
        let got: Vec<f64> = (0..DRAWS_PER_CLONE).map(|_| rng.next_f64()).collect();
        assert!(same_draws(&got, &ten(&t[2..])), "seed bits {}", t[0]);
    }
}

#[test]
fn clone_stream_rows_match_baseline() {
    let all = clone_rows();
    assert_eq!(all.len(), 252);
    for row in all {
        assert_eq!(clone_seed(row.seed, row.k), row.seed_u32, "{row:?}");
        let got = clone_draws(row.seed, row.k).as_array();
        assert!(same_draws(&got, &row.draws), "{row:?}");
    }
}

#[test]
fn pinned_clone_streams_match_baseline_and_the_oracle() {
    // dup.seed 42, clones 0..=3 (seeds differ by the prime stride 7919).
    #[rustfmt::skip]
    let pins: [(u32, u32, [f64; 10]); 4] = [
        (0, 42, [0.6011037519201636, 0.44829055899754167, 0.8524657934904099, 0.6697340414393693, 0.17481389874592423, 0.5265925421845168, 0.2732279943302274, 0.6247446539346129, 0.8654746483080089, 0.4723170551005751]),
        (1, 7961, [0.3985240536276251, 0.9513873523101211, 0.3805503372568637, 0.6879782215692103, 0.808087618323043, 0.5966519413050264, 0.3218535252381116, 0.465765688335523, 0.9091241466812789, 0.49519087211228907]),
        (2, 15880, [0.09749352186918259, 0.8998312323819846, 0.015364649705588818, 0.599194324342534, 0.9351083294022828, 0.2222633147612214, 0.6334611442871392, 0.8403710261918604, 0.7958871186710894, 0.7867354406043887]),
        (3, 23799, [0.2893275264650583, 0.738877231720835, 0.12294989451766014, 0.6606959213968366, 0.9848718319553882, 0.8290579933673143, 0.8966377191245556, 0.7903146424796432, 0.46377892000600696, 0.6018708914052695]),
    ];
    let oracle = clone_rows();
    for (k, seed32, draws) in pins {
        let from_oracle = oracle
            .iter()
            .find(|r| r.seed == JsNumber::Value(42.0) && r.k == k)
            .expect("oracle row");
        assert_eq!(from_oracle.seed_u32, seed32);
        assert!(same_draws(&from_oracle.draws, &draws), "oracle k={k}");
        assert_eq!(clone_seed(JsNumber::Value(42.0), k), seed32, "k={k}");
        assert!(
            same_draws(&clone_draws(JsNumber::Value(42.0), k).as_array(), &draws),
            "k={k}"
        );
    }
    // Wrap-around: 4294960000 + 7919 exceeds 2^32 and reduces to 623.
    #[rustfmt::skip]
    let wrap = [0.21315328311175108, 0.6647334489971399, 0.7813690882176161, 0.7962342933751643, 0.3843901474028826, 0.3574557912070304, 0.20814704592339694, 0.4177495716139674, 0.9298487426713109, 0.27167774364352226];
    let seed = JsNumber::Value(4_294_960_000.0);
    let row = oracle
        .iter()
        .find(|r| r.seed == seed && r.k == 1)
        .expect("wrap row");
    assert_eq!(row.seed_u32, 623);
    assert!(same_draws(&row.draws, &wrap));
    assert_eq!(clone_seed(seed, 1), 623);
    assert!(same_draws(&clone_draws(seed, 1).as_array(), &wrap));
    // An absent seed reads as 0: clone 0 is the mulberry32(0) stream.
    assert_eq!(clone_seed(JsNumber::Undefined, 0), 0);
    assert_eq!(clone_draws(JsNumber::Undefined, 0).rx, 0.26642920868471265);
    assert_eq!(
        clone_draws(JsNumber::Undefined, 0).ry,
        0.0003297457005828619
    );
}

#[test]
fn a_clone_takes_ten_draws_in_the_documented_order() {
    let d: CloneDraws = clone_draws(JsNumber::Value(42.0), 0);
    assert_eq!(d.rx, 0.6011037519201636);
    assert_eq!(d.ry, 0.44829055899754167);
    assert_eq!(d.rrr, 0.8524657934904099);
    assert_eq!(d.rsx, 0.6697340414393693);
    assert_eq!(d.rsy, 0.17481389874592423);
    assert_eq!(d.rop, 0.5265925421845168);
    assert_eq!(d.rz, 0.2732279943302274);
    assert_eq!(d.rrx, 0.6247446539346129);
    assert_eq!(d.rry, 0.8654746483080089);
    assert_eq!(d.rhue, 0.4723170551005751);
    let mut raw = SeededRng::new(42);
    let stream: Vec<f64> = (0..DRAWS_PER_CLONE).map(|_| raw.next_f64()).collect();
    assert!(same_draws(&d.as_array(), &stream));
    assert_eq!(draws_for_seed(42), d);
}

#[test]
fn seeds_reduce_like_the_unsigned_shift() {
    let pins: [(f64, u32); 15] = [
        (0.0, 0),
        (-0.0, 0),
        (1.5, 1),
        (-1.5, 4_294_967_295),
        (-1.0, 4_294_967_295),
        (-7919.0, 4_294_959_377),
        (4_294_967_296.0, 0),
        (4_294_967_297.0, 1),
        (1e10, 1_410_065_408),
        (123_456_789.75, 123_456_789),
        (3_735_928_559.0, 3_735_928_559),
        (9_007_199_254_740_992.0, 0),
        (f64::MAX, 0),
        (f64::NAN, 0),
        (f64::INFINITY, 0),
    ];
    for (x, want) in pins {
        assert_eq!(to_uint32(x), want, "{x}");
    }
    assert_eq!(to_uint32(f64::NEG_INFINITY), 0);
    assert_eq!(CLONE_STRIDE, 7919.0);
}

#[test]
fn oracle_covers_the_seed_families() {
    let rng = rows("rng");
    let bits: Vec<u64> = rng.iter().map(|t| hex(t[0]).to_bits()).collect();
    for needed in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        -0.5,
        1.5,
        4_294_967_296.0,
        f64::MAX,
        9_007_199_254_740_992.0,
    ] {
        let canonical = if needed.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            needed.to_bits()
        };
        assert!(bits.contains(&canonical), "seed {needed}");
    }
    let clones = clone_rows();
    assert!(
        clones.iter().any(|r| r.seed == JsNumber::Undefined)
            && clones.iter().any(|r| r.seed == JsNumber::Null)
    );
    assert!(clones.iter().any(|r| r.k == 899) && clones.iter().any(|r| r.k == 0));
    assert!(clones.iter().any(|r| matches!(r.seed, JsNumber::Value(v) if v + 7919.0 * r.k as f64 > 4_294_967_296.0 && r.seed_u32 as f64 != v + 7919.0 * r.k as f64)));
}
