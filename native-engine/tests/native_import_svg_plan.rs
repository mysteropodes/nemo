//! Tests of the SVG import plan against the baseline `importSVGString` flows (L10-D, G35).

use super::super::rows as geo;
use super::rows::{check_flows, flow_rows, lookup_for, node_total, production, request_of, FIT};
use super::*;
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    Bounds, CompoundChild, Contour, ExplodeError, LeafStyle, PathOrigin, Seg,
};

fn seg(x: f64, y: f64) -> Seg {
    Seg {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

fn style() -> LeafStyle {
    LeafStyle {
        fill: Some("rgb(255,0,0)".to_string()),
        stroke: None,
        stroke_width: 1.0,
        opacity: 1.0,
    }
}

fn plain(x: f64) -> ImportLeaf {
    ImportLeaf::Path {
        contour: Contour {
            segments: vec![seg(x, 0.0), seg(x + 1.0, 0.0), seg(x, 1.0)],
            closed: true,
        },
        style: style(),
    }
}

fn square(x: f64, y: f64, s: f64, clockwise: bool) -> CompoundChild {
    let pts = if clockwise {
        vec![seg(x, y), seg(x + s, y), seg(x + s, y + s), seg(x, y + s)]
    } else {
        vec![seg(x, y), seg(x, y + s), seg(x + s, y + s), seg(x + s, y)]
    };
    CompoundChild {
        contour: Contour {
            segments: pts,
            closed: true,
        },
        clockwise,
        bounds: Bounds {
            x,
            y,
            width: s,
            height: s,
        },
    }
}

fn donut() -> ImportLeaf {
    ImportLeaf::Compound {
        children: vec![
            square(0.0, 0.0, 100.0, true),
            square(40.0, 40.0, 20.0, false),
        ],
        style: style(),
    }
}

fn leaf(l: ImportLeaf) -> ImportNode {
    ImportNode::Node {
        class_name: Some("Group".to_string()),
        children: vec![ImportNode::Leaf(l)],
    }
}

fn node(name: Option<&str>, children: Vec<ImportNode>) -> ImportNode {
    ImportNode::Node {
        class_name: name.map(|n| n.to_string()),
        children,
    }
}

const OK: Option<ActiveLayer> = Some(ActiveLayer {
    locked: false,
    has_symbol: false,
});

fn request(root: ImportNode, layer: Option<ActiveLayer>) -> SvgImportRequest {
    SvgImportRequest {
        parse: ParseOutcome::Parsed(root),
        layer,
        fit: FIT,
    }
}

fn english(key: &str) -> String {
    lookup_for("en")(key)
}

#[test]
fn every_baseline_flow_is_reproduced() {
    let (rows, bad, first) = check_flows(&production());
    assert_eq!(rows, 54);
    assert_eq!(bad, 0, "{:?}", first);
}

#[test]
fn the_flow_rows_cover_every_outcome_and_locale() {
    let (mut failed, mut empty, mut refused, mut ready, mut skipping) = (0, 0, 0, 0, 0);
    let mut locales: Vec<String> = Vec::new();
    for row in flow_rows() {
        match plan_svg_import(&request_of(&row)) {
            SvgImportOutcome::ParseFailed => failed += 1,
            SvgImportOutcome::Empty { .. } => empty += 1,
            SvgImportOutcome::Refused { .. } => refused += 1,
            SvgImportOutcome::ExplodeFailed { .. } => panic!("no baseline flow throws"),
            SvgImportOutcome::Ready(plan) => {
                ready += 1;
                skipping += usize::from(!plan.skipped.is_empty());
            }
        }
        if !locales.contains(&row.locale) {
            locales.push(row.locale.clone());
        }
    }
    assert_eq!((failed, empty, refused, ready, skipping), (5, 8, 9, 32, 12));
    assert_eq!(locales, ["en", "fr", "ja", "es"]);
}

#[test]
fn nothing_is_absent_from_every_list() {
    for row in flow_rows() {
        let request = request_of(&row);
        let ParseOutcome::Parsed(root) = &request.parse else {
            continue;
        };
        let total = node_total(root);
        match plan_svg_import(&request) {
            SvgImportOutcome::Ready(plan) => {
                let mut ids: Vec<usize> = plan.leaves.iter().map(|l| l.node_index).collect();
                ids.extend(plan.skipped.iter().map(|s| s.node_index));
                ids.sort();
                ids.dedup();
                assert_eq!(
                    ids.len(),
                    plan.leaves.len() + plan.skipped.len(),
                    "indices unique"
                );
                assert!(ids.iter().all(|&i| i < total));
                assert_eq!(
                    plan.node_count(),
                    total,
                    "leaves + skipped + containers = every node"
                );
            }
            SvgImportOutcome::Empty { skipped } | SvgImportOutcome::Refused { skipped, .. } => {
                assert!(skipped.iter().all(|s| s.node_index < total));
            }
            other => panic!("{:?}", other),
        }
    }
}

#[test]
fn collect_leaves_classes_and_the_inconnu_fallback_are_pinned() {
    let root = node(
        Some("Group"),
        vec![
            ImportNode::Leaf(plain(0.0)),
            node(Some("PointText"), vec![]),
            node(Some("Group"), vec![]),
            node(
                Some("Group"),
                vec![
                    node(None, vec![]),
                    ImportNode::Leaf(donut()),
                    node(Some(""), vec![]),
                ],
            ),
            node(Some("Raster"), vec![]),
        ],
    );
    let SvgImportOutcome::Ready(plan) = plan_svg_import(&request(root, OK)) else {
        panic!("ready")
    };
    let names: Vec<(usize, &str)> = plan
        .skipped
        .iter()
        .map(|s| (s.node_index, s.class_name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            (2, "PointText"),
            (3, "Group"),
            (5, "inconnu"),
            (7, "inconnu"),
            (8, "Raster")
        ]
    );
    let kinds: Vec<(usize, LeafKind, usize, usize)> = plan
        .leaves
        .iter()
        .map(|l| (l.node_index, l.kind, l.children, l.produced))
        .collect();
    assert_eq!(
        kinds,
        [(1, LeafKind::Path, 0, 1), (6, LeafKind::Compound, 2, 1)]
    );
    assert_eq!(plan.containers, 2, "root and the non-empty inner group");
    assert_eq!(plan.node_count(), 9);
    assert_eq!(resolve_class_name(&None), "inconnu");
    assert_eq!(resolve_class_name(&Some(String::new())), "inconnu");
    assert_eq!(resolve_class_name(&Some("Shape".to_string())), "Shape");
}

#[test]
fn the_check_order_is_parse_then_empty_then_layer() {
    let locked = Some(ActiveLayer {
        locked: true,
        has_symbol: false,
    });
    let symbol = Some(ActiveLayer {
        locked: false,
        has_symbol: true,
    });
    let both = Some(ActiveLayer {
        locked: true,
        has_symbol: true,
    });
    let none_failed = SvgImportRequest {
        parse: ParseOutcome::Failed,
        layer: None,
        fit: FIT,
    };
    assert_eq!(plan_svg_import(&none_failed), SvgImportOutcome::ParseFailed);
    let empty_tree = || node(Some("Group"), vec![node(Some("PointText"), vec![])]);
    for layer in [locked, symbol, both, None, OK] {
        let outcome = plan_svg_import(&request(empty_tree(), layer));
        let SvgImportOutcome::Empty { skipped } = outcome else {
            panic!("empty beats the layer check")
        };
        assert_eq!(skipped.len(), 1);
    }
    let one = || leaf(plain(0.0));
    let refusal = |layer| match plan_svg_import(&request(one(), layer)) {
        SvgImportOutcome::Refused { reason, .. } => Some(reason),
        _ => None,
    };
    assert_eq!(refusal(locked), Some(LayerRefusal::Locked));
    assert_eq!(refusal(symbol), Some(LayerRefusal::SymbolLayer));
    assert_eq!(
        refusal(both),
        Some(LayerRefusal::Locked),
        "locked is tested before the symbol"
    );
    assert_eq!(refusal(None), Some(LayerRefusal::NoActiveLayer));
    assert_eq!(refusal(OK), None);
    assert_eq!(layer_refusal(&OK), None);
}

#[test]
fn outcomes_map_to_the_baseline_toasts_and_return_values() {
    let ready = plan_svg_import(&request(leaf(donut()), OK));
    assert_eq!(
        ready.toast(),
        Some(SvgToast::Imported {
            count: 1,
            skipped: vec![]
        })
    );
    assert_eq!(ready.imported_count(), 1);
    let refused = plan_svg_import(&request(leaf(plain(0.0)), None));
    assert_eq!(refused.toast(), Some(SvgToast::InvalidOrLockedLayer));
    assert_eq!(
        (
            refused.imported_count(),
            SvgToast::InvalidOrLockedLayer.key()
        ),
        (0, "toastInvalidLockedLayer")
    );
    assert_eq!(english("toastInvalidLockedLayer"), "Invalid/locked layer");
    assert_eq!(SvgToast::Empty.key(), "toastSvgImportEmpty");
    assert_eq!(SvgToast::ParseFailed.key(), "toastSvgImportFailed");
    assert_eq!(
        SvgToast::ParseFailed.message(&english),
        "This SVG could not be read"
    );
    assert_eq!(
        SvgToast::Empty.message(&english),
        "This SVG has no visible shape"
    );
}

#[test]
fn the_success_message_matches_the_baseline_text() {
    let skipped = vec![
        "PointText".to_string(),
        "Group".to_string(),
        "PointText".to_string(),
    ];
    let toast = SvgToast::Imported { count: 5, skipped };
    assert_eq!(
        toast.message(&english),
        "5 vector shape(s) imported \u{2014} 3 unsupported element(s) skipped (PointText, Group, PointText)"
    );
    assert_eq!(
        SvgToast::Imported {
            count: 1,
            skipped: vec![]
        }
        .message(&english),
        "1 vector shape(s) imported"
    );
    // Only the FIRST {n} of a template is replaced (String.prototype.replace with a string pattern).
    let twice = |key: &str| {
        if key == "toastSvgImported" {
            "{n} and {n}".to_string()
        } else {
            "[{n}]".to_string()
        }
    };
    let toast = SvgToast::Imported {
        count: 3,
        skipped: vec!["A".to_string()],
    };
    assert_eq!(toast.message(&twice), "3 and {n} \u{2014} [1] (A)");
    let fr = lookup_for("fr");
    assert_eq!(
        SvgToast::Imported { count: 2, skipped: vec!["Group".to_string()] }.message(&fr),
        "2 forme(s) vectorielle(s) import\u{e9}e(s) \u{2014} 1 \u{e9}l\u{e9}ment(s) non pris en charge ignor\u{e9}(s) (Group)"
    );
    assert_eq!(SKIPPED_SEPARATOR, " \u{2014} ");
}

#[test]
fn literal_toasts_are_baseline_flow_rows() {
    // The English success-with-skip message above is the toast of an oracle flow.
    let want = "1 vector shape(s) imported \u{2014} 1 unsupported element(s) skipped (PointText)";
    let hit = flow_rows()
        .iter()
        .any(|r| r.locale == "en" && r.toasts == [want]);
    assert!(hit, "no oracle flow toasted {:?}", want);
    let fr = "Calque invalide/verrouill\u{e9}";
    assert!(flow_rows()
        .iter()
        .any(|r| r.locale == "fr" && r.toasts == [fr] && r.layer == "locked"));
    assert!(flow_rows()
        .iter()
        .any(|r| r.toasts == ["0 vector shape(s) imported"] && r.ret == 0));
}

#[test]
fn one_batch_in_tree_order_with_provenance() {
    let root = node(
        Some("Group"),
        vec![
            ImportNode::Leaf(donut()),
            ImportNode::Leaf(plain(7.0)),
            ImportNode::Leaf(donut()),
        ],
    );
    let SvgImportOutcome::Ready(plan) = plan_svg_import(&request(root, OK)) else {
        panic!("ready")
    };
    assert_eq!(plan.paths.len(), 3);
    assert!(
        matches!(&plan.paths[0].origin, PathOrigin::Merged { exterior: 0, holes } if holes == &vec![1])
    );
    assert_eq!(plan.paths[1].origin, PathOrigin::Passthrough);
    assert_eq!(plan.paths[1].contour.segments[0].point, [7.0, 0.0]);
    assert!(matches!(
        &plan.paths[2].origin,
        PathOrigin::Merged { exterior: 0, .. }
    ));
    let produced: Vec<usize> = plan.leaves.iter().map(|l| l.produced).collect();
    assert_eq!(produced, [1, 1, 1]);
    assert_eq!(plan.paths.len(), produced.iter().sum::<usize>());
}

#[test]
fn the_fit_record_is_echoed_unchanged() {
    for fit in [
        FIT,
        FitRecord {
            no_fit: true,
            scale: 1.0,
            center: [0.0, 0.0],
            translate: [0.0, 0.0],
        },
    ] {
        let mut request = request(leaf(donut()), OK);
        request.fit = fit;
        let SvgImportOutcome::Ready(plan) = plan_svg_import(&request) else {
            panic!("ready")
        };
        assert_eq!(plan.fit, fit);
    }
}

#[test]
fn a_compound_that_produces_nothing_is_still_a_leaf_and_says_so() {
    let ccw = ImportLeaf::Compound {
        children: vec![
            square(0.0, 0.0, 100.0, false),
            square(40.0, 40.0, 20.0, false),
        ],
        style: style(),
    };
    let SvgImportOutcome::Ready(plan) = plan_svg_import(&request(leaf(ccw), OK)) else {
        panic!("ready, not empty")
    };
    assert!(plan.paths.is_empty());
    assert_eq!(plan.leaves[0].produced, 0);
    assert_eq!(plan.leaves[0].unclaimed_holes, vec![0, 1]);
    let empty = ImportLeaf::Compound {
        children: vec![],
        style: style(),
    };
    let outcome = plan_svg_import(&request(leaf(empty), OK));
    let SvgImportOutcome::Ready(plan) = &outcome else {
        panic!("an empty compound is a leaf")
    };
    assert_eq!((plan.leaves[0].children, plan.leaves[0].produced), (0, 0));
    assert_eq!(
        outcome.toast().map(|t| t.message(&english)),
        Some("0 vector shape(s) imported".to_string())
    );
}

#[test]
fn an_explode_failure_publishes_nothing() {
    let broken = ImportLeaf::Compound {
        children: vec![
            square(0.0, 0.0, 100.0, true),
            CompoundChild {
                contour: Contour {
                    segments: vec![],
                    closed: true,
                },
                clockwise: false,
                bounds: Bounds {
                    x: 40.0,
                    y: 40.0,
                    width: 20.0,
                    height: 20.0,
                },
            },
        ],
        style: style(),
    };
    let root = node(
        Some("Group"),
        vec![ImportNode::Leaf(plain(0.0)), ImportNode::Leaf(broken)],
    );
    let outcome = plan_svg_import(&request(root, OK));
    assert_eq!(
        outcome,
        SvgImportOutcome::ExplodeFailed {
            leaf: 1,
            error: ExplodeError::EmptyContour { child: 1 }
        }
    );
    assert_eq!(outcome.toast(), None);
    assert_eq!(outcome.imported_count(), 0);
}

#[test]
fn flow_leaf_definitions_decode_to_what_the_rows_encode() {
    let mut checked = 0;
    for row in flow_rows() {
        for fields in &row.leaves {
            let decoded = geo::dec_leaf(fields);
            assert_eq!(geo::enc_leaf(&decoded), fields.join(" "));
            checked += 1;
        }
    }
    assert_eq!(checked, 71);
}
