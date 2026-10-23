//! Read-queries facade for the parametric-shape kernels (G22, leaf L01-G); no state, no publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the sealed parameter edit reaches the L01-B shape builders and the
//! Paper.js move/scale arithmetic through the plain-number functions of this one file, exactly like
//! `ObjectSnapshot::sample_property_raw` reaches the raw sampler. Without the `evaluation` feature every
//! function reports `KernelUnavailable`; nothing is defaulted, guessed or approximated.
//!
//! Plain numbers in, plain numbers out: the stored record types belong to the document layer and are converted
//! by the caller. Inputs are borrowed and never changed, results are new owned values, so the caller publishes
//! them (or drops them) atomically.
//!
//! - [`rebuild`] is one panel commit of `applyParamShapeRect|Ellipse|Star(path)`: the anchors are rebuilt from
//!   the stored `paramShape.box` (a missing box is stamped from the CURRENT bounds, `paramShapeBoxOf`).
//!   The current bounds are computed only for a path whose handles are all exactly zero; a curved path without
//!   a box reports `MissingBox` because native curve bounds are not part of this leaf.
//! - [`translate`] is `p.translate(delta)` + `syncParamShapeBoxOnTranslate`.
//! - [`scale`] is `p.scale(sx, sy, anchor)` + `syncParamShapeBoxOnScale` (a rect with a box is rebuilt from the
//!   scaled box with its unchanged radii; every other kind keeps the scaled anchors).
use crate::request_receipts::DispatchErrorCode;

#[cfg(feature = "evaluation")]
use crate::evaluation::object_vector_kernels::object_parametric_geometry::boxed::{
    apply_param_shape, straight_bounds, sync_box_on_translate, sync_on_scale, ParamBox, ParamShape,
};
#[cfg(feature = "evaluation")]
use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    ParamGeomError, ParamSegment,
};
#[cfg(feature = "evaluation")]
use crate::evaluation::object_vector_kernels::object_parametric_transform::{
    scale_segments, translate_segments,
};

/// One anchor: point and RELATIVE handles, all present.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Anchor {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// `paramShape.box`: the axis-aligned generator frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    pub(crate) x2: f64,
    pub(crate) y2: f64,
}

/// The declared intent of a dynamic shape; an absent member is `None` (the baseline reads it with `||`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Intent {
    Rect {
        tl: Option<f64>,
        tr: Option<f64>,
        br: Option<f64>,
        bl: Option<f64>,
    },
    Ellipse {
        start_angle: Option<f64>,
        sweep: Option<f64>,
        inner_radius: Option<f64>,
    },
    Star {
        point_count: Option<f64>,
        inner_ratio: Option<f64>,
        corner_radius: Option<f64>,
    },
}

/// Why a shape edit could not be computed. Nothing is published when one is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeRefusal {
    /// The shape kernels are not part of this build.
    KernelUnavailable,
    /// A number is NaN, infinite or overflowed.
    NonFinite,
    /// `Math.round(pointCount || 5)` exceeds the native star budget.
    StarPointBudget,
    EmptyGeometry,
    /// No stored box and the current bounds are not computable without curve extrema.
    MissingBox,
}

impl ShapeRefusal {
    pub(crate) fn code(self) -> DispatchErrorCode {
        match self {
            ShapeRefusal::NonFinite | ShapeRefusal::StarPointBudget => {
                DispatchErrorCode::InvalidRequest
            }
            ShapeRefusal::KernelUnavailable
            | ShapeRefusal::EmptyGeometry
            | ShapeRefusal::MissingBox => DispatchErrorCode::Unavailable,
        }
    }
}

/// A rebuilt shape: the new anchors and the box to store (the stored one, or the stamped one).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rebuilt {
    pub(crate) anchors: Vec<Anchor>,
    pub(crate) frame: Frame,
}

/// A moved or scaled shape. `frame` is `None` when the shape had no box (the baseline stamps nothing);
/// `rebuilt` is true when the anchors are a fresh rect rebuild (whose path is closed again).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Moved {
    pub(crate) anchors: Vec<Anchor>,
    pub(crate) frame: Option<Frame>,
    pub(crate) rebuilt: bool,
}

#[cfg(feature = "evaluation")]
fn segment_of(anchor: &Anchor) -> ParamSegment {
    ParamSegment {
        point: anchor.point,
        handle_in: anchor.handle_in,
        handle_out: anchor.handle_out,
    }
}

#[cfg(feature = "evaluation")]
fn anchor_of(segment: &ParamSegment) -> Anchor {
    Anchor {
        point: segment.point,
        handle_in: segment.handle_in,
        handle_out: segment.handle_out,
    }
}

#[cfg(feature = "evaluation")]
fn segments_of(anchors: &[Anchor]) -> Vec<ParamSegment> {
    anchors.iter().map(segment_of).collect()
}

#[cfg(feature = "evaluation")]
fn anchors_of(segments: &[ParamSegment]) -> Vec<Anchor> {
    segments.iter().map(anchor_of).collect()
}

#[cfg(feature = "evaluation")]
fn kernel_box(frame: Frame) -> ParamBox {
    ParamBox {
        x1: frame.x1,
        y1: frame.y1,
        x2: frame.x2,
        y2: frame.y2,
    }
}

#[cfg(feature = "evaluation")]
fn frame_of(kernel: ParamBox) -> Frame {
    Frame {
        x1: kernel.x1,
        y1: kernel.y1,
        x2: kernel.x2,
        y2: kernel.y2,
    }
}

#[cfg(feature = "evaluation")]
fn kernel_shape(intent: &Intent) -> ParamShape {
    match *intent {
        Intent::Rect { tl, tr, br, bl } => ParamShape::Rect { tl, tr, br, bl },
        Intent::Ellipse {
            start_angle,
            sweep,
            inner_radius,
        } => ParamShape::Ellipse {
            start_angle,
            sweep,
            inner_radius,
        },
        Intent::Star {
            point_count,
            inner_ratio,
            corner_radius,
        } => ParamShape::Star {
            point_count,
            inner_ratio,
            corner_radius,
        },
    }
}

#[cfg(feature = "evaluation")]
fn refusal(error: ParamGeomError) -> ShapeRefusal {
    match error {
        ParamGeomError::NonFinite => ShapeRefusal::NonFinite,
        ParamGeomError::StarPointBudget => ShapeRefusal::StarPointBudget,
        ParamGeomError::EmptyGeometry => ShapeRefusal::EmptyGeometry,
        ParamGeomError::MissingBox => ShapeRefusal::MissingBox,
    }
}

/// `applyParamShapeRect|Ellipse|Star` over the stored anchors; `box_angle` is `data.boxAngle || 0`.
pub(crate) fn rebuild(
    intent: &Intent,
    stored: Option<Frame>,
    anchors: &[Anchor],
    box_angle: f64,
) -> Result<Rebuilt, ShapeRefusal> {
    #[cfg(feature = "evaluation")]
    {
        let current = segments_of(anchors);
        let applied = apply_param_shape(
            &kernel_shape(intent),
            stored.map(kernel_box),
            straight_bounds(&current),
            box_angle,
        )
        .map_err(refusal)?;
        Ok(Rebuilt {
            anchors: anchors_of(&applied.segments),
            frame: frame_of(applied.box_),
        })
    }
    #[cfg(not(feature = "evaluation"))]
    {
        let _ = (intent, stored, anchors, box_angle);
        Err(ShapeRefusal::KernelUnavailable)
    }
}

/// `p.translate(new Point(dx, dy))` and `syncParamShapeBoxOnTranslate(p, dx, dy)`.
pub(crate) fn translate(
    anchors: &[Anchor],
    stored: Option<Frame>,
    dx: f64,
    dy: f64,
) -> Result<Moved, ShapeRefusal> {
    #[cfg(feature = "evaluation")]
    {
        let moved = translate_segments(&segments_of(anchors), dx, dy).map_err(refusal)?;
        let frame = sync_box_on_translate(stored.map(kernel_box), dx, dy).map_err(refusal)?;
        Ok(Moved {
            anchors: anchors_of(&moved),
            frame: frame.map(frame_of),
            rebuilt: false,
        })
    }
    #[cfg(not(feature = "evaluation"))]
    {
        let _ = (anchors, stored, dx, dy);
        Err(ShapeRefusal::KernelUnavailable)
    }
}

/// `p.scale(sx, sy, anchor)` and `syncParamShapeBoxOnScale(p, sx, sy, anchor)`.
pub(crate) fn scale(
    intent: &Intent,
    anchors: &[Anchor],
    stored: Option<Frame>,
    box_angle: f64,
    factors: [f64; 2],
    pivot: [f64; 2],
) -> Result<Moved, ShapeRefusal> {
    #[cfg(feature = "evaluation")]
    {
        let scaled = scale_segments(&segments_of(anchors), factors[0], factors[1], pivot)
            .map_err(refusal)?;
        let synced = sync_on_scale(
            &kernel_shape(intent),
            stored.map(kernel_box),
            box_angle,
            factors[0],
            factors[1],
            pivot,
        )
        .map_err(refusal)?;
        Ok(match synced {
            None => Moved {
                anchors: anchors_of(&scaled),
                frame: None,
                rebuilt: false,
            },
            Some(sync) => match sync.rebuilt {
                Some(rebuilt) => Moved {
                    anchors: anchors_of(&rebuilt),
                    frame: Some(frame_of(sync.box_)),
                    rebuilt: true,
                },
                None => Moved {
                    anchors: anchors_of(&scaled),
                    frame: Some(frame_of(sync.box_)),
                    rebuilt: false,
                },
            },
        })
    }
    #[cfg(not(feature = "evaluation"))]
    {
        let _ = (intent, anchors, stored, box_angle, factors, pivot);
        Err(ShapeRefusal::KernelUnavailable)
    }
}
