//! Hand-derived literals and mutation controls of the pure glyph-source
//! arithmetic. Literals are worked out from the opentype.js source text
//! (`glyfToPath`, `getPath`, `Dt`) or taken from a JS evaluation of the same
//! expression, never from the code under test; each mutation control proves
//! the oracle rows reject a plausible variant while production equals them.
use super::oracle::{design, num, outline, points, rows};
use super::{
    assemble_composite, design_commands, scale_commands, transform_points, ComponentTransform,
    DesignCommand as C, DesignPoint, GlyphSourceError,
};
use crate::object_document::object_text_layout_kernels::text_glyph_contours::OutlineCommand as O;

fn pt(x: f64, y: f64, on_curve: bool, last_of_contour: bool) -> DesignPoint {
    DesignPoint {
        x,
        y,
        on_curve,
        last_of_contour,
    }
}

#[test]
fn contour_starts_at_the_last_point_when_it_is_on_the_curve() {
    let square = [
        pt(0.0, 0.0, true, false),
        pt(10.0, 0.0, true, false),
        pt(10.0, 10.0, true, false),
        pt(0.0, 10.0, true, true),
    ];
    assert_eq!(
        design_commands(&square).unwrap(),
        vec![
            C::Move { x: 0.0, y: 10.0 },
            C::Line { x: 0.0, y: 0.0 },
            C::Line { x: 10.0, y: 0.0 },
            C::Line { x: 10.0, y: 10.0 },
            C::Line { x: 0.0, y: 10.0 },
            C::Close,
        ]
    );
}

#[test]
fn contour_starts_at_the_first_point_when_only_it_is_on_the_curve() {
    let points = [
        pt(0.0, 0.0, true, false),
        pt(10.0, 0.0, false, false),
        pt(10.0, 10.0, true, false),
        pt(0.0, 10.0, false, true),
    ];
    assert_eq!(
        design_commands(&points).unwrap(),
        vec![
            C::Move { x: 0.0, y: 0.0 },
            C::Line { x: 0.0, y: 0.0 },
            C::Quad {
                x1: 10.0,
                y1: 0.0,
                x: 10.0,
                y: 10.0
            },
            C::Line { x: 10.0, y: 10.0 },
            C::Quad {
                x1: 0.0,
                y1: 10.0,
                x: 0.0,
                y: 0.0
            },
            C::Close,
        ]
    );
}

#[test]
fn contour_starts_at_the_midpoint_when_first_and_last_are_off_the_curve() {
    let points = [
        pt(0.0, 0.0, false, false),
        pt(10.0, 0.0, true, false),
        pt(10.0, 10.0, false, true),
    ];
    assert_eq!(
        design_commands(&points).unwrap(),
        vec![
            C::Move { x: 5.0, y: 5.0 },
            C::Quad {
                x1: 0.0,
                y1: 0.0,
                x: 10.0,
                y: 0.0
            },
            C::Line { x: 10.0, y: 0.0 },
            C::Quad {
                x1: 10.0,
                y1: 10.0,
                x: 5.0,
                y: 5.0
            },
            C::Close,
        ]
    );
}

#[test]
fn consecutive_off_curve_points_end_at_their_midpoint() {
    let points = [
        pt(0.0, 0.0, true, false),
        pt(10.0, 0.0, false, false),
        pt(20.0, 0.0, false, false),
        pt(30.0, 0.0, true, true),
    ];
    assert_eq!(
        design_commands(&points).unwrap(),
        vec![
            C::Move { x: 30.0, y: 0.0 },
            C::Line { x: 0.0, y: 0.0 },
            C::Quad {
                x1: 10.0,
                y1: 0.0,
                x: 15.0,
                y: 0.0
            },
            C::Quad {
                x1: 20.0,
                y1: 0.0,
                x: 30.0,
                y: 0.0
            },
            C::Line { x: 30.0, y: 0.0 },
            C::Close,
        ]
    );
}

#[test]
fn a_single_point_contour_and_several_contours_keep_their_order() {
    let points = [
        pt(7.0, 9.0, true, true),
        pt(1.0, 1.0, true, false),
        pt(2.0, 2.0, true, true),
    ];
    assert_eq!(
        design_commands(&points).unwrap(),
        vec![
            C::Move { x: 7.0, y: 9.0 },
            C::Line { x: 7.0, y: 9.0 },
            C::Close,
            C::Move { x: 2.0, y: 2.0 },
            C::Line { x: 1.0, y: 1.0 },
            C::Line { x: 2.0, y: 2.0 },
            C::Close,
        ]
    );
    assert_eq!(design_commands(&[]).unwrap(), Vec::new());
}

#[test]
fn points_after_the_last_contour_end_are_refused() {
    let dangling = [pt(0.0, 0.0, true, true), pt(1.0, 1.0, true, false)];
    assert_eq!(
        design_commands(&dangling),
        Err(GlyphSourceError::UnterminatedContour)
    );
}

#[test]
fn scaling_is_one_over_upm_times_size_with_the_y_flip() {
    let commands = [
        C::Move {
            x: 1024.0,
            y: 2048.0,
        },
        C::Quad {
            x1: 512.0,
            y1: 1536.0,
            x: 2048.0,
            y: 0.0,
        },
        C::Close,
    ];
    // 1 / 2048 * 100 = 0.048828125 exactly.
    assert_eq!(
        scale_commands(&commands, 2048.0, 10.0, 20.0, 100.0).unwrap(),
        vec![
            O::Move { x: 60.0, y: -80.0 },
            O::Quad {
                x1: 35.0,
                y1: -55.0,
                x: 110.0,
                y: 20.0
            },
            O::Close,
        ]
    );
}

#[test]
fn scaling_multiplies_the_reciprocal_not_divides_by_upm() {
    // JS: 1 / 1000 * 26 = 0.026000000000000002, so 10 + 777 * it = 30.202 and
    // 20 + -777 * it = -0.20200000000000173; 26 / 1000 would give
    // 30.201999999999998 and -0.20199999999999818.
    let commands = [C::Move { x: 777.0, y: 777.0 }];
    assert_eq!(
        scale_commands(&commands, 1000.0, 10.0, 20.0, 26.0).unwrap(),
        vec![O::Move {
            x: 30.202,
            y: -0.20200000000000173
        }]
    );
    assert_ne!(10.0 + 777.0 * (26.0 / 1000.0), 30.202);
}

#[test]
fn scaling_refuses_bad_metrics_and_overflow() {
    let commands = [C::Move { x: 1.0, y: 1.0 }];
    for upm in [0.0, -2048.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            scale_commands(&commands, upm, 0.0, 0.0, 10.0),
            Err(GlyphSourceError::InvalidMetrics)
        );
    }
    let huge = [C::Move { x: 1e300, y: 1.0 }];
    assert_eq!(
        scale_commands(&huge, 1.0, 0.0, 0.0, 1e300),
        Err(GlyphSourceError::NonFiniteGeometry)
    );
}

#[test]
fn component_transform_applies_scale01_to_y_in_the_x_row() {
    let t = ComponentTransform {
        x_scale: 0.5,
        scale01: 0.25,
        scale10: -0.5,
        y_scale: 2.0,
        dx: 3.0,
        dy: -4.0,
    };
    let moved = transform_points(&[pt(10.0, 20.0, false, true)], &t);
    // x' = 0.5*10 + 0.25*20 + 3, y' = -0.5*10 + 2*20 - 4 (opentype.js Dt); the
    // TrueType 2x2 layout would put -0.5 in the x row and give x' = -2.
    assert_eq!(moved, vec![pt(13.0, 31.0, false, true)]);
}

#[test]
fn composite_assembly_concatenates_parts_in_component_order() {
    let a = vec![pt(1.0, 1.0, true, true)];
    let b = vec![pt(2.0, 2.0, true, true)];
    let assembled = assemble_composite(&[
        (a, ComponentTransform::offset(100.0, 0.0)),
        (b, ComponentTransform::offset(0.0, -50.0)),
    ]);
    assert_eq!(
        assembled,
        vec![pt(101.0, 1.0, true, true), pt(2.0, -48.0, true, true)]
    );
    assert_eq!(assemble_composite(&[]), Vec::new());
}

/// Mutant: no implied midpoints; a contour starts at its last point unless that
/// is off the curve, and an off-curve point always ends at the next point.
fn no_implied_midpoint(points: &[DesignPoint]) -> Vec<C> {
    let mut rebuilt = Vec::new();
    let mut start = 0;
    for (index, point) in points.iter().enumerate() {
        if !point.last_of_contour {
            continue;
        }
        let contour = &points[start..=index];
        start = index + 1;
        let count = contour.len();
        let first = if contour[count - 1].on_curve {
            contour[count - 1]
        } else {
            contour[0]
        };
        rebuilt.push(C::Move {
            x: first.x,
            y: first.y,
        });
        for i in 0..count {
            let current = contour[i];
            let next = contour[(i + 1) % count];
            if current.on_curve {
                rebuilt.push(C::Line {
                    x: current.x,
                    y: current.y,
                });
            } else {
                rebuilt.push(C::Quad {
                    x1: current.x,
                    y1: current.y,
                    x: next.x,
                    y: next.y,
                });
            }
        }
        rebuilt.push(C::Close);
    }
    rebuilt
}

/// Mutant: `getPath` without the y flip (`y + cy * scale`).
fn unflipped(commands: &[C], upm: f64, x: f64, y: f64, size: f64) -> Vec<O> {
    let scale = 1.0 / upm * size;
    commands
        .iter()
        .map(|command| match *command {
            C::Move { x: cx, y: cy } => O::Move {
                x: x + cx * scale,
                y: y + cy * scale,
            },
            C::Line { x: cx, y: cy } => O::Line {
                x: x + cx * scale,
                y: y + cy * scale,
            },
            C::Quad {
                x1,
                y1,
                x: cx,
                y: cy,
            } => O::Quad {
                x1: x + x1 * scale,
                y1: y + y1 * scale,
                x: x + cx * scale,
                y: y + cy * scale,
            },
            C::Close => O::Close,
        })
        .collect()
}

#[test]
fn mutation_controls_the_oracle_rejects_each_plausible_variant() {
    let pure = rows("PURE");
    let mut midpoint_rejected = 0;
    let mut offset_ignored_rejected = 0;
    let mut unflipped_rejected = 0;
    for row in &pure {
        let expected = design(&row["cmds"]);
        assert_eq!(design_commands(&points(&row["pts"])).unwrap(), expected);
        if no_implied_midpoint(&points(&row["pts"])) != expected {
            midpoint_rejected += 1;
        }
        // Mutant: components placed without their dx / dy.
        let parts: Vec<(Vec<DesignPoint>, ComponentTransform)> = row["comps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|component| {
                (
                    points(&component["pts"]),
                    ComponentTransform::offset(0.0, 0.0),
                )
            })
            .collect();
        if !parts.is_empty() && assemble_composite(&parts) != points(&row["pts"]) {
            offset_ignored_rejected += 1;
        }
        let scaled = &row["scaled"][1];
        let flat = unflipped(
            &expected,
            num(&row["upm"]),
            num(&scaled["x"]),
            num(&scaled["y"]),
            num(&scaled["size"]),
        );
        if flat != outline(&scaled["cmds"]) {
            unflipped_rejected += 1;
        }
    }
    assert_eq!(
        midpoint_rejected, 18,
        "rows that reject the no-midpoint variant"
    );
    assert_eq!(
        offset_ignored_rejected, 6,
        "every composite row needs its offsets"
    );
    // The space glyph has no commands, so it cannot tell the two scalings apart.
    assert_eq!(unflipped_rejected, 21);

    // Mutant: the TrueType spec's 2x2 placement (scale10 in the x row).
    let mut transposed_rejected = 0;
    for row in rows("XF") {
        let t = ComponentTransform {
            x_scale: num(&row["xs"]),
            scale01: num(&row["s10"]),
            scale10: num(&row["s01"]),
            y_scale: num(&row["ys"]),
            dx: num(&row["dx"]),
            dy: num(&row["dy"]),
        };
        if transform_points(&points(&row["input"]), &t) != points(&row["output"]) {
            transposed_rejected += 1;
        }
    }
    assert_eq!(
        transposed_rejected, 31,
        "XF rows that reject the transposed 2x2"
    );
}
