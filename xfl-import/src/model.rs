//! Neutral document model. Coordinates are in pixels (XFL stores twips,
//! 1/20 px; the edge decoder divides). Nothing here is Nemo-specific: the
//! mapping onto a Nemo document is a separate, later slice.

use serde::Serialize;

use crate::bitmap::BitmapData;
use crate::report::ImportReport;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// sRGB colour, 8 bits per channel, alpha in 0..=1 (XFL keeps alpha separate).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 1.0,
    };

    /// `#rrggbbaa`, the form Nemo persists colours with alpha in.
    pub fn hex8(&self) -> String {
        let a = (self.a.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, a)
    }
}

/// Affine transform `[a c tx; b d ty]`, the XFL `<Matrix>` convention.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn apply(&self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.tx,
            y: self.b * p.x + self.d * p.y + self.ty,
        }
    }

    /// `self ∘ other`: apply `other` first, then `self`.
    pub fn then_after(&self, other: &Matrix) -> Matrix {
        Matrix {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Matrix::IDENTITY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Segment {
    Line { to: Point },
    Quad { ctrl: Point, to: Point },
}

impl Segment {
    pub fn end(&self) -> Point {
        match self {
            Segment::Line { to } | Segment::Quad { to, .. } => *to,
        }
    }

    /// Exact degree elevation of a quadratic to a cubic (the 2/3 rule).
    /// A line becomes a cubic whose handles sit on its end points.
    pub fn to_cubic(&self, from: Point) -> (Point, Point, Point) {
        match *self {
            Segment::Line { to } => (from, to, to),
            Segment::Quad { ctrl, to } => (
                Point {
                    x: from.x + 2.0 / 3.0 * (ctrl.x - from.x),
                    y: from.y + 2.0 / 3.0 * (ctrl.y - from.y),
                },
                Point {
                    x: to.x + 2.0 / 3.0 * (ctrl.x - to.x),
                    y: to.y + 2.0 / 3.0 * (ctrl.y - to.y),
                },
                to,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Contour {
    pub start: Point,
    pub segments: Vec<Segment>,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GradientStop {
    /// 0..=1 (XFL stores 0..=1 already).
    pub ratio: f64,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Paint {
    Solid {
        color: Rgba,
    },
    /// Gradient space: the unit gradient spans x in -819.2..819.2 px before `matrix`.
    Linear {
        matrix: Matrix,
        stops: Vec<GradientStop>,
        spread: Spread,
    },
    Radial {
        matrix: Matrix,
        stops: Vec<GradientStop>,
        spread: Spread,
        focal_ratio: f64,
    },
    Bitmap {
        bitmap_path: String,
        matrix: Matrix,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stroke {
    pub paint: Paint,
    pub weight: f64,
    pub caps: String,
    pub joints: String,
    pub miter_limit: f64,
    pub scale_mode: Option<String>,
    /// `SolidStroke`, `DashedStroke`… Non-solid styles are reported and drawn solid.
    pub style: String,
}

/// One fill style or one stroke style of a shape, with all its contours.
/// A fill path may hold several loops (outer boundaries and holes, with
/// opposite windings); splitting islands is the mapping slice's job.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapePath {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub contours: Vec<Contour>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shape {
    pub matrix: Matrix,
    pub is_drawing_object: bool,
    pub paths: Vec<ShapePath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolType {
    Graphic,
    MovieClip,
    Button,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SymbolInstance {
    pub library_item: String,
    pub symbol_type: SymbolType,
    /// `loop`, `play once`, `single frame` (graphic symbols); movie clips always loop.
    pub loop_mode: String,
    pub first_frame: u32,
    pub matrix: Matrix,
    pub transformation_point: Point,
    /// Colour transform alpha multiplier; tint/brightness/advanced are reported.
    pub alpha: f64,
    pub instance_name: Option<String>,
    pub blend_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Element {
    Shape(Shape),
    Group {
        members: Vec<Element>,
    },
    SymbolInstance(SymbolInstance),
    BitmapInstance {
        library_item: String,
        matrix: Matrix,
    },
    Text {
        element: String,
        matrix: Matrix,
        text: String,
    },
    /// Read but not modelled (reported).
    Other {
        element: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TweenKind {
    /// Classic tween.
    Motion,
    Shape,
    /// CS4+ motion tween (property XML); reported, converted in v2.
    MotionObject,
    Other(String),
}

/// One `<Ease>` or `<CustomEase>` child of a frame's `<tweens>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TweenEase {
    /// `all`, `position`, `rotation`, `scale`, `color` or `filters`.
    pub target: String,
    /// `true` for `<CustomEase>` (uses `points`), `false` for `<Ease>`.
    pub custom: bool,
    /// `<Ease intensity>`: a simple ease, -100 ..= 100.
    pub intensity: Option<f64>,
    /// `<Ease method>`: a named preset (kept, not evaluated).
    pub method: Option<String>,
    /// Anchor, out-control, in-control, anchor, …; x is time, y is progress.
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tween {
    pub kind: TweenKind,
    /// Simple ease, -100 (ease in) ..= 100 (ease out).
    pub acceleration: f64,
    /// `hasCustomEase`, or a `<CustomEase>` is present when the attribute is absent.
    pub custom_ease: bool,
    /// `useSingleEaseCurve`: one `all` curve drives every property.
    pub use_single_ease_curve: Option<bool>,
    pub eases: Vec<TweenEase>,
    pub rotate: Option<String>,
    pub rotate_times: u32,
    /// `motionTweenScale` (default true).
    pub tween_scale: bool,
    /// `motionTweenOrientToPath`.
    pub orient_to_path: bool,
    /// `motionTweenSnap`: an authoring aid (the instance was snapped onto the
    /// guide when placed); it does not change how the tween is sampled.
    pub snap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    pub name: String,
    /// `name`, `comment` or `anchor`.
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Frame {
    /// 0-based first frame of this keyframe span.
    pub index: u32,
    /// Number of frames the keyframe is held for (at least 1).
    pub duration: u32,
    pub label: Option<Label>,
    pub tween: Option<Tween>,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerKind {
    Normal,
    Folder,
    Guide,
    Mask,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layer {
    pub name: String,
    pub kind: LayerKind,
    /// Index of the folder, mask or guide layer this layer sits under.
    pub parent_layer_index: Option<usize>,
    pub visible: bool,
    pub locked: bool,
    pub outline: bool,
    pub color: Option<String>,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Timeline {
    pub name: String,
    pub layers: Vec<Layer>,
}

impl Timeline {
    /// Length in frames (end of the last keyframe span across layers).
    pub fn frame_count(&self) -> u32 {
        self.layers
            .iter()
            .flat_map(|l| l.frames.iter())
            .map(|f| f.index.saturating_add(f.duration))
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibrarySymbol {
    pub name: String,
    pub symbol_type: SymbolType,
    pub timeline: Timeline,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryBitmap {
    pub name: String,
    pub href: Option<String>,
    /// Entry under `bin/` holding the pixels.
    pub bitmap_data_href: Option<String>,
    pub data_present: bool,
    /// Decoded or encoded pixels; `None` when missing or not decodable (reported).
    pub data: Option<BitmapData>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XflDocument {
    pub width: f64,
    pub height: f64,
    pub frame_rate: f64,
    pub background: Rgba,
    pub timelines: Vec<Timeline>,
    pub symbols: Vec<LibrarySymbol>,
    pub bitmaps: Vec<LibraryBitmap>,
    pub report: ImportReport,
}
