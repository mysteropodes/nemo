//! SVG import plan: leaf collection, skipped report, refusals and toast text (L10-D, G35).
//!
//! Pure port of the decisions of `importSVGString` and `collectLeaves` of `src/js/svg-import.js`
//! at the agreed desktop baseline `fbe4b8a3`, child of the explode kernel. The adapter parses with
//! Paper.js (`project.importSVG(text, {insert: false, expandShapes: true})`), bakes every leaf's
//! accumulated world matrix, applies the shrink-only fit-to-canvas (or `noFit`) and hands the
//! resulting tree over as an [`SvgImportRequest`]; the fit is echoed untouched ([`FitRecord`]).
//! The kernel decides, in the baseline order: parse failure, empty (no leaf), refused layer
//! (missing, locked or symbol layer), then explodes every leaf into ONE batch of paths
//! ([`SvgImportPlan::paths`], insertion order) so the transaction leaf publishes once or not at all.
//!
//! Baseline facts the kernel keeps on purpose:
//! - `collectLeaves` takes every `Path` and `CompoundPath` as a leaf (a compound is NOT descended
//!   into: its sub-paths stay grouped with the exterior they were cut from); any other item with
//!   children is descended into; any other item without children (a `PointText`, a `Raster`, a
//!   `Shape`, an empty `Group`...) is skipped and reported under `className || 'inconnu'`;
//! - the skipped list is part of the success message only: an empty import reports
//!   `toastSvgImportEmpty` and a refused layer `toastInvalidLockedLayer`, never the skipped list.
//!   The kernel still returns the skipped list in every outcome that got far enough to collect;
//! - the check order is parse, empty, layer: an empty SVG on a locked layer says "empty";
//! - the success message is `toastSvgImported` with its first `{n}` replaced by the number of
//!   inserted paths, plus (only if something was skipped) ` \u{2014} ` + `toastSvgSkipped` with
//!   `{n}` replaced by the skipped count + ` (` + the class names joined by `, ` + `)`.
//!
//! Nothing is silently absent: every node of the tree is exactly one of a leaf, a skipped
//! element or a descended container ([`SvgImportPlan::node_count`]), and every leaf has a
//! [`LeafReport`] saying how many paths it produced and which holes no exterior claimed.

use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    explode_leaf, ExplodeError, ExplodedPath, ImportLeaf,
};

/// `item.className || 'inconnu'` fallback for an unnamed skipped item.
pub(crate) const UNNAMED_CLASS: &str = "inconnu";
/// Separator of the skipped suffix: ` \u{2014} ` (space, em dash, space).
pub(crate) const SKIPPED_SEPARATOR: &str = " \u{2014} ";

/// A node of the parsed SVG tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImportNode {
    /// A `Path` or `CompoundPath` (already baked and fitted).
    Leaf(ImportLeaf),
    /// Any other Paper item: its `className` (`None` or empty when unreadable) and children.
    Node {
        class_name: Option<String>,
        children: Vec<ImportNode>,
    },
}

/// Outcome of `project.importSVG`: a thrown error or a falsy root is a failure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ParseOutcome {
    Failed,
    Parsed(ImportNode),
}

/// The active layer as `state.layers[state.activeLayerIdx]` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActiveLayer {
    pub(crate) locked: bool,
    pub(crate) has_symbol: bool,
}

/// The adapter-side fit-to-canvas record, echoed unchanged (the kernel never recomputes it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FitRecord {
    pub(crate) no_fit: bool,
    pub(crate) scale: f64,
    pub(crate) center: [f64; 2],
    pub(crate) translate: [f64; 2],
}

/// Everything the baseline reads before it inserts anything.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SvgImportRequest {
    pub(crate) parse: ParseOutcome,
    /// `None` is a missing active layer (`!ld`).
    pub(crate) layer: Option<ActiveLayer>,
    pub(crate) fit: FitRecord,
}

/// Why the active layer cannot take the import (`!ld || ld.locked || ld.symbolId`, in order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerRefusal {
    NoActiveLayer,
    Locked,
    SymbolLayer,
}

/// A skipped element: its pre-order node index in the tree and its (resolved) class name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkippedElement {
    pub(crate) node_index: usize,
    pub(crate) class_name: String,
}

/// Which kind of leaf a report is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafKind {
    Path,
    Compound,
}

/// What one leaf contributed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LeafReport {
    pub(crate) node_index: usize,
    pub(crate) kind: LeafKind,
    /// Compound child count (0 for a plain path).
    pub(crate) children: usize,
    pub(crate) produced: usize,
    pub(crate) unclaimed_holes: Vec<usize>,
}

/// The single batch of an import that will be published.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SvgImportPlan {
    pub(crate) paths: Vec<ExplodedPath>,
    pub(crate) leaves: Vec<LeafReport>,
    pub(crate) skipped: Vec<SkippedElement>,
    pub(crate) containers: usize,
    pub(crate) fit: FitRecord,
}

impl SvgImportPlan {
    /// Leaves + skipped + descended containers = every node of the tree.
    pub(crate) fn node_count(&self) -> usize {
        self.leaves.len() + self.skipped.len() + self.containers
    }
}

/// The typed result of planning one SVG import.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SvgImportOutcome {
    ParseFailed,
    Empty {
        skipped: Vec<SkippedElement>,
    },
    Refused {
        reason: LayerRefusal,
        skipped: Vec<SkippedElement>,
    },
    /// A leaf could not be exploded (the baseline throws); nothing is planned.
    ExplodeFailed {
        leaf: usize,
        error: ExplodeError,
    },
    Ready(SvgImportPlan),
}

/// The toast the baseline shows for an outcome (text keys are the i18n keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SvgToast {
    ParseFailed,
    Empty,
    InvalidOrLockedLayer,
    Imported { count: usize, skipped: Vec<String> },
}

impl SvgToast {
    /// The i18n key of the (first) message.
    pub(crate) fn key(&self) -> &'static str {
        match self {
            SvgToast::ParseFailed => "toastSvgImportFailed",
            SvgToast::Empty => "toastSvgImportEmpty",
            SvgToast::InvalidOrLockedLayer => "toastInvalidLockedLayer",
            SvgToast::Imported { .. } => "toastSvgImported",
        }
    }

    /// The toast text for a locale lookup (`SM.t`). The first `{n}` of a template is replaced,
    /// like `String.prototype.replace` with a string pattern.
    pub(crate) fn message(&self, lookup: &dyn Fn(&str) -> String) -> String {
        match self {
            SvgToast::Imported { count, skipped } => {
                let mut msg = lookup("toastSvgImported").replacen("{n}", &count.to_string(), 1);
                if !skipped.is_empty() {
                    msg.push_str(SKIPPED_SEPARATOR);
                    msg.push_str(&lookup("toastSvgSkipped").replacen(
                        "{n}",
                        &skipped.len().to_string(),
                        1,
                    ));
                    msg.push_str(" (");
                    msg.push_str(&skipped.join(", "));
                    msg.push(')');
                }
                msg
            }
            other => lookup(other.key()),
        }
    }
}

impl SvgImportOutcome {
    /// The baseline toast of this outcome; `None` where the baseline throws instead.
    pub(crate) fn toast(&self) -> Option<SvgToast> {
        match self {
            SvgImportOutcome::ParseFailed => Some(SvgToast::ParseFailed),
            SvgImportOutcome::Empty { .. } => Some(SvgToast::Empty),
            SvgImportOutcome::Refused { .. } => Some(SvgToast::InvalidOrLockedLayer),
            SvgImportOutcome::ExplodeFailed { .. } => None,
            SvgImportOutcome::Ready(plan) => Some(SvgToast::Imported {
                count: plan.paths.len(),
                skipped: plan.skipped.iter().map(|s| s.class_name.clone()).collect(),
            }),
        }
    }

    /// The number `importSVGString` returns (inserted paths; 0 for every refusal).
    pub(crate) fn imported_count(&self) -> usize {
        match self {
            SvgImportOutcome::Ready(plan) => plan.paths.len(),
            _ => 0,
        }
    }
}

struct Walk<'a> {
    next_index: usize,
    leaves: Vec<(usize, &'a ImportLeaf)>,
    skipped: Vec<SkippedElement>,
    containers: usize,
}

/// `item.className || 'inconnu'`.
pub(crate) fn resolve_class_name(class_name: &Option<String>) -> String {
    match class_name {
        Some(name) if !name.is_empty() => name.clone(),
        _ => UNNAMED_CLASS.to_string(),
    }
}

/// `collectLeaves`, in pre-order: leaves and skipped elements in tree order.
fn walk<'a>(node: &'a ImportNode, w: &mut Walk<'a>) {
    let index = w.next_index;
    w.next_index += 1;
    match node {
        ImportNode::Leaf(leaf) => w.leaves.push((index, leaf)),
        ImportNode::Node {
            class_name,
            children,
        } => {
            if children.is_empty() {
                w.skipped.push(SkippedElement {
                    node_index: index,
                    class_name: resolve_class_name(class_name),
                });
            } else {
                w.containers += 1;
                for child in children {
                    walk(child, w);
                }
            }
        }
    }
}

/// `!ld || ld.locked || ld.symbolId`: the first reason that applies.
pub(crate) fn layer_refusal(layer: &Option<ActiveLayer>) -> Option<LayerRefusal> {
    match layer {
        None => Some(LayerRefusal::NoActiveLayer),
        Some(l) if l.locked => Some(LayerRefusal::Locked),
        Some(l) if l.has_symbol => Some(LayerRefusal::SymbolLayer),
        Some(_) => None,
    }
}

/// Plan one SVG import: parse result, leaf collection, empty and layer checks, explode.
pub(crate) fn plan_svg_import(request: &SvgImportRequest) -> SvgImportOutcome {
    let root = match &request.parse {
        ParseOutcome::Failed => return SvgImportOutcome::ParseFailed,
        ParseOutcome::Parsed(root) => root,
    };
    let mut w = Walk {
        next_index: 0,
        leaves: Vec::new(),
        skipped: Vec::new(),
        containers: 0,
    };
    walk(root, &mut w);
    if w.leaves.is_empty() {
        return SvgImportOutcome::Empty { skipped: w.skipped };
    }
    if let Some(reason) = layer_refusal(&request.layer) {
        return SvgImportOutcome::Refused {
            reason,
            skipped: w.skipped,
        };
    }
    let mut paths: Vec<ExplodedPath> = Vec::new();
    let mut leaves: Vec<LeafReport> = Vec::new();
    for (position, &(node_index, leaf)) in w.leaves.iter().enumerate() {
        match explode_leaf(leaf) {
            Ok(exploded) => {
                let (kind, children) = match leaf {
                    ImportLeaf::Path { .. } => (LeafKind::Path, 0),
                    ImportLeaf::Compound { children, .. } => (LeafKind::Compound, children.len()),
                };
                leaves.push(LeafReport {
                    node_index,
                    kind,
                    children,
                    produced: exploded.paths.len(),
                    unclaimed_holes: exploded.unclaimed_holes,
                });
                paths.extend(exploded.paths);
            }
            Err(error) => {
                return SvgImportOutcome::ExplodeFailed {
                    leaf: position,
                    error,
                }
            }
        }
    }
    SvgImportOutcome::Ready(SvgImportPlan {
        paths,
        leaves,
        skipped: w.skipped,
        containers: w.containers,
        fit: request.fit,
    })
}

#[cfg(test)]
#[path = "../tests/native_import_svg_plan_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/native_import_svg_plan.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_svg_plan_controls.rs"]
mod controls;
