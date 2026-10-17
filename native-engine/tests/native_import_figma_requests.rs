//! Tests of the text and image requests against the oracle `V` flows (L10-E, G36).
//! The flows ran the verbatim baseline `convertFileJson` with succeeding stub providers; every
//! request value asserted here is also visible in the flow's `x text` / `x raster` / `x skip` lines.

use super::super::flows::{block_has, flow};
use super::super::rows::escape;
use super::ImageSlot;

#[test]
fn image_fills_are_requested_only_with_an_image_map_entry() {
    let (_, hero) = flow("doc-one-frame");
    match &hero.frames[0].image_slots[0] {
        ImageSlot::Request(r) => {
            assert_eq!(
                (r.image_ref.as_str(), r.source_url.as_str()),
                ("hero-ref", "https://img/hero.png")
            );
            assert_eq!(
                (r.size, r.position, r.opacity),
                ([150.0, 100.0], [275.0, 60.0], 0.9)
            );
        }
        other => panic!("expected a request, got {:?}", other),
    }
    let summary = "1 image fill(s) skipped \u{2014} pass opts.imageMap (GET /v1/files/:key/images) to import them";
    let (block, absent) = flow("doc-no-image-map");
    assert_eq!(
        absent.frames[0].image_summary_skip.as_deref(),
        Some(summary)
    );
    assert!(absent.frames[0].image_slots.is_empty());
    assert!(block_has(&block, &format!("x skip {}", escape(summary))));
    for (id, text) in [
        (
            "doc-image-map-miss",
            "IMAGE fill (Hero): no imageMap entry for hero-ref",
        ),
        (
            "doc-image-map-empty-url",
            "IMAGE fill (Hero): no imageMap entry for hero-ref",
        ),
        (
            "doc-empty-image-map",
            "IMAGE fill (Hero): no imageMap entry for hero-ref",
        ),
    ] {
        let (block, plan) = flow(id);
        assert!(
            matches!(plan.frames[0].image_slots[0], ImageSlot::Missing { .. }),
            "{}",
            id
        );
        assert!(plan.frames[0].image_summary_skip.is_none(), "{}", id);
        assert_eq!(plan.skipped(), vec![text.to_string()], "{}", id);
        assert!(
            block_has(&block, &format!("x skip {}", escape(text))),
            "{}",
            id
        );
    }
    let (block, unnamed) = flow("doc-unnamed-image-miss");
    assert_eq!(
        unnamed.skipped(),
        vec![
            "IMAGE fill (undefined): no imageMap entry for rq".to_string(),
            "IMAGE fill (): no imageMap entry for rq2".to_string()
        ]
    );
    for line in unnamed.skipped() {
        assert!(block_has(&block, &format!("x skip {}", escape(&line))));
    }
}

#[test]
fn image_sizes_are_at_least_one_and_positions_use_only_the_translation() {
    let (_, tiny) = flow("doc-image-negative-size");
    match &tiny.frames[0].image_slots[0] {
        ImageSlot::Request(r) => {
            assert_eq!(r.size, [1.0, 1.0], "Math.max(1, -4) and Math.max(1, 0.2)");
            assert_eq!(r.position, [3.0, 5.1]);
        }
        other => panic!("expected a request, got {:?}", other),
    }
    let (_, stroked) = flow("doc-stroked-image");
    assert_eq!(
        stroked.frames[0].svg_tag_count, 1,
        "the stroke of an image-filled node still reaches the SVG path"
    );
    assert_eq!(stroked.image_total(), 1);
}

#[test]
fn text_requests_carry_the_baseline_defaults_and_empty_texts_are_reported() {
    let (block, plan) = flow("doc-text-variants");
    let frame = &plan.frames[0];
    assert_eq!(frame.empty_texts, vec![Some("Empty".to_string())]);
    assert!(
        block.lines.iter().any(|l| *l == "x counts 1 0 6 0"),
        "the baseline imported six of the seven texts"
    );
    let by = |t: &str| {
        frame
            .texts
            .iter()
            .find(|r| r.text == t)
            .expect("text exists")
    };
    let plain = by("plain");
    assert_eq!(
        (
            plain.size,
            plain.align,
            plain.width,
            plain.bold,
            plain.italic
        ),
        (24.0, "left", Some(10.0), false, false)
    );
    assert_eq!((plain.line_height_mult, plain.letter_spacing), (None, 0.0));
    let justified = by("j");
    assert_eq!(
        (
            justified.size,
            justified.align,
            justified.width,
            justified.bold
        ),
        (24.0, "left", None, false),
        "JUSTIFIED, size 0, width 0 and weight 699"
    );
    assert_eq!(justified.line_height_mult, None, "lineHeightPx 0 is falsy");
    let weights = by("w");
    assert_eq!(weights.font_family.as_deref(), Some("  Space Grotesk  "));
    assert_eq!(
        (weights.size, weights.bold, weights.italic, weights.width),
        (12.0, true, false, Some(33.0))
    );
    assert_eq!(
        (weights.line_height_mult, weights.letter_spacing),
        (Some(1.5), -0.25)
    );
    assert_eq!(by("b").align, "left", "an unknown alignment is left");
    assert_eq!(
        by("nb").width,
        Some(200.0),
        "no bounding box means width 200"
    );
    let undefined = by("u");
    assert_eq!(
        (
            undefined.font_family.clone(),
            undefined.size,
            undefined.italic
        ),
        (None, 7.0, true)
    );
}
