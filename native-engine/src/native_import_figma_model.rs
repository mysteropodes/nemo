//! Typed Figma node model and walk records (L10-E, G36).
//!
//! `src/js/figma-import.js` (baseline `fbe4b8a3`) reads the Figma REST JSON loosely: `node.visible
//! === false`, `paint.opacity != null`, `t.length === 2`, `x || default`. The adapter that parses the
//! network response (L10-J) builds these records once, with one rule per field kept in the field's
//! documentation: `Option` is "undefined or null" (every baseline test of an optional field treats the
//! two alike), a `Vec` is "absent or empty" (every baseline test is a truthy `.length`), and a field
//! that the baseline only tests for truthiness is stored already coerced. Nothing here parses JSON,
//! fetches, or touches Paper.js.

/// 2x3 affine matrix in the SVG `matrix(a b c d e f)` convention: `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mat {
    pub(crate) a: f64,
    pub(crate) b: f64,
    pub(crate) c: f64,
    pub(crate) d: f64,
    pub(crate) e: f64,
    pub(crate) f: f64,
}

/// `paint.type` as the baseline distinguishes it: `'SOLID'`, `'IMAGE'`, anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintKind {
    Solid,
    Image,
    Other,
}

/// `paint.color`: red, green, blue as numbers (a missing channel is an adapter refusal) and an optional alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Color {
    pub(crate) r: f64,
    pub(crate) g: f64,
    pub(crate) b: f64,
    pub(crate) a: Option<f64>,
}

/// One entry of a `fills` / `strokes` array (bottom to top, as Figma orders them).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Paint {
    pub(crate) kind: PaintKind,
    /// `paint.visible`; only an explicit `Some(false)` hides the paint.
    pub(crate) visible: Option<bool>,
    pub(crate) color: Option<Color>,
    pub(crate) opacity: Option<f64>,
    /// `paint.imageRef`; an empty string is falsy in the baseline and never selects an image.
    pub(crate) image_ref: Option<String>,
}

/// One `{path, windingRule}` entry of `fillGeometry` / `strokeGeometry`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GeomPath {
    /// The SVG path data; empty is the baseline's falsy `!g.path`.
    pub(crate) path: String,
    /// `'EVENODD'` selects `evenodd`; anything else (including absent) is `nonzero`.
    pub(crate) winding_rule: Option<String>,
}

/// `absoluteBoundingBox`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BBox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// The `style` object of a TEXT node (`node.style || {}`); each field is what the baseline reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextStyle {
    pub(crate) font_family: Option<String>,
    pub(crate) font_size: Option<f64>,
    pub(crate) text_align_horizontal: Option<String>,
    pub(crate) font_weight: Option<f64>,
    /// `!!style.italic`.
    pub(crate) italic: bool,
    pub(crate) line_height_px: Option<f64>,
    pub(crate) letter_spacing: Option<f64>,
}

/// A Figma node, with only the fields `walkNode` and `convertFileJson` read.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FigmaNode {
    /// `node.type`; `None` prints as `undefined`, like the baseline's skipped message.
    pub(crate) node_type: Option<String>,
    /// `node.name`.
    pub(crate) name: Option<String>,
    /// `node.visible`; only an explicit `Some(false)` hides the node.
    pub(crate) visible: Option<bool>,
    /// `node.absoluteTransform` rows as received (`None` is absent); the baseline requires exactly two rows.
    pub(crate) abs_transform: Option<Vec<Vec<f64>>>,
    pub(crate) abs_bbox: Option<BBox>,
    pub(crate) opacity: Option<f64>,
    /// `node.characters`.
    pub(crate) characters: Option<String>,
    pub(crate) style: TextStyle,
    pub(crate) fills: Vec<Paint>,
    pub(crate) strokes: Vec<Paint>,
    pub(crate) fill_geometry: Vec<GeomPath>,
    pub(crate) stroke_geometry: Vec<GeomPath>,
    pub(crate) children: Vec<FigmaNode>,
}

/// `out.texts[i]` of `walkNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextRecord {
    pub(crate) text: String,
    pub(crate) name: Option<String>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) style: TextStyle,
    pub(crate) color: String,
}

/// `out.images[i]` of `walkNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageRecord {
    pub(crate) image_ref: String,
    pub(crate) mat: Mat,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) opacity: f64,
    pub(crate) name: Option<String>,
}

/// Which geometry array of a node a record talks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeometryRole {
    Fill,
    Stroke,
}

/// Why a geometry array produced no SVG tag. The baseline emits nothing and says nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnpaintableReason {
    /// No visible SOLID paint resolved (a gradient, a pattern, a hidden paint, an empty paint list).
    NoSolidPaint,
    /// A solid paint resolved but every geometry entry has an empty path.
    EmptyPaths,
}

/// A node whose geometry array is present and produced no tag: reported, never silent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Unpaintable {
    pub(crate) name: Option<String>,
    pub(crate) node_type: Option<String>,
    pub(crate) role: GeometryRole,
    pub(crate) reason: UnpaintableReason,
}

/// Why the walk did not descend into a node's children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnwalkedCause {
    /// An image fill ends the node (baseline `return` after the image record).
    ImageFill,
    /// A node with fill or stroke geometry is a leaf (baseline: BOOLEAN_OPERATION carries its own geometry).
    GeometryLeaf,
    /// Native stack guard; the baseline would throw a RangeError at an engine-dependent depth.
    DepthLimit,
}

/// A node whose children the baseline never visits, reported with how many it left behind.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Unwalked {
    pub(crate) name: Option<String>,
    pub(crate) node_type: Option<String>,
    pub(crate) children: usize,
    pub(crate) cause: UnwalkedCause,
}

/// The accumulator of `walkNode` (`out`), plus the additive reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WalkOut {
    pub(crate) svg_tags: Vec<String>,
    pub(crate) texts: Vec<TextRecord>,
    pub(crate) images: Vec<ImageRecord>,
    pub(crate) skipped: Vec<String>,
    pub(crate) unpaintable: Vec<Unpaintable>,
    pub(crate) unwalked: Vec<Unwalked>,
}
