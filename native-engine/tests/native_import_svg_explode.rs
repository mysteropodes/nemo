//! Tests of the SVG explode kernel against the baseline oracle (L10-D, G35).
//!
//! Row-by-row comparisons run the production functions over every oracle row (the rows come
//! from the verbatim baseline functions, see `tools/gen.mjs`). The literal examples live in
//! `native_import_svg_explode_pins.rs`.

use super::rows::{
    check, dec_leaf, enc_pts, enc_rect, escape, hex, oracle_has, oracle_rows, production, unescape,
    unhex,
};
use super::*;

#[test]
fn oracle_row_counts_are_pinned() {
    for (kind, n) in [
        ("B", 95),
        ("P", 69),
        ("Mo", 80),
        ("Ma", 80),
        ("E", 277),
        ("T", 20),
        ("F", 54),
    ] {
        let rows = super::rows::ORACLE
            .lines()
            .filter(|l| l.starts_with(&format!("{} ", kind)))
            .count();
        assert_eq!(rows, n, "row kind {}", kind);
    }
    assert!(
        super::rows::ORACLE.contains("# baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc")
    );
}

#[test]
fn rectangle_contains_matches_every_paper_row() {
    let rows = oracle_rows("B");
    for fields in &rows {
        let outer = super::rows::dec_rect(fields[0]);
        let inner = super::rows::dec_rect(fields[1]);
        let want = fields[3] == "1";
        assert_eq!(
            outer.contains(&inner),
            want,
            "B {} {}",
            fields[0],
            fields[1]
        );
    }
    let contained = rows.iter().filter(|f| f[3] == "1").count();
    assert!(
        contained > 10 && contained < rows.len() - 10,
        "both outcomes covered: {}",
        contained
    );
}

#[test]
fn rectangle_contains_pins() {
    let outer = Bounds {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let r = |x: f64, y: f64, w: f64, h: f64| Bounds {
        x,
        y,
        width: w,
        height: h,
    };
    let cases = [
        (r(1.0, 1.0, 2.0, 2.0), true),
        (r(0.0, 0.0, 10.0, 10.0), true),
        (r(10.0, 10.0, 0.0, 0.0), true),
        (r(0.0, 0.0, 11.0, 10.0), false),
        (r(-1.0, 0.0, 5.0, 5.0), false),
        (r(5.0, 5.0, 5.000_000_1, 5.0), false),
        (r(10.5, 10.0, 0.0, 0.0), false),
    ];
    for (inner, want) in cases {
        assert_eq!(outer.contains(&inner), want, "{:?}", inner);
        let line = format!(
            "B {} {} => {}",
            enc_rect(&outer),
            enc_rect(&inner),
            if want { 1 } else { 0 }
        );
        assert!(oracle_has(&line), "not an oracle row: {}", line);
    }
    let nan = r(f64::NAN, 0.0, 1.0, 1.0);
    assert!(!outer.contains(&nan));
    assert!(!nan.contains(&outer));
}

#[test]
fn closest_pair_matches_every_baseline_row() {
    let (rows, bad, first) = check("P", &production());
    assert_eq!(rows, 69);
    assert_eq!(bad, 0, "{:?}", first);
}

#[test]
fn closest_pair_pins() {
    let p = |x: f64, y: f64| [x, y];
    let cases: Vec<(Vec<Pt>, Vec<Pt>, (usize, usize))> = vec![
        (vec![p(0.0, 0.0), p(10.0, 0.0)], vec![p(5.0, 0.0)], (0, 0)),
        (
            vec![p(0.0, 0.0)],
            vec![p(3.0, 4.0), p(4.0, 3.0), p(-3.0, -4.0)],
            (0, 0),
        ),
        (vec![p(1.0, 1.0)], vec![], (0, 0)),
        (vec![], vec![p(1.0, 1.0)], (0, 0)),
        (
            vec![p(f64::INFINITY, 0.0), p(0.0, 0.0)],
            vec![p(1.0, 1.0)],
            (1, 0),
        ),
        (
            vec![p(0.0, 0.0), p(20.0, 0.0)],
            vec![p(18.0, 1.0), p(1.0, 1.0)],
            (0, 1),
        ),
    ];
    for (a, b, want) in cases {
        assert_eq!(closest_pair(&a, &b), want, "{:?} {:?}", a, b);
    }
    // NaN distances never beat Infinity: the (0, 0) the baseline returns for array points.
    assert_eq!(
        closest_pair(&[[f64::NAN, 0.0], [1.0, 1.0]], &[[0.0, 0.0]]),
        (1, 0)
    );
    assert_eq!(
        closest_pair(&[[f64::NAN, f64::NAN]], &[[0.0, 0.0], [1.0, 1.0]]),
        (0, 0)
    );
    // Two of the pins are oracle rows (the baseline function itself produced them).
    for (a, b, i, j) in [
        (vec![p(0.0, 0.0), p(10.0, 0.0)], vec![p(5.0, 0.0)], 0, 0),
        (
            vec![p(f64::INFINITY, 0.0), p(0.0, 0.0)],
            vec![p(1.0, 1.0)],
            1,
            0,
        ),
    ] {
        let line = format!("P {} {} => {} {}", enc_pts(&a), enc_pts(&b), i, j);
        assert!(oracle_has(&line), "not an oracle row: {}", line);
    }
}

#[test]
fn merge_matches_every_baseline_row_with_object_points() {
    let (rows, bad, first) = check("Mo", &production());
    assert_eq!(rows, 80);
    assert_eq!(bad, 0, "{:?}", first);
}

#[test]
fn merge_matches_every_baseline_row_with_array_points() {
    let (rows, bad, first) = check("Ma", &production());
    assert_eq!(rows, 80);
    assert_eq!(bad, 0, "{:?}", first);
}

#[test]
fn explode_matches_every_baseline_row() {
    let (rows, bad, first) = check("E", &production());
    assert_eq!(rows, 277);
    assert_eq!(bad, 0, "{:?}", first);
}

#[test]
fn merged_length_is_exterior_plus_two_extra_segments_per_hole() {
    let mut merged_paths = 0;
    let mut islands = 0;
    let mut nothing = 0;
    for fields in oracle_rows("E") {
        let at = fields.iter().position(|f| *f == "=>").expect("=>");
        let leaf = dec_leaf(&fields[..at]);
        let ImportLeaf::Compound { children, .. } = &leaf else {
            continue;
        };
        let out = explode_leaf(&leaf).expect("oracle rows explode");
        nothing += usize::from(out.paths.is_empty());
        for path in &out.paths {
            islands += usize::from(matches!(path.origin, PathOrigin::Island { .. }));
            if let PathOrigin::Merged { exterior, holes } = &path.origin {
                let want = children[*exterior].contour.segments.len()
                    + holes
                        .iter()
                        .map(|h| children[*h].contour.segments.len() + 2)
                        .sum::<usize>();
                assert_eq!(path.contour.segments.len(), want);
                assert!(path.contour.closed);
                merged_paths += 1;
            }
        }
    }
    assert_eq!(
        (merged_paths, islands, nothing),
        (459, 75, 10),
        "coverage of the E rows"
    );
}

#[test]
fn the_default_pairing_is_the_baseline_one_and_the_kernel_is_pure() {
    for fields in oracle_rows("E") {
        let at = fields.iter().position(|f| *f == "=>").expect("=>");
        let leaf = dec_leaf(&fields[..at]);
        let before = leaf.clone();
        let a = explode_leaf(&leaf);
        assert_eq!(
            a,
            explode_leaf_with(&leaf, HolePairing::BaselineArrayPoints)
        );
        assert_eq!(a, explode_leaf(&leaf));
        assert_eq!(leaf, before);
    }
}

#[test]
fn token_codec_round_trips_bits_and_text() {
    assert_eq!(hex(0.5), "3fe0000000000000");
    assert_eq!(hex(-0.0), "8000000000000000");
    assert!(unhex(&hex(f64::NAN)).is_nan());
    for v in [0.0, -1.5, 1e-300, f64::INFINITY, 0.1 + 0.2] {
        assert_eq!(unhex(&hex(v)).to_bits(), v.to_bits());
    }
    for text in ["", "rgb(255,0,0)", "rgba(0,128,255,0.5)", "\u{2014} a b%~$"] {
        assert_eq!(unescape(&escape(text)), text);
    }
    assert_eq!(escape(""), "$");
    assert_eq!(escape("a b"), "a%20b");
}
