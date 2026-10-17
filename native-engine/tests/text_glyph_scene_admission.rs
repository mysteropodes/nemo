//! Admission controls of the text scene join (G38b / L06-K): the oversize refusal
//! and the two policies on hand-built outlines with literal expectations. The
//! helpers (ports, layout, scene runner) live in `text_glyph_scene_controls.rs`.

use super::controls::{
    l, layout_of, m, scene_of, seg, segments_of, Constant, Override, DROP, STRICT,
};
use super::oracle::{lay_out, read, Replay};
use super::tests::{COUNT, DEGENERATE};
use super::{SceneRole, TextSceneError};
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    OutlineCommand, PathFlag,
};

fn polygon(points: usize) -> Vec<OutlineCommand> {
    let mut commands = Vec::new();
    for k in 0..points {
        let angle = (k as f64) * std::f64::consts::TAU / (points as f64);
        let (x, y) = (100.0 + 50.0 * angle.cos(), 100.0 + 50.0 * angle.sin());
        commands.push(if k == 0 { m(x, y) } else { l(x, y) });
    }
    commands.push(OutlineCommand::Close);
    commands
}

#[test]
fn more_than_256_segments_refuse_the_whole_block_at_that_glyph_under_both_policies() {
    let oracle = read();
    // translate_layer ("HELIX") is strict-clean, so the oversized last glyph
    // (X, ordinal 4) is the first inadmissible path under either policy.
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "translate_layer")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    for admission in [STRICT, DROP] {
        let port = Override {
            inner: Replay::new(&case.placed),
            unit: u16::from(b'X'),
            commands: Some(polygon(300)),
        };
        let error = scene_of(&layout, Some(&port), &[], admission).unwrap_err();
        assert_eq!(
            error,
            TextSceneError::UnsupportedGeometry {
                role: SceneRole::Glyph {
                    ordinal: 4,
                    path_index: 0
                },
                flag: Some(PathFlag::SegmentCount(300)),
                reason: COUNT,
            }
        );
    }
    // 256 is the largest admitted ring; 257 is refused.
    let one = layout_of("O", false, &oracle.fonts);
    let ok = scene_of(&one, Some(&Constant(Some(polygon(256)))), &[], STRICT).unwrap();
    assert_eq!(ok.contours()[0].contour.segments().len(), 256);
    let over = scene_of(&one, Some(&Constant(Some(polygon(257)))), &[], STRICT).unwrap_err();
    assert!(matches!(
        over,
        TextSceneError::UnsupportedGeometry {
            flag: Some(PathFlag::SegmentCount(257)),
            ..
        }
    ));
}

#[test]
fn a_leading_zero_length_curve_is_refused_strictly_and_removed_only_on_request() {
    let fonts = read().fonts;
    let layout = layout_of("H", false, &fonts);
    let outline = vec![
        m(10.0, 10.0),
        l(10.0, 10.0),
        l(50.0, 10.0),
        l(50.0, 50.0),
        l(10.0, 50.0),
        OutlineCommand::Close,
    ];
    let port = Constant(Some(outline));
    let error = scene_of(&layout, Some(&port), &[], STRICT).unwrap_err();
    assert_eq!(
        error,
        TextSceneError::UnsupportedGeometry {
            role: SceneRole::Glyph {
                ordinal: 0,
                path_index: 0
            },
            flag: Some(PathFlag::DegenerateCurve(0)),
            reason: DEGENERATE,
        }
    );
    let dropped = scene_of(&layout, Some(&port), &[], DROP).expect("normalized");
    let z = [0.0, 0.0];
    assert_eq!(
        segments_of(&dropped, 0),
        vec![
            seg([10.0, 10.0], z, z),
            seg([50.0, 10.0], z, z),
            seg([50.0, 50.0], z, z),
            seg([10.0, 50.0], z, z),
        ]
    );
}

#[test]
fn a_closing_duplicate_goes_from_the_end_and_the_start_anchor_stays_first() {
    let fonts = read().fonts;
    let layout = layout_of("H", false, &fonts);
    // No Z: the last point equals the first exactly, so the closing curve is a point.
    let outline = vec![
        m(10.0, 10.0),
        l(50.0, 10.0),
        l(50.0, 50.0),
        l(10.0, 50.0),
        l(10.0, 10.0),
    ];
    let port = Constant(Some(outline));
    let error = scene_of(&layout, Some(&port), &[], STRICT).unwrap_err();
    assert!(matches!(
        error,
        TextSceneError::UnsupportedGeometry {
            flag: Some(PathFlag::DegenerateCurve(4)),
            ..
        }
    ));
    let dropped = scene_of(&layout, Some(&port), &[], DROP).unwrap();
    let z = [0.0, 0.0];
    assert_eq!(
        segments_of(&dropped, 0),
        vec![
            seg([10.0, 10.0], z, z),
            seg([50.0, 10.0], z, z),
            seg([50.0, 50.0], z, z),
            seg([10.0, 50.0], z, z),
        ]
    );
}

#[test]
fn removal_carries_the_removed_outgoing_control_as_an_absolute_point() {
    let fonts = read().fonts;
    let layout = layout_of("H", false, &fonts);
    let outline = vec![
        m(0.0, 0.0),
        OutlineCommand::Cubic {
            x1: 10.0,
            y1: 0.0,
            x2: 20.0,
            y2: 10.0,
            x: 30.0,
            y: 30.0,
        },
        l(30.0, 30.0),
        OutlineCommand::Cubic {
            x1: 30.0,
            y1: 40.0,
            x2: 20.0,
            y2: 50.0,
            x: 0.0,
            y: 30.0,
        },
        OutlineCommand::Close,
    ];
    let port = Constant(Some(outline));
    let dropped = scene_of(&layout, Some(&port), &[], DROP).unwrap();
    // The repeated anchor (30, 30) goes; the anchor before it takes its outgoing
    // control (0, 10), so the traced curve to (0, 30) is unchanged.
    assert_eq!(
        segments_of(&dropped, 0),
        vec![
            seg([0.0, 0.0], [0.0, 0.0], [10.0, 0.0]),
            seg([30.0, 30.0], [-10.0, -20.0], [0.0, 10.0]),
            seg([0.0, 30.0], [20.0, 20.0], [0.0, 0.0]),
        ]
    );
}
