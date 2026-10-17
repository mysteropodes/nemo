//! Tests of the drop classification and batch plan (L10-C, G37). Expected values
//! are literals; each is also tied to the baseline oracle by looking the exact row
//! up in `oracle.txt` (generated from fbe4b8a3, see `native_import_drop_route_rows.rs`).

use super::rows::{
    drop_line, f, file_of, groups_of, oracle_has, oracle_rows, run, split_arrow, PRODUCTION,
};
use super::*;

fn labels(file: &DropFile) -> String {
    let m = memberships(file);
    DropGroup::EXECUTION_ORDER
        .iter()
        .filter(|g| m.has(**g))
        .map(|g| g.label())
        .collect::<Vec<_>>()
        .join("+")
}

#[test]
fn oracle_row_counts_are_pinned() {
    let counts = [
        ('S', 126),
        ('D', 10),
        ('G', 617),
        ('H', 20),
        ('L', 46),
        ('W', 1),
        ('P', 684),
        ('R', 19),
        ('O', 18),
    ];
    for (kind, count) in counts {
        assert_eq!(oracle_rows(kind).len(), count, "rows of kind {}", kind);
    }
}

#[test]
fn production_matches_every_baseline_row() {
    let report = run(&PRODUCTION);
    assert_eq!(report.total(), 0, "disagreeing rows: {:?}", report.bad);
}

#[test]
fn single_file_classification_is_pinned() {
    // mime|name|groups in execution order|calls|toast
    let table = [
        "audio/mpeg|a.mp3|audio|audio:0|Import : 1 piste(s) audio",
        "image/png|a.png|image|image:0|Import : 1 image(s)",
        "image/svg+xml|a.svg|svg|svg:0|Import : 1 SVG (vecteurs)",
        "image/png|a.svg|svg|svg:0|Import : 1 SVG (vecteurs)",
        "image/svg+xml|a.png|svg|svg:0|Import : 1 SVG (vecteurs)",
        "image/png|A.SVG|svg|svg:0|Import : 1 SVG (vecteurs)",
        "video/mp4|v.mp4|video|video:0|Import : 1 vidéo(s)",
        "application/pdf|a.pdf|other|other:0|Import : référence",
        "Audio/mpeg|a.mp3|other|other:0|Import : référence",
        "IMAGE/PNG|a.png|other|other:0|Import : référence",
        "image/|z|image|image:0|Import : 1 image(s)",
        "audio/mpeg|x.svg|audio+svg|audio:0,svg:0|Import : 1 piste(s) audio + 1 SVG (vecteurs)",
        "video/mp4|y.svg|svg+video|svg:0,video:0|Import : 1 SVG (vecteurs) + 1 vidéo(s)",
        "text/plain|a.txt|other|other:0|Import : référence",
    ];
    for row in table {
        let p: Vec<&str> = row.split('|').collect();
        let file = f(p[0], p[1]);
        assert_eq!(labels(&file), p[2], "{}", row);
        let files = [file];
        let plan = plan_groups(&files).expect("plan");
        assert_eq!(plan.toast_text(), p[4]);
        assert!(
            oracle_has(&drop_line(&files, p[3], p[4])),
            "no oracle row for {}",
            row
        );
    }
}

#[test]
fn svg_is_peeled_before_the_image_pile_and_out_of_other() {
    let svg_named = f("image/png", "a.svg");
    assert!(is_svg_file(&svg_named));
    let m = memberships(&svg_named);
    assert!(m.svg && !m.image && !m.other);
    let m = memberships(&f("text/xml", "b.svg"));
    assert!(m.svg && !m.other && !m.image && !m.audio && !m.video);
    // the `.svg` suffix folds ASCII case only; the long s does not fold
    assert!(is_svg_file(&f("", "a.SvG")));
    assert!(!is_svg_file(&f("", "a.\u{17f}vg")));
    assert!(!is_svg_file(&f("image/SVG+XML", "a.png")));
    assert!(!is_svg_file(&f("image/svg+xml;charset=utf-8", "c.txt")));
    assert!(!is_svg_file(&f("", "a.svg\n")));
    assert!(!is_svg_file(&f("", "a.svg ")));
}

#[test]
fn execution_order_differs_from_toast_order() {
    let labels_of = |order: [DropGroup; 5]| order.iter().map(|g| g.label()).collect::<Vec<_>>();
    assert_eq!(
        labels_of(DropGroup::EXECUTION_ORDER),
        ["audio", "svg", "image", "video", "other"]
    );
    assert_eq!(
        labels_of(DropGroup::TOAST_ORDER),
        ["audio", "image", "svg", "video", "other"]
    );
    let files = [
        f("audio/mpeg", "a.mp3"),
        f("image/svg+xml", "a.svg"),
        f("image/png", "a.png"),
        f("video/mp4", "v.mp4"),
        f("application/pdf", "a.pdf"),
    ];
    let plan = plan_groups(&files).expect("plan");
    let want: Vec<(String, Vec<usize>)> = ["audio", "svg", "image", "video", "other"]
        .iter()
        .enumerate()
        .map(|(i, l)| (l.to_string(), vec![i]))
        .collect();
    assert_eq!(groups_of(&plan), want);
    let toast =
        "Import : 1 piste(s) audio + 1 image(s) + 1 SVG (vecteurs) + 1 vidéo(s) + référence";
    assert_eq!(plan.toast_text(), toast);
    assert!(oracle_has(&drop_line(
        &files,
        "audio:0,svg:1,image:2,video:3,other:4",
        toast
    )));
}

#[test]
fn counts_and_stable_indices_are_pinned() {
    let three_pngs = [
        f("image/png", "a.png"),
        f("image/png", "a.png"),
        f("image/png", "a.png"),
    ];
    let plan = plan_groups(&three_pngs).expect("plan");
    assert_eq!(plan.indices(DropGroup::Image), [0, 1, 2]);
    assert_eq!(plan.toast_text(), "Import : 3 image(s)");
    assert!(oracle_has(&drop_line(
        &three_pngs,
        "image:0.1.2",
        "Import : 3 image(s)"
    )));
    let pairs = [
        f("audio/mpeg", "a.mp3"),
        f("audio/mpeg", "a.mp3"),
        f("video/mp4", "v.mp4"),
        f("video/mp4", "v.mp4"),
    ];
    let plan = plan_groups(&pairs).expect("plan");
    assert_eq!(plan.indices(DropGroup::Audio), [0, 1]);
    assert_eq!(plan.indices(DropGroup::Video), [2, 3]);
    let toast = "Import : 2 piste(s) audio + 2 vidéo(s)";
    assert_eq!(plan.toast_text(), toast);
    assert!(oracle_has(&drop_line(
        &pairs,
        "audio:0,audio:1,video:2,video:3",
        toast
    )));
}

#[test]
fn nothing_is_dropped_and_overlap_is_explicit() {
    // the baseline double-routes a `.svg` file typed audio/* or video/*
    let files = [
        f("audio/mpeg", "x.svg"),
        f("video/mp4", "y.svg"),
        f("image/svg+xml", "a.svg"),
    ];
    let plan = plan_groups(&files).expect("plan");
    assert_eq!(plan.indices(DropGroup::Audio), [0]);
    assert_eq!(plan.indices(DropGroup::Svg), [0, 1, 2]);
    assert_eq!(plan.indices(DropGroup::Video), [1]);
    assert_eq!(
        (
            plan.appearances(0),
            plan.appearances(1),
            plan.appearances(2)
        ),
        (2, 2, 1)
    );
    let toast = "Import : 1 piste(s) audio + 3 SVG (vecteurs) + 1 vidéo(s)";
    assert_eq!(plan.toast_text(), toast);
    assert!(oracle_has(&drop_line(
        &files,
        "audio:0,svg:0.1.2,video:1",
        toast
    )));
    // over the whole grid: every file is in at least one group, in exactly the groups
    // its memberships name, and only svg+audio / svg+video overlap
    for row in oracle_rows('S') {
        let file = file_of(row[0]);
        let m = memberships(&file);
        let one = [file.clone()];
        let plan = plan_groups(&one).expect("plan");
        assert!(plan.covers_every_file(), "{:?}", file);
        let named = [m.audio, m.svg, m.image, m.video, m.other]
            .iter()
            .filter(|b| **b)
            .count();
        assert_eq!(plan.appearances(0), named);
        assert_eq!(named == 2, m.svg && (m.audio || m.video), "{:?}", file);
        assert!(!(m.svg && m.image) && !(m.other && named > 1));
    }
    for row in oracle_rows('G') {
        let (inputs, _) = split_arrow(&row);
        let files: Vec<DropFile> = inputs.iter().map(|t| file_of(t)).collect();
        match plan_groups(&files) {
            Some(plan) => {
                assert!(plan.covers_every_file());
                assert!(plan
                    .groups
                    .iter()
                    .all(|g| g.file_indices.windows(2).all(|w| w[0] < w[1])));
            }
            None => assert!(files.is_empty()),
        }
    }
}

#[test]
fn empty_drop_has_no_plan_and_routes_to_empty() {
    assert_eq!(plan_groups(&[]), None);
    assert_eq!(
        route_drop(&[], &DropPorts::NONE_AVAILABLE),
        DropRoute::Empty
    );
    assert!(oracle_has("G => - -"));
}

#[test]
fn drag_gate_needs_the_exact_files_type() {
    assert!(is_dragging_files(Some(&["Files"])));
    assert!(is_dragging_files(Some(&["text/plain", "Files"])));
    assert!(!is_dragging_files(Some(&[])));
    assert!(!is_dragging_files(Some(&["files"])));
    assert!(!is_dragging_files(Some(&["Files "])));
    assert!(!is_dragging_files(None));
    let lines = [
        "D Files => 1",
        "D text/plain,Files => 1",
        "D _ => 0",
        "D files => 0",
        "D Files%20 => 0",
        "D ~ => 0",
        "D ! => 0",
    ];
    for line in lines {
        assert!(oracle_has(line), "{}", line);
    }
}
