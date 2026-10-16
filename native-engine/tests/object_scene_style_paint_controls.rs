//! Behavior pins, negative controls and drift guards for the scene paint kernel.
//!
//! The mutation controls rebuild `PaintRules` with one baseline constant changed
//! and require a tagged pinned case or an oracle section to fail. Two mutants are
//! deliberately absent because they are observationally equivalent: an alpha
//! ceiling of 256 or above (the `as u8` cast saturates at 255) and a negative
//! alpha floor (nothing below 0 survives the cast either).

use super::tests::{fill, oracle_mismatches, pinned_mismatches, ORACLE};
use super::{
    CssColor, FillResolution, GradientInput, GradientOutcome, GradientRejection, PaintRules,
    PaintStep, ScenePaintError, SceneSolid, StopInput,
};

const COMPOSITOR: &str = include_str!("../src/object_compositor.rs");

#[test]
fn fill_outcomes_none_solid_gradient_and_fallbacks_are_distinct() {
    let rules = PaintRules::BASELINE;
    let flat = || {
        SceneSolid::from_fill(&fill(r#"{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":1}"#)).unwrap()
    };
    let (from, to) = ([0.0, 0.0], [4.0, 0.0]);
    let short_from: [f64; 1] = [0.0];
    let stops = [
        StopInput {
            offset: 0.0,
            color: Some("#ff0000"),
        },
        StopInput {
            offset: 1.0,
            color: Some("#0000ff"),
        },
    ];
    let good = GradientInput {
        kind: "linear",
        from: Some(&from),
        to: Some(&to),
        stops: Some(&stops),
    };
    let short = GradientInput {
        stops: Some(&stops[..1]),
        ..good
    };
    let empty = GradientInput {
        stops: Some(&stops[..0]),
        ..good
    };
    let bad_from = GradientInput {
        from: Some(&short_from),
        ..good
    };
    let absent = GradientInput { to: None, ..good };

    assert_eq!(
        rules.resolve_fill(None, None, None),
        Ok(FillResolution::Empty)
    );
    assert_eq!(
        rules.resolve_fill(None, Some(flat()), None),
        Ok(FillResolution::Solid(flat()))
    );
    let drawn = rules.resolve_fill(Some(&good), Some(flat()), None).unwrap();
    assert!(drawn.gradient().is_some() && drawn.solid().is_none() && drawn.fallback().is_none());
    for (input, why) in [
        (&short, GradientRejection::TooFewStops),
        (&empty, GradientRejection::MissingGeometry),
        (&bad_from, GradientRejection::MissingGeometry),
        (&absent, GradientRejection::MissingGeometry),
    ] {
        let to_flat = rules.resolve_fill(Some(input), Some(flat()), None).unwrap();
        assert_eq!(
            to_flat,
            FillResolution::Fallback {
                rejection: why,
                flat: Some(flat())
            }
        );
        assert_eq!(to_flat.solid(), Some(&flat()));
        let to_none = rules.resolve_fill(Some(input), None, None).unwrap();
        assert_eq!(
            to_none,
            FillResolution::Fallback {
                rejection: why,
                flat: None
            }
        );
        assert!(to_none.solid().is_none() && to_none.gradient().is_none());
        assert_ne!(to_none, FillResolution::Empty);
    }
}

#[test]
fn stop_colors_use_the_item_opacity_and_unparsable_colors_are_transparent() {
    let (from, to) = ([0.0, 0.0], [1.0, 0.0]);
    let stops = [
        StopInput {
            offset: 0.0,
            color: Some("#ff0000"),
        },
        StopInput {
            offset: 0.5,
            color: Some("not a color"),
        },
        StopInput {
            offset: 1.0,
            color: None,
        },
    ];
    let input = GradientInput {
        kind: "linear",
        from: Some(&from),
        to: Some(&to),
        stops: Some(&stops),
    };
    let Ok(GradientOutcome::Ready(g)) = PaintRules::BASELINE.prepare_gradient(&input, Some(0.5))
    else {
        panic!("gradient must be ready");
    };
    let colors: Vec<[u8; 4]> = g.stops.iter().map(|s| s.color).collect();
    assert_eq!(colors, vec![[255, 0, 0, 128], [0, 0, 0, 0], [0, 0, 0, 0]]);
    assert_eq!(
        g.stops.iter().map(|s| s.offset).collect::<Vec<_>>(),
        vec![0.0, 0.5, 1.0]
    );
}

#[test]
fn non_finite_gradient_numbers_and_unrepresentable_colors_are_typed_errors() {
    let rules = PaintRules::BASELINE;
    let (from, to) = ([0.0, 0.0], [1.0, 0.0]);
    let stops = [
        StopInput {
            offset: 0.0,
            color: Some("#000"),
        },
        StopInput {
            offset: 1.0,
            color: Some("#fff"),
        },
    ];
    let base = GradientInput {
        kind: "radial",
        from: Some(&from),
        to: Some(&to),
        stops: Some(&stops),
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let nan_to = [bad, 0.0];
        let nan_from = [0.0, bad];
        let nan_stops = [
            stops[0],
            StopInput {
                offset: bad,
                color: None,
            },
        ];
        for input in [
            GradientInput {
                to: Some(&nan_to),
                ..base
            },
            GradientInput {
                from: Some(&nan_from),
                ..base
            },
            GradientInput {
                stops: Some(&nan_stops),
                ..base
            },
        ] {
            assert_eq!(
                rules.prepare_gradient(&input, None),
                Err(ScenePaintError::NonFiniteGradientNumber)
            );
        }
    }
    let negative = [
        stops[0],
        StopInput {
            offset: 1.0,
            color: Some("#-1-1-1"),
        },
    ];
    let input = GradientInput {
        stops: Some(&negative),
        ..base
    };
    assert_eq!(
        rules.prepare_gradient(&input, None),
        Err(ScenePaintError::UnrepresentableCssColor)
    );
    let nan_alpha = [
        stops[0],
        StopInput {
            offset: 1.0,
            color: Some("#ff8040zz"),
        },
    ];
    let input = GradientInput {
        stops: Some(&nan_alpha),
        ..base
    };
    assert_eq!(
        rules.prepare_gradient(&input, None),
        Err(ScenePaintError::UnrepresentableCssColor)
    );
}

#[test]
fn css_color_literals_pin_the_javascript_quirks() {
    let css = |c: Option<&str>, o: Option<f64>| PaintRules::BASELINE.css_color_to_rgba(c, o);
    let rgba = |a: [u8; 4]| Some(CssColor::Rgba(a));
    assert_eq!(css(Some("#fff"), Some(0.5)), rgba([255, 255, 255, 128]));
    assert_eq!(css(Some("#abc"), None), rgba([170, 187, 204, 255]));
    assert_eq!(css(Some("#ff804080"), Some(0.5)), rgba([255, 128, 64, 64]));
    assert_eq!(css(Some("#ff804080"), Some(1.5)), rgba([255, 128, 64, 192]));
    assert_eq!(css(Some("#0a141e"), Some(0.75)), rgba([10, 20, 30, 191]));
    assert_eq!(css(Some("#f f f "), None), rgba([15, 15, 15, 255]));
    assert_eq!(css(Some("#+1+1+1"), None), rgba([1, 1, 1, 255]));
    assert_eq!(css(Some("#-0-0-0"), None), rgba([0, 0, 0, 255]));
    assert_eq!(css(Some("#ff0000-f"), None), rgba([255, 0, 0, 0]));
    assert_eq!(css(Some("#-1-1-1"), None), Some(CssColor::Unrepresentable));
    assert_eq!(
        css(Some("#ff8040zz"), None),
        Some(CssColor::Unrepresentable)
    );
    assert_eq!(
        css(Some("#fff"), Some(f64::NAN)),
        Some(CssColor::Unrepresentable)
    );
    assert_eq!(css(Some("#0x0x0x"), None), None);
    assert_eq!(css(Some("##fff"), None), None);
    assert_eq!(
        css(Some("#\u{1f600}f"), None),
        None,
        "length counts UTF-16 units"
    );
    assert_eq!(
        css(Some("#ff8040\u{1f600}"), None),
        Some(CssColor::Unrepresentable)
    );
    assert_eq!((css(Some(""), None), css(None, None)), (None, None));
}

#[test]
fn paint_order_steps_put_the_stroke_first_only_for_the_exact_name() {
    let rules = PaintRules::BASELINE;
    assert_eq!(
        rules.paint_order(Some("strokeFirst")).steps(),
        [PaintStep::Stroke, PaintStep::Fill]
    );
    for other in [
        None,
        Some("fillFirst"),
        Some("strokefirst"),
        Some("StrokeFirst"),
        Some(" strokeFirst"),
        Some(""),
    ] {
        assert_eq!(
            rules.paint_order(other).steps(),
            [PaintStep::Fill, PaintStep::Stroke],
            "{other:?}"
        );
    }
}

fn tagged(rules: &PaintRules, tag: &str) -> bool {
    pinned_mismatches(rules).iter().any(|m| m.starts_with(tag))
}

#[test]
fn mutating_the_two_stop_fallback_is_caught() {
    for min_stops in [1usize, 3] {
        let mutant = PaintRules {
            min_stops,
            ..PaintRules::BASELINE
        };
        assert!(tagged(&mutant, "min_stops:"), "pinned case, {min_stops}");
        assert!(
            !oracle_mismatches(ORACLE, &mutant, "V").is_empty(),
            "V rows, {min_stops}"
        );
    }
}

#[test]
fn mutating_the_radial_linear_selector_is_caught() {
    for radial_name in ["Radial", "radial2", "", "linear"] {
        let mutant = PaintRules {
            radial_name,
            ..PaintRules::BASELINE
        };
        assert!(
            tagged(&mutant, "radial_name:"),
            "pinned case, {radial_name:?}"
        );
        assert!(
            !oracle_mismatches(ORACLE, &mutant, "K").is_empty(),
            "K rows, {radial_name:?}"
        );
    }
}

#[test]
fn mutating_the_radius_floor_is_caught() {
    for radius_floor in [0.0f32, 0.02, f32::from_bits(0x3c23_d70b)] {
        let mutant = PaintRules {
            radius_floor,
            ..PaintRules::BASELINE
        };
        assert!(
            tagged(&mutant, "radius_floor:"),
            "pinned case, {radius_floor}"
        );
        assert!(
            !oracle_mismatches(ORACLE, &mutant, "K").is_empty(),
            "K rows, {radius_floor}"
        );
    }
}

#[test]
fn mutating_the_alpha_clamp_is_caught() {
    for alpha_ceiling in [254.0f64, 200.0, 128.0] {
        let mutant = PaintRules {
            alpha_ceiling,
            ..PaintRules::BASELINE
        };
        assert!(
            tagged(&mutant, "alpha_ceiling:"),
            "pinned case, {alpha_ceiling}"
        );
        assert!(
            !oracle_mismatches(ORACLE, &mutant, "C").is_empty(),
            "C rows, {alpha_ceiling}"
        );
    }
}

#[test]
fn mutating_the_stroke_first_test_is_caught() {
    for stroke_first_name in ["strokefirst", "fillFirst", "", "StrokeFirst"] {
        let mutant = PaintRules {
            stroke_first_name,
            ..PaintRules::BASELINE
        };
        assert!(
            tagged(&mutant, "stroke_first_name:"),
            "{stroke_first_name:?}"
        );
    }
}

#[test]
fn a_corrupted_oracle_row_is_detected_in_every_section() {
    let flip_last = |line: &str| {
        let (head, last) = line.split_at(line.len() - 1);
        format!("{head}{}", if last == "0" { "1" } else { "0" })
    };
    let prefixes = [
        ("C", "C 23666666 U "),
        ("V", "V 2 2 2 "),
        ("K", "K 726164"),
        ("O", "O "),
    ];
    for (section, prefix) in prefixes {
        let line = ORACLE
            .lines()
            .find(|l| l.starts_with(prefix) && (section != "K" || l.contains(" R ")))
            .expect("oracle row exists");
        let corrupted = ORACLE.replacen(line, &flip_last(line), 1);
        assert_ne!(corrupted, ORACLE);
        let bad = oracle_mismatches(&corrupted, &PaintRules::BASELINE, section);
        assert_eq!(bad.len(), 1, "section {section}: {bad:?}");
    }
}

#[test]
fn scene_solid_range_rule_is_the_compositor_color_rule() {
    // L07-F: the compositor no longer carries a second copy of the rule; it converts
    // through SceneSolid::rgba_f32 and reports the kernel's own message.
    assert!(COMPOSITOR.contains(".solid()"));
    assert!(COMPOSITOR.contains(".rgba_f32()"));
    assert!(COMPOSITOR.contains("CompositorError::new(error.message())"));
    assert!(!COMPOSITOR.contains("(0.0..=1.0).contains"));
    let message = ScenePaintError::UnrepresentableStraightRgba.message();
    assert_eq!(message, "unrepresentable object straight RGBA");
}

#[test]
fn scene_solid_admission_agrees_with_the_codec_channel_rule() {
    let values = [
        "0",
        "1",
        "0.5",
        "-0.0",
        "1.0000000000000002",
        "-1",
        "2",
        "1e300",
        "-1e-300",
    ];
    for r in values {
        for a in values {
            let json = format!(r#"{{"kind":"solid","r":{r},"g":0.25,"b":1,"a":{a}}}"#);
            let source = fill(&json);
            assert_eq!(
                SceneSolid::from_fill(&source).is_ok(),
                source.valid_channels(),
                "{json}"
            );
        }
    }
}
