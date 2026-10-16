//! Refusal, bound, mutation and coverage controls for the text layout kernel:
//! typed errors that leave no output, the length bound, a face that cannot
//! measure, and mutation controls proving the oracle's boundary rows
//! discriminate the wrap rules (`runWidth`, `>`, word spacing).

use super::flow::{wrap_lines, Span};
use super::model::{
    FontAdvancePort, FontCatalogPort, TextCase, TextLayoutError, TextLayoutRequest, TextStyle,
};
use super::{layout_text, unit_advances, MAX_TEXT_UNITS};

use super::controls::{flat, lay, request};
use super::oracle::{parse_oracle, text_of, Case, Expect, Oracle};

#[test]
fn refusals_are_typed_and_produce_no_layout() {
    let ok = request("ab", TextStyle::default());
    assert_eq!(
        layout_text(&ok, None),
        Err(TextLayoutError::FontUnavailable)
    );
    let mut unknown = ok;
    unknown.font_key = "Nope";
    assert_eq!(
        layout_text(&unknown, Some(&flat())),
        Err(TextLayoutError::FontFaceUnavailable("Nope".into()))
    );
    let mut cases: Vec<(TextLayoutRequest<'_>, TextLayoutError)> = Vec::new();
    for size in [f64::NAN, -1.0, 0.0, f64::INFINITY] {
        cases.push((
            TextLayoutRequest { size, ..ok },
            TextLayoutError::InvalidSize,
        ));
    }
    for spacing in [f64::NAN, f64::INFINITY] {
        let style = TextStyle {
            letter_spacing: spacing,
            ..ok.style
        };
        cases.push((
            TextLayoutRequest { style, ..ok },
            TextLayoutError::InvalidSpacing,
        ));
        let style = TextStyle {
            word_spacing: spacing,
            ..ok.style
        };
        cases.push((
            TextLayoutRequest { style, ..ok },
            TextLayoutError::InvalidSpacing,
        ));
    }
    for mult in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let style = TextStyle {
            line_height_mult: Some(mult),
            ..ok.style
        };
        cases.push((
            TextLayoutRequest { style, ..ok },
            TextLayoutError::InvalidLineHeightMultiplier,
        ));
    }
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        cases.push((
            TextLayoutRequest {
                fixed_width: Some(bad),
                ..ok
            },
            TextLayoutError::InvalidFixedWidth,
        ));
        cases.push((
            TextLayoutRequest {
                fixed_height: Some(bad),
                ..ok
            },
            TextLayoutError::InvalidFixedHeight,
        ));
    }
    cases.push((
        TextLayoutRequest {
            top_left: [f64::NAN, 0.0],
            ..ok
        },
        TextLayoutError::InvalidOrigin,
    ));
    cases.push((
        TextLayoutRequest {
            top_left: [0.0, f64::INFINITY],
            ..ok
        },
        TextLayoutError::InvalidOrigin,
    ));
    let long_id = "x".repeat(256);
    for holder_id in ["", "  ", "a\0b", long_id.as_str()] {
        cases.push((
            TextLayoutRequest { holder_id, ..ok },
            TextLayoutError::InvalidHolderId,
        ));
    }
    cases.push((
        TextLayoutRequest {
            size: 1e308,
            style: TextStyle {
                line_height_mult: Some(10.0),
                ..ok.style
            },
            ..ok
        },
        TextLayoutError::NonFiniteLayout,
    ));
    for (bad, expected) in cases {
        assert_eq!(layout_text(&bad, Some(&flat())), Err(expected), "{bad:?}");
    }
}

#[test]
fn text_length_is_bounded_after_case_mapping() {
    let at_limit = "a".repeat(MAX_TEXT_UNITS);
    assert_eq!(
        lay(&request(&at_limit, TextStyle::default())).lines[0]
            .glyphs
            .len(),
        MAX_TEXT_UNITS
    );
    let over = "a".repeat(MAX_TEXT_UNITS + 1);
    assert_eq!(
        layout_text(&request(&over, TextStyle::default()), Some(&flat())),
        Err(TextLayoutError::TextTooLong)
    );
    // 40_000 sharp s expand to 80_000 units under upper case.
    let upper = TextStyle {
        text_case: TextCase::Upper,
        ..TextStyle::default()
    };
    let sharp = "\u{00DF}".repeat(40_000);
    assert_eq!(
        layout_text(&request(&sharp, upper), Some(&flat())),
        Err(TextLayoutError::TextTooLong)
    );
}

#[test]
fn a_face_that_cannot_measure_refuses_the_whole_layout() {
    struct Face(f64, Option<f64>, u16);
    impl FontAdvancePort for Face {
        fn units_per_em(&self) -> f64 {
            self.0
        }
        fn advance_units(&self, unit: u16) -> Option<f64> {
            if unit == self.2 {
                self.1
            } else {
                Some(512.0)
            }
        }
    }
    struct One(Face);
    impl FontCatalogPort for One {
        fn knows_key(&self, key: &str) -> bool {
            key == "Flat-Regular"
        }
        fn face(&self, _: &str) -> Option<&dyn FontAdvancePort> {
            Some(&self.0)
        }
    }
    let r = request("ab", TextStyle::default());
    let go = |face| layout_text(&r, Some(&One(face)));
    assert_eq!(
        go(Face(1024.0, None, 0x62)),
        Err(TextLayoutError::GlyphAdvanceUnavailable(0x62))
    );
    assert_eq!(
        go(Face(0.0, Some(1.0), 0x62)),
        Err(TextLayoutError::InvalidFontMetrics)
    );
    assert_eq!(
        go(Face(f64::NAN, Some(1.0), 0x62)),
        Err(TextLayoutError::InvalidFontMetrics)
    );
    assert_eq!(
        go(Face(1024.0, Some(-1.0), 0x62)),
        Err(TextLayoutError::InvalidFontMetrics)
    );
    assert_eq!(
        go(Face(1024.0, Some(f64::NAN), 0x62)),
        Err(TextLayoutError::InvalidFontMetrics)
    );
    // A unit the area cap would drop is still measured first.
    let mut capped = r;
    capped.text = "a\nb";
    capped.fixed_height = Some(1.0);
    assert_eq!(
        layout_text(&capped, Some(&One(Face(1024.0, None, 0x62)))),
        Err(TextLayoutError::GlyphAdvanceUnavailable(0x62))
    );
}

fn boundary(oracle: &Oracle, name: &str) -> (usize, Vec<String>) {
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("oracle row {name}"));
    let Expect::Layout(want) = &case.expect else {
        panic!("{name} must lay out")
    };
    (
        want.kept,
        want.lines.iter().map(|l| text_of(&l.text)).collect(),
    )
}

/// Wraps with `wrap_lines` after tweaking one rule; the oracle rows are the judge.
fn wrapped(case: &Case, oracle: &Oracle, limit: f64, ls: f64, ws: f64) -> Vec<String> {
    let face = oracle.catalog.face(&case.font).expect("face");
    let style = TextStyle {
        word_spacing: ws,
        ..case.style
    };
    let all = case.text.clone();
    let adv = unit_advances(face, &all, case.size, &style).expect("advances");
    let spans: Vec<Span> = wrap_lines(&all, &adv, Some(limit), ls);
    spans
        .iter()
        .map(|s| text_of(&all[s.start..s.end]))
        .collect()
}

#[test]
fn mutation_controls_the_boundary_rows_discriminate_each_wrap_rule() {
    let oracle = parse_oracle();
    for name in [
        "boundary_plain",
        "boundary_tracking",
        "boundary_neg_tracking",
        "boundary_word_spacing",
    ] {
        let exact = oracle
            .cases
            .iter()
            .find(|c| c.name == format!("{name}_exact_fit"))
            .expect("exact row");
        let narrow = oracle
            .cases
            .iter()
            .find(|c| c.name == format!("{name}_one_ulp_narrower"))
            .expect("narrow row");
        let (fw_exact, fw_narrow) = (
            exact.fixed_width.expect("fw"),
            narrow.fixed_width.expect("fw"),
        );
        let ls = exact.style.letter_spacing;
        let ws = exact.style.word_spacing;
        let (exact_kept, exact_lines) = boundary(&oracle, &format!("{name}_exact_fit"));
        let (narrow_kept, narrow_lines) = boundary(&oracle, &format!("{name}_one_ulp_narrower"));
        assert_eq!(
            (exact_kept, narrow_kept),
            (1, 2),
            "{name}: the pair straddles the wrap"
        );
        assert_eq!(
            wrapped(exact, &oracle, fw_exact, ls, ws),
            exact_lines,
            "{name} exact"
        );
        assert_eq!(
            wrapped(narrow, &oracle, fw_narrow, ls, ws),
            narrow_lines,
            "{name} narrow"
        );
        // Mutation 1: `>=` instead of `>` (a box one ulp tighter) breaks the exact fit.
        let tighter = f64::from_bits(fw_exact.to_bits() - 1);
        assert_ne!(
            wrapped(exact, &oracle, tighter, ls, ws),
            exact_lines,
            "{name}: >= mutant survived"
        );
        // Mutation 2: measure raw advances (trailing tracking kept) instead of runWidth.
        if ls != 0.0 {
            let raw = (
                wrapped(exact, &oracle, fw_exact, 0.0, ws),
                wrapped(narrow, &oracle, fw_narrow, 0.0, ws),
            );
            assert_ne!(
                raw,
                (exact_lines.clone(), narrow_lines.clone()),
                "{name}: raw-advance mutant survived"
            );
        }
        // Mutation 3: word spacing left out of the measure.
        if ws != 0.0 {
            let none = (
                wrapped(exact, &oracle, fw_exact, ls, 0.0),
                wrapped(narrow, &oracle, fw_narrow, ls, 0.0),
            );
            assert_ne!(
                none,
                (exact_lines, narrow_lines),
                "{name}: no-word-spacing mutant survived"
            );
        }
    }
}

#[test]
fn oracle_covers_every_quirk_the_controls_rely_on() {
    let oracle = parse_oracle();
    for name in [
        "wrap_leading_spaces",
        "wrap_trailing_spaces",
        "wrap_long_word_unsplit",
        "empty_text_neg_tracking",
        "area_overflow_blank_kept_line",
        "surrogate_pair_two_units",
        "surrogate_pair_wrap",
        "whitespace_units_emit_no_glyph",
        "zero_width_space_is_a_glyph",
        "carriage_return_is_whitespace",
        "bold_unknown_weight_falls_back",
        "regular_from_bold_key",
        "case_capitalize",
        "italic_underline_strike",
        "decoration_skips_whitespace_only_line",
        "line_height_zero_is_default",
        "align_unknown_is_left",
        "wrap_zero_width_is_none",
        "area_zero_height_is_none",
    ] {
        assert!(
            oracle.cases.iter().any(|c| c.name == name),
            "oracle lacks {name}"
        );
    }
    let failing: Vec<&str> = oracle
        .cases
        .iter()
        .filter(|c| matches!(c.expect, Expect::Fail(_)))
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        failing,
        ["font_without_suffix_unknown", "font_inter_bold_key_unknown"]
    );
    let faces = &oracle.catalog.faces;
    assert_ne!(
        faces[0].1.advances.get(&0x61),
        faces[1].1.advances.get(&0x61)
    );
}
