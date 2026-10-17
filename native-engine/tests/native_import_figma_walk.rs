//! Tests of `walk_node` against the oracle `W` cases (L10-E, G36).
//!
//! Every case was produced by the verbatim baseline `walkNode` on Figma-shaped nodes. The first test
//! compares ALL of them by encoding production output into the oracle's token form; the pins then
//! name the baseline facts the lane acceptance lists (image fill beats `fillGeometry`, top-most
//! paint, winding groups, text records, skipped messages, invisible nodes) and the additive reports
//! that keep the baseline's silent spots from being silent.

use super::decode::{dec_mat, dec_tree, enc_node_lines, enc_walk_out};
use super::model::{FigmaNode, GeometryRole, UnpaintableReason, UnwalkedCause, WalkOut};
use super::paint::{first_image_paint, mat_identity};
use super::rows::{blocks, Block};
use super::{walk_node, MAX_WALK_DEPTH};

fn case(id: &str) -> Block {
    blocks("W")
        .into_iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("no W case {}", id))
}

fn inverse_of(block: &Block) -> super::model::Mat {
    let token = block
        .lines
        .iter()
        .find_map(|l| l.strip_prefix("inv "))
        .expect("inv line");
    dec_mat(token)
}

fn expected_lines(block: &Block) -> Vec<String> {
    block
        .lines
        .iter()
        .filter(|l| l.starts_with("x "))
        .map(|l| l.to_string())
        .collect()
}

fn run(id: &str) -> (FigmaNode, WalkOut, Vec<String>) {
    let block = case(id);
    let node = dec_tree(&block.lines);
    let mut out = WalkOut::default();
    walk_node(&node, &inverse_of(&block), &mut out);
    (node, out, expected_lines(&block))
}

#[test]
fn oracle_case_count_is_pinned() {
    assert_eq!(blocks("W").len(), 40);
}

#[test]
fn the_decoder_rebuilds_exactly_the_nodes_the_baseline_received() {
    for block in blocks("W") {
        let node = dec_tree(&block.lines);
        let mut again: Vec<String> = Vec::new();
        enc_node_lines(&node, 0, &mut again);
        let original: Vec<String> = block
            .lines
            .iter()
            .filter(|l| l.starts_with("n "))
            .map(|l| l.to_string())
            .collect();
        assert_eq!(again, original, "W {}", block.id);
    }
}

#[test]
fn walk_matches_every_oracle_case() {
    for block in blocks("W") {
        let node = dec_tree(&block.lines);
        let mut out = WalkOut::default();
        walk_node(&node, &inverse_of(&block), &mut out);
        assert_eq!(enc_walk_out(&out), expected_lines(&block), "W {}", block.id);
    }
}

#[test]
fn walking_does_not_touch_the_input_and_is_repeatable() {
    for block in blocks("W") {
        let node = dec_tree(&block.lines);
        let before = node.clone();
        let inv = inverse_of(&block);
        let mut first = WalkOut::default();
        let mut second = WalkOut::default();
        walk_node(&node, &inv, &mut first);
        walk_node(&node, &inv, &mut second);
        assert_eq!(node, before, "W {}", block.id);
        assert_eq!(first, second, "W {}", block.id);
    }
}

#[test]
fn an_image_fill_wins_over_fill_geometry_and_still_emits_its_stroke() {
    let (node, out, lines) = run("w-image-with-geometry-regression");
    assert!(
        !node.fill_geometry.is_empty(),
        "the node carries fillGeometry, as geometry=paths returns it"
    );
    assert_eq!(out.images.len(), 1);
    let image = &out.images[0];
    assert_eq!(image.image_ref, "ref-a");
    assert_eq!((image.w, image.h, image.opacity), (300.0, 200.0, 0.8));
    assert_eq!((image.mat.e, image.mat.f), (100.0, 50.0));
    assert_eq!(out.svg_tags.len(), 1, "only the stroke becomes a tag");
    assert!(out.svg_tags[0].contains("d=\"M0 0L300 0\""));
    assert!(out.svg_tags[0].ends_with("fill=\"rgba(0,0,0,0.8)\"/>"));
    assert!(
        !out.svg_tags.iter().any(|t| t.contains("L300 200")),
        "the fill outline is not drawn"
    );
    assert_eq!(
        lines.len(),
        2,
        "the baseline emitted exactly the stroke tag and the image"
    );
    assert!(out.skipped.is_empty() && out.texts.is_empty());
}

#[test]
fn an_image_fill_is_found_under_a_solid_but_not_when_hidden_or_without_a_ref() {
    let (_, under, _) = run("w-image-under-solid");
    assert_eq!(under.images.len(), 1);
    assert_eq!(under.images[0].image_ref, "ref-d");
    assert!(
        under.svg_tags.is_empty(),
        "the geometry branch is not reached"
    );
    let (_, hidden, _) = run("w-image-hidden-then-solid");
    assert!(hidden.images.is_empty());
    assert_eq!(hidden.svg_tags.len(), 1);
    let (_, refless, _) = run("w-image-without-ref");
    assert!(refless.images.is_empty());
    assert_eq!(refless.svg_tags.len(), 1);
    let (node, _, _) = run("w-image-hidden-then-solid");
    assert!(first_image_paint(&node.fills).is_none());
}

#[test]
fn fill_then_stroke_tags_use_the_top_most_visible_solid_paint() {
    let (_, both, _) = run("w-fill-and-stroke");
    assert_eq!(both.svg_tags.len(), 2);
    assert!(both.svg_tags[0].contains("fill=\"rgba(51,102,153,1)\""));
    assert!(both.svg_tags[1].contains("fill=\"rgba(255,0,0,1)\""));
    let (_, top, _) = run("w-two-fills-top-wins");
    assert_eq!(top.svg_tags.len(), 1);
    assert!(top.svg_tags[0].contains("fill=\"rgba(0,0,255,1)\""));
    let (_, hidden, _) = run("w-hidden-top-fill");
    assert!(hidden.svg_tags[0].contains("fill=\"rgba(255,0,0,1)\""));
    let (_, stroke_only, _) = run("w-stroke-only");
    assert_eq!(stroke_only.svg_tags.len(), 1);
    assert!(stroke_only.svg_tags[0].contains("d=\"M0 0L9 9\""));
}

#[test]
fn winding_rules_are_grouped_in_first_seen_order_and_unknown_rules_are_nonzero() {
    let (_, mixed, _) = run("w-winding-mixed");
    assert_eq!(mixed.svg_tags.len(), 2);
    assert!(
        mixed.svg_tags[0].contains("d=\"M0 0Z M2 2Z M3 3Z\"")
            && mixed.svg_tags[0].contains("fill-rule=\"nonzero\"")
    );
    assert!(
        mixed.svg_tags[1].contains("d=\"M1 1Z\"")
            && mixed.svg_tags[1].contains("fill-rule=\"evenodd\"")
    );
    let (_, eo_first, _) = run("w-winding-evenodd-first");
    assert!(eo_first.svg_tags[0].contains("fill-rule=\"evenodd\""));
    assert!(eo_first.svg_tags[1].contains("fill-rule=\"nonzero\""));
}

#[test]
fn a_text_node_becomes_a_text_record_with_the_baseline_defaults() {
    let (_, text, _) = run("w-text-basic");
    assert_eq!(text.texts.len(), 1);
    let t = &text.texts[0];
    assert_eq!(
        (t.text.as_str(), t.x, t.y, t.w),
        ("Hello", 40.0, 60.0, 120.0)
    );
    assert_eq!(t.color, "rgba(255,128,0,0.5)");
    assert_eq!(t.style.font_family.as_deref(), Some("Poppins"));
    assert_eq!(
        (t.style.font_size, t.style.font_weight, t.style.italic),
        (Some(18.0), Some(700.0), true)
    );
    let (_, bare, _) = run("w-text-defaults");
    let b = &bare.texts[0];
    assert_eq!(
        (b.text.as_str(), b.w, b.color.as_str()),
        ("", 200.0, "rgba(0,0,0,1)")
    );
    let (_, outlined, _) = run("w-text-with-geometry");
    assert!(outlined.texts.is_empty(), "geometry wins over TEXT");
    assert_eq!(outlined.svg_tags.len(), 1);
}

#[test]
fn skipped_messages_name_the_type_and_fall_back_to_a_question_mark() {
    let skipped = |id: &str| run(id).1.skipped;
    assert_eq!(
        skipped("w-childless-frame"),
        vec!["FRAME (Hollow)".to_string()]
    );
    assert_eq!(skipped("w-childless-unnamed"), vec!["STAR (?)".to_string()]);
    assert_eq!(
        skipped("w-childless-empty-name"),
        vec!["ELLIPSE (?)".to_string()]
    );
    assert_eq!(
        skipped("w-childless-untyped"),
        vec!["undefined (Mystery)".to_string()]
    );
    assert!(
        skipped("w-empty-group").is_empty(),
        "an empty GROUP is not reported"
    );
    let (_, mixed, _) = run("w-interleaved-order");
    assert_eq!(mixed.skipped, vec!["LINE (Ln)".to_string()]);
    assert_eq!(
        (mixed.svg_tags.len(), mixed.texts.len(), mixed.images.len()),
        (2, 1, 1)
    );
}

#[test]
fn invisible_nodes_are_ignored_with_their_subtree_but_a_null_flag_is_visible() {
    for id in ["w-invisible-node"] {
        let (_, out, lines) = run(id);
        assert!(lines.is_empty() && out == WalkOut::default(), "{}", id);
    }
    let (_, group, _) = run("w-invisible-child");
    assert_eq!(group.svg_tags.len(), 1);
    assert!(group.svg_tags[0].contains("matrix(1 0 0 1 1 1)"));
    let (node, shown, _) = run("w-visible-null");
    assert_eq!(node.visible, None);
    assert_eq!(shown.svg_tags.len(), 1);
}

#[test]
fn a_geometry_array_without_a_tag_is_reported_never_silent() {
    let (_, gradient, lines) = run("w-gradient-geometry");
    assert!(
        lines.is_empty(),
        "the baseline emitted nothing and said nothing"
    );
    assert_eq!(gradient.unpaintable.len(), 1);
    let u = &gradient.unpaintable[0];
    assert_eq!(
        (u.role, u.reason),
        (GeometryRole::Fill, UnpaintableReason::NoSolidPaint)
    );
    assert_eq!(u.name.as_deref(), Some("Gradient"));
    let (_, stroke, lines) = run("w-gradient-stroke-geometry");
    assert_eq!(
        lines.len(),
        1,
        "the baseline drew the fill and lost the stroke"
    );
    assert_eq!(stroke.svg_tags.len(), 1);
    assert_eq!(stroke.unpaintable.len(), 1);
    assert_eq!(stroke.unpaintable[0].role, GeometryRole::Stroke);
    let (_, empty, lines) = run("w-empty-paths");
    assert!(lines.is_empty());
    assert_eq!(empty.unpaintable[0].reason, UnpaintableReason::EmptyPaths);
    let (_, image_stroke, _) = run("w-image-stroke-gradient");
    assert_eq!(image_stroke.images.len(), 1);
    assert_eq!(image_stroke.unpaintable.len(), 1);
    assert_eq!(image_stroke.unpaintable[0].role, GeometryRole::Stroke);
}

#[test]
fn fully_painted_nodes_produce_no_additive_report() {
    for id in [
        "w-fill-rect",
        "w-fill-and-stroke",
        "w-two-fills-top-wins",
        "w-winding-mixed",
        "w-text-basic",
        "w-group-recursion",
        "w-rotated-nested",
        "w-deep-chain",
    ] {
        let (_, out, _) = run(id);
        assert!(
            out.unpaintable.is_empty() && out.unwalked.is_empty(),
            "{}",
            id
        );
    }
}

#[test]
fn children_the_baseline_never_visits_are_reported_with_their_count() {
    let (_, image_frame, lines) = run("w-image-frame-with-children");
    assert_eq!(image_frame.unwalked.len(), 1);
    assert_eq!(image_frame.unwalked[0].children, 1);
    assert_eq!(image_frame.unwalked[0].cause, UnwalkedCause::ImageFill);
    assert_eq!(
        lines.len(),
        1,
        "only the image line: the child rectangle is absent from the baseline output"
    );
    let (_, panel, lines) = run("w-filled-frame-with-children");
    assert_eq!(panel.unwalked[0].cause, UnwalkedCause::GeometryLeaf);
    assert_eq!(panel.unwalked[0].children, 1);
    assert_eq!(
        lines.len(),
        1,
        "the child 'Lost' never reaches the baseline output"
    );
    let (_, union, _) = run("w-boolean-op");
    assert_eq!(
        (union.unwalked[0].cause, union.unwalked[0].children),
        (UnwalkedCause::GeometryLeaf, 2)
    );
    assert_eq!(
        union.unwalked[0].node_type.as_deref(),
        Some("BOOLEAN_OPERATION")
    );
}

#[test]
fn a_container_chain_deeper_than_the_guard_is_reported_not_followed() {
    let mut node = FigmaNode {
        node_type: Some("RECTANGLE".to_string()),
        name: Some("Bottom".to_string()),
        ..FigmaNode::default()
    };
    for level in 0..(MAX_WALK_DEPTH + 88) {
        node = FigmaNode {
            node_type: Some("GROUP".to_string()),
            name: Some(format!("G{}", level)),
            children: vec![node],
            ..FigmaNode::default()
        };
    }
    let mut out = WalkOut::default();
    walk_node(&node, &mat_identity(), &mut out);
    assert_eq!(out.unwalked.len(), 1);
    assert_eq!(out.unwalked[0].cause, UnwalkedCause::DepthLimit);
    assert_eq!(out.unwalked[0].children, 1);
    assert!(
        out.skipped.is_empty(),
        "the bottom rectangle was never reached"
    );
}
