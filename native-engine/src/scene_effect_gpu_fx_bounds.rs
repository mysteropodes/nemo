//! The box an fx pass is attached to, ported from the baseline
//! `items_bbox_px` and `document_bbox_px` (`geometry-wasm/src/engine.rs`).
//!
//! Pure `f64` affine math with the exact kurbo 0.13 formulas, narrowed to `f32`
//! at the end. Item geometry is supplied already resolved by the scene
//! producer (the local bounding box of a path, or an image rectangle); an item
//! the producer could not resolve is simply left out, as the baseline `continue`s
//! past it. This module owns no scene, path store or viewport state.

/// `[a, b, c, d, e, f]` of `x' = a*x + c*y + e`, `y' = b*x + d*y + f`
/// (kurbo `Affine`).
pub(crate) type Affine = [f64; 6];

const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// One painted item of the layer, in the shape `items_bbox_px` reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FxItemBounds {
    /// A placed image: its rectangle and a rotation in degrees about its centre.
    Image {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rotation_degrees: f64,
    },
    /// A path: the bounding box `[x0, y0, x1, y1]` of its geometry and the
    /// optional `pathTransform` that places it.
    Path {
        bounds: [f64; 4],
        transform: Option<Affine>,
    },
}

/// The editor viewport (`Viewport` of the baseline engine).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FxViewport {
    pub(crate) pan_x: f64,
    pub(crate) pan_y: f64,
    pub(crate) zoom: f64,
    pub(crate) rotation: f64,
    pub(crate) pivot_x: f64,
    pub(crate) pivot_y: f64,
}

fn multiply(a: Affine, b: Affine) -> Affine {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

fn translate(x: f64, y: f64) -> Affine {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn rotate(radians: f64) -> Affine {
    let (sin, cos) = radians.sin_cos();
    [cos, sin, -sin, cos, 0.0, 0.0]
}

fn apply(m: Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `Affine::transform_rect_bbox`: the box of the four transformed corners,
/// `[x0, y0, x1, y1]` with non-negative extent.
fn transform_rect_bbox(m: Affine, rect: [f64; 4]) -> [f64; 4] {
    let p00 = apply(m, rect[0], rect[1]);
    let p01 = apply(m, rect[0], rect[3]);
    let p10 = apply(m, rect[2], rect[1]);
    let p11 = apply(m, rect[2], rect[3]);
    let first = [
        p00.0.min(p01.0),
        p00.1.min(p01.1),
        p00.0.max(p01.0),
        p00.1.max(p01.1),
    ];
    let second = [
        p10.0.min(p11.0),
        p10.1.min(p11.1),
        p10.0.max(p11.0),
        p10.1.max(p11.1),
    ];
    [
        first[0].min(second[0]),
        first[1].min(second[1]),
        first[2].max(second[2]),
        first[3].max(second[3]),
    ]
}

/// `Viewport::transform`: pan, then pivot, rotation, zoom, and back from the pivot.
pub(crate) fn viewport_transform(viewport: &FxViewport) -> Affine {
    let mut m = multiply(
        translate(viewport.pan_x, viewport.pan_y),
        translate(viewport.pivot_x, viewport.pivot_y),
    );
    m = multiply(m, rotate(viewport.rotation));
    m = multiply(m, [viewport.zoom, 0.0, 0.0, viewport.zoom, 0.0, 0.0]);
    multiply(m, translate(-viewport.pivot_x, -viewport.pivot_y))
}

fn item_rect(item: &FxItemBounds, view_transform: Affine) -> [f64; 4] {
    match item {
        FxItemBounds::Image {
            x,
            y,
            width,
            height,
            rotation_degrees,
        } => {
            let rect = [*x, *y, *x + *width, *y + *height];
            let place = if *rotation_degrees != 0.0 {
                let (cx, cy) = (*x + *width / 2.0, *y + *height / 2.0);
                multiply(
                    multiply(translate(cx, cy), rotate(rotation_degrees.to_radians())),
                    translate(-cx, -cy),
                )
            } else {
                IDENTITY
            };
            transform_rect_bbox(multiply(view_transform, place), rect)
        }
        FxItemBounds::Path { bounds, transform } => {
            let item_transform = match transform {
                Some(m) => multiply(view_transform, *m),
                None => view_transform,
            };
            transform_rect_bbox(item_transform, *bounds)
        }
    }
}

/// `items_bbox_px`: the union device-pixel box of `items` under `view_transform`
/// as `(x, y, width, height)`; the full canvas when no item resolves to a box
/// wider and taller than half a pixel.
pub(crate) fn items_bbox_px(
    items: &[FxItemBounds],
    view_transform: Affine,
    canvas: (u32, u32),
) -> (f32, f32, f32, f32) {
    let mut union: Option<[f64; 4]> = None;
    for item in items {
        let rect = item_rect(item, view_transform);
        union = Some(match union {
            Some(a) => [
                a[0].min(rect[0]),
                a[1].min(rect[1]),
                a[2].max(rect[2]),
                a[3].max(rect[3]),
            ],
            None => rect,
        });
    }
    match union {
        Some(r) if r[2] - r[0] > 0.5 && r[3] - r[1] > 0.5 => (
            r[0] as f32,
            r[1] as f32,
            (r[2] - r[0]) as f32,
            (r[3] - r[1]) as f32,
        ),
        _ => (0.0, 0.0, canvas.0 as f32, canvas.1 as f32),
    }
}

/// `document_bbox_px`: the document `(0, 0, 2*pivot_x, 2*pivot_y)` under the
/// viewport, the box of an adjustment/effect layer. A viewport without a pivot
/// keeps the full target.
pub(crate) fn document_bbox_px(viewport: &FxViewport, canvas: (u32, u32)) -> (f32, f32, f32, f32) {
    if viewport.pivot_x <= 0.0 || viewport.pivot_y <= 0.0 {
        return (0.0, 0.0, canvas.0 as f32, canvas.1 as f32);
    }
    let document = [0.0, 0.0, viewport.pivot_x * 2.0, viewport.pivot_y * 2.0];
    let r = transform_rect_bbox(viewport_transform(viewport), document);
    (
        r[0] as f32,
        r[1] as f32,
        (r[2] - r[0]) as f32,
        (r[3] - r[1]) as f32,
    )
}
