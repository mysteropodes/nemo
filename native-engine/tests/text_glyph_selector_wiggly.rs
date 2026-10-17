//! Controls for the Wiggly selector and the glyph-identity join (G38b /
//! L06-G): the weight is a function of `timeSec`, the seed and its own fields
//! only, identical inputs give identical bits, a text of more than 567 units (a
//! hash phase beyond 1.6 million radians) is still served, and the selector
//! reads the identity the REAL L06-A text layout stamps on its glyphs
//! (`charIndex`, 1-based `wordIndex`, `lineIndex`).

use super::oracle::same;
use super::{
    unit_count, unit_index_of, weight_at, weights, BasedOn, Selector, SelectorType, UnitIndices,
    Units, SHAPE_RAMP_UP,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    FontAdvancePort, FontCatalogPort, TextAlign, TextLayoutRequest, TextStyle,
};

fn wiggly() -> Selector {
    Selector {
        selector_type: SelectorType::Wiggly,
        seed: 42.0,
        ..Selector::default_selector()
    }
}

// ---- the Wiggly selector is a function of time, seed and its own fields only ----------------------------------------

#[test]
fn wiggly_ignores_every_range_field_and_the_unit_count() {
    let base = wiggly();
    let want = weights(&base, 6.0, 1.37).unwrap();
    let noisy = Selector {
        start: 40.0,
        end: Some(5.0),
        offset: 70.0,
        units: Units::Index,
        based_on: BasedOn::Lines,
        shape: 6.0,
        amount: Some(7.0),
        ease_high: 80.0,
        ease_low: -50.0,
        smooth: Some(3.0),
        ..base
    };
    assert_eq!(weights(&noisy, 6.0, 1.37).unwrap(), want);
    // the unit COUNT is not an input either: unit 3 weighs the same in a block of 6 and of 60
    assert!(same(
        weight_at(&base, 3.0, 6.0, 1.37).unwrap(),
        weight_at(&base, 3.0, 60.0, 1.37).unwrap()
    ));
    // time, seed and the wiggle fields are inputs
    assert_ne!(weights(&base, 6.0, 1.38).unwrap(), want);
    assert_ne!(
        weights(&Selector { seed: 43.0, ..base }, 6.0, 1.37).unwrap(),
        want
    );
    assert_ne!(
        weights(
            &Selector {
                correlation: Some(10.0),
                ..base
            },
            6.0,
            1.37
        )
        .unwrap(),
        want
    );
    assert_ne!(
        weights(
            &Selector {
                wiggle_max: Some(80.0),
                ..base
            },
            6.0,
            1.37
        )
        .unwrap(),
        want
    );
}

#[test]
fn identical_inputs_give_identical_bits_on_every_call() {
    let sel = wiggly();
    let first = weights(&sel, 8.0, 12.8).unwrap();
    for _ in 0..50 {
        let again = weights(&sel, 8.0, 12.8).unwrap();
        assert!(first.iter().zip(&again).all(|(a, b)| same(*a, *b)));
    }
    let smooth = Selector {
        shape: 6.0,
        ..Selector::default_selector()
    };
    let a = weights(&smooth, 9.0, 0.0).unwrap();
    assert!(a
        .iter()
        .zip(weights(&smooth, 9.0, 99.0).unwrap())
        .all(|(x, y)| same(*x, y)));
}

#[test]
fn a_wiggly_text_of_more_than_567_units_is_still_served() {
    // unit 700 phases the hash at ind * 37.13 * 78.233 > 1647100: the baseline answers, so does the kernel
    let sel = wiggly();
    let w = weight_at(&sel, 700.0, 701.0, 1.37).unwrap();
    assert!((-0.5001..=0.5001).contains(&w), "{w}");
    assert!(w.is_finite());
    assert_eq!(weights(&sel, 1200.0, 0.0).unwrap().len(), 1200);
}

// ---- the join with the real L06-A glyph identity -------------------------------------------------------------------

struct FlatFace;

impl FontAdvancePort for FlatFace {
    fn units_per_em(&self) -> f64 {
        1024.0
    }
    fn advance_units(&self, unit: u16) -> Option<f64> {
        Some(if unit == 32 { 256.0 } else { 512.0 })
    }
}

struct FlatCatalog(FlatFace);

impl FontCatalogPort for FlatCatalog {
    fn knows_key(&self, key: &str) -> bool {
        key == "Flat-Regular"
    }
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort> {
        (key == "Flat-Regular").then_some(&self.0 as &dyn FontAdvancePort)
    }
}

#[test]
fn the_selector_reads_the_identity_the_real_text_layout_stamps() {
    let request = TextLayoutRequest {
        holder_id: "holder-1",
        text: "ab cd\nef",
        font_key: "Flat-Regular",
        size: 20.0,
        align: TextAlign::Left,
        fixed_width: None,
        fixed_height: None,
        top_left: [0.0, 0.0],
        style: TextStyle::default(),
    };
    let laid = layout_text(&request, Some(&FlatCatalog(FlatFace))).expect("layout");
    let glyphs: Vec<UnitIndices> = laid
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .map(UnitIndices::of_glyph)
        .collect();
    assert_eq!(glyphs.len(), 6, "a b c d e f; whitespace has no glyph");
    let chars: Vec<f64> = glyphs
        .iter()
        .map(|g| unit_index_of(Some(g), BasedOn::Chars).unwrap())
        .collect();
    let words: Vec<f64> = glyphs
        .iter()
        .map(|g| unit_index_of(Some(g), BasedOn::Words).unwrap())
        .collect();
    let lines: Vec<f64> = glyphs
        .iter()
        .map(|g| unit_index_of(Some(g), BasedOn::Lines).unwrap())
        .collect();
    assert_eq!(chars, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(
        words,
        [1.0, 1.0, 2.0, 2.0, 3.0, 3.0],
        "wordIndex is 1-based and runs over the block"
    );
    assert_eq!(lines, [0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    // taUnitCount is the highest index plus one: the 1-based word index makes 3 words count 4 (baseline quirk, kept)
    assert_eq!(unit_count(&glyphs, BasedOn::Chars), 6.0);
    assert_eq!(unit_count(&glyphs, BasedOn::Words), 4.0);
    assert_eq!(unit_count(&glyphs, BasedOn::Lines), 2.0);
    // a glyph's weight is the weight of the unit it counts as
    let sel = Selector {
        shape: SHAPE_RAMP_UP,
        ..Selector::default_selector()
    };
    let all = weights(&sel, 6.0, 0.0).unwrap();
    for (i, g) in glyphs.iter().enumerate() {
        let idx = unit_index_of(Some(g), BasedOn::Chars).unwrap();
        assert!(
            same(weight_at(&sel, idx, 6.0, 0.0).unwrap(), all[i]),
            "glyph {i}"
        );
    }
}

#[test]
fn unit_index_of_and_unit_count_follow_the_null_checks() {
    let some = |c, w, l| UnitIndices {
        char_index: c,
        word_index: w,
        line_index: l,
    };
    // 0 is a real index (`!= null`), so it must not fall back to the character index
    let sd = some(Some(5.0), Some(0.0), Some(0.0));
    assert_eq!(unit_index_of(Some(&sd), BasedOn::Words), Some(0.0));
    assert_eq!(unit_index_of(Some(&sd), BasedOn::Lines), Some(0.0));
    let missing = some(Some(5.0), None, None);
    assert_eq!(unit_index_of(Some(&missing), BasedOn::Words), Some(5.0));
    assert_eq!(unit_index_of(Some(&missing), BasedOn::Lines), Some(5.0));
    assert_eq!(
        unit_index_of(Some(&some(None, None, None)), BasedOn::Chars),
        None
    );
    assert_eq!(
        unit_index_of(Some(&some(None, Some(2.0), None)), BasedOn::Chars),
        None
    );
    // taUnitCount: strokes without an index are skipped, no strokes (or no indices) count 0
    assert_eq!(unit_count(&[], BasedOn::Chars), 0.0);
    assert_eq!(unit_count(&[some(None, None, None)], BasedOn::Words), 0.0);
    assert_eq!(
        unit_count(
            &[
                some(Some(2.0), None, None),
                some(Some(9.0), Some(1.0), None)
            ],
            BasedOn::Words
        ),
        3.0
    );
}
