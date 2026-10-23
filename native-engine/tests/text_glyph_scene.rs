//! Oracle conformance of the text scene join (G38b / L06-K): the real L06-A
//! layout, the real L06-B contour kernel and the real D30 chain, fed by the
//! recorded `glyph.getPath` calls of the real baseline, must reproduce the real
//! baseline's final paths (Roboto, DejaVu, Inter and Manrope glyphs, holes, marks,
//! italic, decorations, wrapping, element/layer/parent stages) under each
//! admission policy.

use super::oracle::{
    glyph_count, lay_out, read, Case, ExpectedPath, FontTable, Replay, Strict, ORACLE,
    TRIG_TOLERANCE,
};
use super::{
    prepare_text_glyph_scene, GeometryAdmission, SceneContour, SceneRole, TextGlyphScene,
    TextSceneError, TextSceneInput,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::DecorationKind;
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{GlyphSegment, PathFlag};
use crate::request_receipts::sha256;

pub(super) const DEGENERATE: &str =
    "cubic segment is degenerate at Vello's local encoding precision";
pub(super) const COUNT: &str = "closed cubic paths require 2..=256 segments";

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn run(
    case: &Case,
    fonts: &FontTable,
    admission: GeometryAdmission,
) -> (Result<TextGlyphScene, TextSceneError>, usize) {
    let layout = lay_out(case, fonts);
    let stages = case.glyph_stages(glyph_count(&layout));
    let chain = case.chain();
    let replay = Replay::new(&case.placed);
    let input = TextSceneInput {
        layout: &layout,
        outlines: Some(&replay),
        glyph_stages: &stages,
        chain: &chain,
        item_opacity: case.item_opacity,
        admission,
    };
    let result = prepare_text_glyph_scene(&input);
    (result, replay.calls.get())
}

fn close(got: f64, want: f64, trig: bool) -> bool {
    if trig {
        (got - want).abs() <= TRIG_TOLERANCE * got.abs().max(want.abs()).max(1.0)
    } else {
        got == want
    }
}

pub(super) fn same_segments(contour: &SceneContour, want: &[GlyphSegment], trig: bool) -> bool {
    let got = contour.contour.segments();
    got.len() == want.len()
        && got.iter().zip(want).all(|(g, w)| {
            (0..2).all(|k| {
                close(g.point[k], w.point[k], trig)
                    && close(g.handle_in[k], w.handle_in[k], trig)
                    && close(g.handle_out[k], w.handle_out[k], trig)
            })
        })
}

pub(super) fn role_of(path: &ExpectedPath) -> SceneRole {
    match path.decoration_kind() {
        Some(kind) => SceneRole::Decoration {
            line_index: path.line,
            kind,
        },
        None => SceneRole::Glyph {
            ordinal: path.ordinal.expect("glyph ordinal"),
            path_index: path.path_index.expect("glyph path index"),
        },
    }
}

#[test]
fn the_oracle_is_the_pinned_real_baseline_run() {
    assert!(ORACLE.contains("at fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert_eq!(
        hex(sha256(ORACLE.as_bytes())),
        "77779679ede6bf435cbe887a67920e715f2c85dd5fbc7f818ab6bca293816955"
    );
    let oracle = read();
    assert_eq!(oracle.cases.len(), 12);
    assert_eq!(oracle.fonts.faces.len(), 4);
    let paths: usize = oracle.cases.iter().map(|c| c.paths.len()).sum();
    assert_eq!(paths, 148);
    let decorations = oracle
        .cases
        .iter()
        .flat_map(|c| &c.paths)
        .filter(|p| p.kind != 'G')
        .count();
    assert_eq!((decorations, paths - decorations), (7, 141));
    let clean = oracle
        .cases
        .iter()
        .filter(|c| c.strict == Strict::Ok)
        .count();
    assert_eq!(clean, 3);
    let dropped: usize = oracle.cases.iter().map(|c| c.dropped.len()).sum();
    assert_eq!(dropped, 38);
    assert_eq!(oracle.cases.iter().filter(|c| c.trig).count(), 1);
    let placed: usize = oracle.cases.iter().map(|c| c.placed.len()).sum();
    assert_eq!(placed, 78);
    assert_eq!(
        oracle
            .cases
            .iter()
            .filter(|c| !c.element.is_empty())
            .count(),
        2
    );
}

#[test]
fn the_layout_places_every_glyph_where_the_baseline_asked_for_its_outline() {
    let oracle = read();
    for case in &oracle.cases {
        let (result, calls) = run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves);
        result.unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
        // One getPath per placed glyph, every one found by exact origin and size.
        assert_eq!(calls, case.placed.len(), "{}", case.name);
    }
}

#[test]
fn strict_admission_reproduces_the_baseline_or_refuses_the_first_inadmissible_path() {
    let oracle = read();
    let mut admitted = 0;
    let mut refused = 0;
    for case in &oracle.cases {
        let (result, _) = run(case, &oracle.fonts, GeometryAdmission::Strict);
        match (&case.strict, result) {
            (Strict::Ok, Ok(scene)) => {
                admitted += 1;
                assert_eq!(scene.contours().len(), case.paths.len(), "{}", case.name);
                for (contour, want) in scene.contours().iter().zip(&case.paths) {
                    assert_eq!(contour.role, role_of(want), "{}", case.name);
                    assert!(
                        same_segments(contour, &want.segments, case.trig),
                        "{} {:?}",
                        case.name,
                        contour.role
                    );
                    assert_eq!(contour.opacity, want.opacity, "{}", case.name);
                    assert_eq!(
                        contour.envelope,
                        control_bounds(&want.segments),
                        "{}",
                        case.name
                    );
                }
            }
            (Strict::Refuse { position, why }, Err(error)) => {
                refused += 1;
                let TextSceneError::UnsupportedGeometry { role, flag, reason } = error else {
                    panic!("{}: unexpected {error:?}", case.name);
                };
                assert_eq!(role, role_of(&case.paths[*position]), "{}", case.name);
                match why.as_str() {
                    "degenerate" => {
                        assert_eq!(reason, DEGENERATE, "{}", case.name);
                        assert!(matches!(flag, Some(PathFlag::DegenerateCurve(_))));
                    }
                    "count" => {
                        assert_eq!(reason, COUNT);
                        assert!(matches!(flag, Some(PathFlag::SegmentCount(_))));
                    }
                    other => panic!("unknown verdict {other}"),
                }
            }
            (want, got) => panic!("{}: oracle {want:?}, native {got:?}", case.name),
        }
    }
    assert_eq!((admitted, refused), (3, 9));
}

/// Independent envelope: the box of every anchor and absolute control.
fn control_bounds(segments: &[GlyphSegment]) -> [f64; 4] {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for s in segments {
        for h in [[0.0, 0.0], s.handle_in, s.handle_out] {
            let p = [s.point[0] + h[0], s.point[1] + h[1]];
            bounds = [
                bounds[0].min(p[0]),
                bounds[1].min(p[1]),
                bounds[2].max(p[0]),
                bounds[3].max(p[1]),
            ];
        }
    }
    bounds
}

#[test]
fn drop_coincident_admission_reproduces_the_oracle_and_changes_only_degenerate_paths() {
    let oracle = read();
    let mut changed = 0;
    for case in &oracle.cases {
        let (result, _) = run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves);
        let scene = result.unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
        assert_eq!(scene.contours().len(), case.paths.len(), "{}", case.name);
        for (position, (contour, want)) in scene.contours().iter().zip(&case.paths).enumerate() {
            assert_eq!(contour.role, role_of(want), "{}", case.name);
            let expected = match case.dropped.iter().find(|(p, _)| *p == position) {
                Some((_, segments)) => {
                    changed += 1;
                    assert!(segments.len() < want.segments.len());
                    segments
                }
                None => &want.segments,
            };
            assert!(
                same_segments(contour, expected, case.trig),
                "{} path {position}",
                case.name
            );
            assert_eq!(contour.opacity, want.opacity, "{}", case.name);
        }
    }
    assert_eq!(changed, 38);
}

#[test]
fn paint_order_keys_and_roles_follow_the_layout() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "lines_decorations")
        .expect("decoration case");
    let (result, _) = run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves);
    let scene = result.expect("admitted");
    assert_eq!(scene.holder_id(), "oracle/lines_decorations");
    let keys: Vec<&str> = scene.contours().iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys[0], "oracle/lines_decorations/line/0/underline");
    assert_eq!(keys[1], "oracle/lines_decorations/line/0/strike");
    assert_eq!(keys[2], "oracle/lines_decorations/glyph/0/path/0");
    // Line 1 starts with its own decorations before its glyphs.
    let line1 = scene
        .contours()
        .iter()
        .position(|c| {
            c.role
                == SceneRole::Decoration {
                    line_index: 1,
                    kind: DecorationKind::Underline,
                }
        })
        .expect("line 1 underline");
    assert!(matches!(
        scene.contours()[line1 + 2].role,
        SceneRole::Glyph { .. }
    ));
    let mut unique = keys.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        keys.len(),
        "every contour has its own identity"
    );
}

#[test]
fn the_same_input_yields_the_same_contours_for_every_consumer() {
    let oracle = read();
    for case in &oracle.cases {
        let (first, _) = run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves);
        let (second, _) = run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves);
        let (first, second) = (first.unwrap(), second.unwrap());
        assert_eq!(first, second, "{}", case.name);
        for (a, b) in first.contours().iter().zip(second.contours()) {
            assert_eq!(a.contour.commands(), b.contour.commands());
            assert_eq!(a.envelope, b.envelope);
        }
    }
}
