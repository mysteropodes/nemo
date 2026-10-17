//! Flow rows (`F`) and template rows (`T`) of the SVG import oracle (L10-D, G35).
//!
//! An `F` row is one complete run of the verbatim baseline `importSVGString` (tools/gen.mjs): the
//! tree `project.importSVG` returned, the active layer, the locale, the post-bake, post-fit leaves
//! (Paper's own numbers) and everything the baseline then did: the toast(s), the return value and
//! the paths it inserted. The harness rebuilds the request from the row, runs the plan and
//! renders its outcome in the same terms.

use super::super::rows as geo;
use super::{
    plan_svg_import, ActiveLayer, FitRecord, ImportNode, ParseOutcome, SvgImportOutcome,
    SvgImportRequest,
};
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::ImportLeaf;

/// An arbitrary adapter fit record; the kernel must echo it untouched.
pub(crate) const FIT: FitRecord = FitRecord {
    no_fit: false,
    scale: 0.25,
    center: [480.0, 270.0],
    translate: [-3.5, 7.25],
};

pub(crate) struct FlowRow {
    pub(crate) locale: String,
    pub(crate) layer: String,
    pub(crate) tree: Vec<&'static str>,
    pub(crate) leaves: Vec<Vec<&'static str>>,
    pub(crate) toasts: Vec<String>,
    pub(crate) ret: usize,
    pub(crate) items: &'static str,
}

/// The `F` rows, split on ` | `.
pub(crate) fn flow_rows() -> Vec<FlowRow> {
    let mut rows = Vec::new();
    for line in geo::ORACLE.lines().filter(|l| l.starts_with("F ")) {
        let fields: Vec<&'static str> = line.split(" | ").collect();
        let arrow = fields.iter().position(|f| *f == "=>").expect("=>");
        let toasts = if fields[arrow + 1] == "none" {
            Vec::new()
        } else {
            fields[arrow + 1].split(' ').map(geo::unescape).collect()
        };
        rows.push(FlowRow {
            locale: fields[0][2..].to_string(),
            layer: fields[1].to_string(),
            tree: fields[2].split(' ').collect(),
            leaves: fields[3..arrow]
                .iter()
                .map(|l| l.split(' ').collect())
                .collect(),
            toasts,
            ret: fields[arrow + 2].parse().expect("return value"),
            items: fields[arrow + 3],
        });
    }
    rows
}

/// `SM.t` for one locale, from the `T` rows (templates read from i18n.js).
pub(crate) fn lookup_for(locale: &str) -> impl Fn(&str) -> String {
    let rows = geo::oracle_rows("T");
    let locale = locale.to_string();
    move |key: &str| {
        for f in &rows {
            if f[0] == locale && f[1] == key {
                return geo::unescape(f[2]);
            }
        }
        panic!("no template {} {}", locale, key);
    }
}

pub(crate) fn layer_of(name: &str) -> Option<ActiveLayer> {
    match name {
        "ok" => Some(ActiveLayer {
            locked: false,
            has_symbol: false,
        }),
        "locked" => Some(ActiveLayer {
            locked: true,
            has_symbol: false,
        }),
        "symbol" => Some(ActiveLayer {
            locked: false,
            has_symbol: true,
        }),
        "both" => Some(ActiveLayer {
            locked: true,
            has_symbol: true,
        }),
        "none" => None,
        other => panic!("layer {}", other),
    }
}

fn parse_node(
    tokens: &mut std::slice::Iter<&str>,
    leaves: &mut std::vec::IntoIter<ImportLeaf>,
) -> ImportNode {
    let token = *tokens.next().expect("tree token");
    if token == "p" || token == "c" {
        return ImportNode::Leaf(leaves.next().expect("leaf definition"));
    }
    let parts: Vec<&str> = token.split(':').collect();
    assert_eq!(parts[0], "n", "node token {}", token);
    let class_name = if parts[1] == "~" {
        None
    } else {
        Some(geo::unescape(parts[1]))
    };
    let count: usize = parts[2].parse().expect("child count");
    let mut children = Vec::new();
    for _ in 0..count {
        children.push(parse_node(tokens, leaves));
    }
    ImportNode::Node {
        class_name,
        children,
    }
}

/// The adapter hand-off the row describes.
pub(crate) fn request_of(row: &FlowRow) -> SvgImportRequest {
    let parse = if row.tree == ["fail"] {
        ParseOutcome::Failed
    } else {
        let leaves: Vec<ImportLeaf> = row.leaves.iter().map(|l| geo::dec_leaf(l)).collect();
        let mut tokens = row.tree.iter();
        let mut leaves = leaves.into_iter();
        let root = parse_node(&mut tokens, &mut leaves);
        assert!(
            tokens.next().is_none() && leaves.next().is_none(),
            "row consumed exactly"
        );
        ParseOutcome::Parsed(root)
    };
    SvgImportRequest {
        parse,
        layer: layer_of(&row.layer),
        fit: FIT,
    }
}

/// How many nodes (leaves, skipped items and containers) a request's tree has.
pub(crate) fn node_total(node: &ImportNode) -> usize {
    match node {
        ImportNode::Leaf(_) => 1,
        ImportNode::Node { children, .. } => 1 + children.iter().map(node_total).sum::<usize>(),
    }
}

/// The three functions a flow check runs through (production or a deliberate mutant).
pub(crate) struct FlowImpl<'a> {
    pub(crate) plan: &'a dyn Fn(&SvgImportRequest) -> SvgImportOutcome,
    pub(crate) render: &'a dyn Fn(&SvgImportOutcome, &dyn Fn(&str) -> String) -> Vec<String>,
    pub(crate) count: &'a dyn Fn(&SvgImportOutcome) -> usize,
}

fn production_render(outcome: &SvgImportOutcome, lookup: &dyn Fn(&str) -> String) -> Vec<String> {
    outcome
        .toast()
        .map(|t| t.message(lookup))
        .into_iter()
        .collect()
}

fn production_count(outcome: &SvgImportOutcome) -> usize {
    outcome.imported_count()
}

pub(crate) fn production() -> FlowImpl<'static> {
    FlowImpl {
        plan: &plan_svg_import,
        render: &production_render,
        count: &production_count,
    }
}

/// Flow rows whose toast(s), return value or inserted paths differ from the baseline:
/// (rows, bad, first bad row description).
pub(crate) fn check_flows(imp: &FlowImpl) -> (usize, usize, Option<String>) {
    let rows = flow_rows();
    let mut bad = 0;
    let mut first: Option<String> = None;
    for (n, row) in rows.iter().enumerate() {
        let outcome = (imp.plan)(&request_of(row));
        let lookup = lookup_for(&row.locale);
        let toasts = (imp.render)(&outcome, &lookup);
        let items = match &outcome {
            SvgImportOutcome::Ready(plan) => geo::enc_outs(&plan.paths, false),
            _ => "none".to_string(),
        };
        let same = toasts == row.toasts && (imp.count)(&outcome) == row.ret && items == row.items;
        if !same {
            bad += 1;
            if first.is_none() {
                first = Some(format!(
                    "F row {} ({} {}): toasts {:?} want {:?}",
                    n, row.locale, row.layer, toasts, row.toasts
                ));
            }
        }
    }
    (rows.len(), bad, first)
}
