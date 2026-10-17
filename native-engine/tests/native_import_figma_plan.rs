//! Tests of frame selection and the per-frame plan against the oracle `V` flows (L10-E, G36).
//!
//! Every `V` flow ran the verbatim baseline `convertFileJson` (with recording stubs for the SVG
//! import, vector text and Raster providers, all succeeding) and recorded the layers, canvas
//! resizes, SVG texts, text and image requests, skipped messages, counts and the toast. The first
//! tests compare ALL flows; the pins then name the decisions the lane acceptance lists. Literal
//! values asserted here are checked against the oracle block they come from.

use super::super::decode::{dec_input, dec_tree, enc_node_lines, enc_plan};
use super::super::flows::{flow, x_lines};
use super::super::rows::{blocks, singles, unescape};
use super::{
    plan_import, select_frames, summary_message, svg_document, FigmaImportPlan, ImportSource,
    MissingProvider, Providers, SelectionRefusal, UnavailableRow,
};

fn layer_names(plan: &FigmaImportPlan) -> Vec<&str> {
    plan.frames.iter().map(|f| f.layer_name.as_str()).collect()
}

#[test]
fn oracle_flow_and_summary_counts_are_pinned() {
    assert_eq!(blocks("V").len(), 34);
    assert_eq!(singles("S").len(), 5);
}

#[test]
fn the_decoder_rebuilds_exactly_the_documents_the_baseline_received() {
    for block in blocks("V") {
        if block.lines.iter().any(|l| l.starts_with("cfg mode=nodoc")) {
            continue;
        }
        let tree = dec_tree(&block.lines);
        let mut again: Vec<String> = Vec::new();
        enc_node_lines(&tree, 0, &mut again);
        let original: Vec<String> = block
            .lines
            .iter()
            .filter(|l| l.starts_with("n "))
            .map(|l| l.to_string())
            .collect();
        assert_eq!(again, original, "V {}", block.id);
    }
}

#[test]
fn every_baseline_flow_is_reproduced() {
    for block in blocks("V") {
        let plan = plan_import(&dec_input(&block.lines));
        assert_eq!(enc_plan(&plan), x_lines(&block), "V {}", block.id);
    }
}

#[test]
fn every_selected_frame_is_planned_or_listed_as_invisible() {
    for block in blocks("V") {
        let input = dec_input(&block.lines);
        let plan = plan_import(&input);
        match select_frames(&input.source, &input.options) {
            Ok(frames) => {
                assert!(plan.refusal.is_none(), "V {}", block.id);
                assert_eq!(
                    plan.frames.len() + plan.invisible_frames.len(),
                    frames.len(),
                    "V {}",
                    block.id
                );
                let mut seen: Vec<usize> = plan.frames.iter().map(|f| f.frame_index).collect();
                seen.extend(plan.invisible_frames.iter().copied());
                seen.sort_unstable();
                assert_eq!(
                    seen,
                    (0..frames.len()).collect::<Vec<_>>(),
                    "V {}",
                    block.id
                );
            }
            Err(refusal) => assert_eq!(plan.refusal, Some(refusal), "V {}", block.id),
        }
    }
}

#[test]
fn the_three_refusal_messages_are_the_baseline_texts_verbatim() {
    let skip_token = |id: &str| {
        x_lines(&flow(id).0)
            .iter()
            .find_map(|l| l.strip_prefix("x skip ").map(|t| unescape(t)))
            .expect("skip line")
    };
    let cases = [
        ("doc-empty-children", SelectionRefusal::EmptyDocument, "empty document"),
        ("doc-missing", SelectionRefusal::EmptyDocument, "empty document"),
        ("doc-page-without-frames", SelectionRefusal::NoFrameOnPage, "no FRAME node found on this page"),
        ("doc-frame-names-none-match", SelectionRefusal::NoFrameOnPage, "no FRAME node found on this page"),
        ("scoped-no-frames", SelectionRefusal::NoArtboardInSelection, "La s\u{e9}lection Figma ne contient pas d\u{2019}artboard (frame) \u{2014} s\u{e9}lectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien"),
        ("scoped-bare-group", SelectionRefusal::NoArtboardInSelection, "La s\u{e9}lection Figma ne contient pas d\u{2019}artboard (frame) \u{2014} s\u{e9}lectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien"),
    ];
    for (id, refusal, text) in cases {
        let (_, plan) = flow(id);
        assert_eq!(plan.refusal, Some(refusal), "{}", id);
        assert_eq!(refusal.message(), text, "{}", id);
        assert_eq!(skip_token(id), text, "oracle text of {}", id);
        assert!(plan.frames.is_empty() && plan.invisible_frames.is_empty());
        assert_eq!(plan.skipped(), vec![text.to_string()]);
    }
}

#[test]
fn frame_names_keep_document_order_and_pages_fall_back_to_the_first() {
    let (_, named) = flow("doc-frame-names");
    assert_eq!(
        layer_names(&named),
        vec!["A", "C"],
        "the filter keeps document order, not list order"
    );
    for id in [
        "doc-page-index-out-of-range",
        "doc-page-index-fraction",
        "doc-page-index-negative",
    ] {
        assert_eq!(layer_names(&flow(id).1), vec!["A"], "{}", id);
    }
    assert_eq!(layer_names(&flow("doc-page-index-1").1), vec!["B", "C"]);
    let (_, empty_list) = flow("doc-frame-names-empty-list");
    assert_eq!(
        layer_names(&empty_list),
        vec!["A"],
        "an empty list is no filter"
    );
}

#[test]
fn a_scoped_selection_is_a_frame_or_the_frames_of_a_container() {
    assert_eq!(layer_names(&flow("scoped-frame").1), vec!["Picked"]);
    assert_eq!(layer_names(&flow("scoped-section").1), vec!["F1", "F2"]);
    let (block, ignoring) = flow("scoped-ignores-names");
    assert_eq!(layer_names(&ignoring), vec!["Picked"]);
    assert!(
        block
            .lines
            .iter()
            .any(|l| l.starts_with("cfg mode=scoped") && l.contains("fn=other")),
        "names and page index are given and ignored"
    );
    let (_, off) = flow("scoped-invisible-frame");
    assert!(off.refusal.is_none() && off.frames.is_empty());
    assert_eq!(off.invisible_frames, vec![0]);
    assert!(
        off.skipped().is_empty(),
        "the baseline says nothing about an invisible frame"
    );
}

#[test]
fn an_invisible_first_frame_means_no_canvas_resize_at_all() {
    let (block, plan) = flow("doc-first-frame-invisible");
    assert_eq!(plan.invisible_frames, vec![0]);
    assert_eq!(plan.frames.len(), 1);
    assert_eq!(plan.frames[0].frame_index, 1);
    assert_eq!(plan.frames[0].canvas_size, None);
    assert!(!block.lines.iter().any(|l| l.starts_with("x canvas")));
}

#[test]
fn layer_names_and_canvas_sizes_follow_the_baseline() {
    let (_, unnamed) = flow("doc-unnamed-frames");
    assert_eq!(layer_names(&unnamed), vec!["Figma 1", "Figma 2"]);
    assert_eq!(unnamed.frames[0].canvas_size, Some([10.0, 10.0]));
    assert_eq!(
        unnamed.frames[1].canvas_size, None,
        "only selection index 0 resizes the canvas"
    );
    let (_, fraction) = flow("doc-frame-fraction-size");
    assert_eq!(
        fraction.frames[0].canvas_size,
        Some([101.0, 50.0]),
        "Math.round(100.5) and Math.round(49.5)"
    );
    assert!(fraction.frames[0]
        .svg
        .as_deref()
        .unwrap()
        .contains("width=\"101\" height=\"50\""));
    let (_, flat) = flow("doc-frame-zero-size");
    assert_eq!(flat.frames[0].canvas_size, None);
    assert!(flat.frames[0]
        .svg
        .as_deref()
        .unwrap()
        .contains("width=\"0\" height=\"100\""));
    let (_, no_box) = flow("doc-frame-no-bbox");
    assert_eq!(no_box.frames[0].canvas_size, None);
    assert!(
        no_box.frames[0]
            .svg
            .as_deref()
            .unwrap()
            .contains("width=\"1920\" height=\"1080\""),
        "state.canvasW/H fallback"
    );
}

#[test]
fn the_svg_document_wraps_the_tags_in_the_rounded_frame_box() {
    let tags = vec!["<a/>".to_string(), "<b/>".to_string()];
    assert_eq!(
        svg_document(None, 1920.0, 1080.0, &tags),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\"><a/><b/></svg>"
    );
    let bb = super::super::model::BBox {
        x: 0.0,
        y: 0.0,
        width: 100.5,
        height: 49.4,
    };
    assert_eq!(
        svg_document(Some(&bb), 1.0, 1.0, &tags[..1]),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"101\" height=\"49\"><a/></svg>"
    );
}

#[test]
fn requests_without_a_provider_are_listed_never_dropped() {
    let (_, plan) = flow("doc-one-frame");
    let both_missing = Providers {
        text_assets: false,
        raster_assets: false,
    };
    assert_eq!(
        plan.unavailable(&both_missing),
        vec![
            UnavailableRow {
                frame_index: 0,
                provider: MissingProvider::TextAssets,
                label: "Welcome".to_string()
            },
            UnavailableRow {
                frame_index: 0,
                provider: MissingProvider::RasterAssets,
                label: "hero-ref".to_string()
            },
        ]
    );
    let all_present = Providers {
        text_assets: true,
        raster_assets: true,
    };
    assert!(plan.unavailable(&all_present).is_empty());
    let only_rasters = Providers {
        text_assets: false,
        raster_assets: true,
    };
    assert_eq!(plan.unavailable(&only_rasters).len(), 1);
    let (_, two) = flow("doc-images-two-frames");
    assert_eq!(
        two.unavailable(&both_missing).len(),
        two.text_total() + two.image_total()
    );
    assert_eq!(two.image_total(), 3);
}

#[test]
fn provider_failures_slot_into_the_baseline_order_of_the_skipped_report() {
    let (_, plan) = flow("doc-skipped-everywhere");
    assert_eq!(
        plan.frames[0].skipped_report(&[], &[]),
        vec!["LINE (L)".to_string()]
    );
    let with_text = plan.frames[0].skipped_report(&["TEXT (T): boom".to_string()], &[]);
    assert_eq!(
        with_text,
        vec!["LINE (L)".to_string(), "TEXT (T): boom".to_string()]
    );
    let (_, images) = flow("doc-images-two-frames");
    let failed =
        images.frames[0].skipped_report(&[], &[(1, "IMAGE fill (Two): HTTP 404".to_string())]);
    assert_eq!(failed, vec!["IMAGE fill (Two): HTTP 404".to_string()]);
    let (_, missing) = flow("doc-unnamed-image-miss");
    let mixed = missing.frames[0].skipped_report(
        &["TEXT (x): bad".to_string()],
        &[(0, "never used".to_string())],
    );
    assert_eq!(mixed[0], "TEXT (x): bad");
    assert_eq!(
        mixed.len(),
        3,
        "a failure addressed to a missing slot is not invented"
    );
}

#[test]
fn baseline_silent_spots_are_reported_additively() {
    let (block, plan) = flow("doc-skipped-everywhere");
    assert_eq!(
        plan.frames[0].unpaintable.len(),
        1,
        "the gradient-filled rectangle"
    );
    assert_eq!(plan.frames[0].unpaintable[0].name.as_deref(), Some("Grad"));
    assert!(
        !x_lines(&block).iter().any(|l| l.contains("Grad")),
        "the baseline never mentioned it"
    );
}

#[test]
fn the_toast_matches_the_baseline_for_every_summary_row() {
    for row in singles("S") {
        let n: Vec<usize> = row[..5].iter().map(|t| t.parse().expect("count")).collect();
        assert_eq!(
            summary_message(n[0], n[1], n[2], n[3], n[4]),
            unescape(row[5]),
            "S {:?}",
            &row[..5]
        );
    }
    assert_eq!(
        summary_message(1, 2, 3, 4, 0),
        "1 frame(s), 2 forme(s), 3 texte(s), 4 image(s) import\u{e9}(s)"
    );
    assert_eq!(
        summary_message(1, 2, 3, 4, 7),
        "1 frame(s), 2 forme(s), 3 texte(s), 4 image(s) import\u{e9}(s) \u{2014} 7 \u{e9}l\u{e9}ment(s) ignor\u{e9}(s) (voir console)"
    );
}

#[test]
fn a_missing_document_and_a_childless_one_are_both_empty() {
    for source in [
        ImportSource::File(None),
        ImportSource::File(Some(Default::default())),
    ] {
        let options = Default::default();
        assert_eq!(
            select_frames(&source, &options).err(),
            Some(SelectionRefusal::EmptyDocument)
        );
    }
}
