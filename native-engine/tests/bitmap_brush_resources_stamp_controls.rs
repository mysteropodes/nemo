//! Negative controls, error pins and drift guards for the bitmap-brush stamp plan.
//!
//! Each mutation test rebuilds `StampRules` with ONE baseline constant changed and
//! requires the named oracle case (or the RNG rows) to stop matching, so a constant
//! that drifts, or an oracle that stops constraining it, fails by name. One mutant
//! is deliberately absent because it is observationally equivalent: the
//! `max(1, ceil(extent))` canvas floor (`w` and `SCALE` are positive, and the
//! ceiling of a positive number is already at least 1).

use super::rows::{
    bounds_mismatches, case_named, cases, plan_mismatches, plan_mismatches_all, plan_of,
    quadratic_mismatches, rng_mismatches, ORACLE,
};
use super::tests::{line, open_path, spec};
use super::{
    plan_stamps, PublishError, StampPath, StampPlanError, StampRules, StampSpec, WorldRect,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_policy::StoreRules;

const STRAIGHT: &str = "straight 100px, spacing 2.5, no scatter";
const TIE: &str = "hairpin tie at the cusp gate threshold";
const TIE_LONG: &str = "hairpin tie, longer first curve";

fn caught(rules: &StampRules, name: &str) -> bool {
    !plan_mismatches(rules, &case_named(name)).is_empty()
}

fn all_mismatches(text: &str) -> Vec<String> {
    let mut out = rng_mismatches(&StampRules::BASELINE, text);
    out.extend(quadratic_mismatches(text));
    out.extend(bounds_mismatches(text));
    out.extend(plan_mismatches_all(&StampRules::BASELINE, text));
    out
}

#[test]
fn the_baseline_rules_satisfy_every_oracle_section() {
    assert!(all_mismatches(ORACLE).is_empty());
}

#[test]
fn mutating_the_lcg_constants_is_caught() {
    for (multiplier, increment) in [
        (1664524, 1013904223),
        (1664526, 1013904223),
        (1664525, 1013904222),
        (1664525, 1013904224),
    ] {
        let rules = StampRules {
            lcg_multiplier: multiplier,
            lcg_increment: increment,
            ..StampRules::BASELINE
        };
        assert!(
            !rng_mismatches(&rules, ORACLE).is_empty(),
            "{multiplier} {increment}"
        );
        assert!(caught(&rules, STRAIGHT), "{multiplier} {increment}");
    }
}

#[test]
fn mutating_the_draw_order_is_caught_by_the_stamp_rows_not_the_rng_rows() {
    let rules = StampRules {
        size_draw_first: true,
        ..StampRules::BASELINE
    };
    assert!(
        rng_mismatches(&rules, ORACLE).is_empty(),
        "same stream, other consumer"
    );
    assert!(caught(&rules, "same with scatter 50 seed 12345"));
}

#[test]
fn mutating_the_cusp_gate_threshold_is_caught() {
    let inside = "hairpin just inside the gate (candidate skipped)";
    for (factor, name) in [(0.4, inside), (0.6, TIE), (0.5000000001, TIE)] {
        let rules = StampRules {
            gate_factor: factor,
            ..StampRules::BASELINE
        };
        assert!(caught(&rules, name), "{factor} must change {name}");
    }
}

#[test]
fn a_sample_at_exactly_the_gate_distance_is_kept() {
    let tie = case_named(TIE);
    let kept = plan_of(&StampRules::BASELINE, &tie).unwrap();
    let pair = [&kept.stamps[0], &kept.stamps[1]];
    let (dx, dy) = (pair[1].x - pair[0].x, pair[1].y - pair[0].y);
    assert_eq!(
        dx * dx + dy * dy,
        25.0,
        "(spacing 10 * 0.5)^2, an exact tie in doubles"
    );
    let strict = StampRules {
        gate_keeps_tie: false,
        ..StampRules::BASELINE
    };
    for name in [TIE, TIE_LONG] {
        assert!(caught(&strict, name), "{name}");
    }
    let dropped = plan_of(&strict, &tie).unwrap();
    assert_eq!(dropped.stamps.len() + 1, kept.stamps.len());
}

#[test]
fn mutating_the_spacing_and_extent_floors_is_caught() {
    for floor in [0.5, 2.0] {
        let spacing = StampRules {
            min_spacing: floor,
            ..StampRules::BASELINE
        };
        assert!(caught(&spacing, "spacing 0 percent clamps to 1"), "{floor}");
        let extent = StampRules {
            min_extent: floor,
            ..StampRules::BASELINE
        };
        assert!(
            caught(&extent, "negative size drives w,h to the 1 floor"),
            "{floor}"
        );
    }
}

#[test]
fn mutating_the_texture_clamp_is_caught() {
    for max_tex in [4095.0, 4097.0, 8192.0] {
        let rules = StampRules {
            max_tex,
            ..StampRules::BASELINE
        };
        assert!(caught(&rules, "wide path: SCALE < 2"), "{max_tex}");
    }
    let rules = StampRules {
        max_tex: 2048.0,
        ..StampRules::BASELINE
    };
    assert!(caught(&rules, "width exactly 2048: SCALE 2"));
    for max_scale in [1.5, 2.5] {
        let rules = StampRules {
            max_scale,
            ..StampRules::BASELINE
        };
        assert!(caught(&rules, STRAIGHT), "{max_scale}");
    }
}

#[test]
fn mutating_the_padding_is_caught() {
    for (factor, margin) in [(1.0, 4.0), (1.5, 4.0), (1.25, 3.0), (1.25, 5.0)] {
        let rules = StampRules {
            pad_factor: factor,
            pad_margin: margin,
            ..StampRules::BASELINE
        };
        assert!(caught(&rules, STRAIGHT), "{factor} {margin}");
    }
}

#[test]
fn dropping_the_closed_path_restamp_is_caught() {
    let rules = StampRules {
        restamp_closed_start: false,
        ..StampRules::BASELINE
    };
    assert!(caught(&rules, "closed two segments loop"));
    assert!(!plan_mismatches_all(&rules, ORACLE).is_empty());
}

#[test]
fn the_sample_guard_is_a_boundary_not_a_truncation() {
    let straight = case_named(STRAIGHT);
    let mut rules = StampRules::BASELINE;
    rules.max_samples = 40;
    assert!(
        plan_of(&rules, &straight).is_ok(),
        "40 offsets are 0, 2.5 ..= 97.5"
    );
    rules.max_samples = 39;
    assert_eq!(
        plan_of(&rules, &straight),
        Err(StampPlanError::TooManySamples { limit: 39 })
    );
    assert!(cases()
        .iter()
        .all(|case| plan_of(&StampRules::BASELINE, case).is_ok()));
}

#[test]
fn a_corrupted_oracle_row_is_detected_in_every_section() {
    for (tag, nth) in [('R', 3), ('Q', 5), ('B', 2), ('T', 2), ('K', 100), ('N', 0)] {
        let mut seen = 0;
        let mut text = String::new();
        for row in ORACLE.lines() {
            let mut row = row.to_string();
            if row.starts_with(tag) && row[1..].starts_with(' ') {
                if seen == nth {
                    let last = row.pop().unwrap();
                    row.push(if last == '0' { '1' } else { '0' });
                }
                seen += 1;
            }
            text.push_str(&row);
            text.push('\n');
        }
        assert!(!all_mismatches(&text).is_empty(), "tag {tag}");
    }
}

#[test]
fn a_canvas_ignores_an_alpha_it_cannot_apply() {
    let segments = line(0.0, 0.0, 20.0, 0.0);
    let alpha_of = |opacity: f64| {
        let mut brush = spec(10.0, 50.0, 0.0, 3.0);
        brush.opacity = opacity;
        plan_stamps(&brush, &open_path(&segments)).unwrap().alpha
    };
    for kept in [0.0, 0.25, 1.0] {
        assert_eq!(alpha_of(kept), kept);
    }
    assert_eq!(alpha_of(-0.0).to_bits(), (-0.0f64).to_bits());
    for ignored in [1.5, -0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(alpha_of(ignored), 1.0, "{ignored}");
    }
}

#[test]
fn a_stale_profile_is_ignored_and_an_empty_one_is_absent() {
    let segments = line(0.0, 0.0, 40.0, 0.0);
    let stale = [[f64::NAN, 0.0, 9.0]];
    let off = spec(20.0, 25.0, 0.0, 21.0);
    let with_stale = StampPath {
        segments: &segments,
        closed: false,
        pressure_profile: Some(&stale[..]),
    };
    assert_eq!(
        plan_stamps(&off, &with_stale),
        plan_stamps(&off, &open_path(&segments)),
        "pressure off never reads (or validates) the profile"
    );
    let mut on = off;
    on.pressure = true;
    let empty: [[f64; 3]; 0] = [];
    let with_empty = StampPath {
        segments: &segments,
        closed: false,
        pressure_profile: Some(&empty[..]),
    };
    assert_eq!(
        plan_stamps(&on, &with_empty),
        plan_stamps(&on, &open_path(&segments))
    );
    assert_eq!(
        plan_stamps(&on, &with_stale),
        Err(StampPlanError::NonFinite("pressure"))
    );
}

#[test]
fn non_finite_input_and_overflow_are_typed() {
    let segments = line(0.0, 0.0, 20.0, 0.0);
    let path = open_path(&segments);
    let brush = spec(10.0, 50.0, 10.0, 1.0);
    let with = |size: f64, spacing: f64, scatter: f64| StampSpec {
        size,
        spacing,
        scatter,
        ..brush
    };
    assert_eq!(
        plan_stamps(&with(f64::NAN, 50.0, 10.0), &path),
        Err(StampPlanError::NonFinite("size"))
    );
    assert_eq!(
        plan_stamps(&with(10.0, f64::INFINITY, 10.0), &path),
        Err(StampPlanError::NonFinite("spacing"))
    );
    assert_eq!(
        plan_stamps(&with(10.0, 50.0, f64::NEG_INFINITY), &path),
        Err(StampPlanError::NonFinite("scatter"))
    );
    let mut bad_point = segments.clone();
    bad_point[1].point[0] = f64::NAN;
    assert_eq!(
        plan_stamps(&brush, &open_path(&bad_point)),
        Err(StampPlanError::NonFinite("segment"))
    );
    let mut bad_handle = segments.clone();
    bad_handle[0].handle_out = Some([f64::INFINITY, 0.0]);
    assert_eq!(
        plan_stamps(&brush, &open_path(&bad_handle)),
        Err(StampPlanError::NonFinite("segment"))
    );
    assert_eq!(
        plan_stamps(&with(1.7e308, 50.0, 0.0), &path),
        Err(StampPlanError::NonFiniteGeometry)
    );
    let wide = line(0.0, 0.0, 1e12, 0.0);
    let loose = StampRules {
        max_tex: 1e30,
        max_scale: 1e30,
        ..StampRules::BASELINE
    };
    assert_eq!(
        loose.plan(&brush, &open_path(&wide)),
        Err(StampPlanError::PixelSizeOutOfRange)
    );
}

#[test]
fn a_published_bitmap_must_agree_with_the_plan() {
    let segments = line(0.0, 0.0, 100.0, 0.0);
    let plan = plan_stamps(&spec(10.0, 25.0, 0.0, 1.0), &open_path(&segments)).unwrap();
    let rect = WorldRect {
        min_x: -10.25,
        min_y: -10.25,
        w: 120.5,
        h: 20.5,
    };
    assert_eq!(plan.validate_published(241, 41, rect), Ok(()));
    assert_eq!(
        plan.validate_published(240, 41, rect),
        Err(PublishError::PixelSize {
            expected: (241, 41),
            actual: (240, 41)
        })
    );
    assert_eq!(
        plan.validate_published(241, 42, rect),
        Err(PublishError::PixelSize {
            expected: (241, 41),
            actual: (241, 42)
        })
    );
    for moved in [
        WorldRect {
            min_x: -10.0,
            ..rect
        },
        WorldRect {
            min_y: -11.0,
            ..rect
        },
        WorldRect { w: 120.0, ..rect },
        WorldRect { h: 21.0, ..rect },
        WorldRect {
            w: f64::NAN,
            ..rect
        },
    ] {
        assert_eq!(
            plan.validate_published(241, 41, moved),
            Err(PublishError::WorldRect)
        );
    }
}

#[test]
fn the_texture_cost_is_the_stores_checked_rgba_rule() {
    let segments = line(0.0, 0.0, 100.0, 0.0);
    let plan = plan_stamps(&spec(10.0, 25.0, 0.0, 1.0), &open_path(&segments)).unwrap();
    assert_eq!(plan.byte_cost(), Some(241 * 41 * 4));
    assert_eq!(plan.byte_cost(), StoreRules::BASELINE.cost(241, 41));
    let tall = line(0.0, 0.0, 3.0, 9000.0);
    let plan = plan_stamps(&spec(10.0, 5000.0, 0.0, 6.0), &open_path(&tall)).unwrap();
    let t = plan.texture;
    assert!(
        t.pixel_height >= 4096 && t.pixel_height <= 4097,
        "SCALE bounds the canvas"
    );
    assert_eq!(
        plan.byte_cost(),
        Some(t.pixel_width as usize * t.pixel_height as usize * 4)
    );
}

#[test]
fn the_production_source_keeps_the_module_graph_rules() {
    let source = include_str!("../src/bitmap_brush_resources_stamp.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    for banned in ["use super::", "compositor", "vello", "wgpu", "Math.random"] {
        assert!(!production.contains(banned), "{banned}");
    }
    assert!(
        production.contains("StoreRules::BASELINE.cost("),
        "one cost rule only"
    );
    let bounds = include_str!("../src/bitmap_brush_resources_stamp_bounds.rs");
    assert!(!bounds.contains("use super::"));
}
