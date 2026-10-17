//! Mutation controls of the text scene join (G38b / L06-K): each plausible
//! variant of an ordering or rounding rule must fail the baseline oracle while
//! production equals it, and the join must not mutate what it reads.

use super::controls::{layout_of, scene_of, seg, stage, Constant, DROP, STRICT};
use super::geometry::decoration_segments;
use super::oracle::{lay_out, read, Replay, TRIG_TOLERANCE};
use super::tests::{role_of, run, same_segments};
use super::{prepare_text_glyph_scene, SceneRole, TextSceneError, TextSceneInput};
use crate::evaluation::motion_parent_chain::{apply_chain_to_segments, ChainEntry, Segment};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::ITALIC_SHEAR;
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    glyph_paths, shear_segments, GlyphSegment, ItalicShear,
};

#[test]
fn italic_is_composed_before_the_chain_and_the_reverse_order_fails_the_oracle() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "scale_italic_parents")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let glyph = layout.lines[0].glyphs[0];
    let commands = case
        .placed
        .iter()
        .find(|p| p.unit == glyph.unit && p.x == glyph.origin_x)
        .unwrap()
        .commands
        .clone();
    // Production equals the oracle (path 0 is the 'A' outline).
    let (result, _) = run(case, &oracle.fonts, DROP);
    let scene = result.unwrap();
    let dropped_zero = case.dropped.iter().find(|(p, _)| *p == 0);
    let want = match dropped_zero {
        Some((_, segments)) => segments.clone(),
        None => case.paths[0].segments.clone(),
    };
    assert!(same_segments(&scene.contours()[0], &want, false));
    // Mutation: shear after the chain (the order a careless join would pick).
    let plain = glyph_paths(&commands, None).unwrap();
    let stored: Vec<Segment> = plain[0]
        .segments
        .iter()
        .map(|s| Segment {
            point: s.point,
            handle_in: Some(s.handle_in),
            handle_out: Some(s.handle_out),
        })
        .collect();
    let moved = apply_chain_to_segments(&stored, &case.chain());
    let ring: Vec<GlyphSegment> = moved
        .iter()
        .map(|s| seg(s.point, s.handle_in.unwrap(), s.handle_out.unwrap()))
        .collect();
    let late = shear_segments(
        &ring,
        &ItalicShear::new(ITALIC_SHEAR, glyph.baseline_y).unwrap(),
    );
    let differs = late.iter().zip(&case.paths[0].segments).any(|(a, b)| {
        (a.point[0] - b.point[0]).abs() > 1e-3 || (a.point[1] - b.point[1]).abs() > 1e-3
    });
    assert!(differs, "shear-after-chain must not equal the baseline");
}

#[test]
fn opacity_is_the_sequential_product_and_an_aggregated_multiplier_would_not_match() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "opacity_association")
        .unwrap();
    let (result, _) = run(case, &oracle.fonts, STRICT);
    let scene = result.expect("the opacity case is strict-clean");
    let mut aggregated_differs = 0;
    for (contour, want) in scene.contours().iter().zip(&case.paths) {
        assert_eq!(contour.opacity, want.opacity);
        let mut stages: Vec<ChainEntry> = Vec::new();
        if let SceneRole::Glyph { ordinal, .. } = role_of(want) {
            stages.extend(
                case.element
                    .iter()
                    .filter(|(o, _)| *o == ordinal)
                    .map(|(_, e)| *e),
            );
        }
        stages.extend(case.chain());
        let product = stages.iter().fold(1.0, |acc, e| acc * e.mat.op);
        if case.item_opacity * product != want.opacity {
            aggregated_differs += 1;
        }
    }
    assert_eq!(aggregated_differs, 2);
}

#[test]
fn the_decoration_rectangle_keeps_paper_corner_order_and_the_half_thickness_shift() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "underline_only_center_fw")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let decoration = layout.lines[0].decorations[0];
    let want = &case.paths[0].segments;
    assert_eq!(decoration_segments(&decoration), *want);
    // Mutation 1: Paper's corner order is bl, tl, tr, br; any other ring order is wrong.
    let mut rotated = want.clone();
    rotated.rotate_left(1);
    assert_ne!(rotated, *want);
    // Mutation 2: treating centerY as the top edge misses by half the thickness.
    let mut high = decoration;
    high.center_y += decoration.thickness / 2.0;
    assert_ne!(decoration_segments(&high), *want);
}

#[test]
fn the_trig_tolerance_still_rejects_a_wrong_rotation_sign() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "rotated_trig")
        .unwrap();
    assert!(case.trig && TRIG_TOLERANCE <= 1e-9);
    let layout = lay_out(case, &oracle.fonts);
    let mut chain = case.chain();
    chain[0].mat.rot = -chain[0].mat.rot;
    let stages = case.glyph_stages(layout.lines.iter().map(|l| l.glyphs.len()).sum());
    let port = Replay::new(&case.placed);
    let scene = prepare_text_glyph_scene(&TextSceneInput {
        layout: &layout,
        outlines: Some(&port),
        glyph_stages: &stages,
        chain: &chain,
        item_opacity: case.item_opacity,
        admission: DROP,
    })
    .unwrap();
    let differing = scene
        .contours()
        .iter()
        .zip(&case.paths)
        .filter(|(contour, want)| {
            let expected = match case
                .dropped
                .iter()
                .find(|(p, _)| role_of(&case.paths[*p]) == contour.role)
            {
                Some((_, segments)) => segments,
                None => &want.segments,
            };
            !same_segments(contour, expected, true)
        })
        .count();
    assert!(differing > 0, "a flipped rotation must leave the tolerance");
}

#[test]
fn the_join_never_mutates_what_it_reads() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "lines_decorations")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let before_layout = layout.clone();
    let before_placed = format!("{:?}", case.placed);
    let chain = case.chain();
    let before_chain = chain.clone();
    let stages = case.glyph_stages(layout.lines.iter().map(|l| l.glyphs.len()).sum());
    let before_stages = stages.clone();
    let port = Replay::new(&case.placed);
    let first = prepare_text_glyph_scene(&TextSceneInput {
        layout: &layout,
        outlines: Some(&port),
        glyph_stages: &stages,
        chain: &chain,
        item_opacity: 1.0,
        admission: DROP,
    })
    .unwrap();
    assert_eq!(layout, before_layout);
    assert_eq!(format!("{:?}", case.placed), before_placed);
    assert_eq!(chain, before_chain);
    assert_eq!(stages, before_stages);
    // The output is detached: later preparation of other input leaves it alone.
    let snapshot = first.clone();
    let _ = scene_of(
        &layout_of("H", false, &oracle.fonts),
        Some(&Constant(None)),
        &[],
        STRICT,
    );
    assert_eq!(first, snapshot);
}

#[test]
fn non_finite_opacity_is_refused_before_any_geometry() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "plain_hello")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let port = Replay::new(&case.placed);
    let build = |item_opacity: f64, chain: &[ChainEntry]| {
        prepare_text_glyph_scene(&TextSceneInput {
            layout: &layout,
            outlines: Some(&port),
            glyph_stages: &[],
            chain,
            item_opacity,
            admission: DROP,
        })
    };
    assert_eq!(
        build(f64::NAN, &[]).unwrap_err(),
        TextSceneError::InvalidOpacity
    );
    assert_eq!(
        build(f64::INFINITY, &[]).unwrap_err(),
        TextSceneError::InvalidOpacity
    );
    assert_eq!(
        port.calls.get(),
        0,
        "no outline is requested before the opacity check"
    );
    // An overflowing stage product is caught on the finished contour, never published.
    assert_eq!(
        build(1e308, &[stage(1.0, 1.0, 0.0, 1e308)]).unwrap_err(),
        TextSceneError::InvalidOpacity
    );
}
