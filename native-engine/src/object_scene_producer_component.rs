//! Component evaluated-shape kernel (L07-H; I16a, D32).
//!
//! Given the shapes of a component's internal frame and a RESOLVED placement
//! (internal frame, optional instance `symMatrix`, optional component camera
//! matrix), produce the detached, transformed copies that `getEffectiveStrokes`
//! (`src/js/app.js`, `symbolId` branch) hands to the scene at baseline `fbe4b8a3`:
//!
//! * `cloneStrokeForTransform` then `applyMatrixToStrokeData` per shape, the
//!   component camera FIRST and the instance `symMatrix` SECOND (the inner
//!   camera is baked into the content, the instance placement sits outside);
//! * a path keeps every non-geometry field; its points go through
//!   `Matrix.transform` (`x*a + y*c + tx`, `x*b + y*d + ty`) and its handles
//!   through the linear part only (`transform(h) - transform(0,0)`);
//! * a raster (truthy `isRaster`) becomes a NEW dict holding exactly
//!   `{isRaster, src, x, y, width, height, opacity}`: the centre is transformed,
//!   width/height scale by `sqrt(a*a+b*b)` / `sqrt(c*c+d*d)`, a rotation
//!   degrades to scale-only and every other field (`rotation`, `strokeId`,
//!   `meshId`, ...) is dropped, as the baseline does;
//! * a dict whose `segments` is falsy (absent, `null`, `0`, `""`, `false`) is
//!   returned unchanged.
//!
//! Detachment: the source slice is only borrowed and every output owns its
//! data, so the rest geometry is bit-identical before and after and no output
//! can alias it. Duplicator copies (`isDuplicatorCopy` / `dupIndex`, app.js
//! 873 and 1784) carry a typed synthetic origin and are never persistable.
//!
//! The camera matrix is NOT recomputed here: `ResolvedComponentPlacement`
//! calls `property_camera_eval::camera_matrix_at_frame`. Inputs the baseline
//! accepts but cannot give a meaning (a path handle that is not a number pair,
//! a raster without numeric `x`/`y`/`width`/`height`, a non-number opacity) are
//! typed refusals, and a result that overflows to NaN/Infinity is refused
//! instead of becoming JSON `null` later. Selecting a component's stored
//! shapes, path effects, element/layer Motion, the duplicator placement and
//! `applyOverrides` stay with their own producers.

use crate::evaluation::object_component_kernels::object_component_playback::ResolvedFrame;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::{
    truthy, Affine, ShapeData, ShapeError,
};
use crate::evaluation::property_camera_eval::{camera_matrix_at_frame, CameraKey};
use serde_json::Value;

/// A resolved component instance placement: the port L07-K fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedComponentPlacement {
    /// `resolveSymbolFrameIdx` result (`ii`), the camera is sampled here.
    pub(crate) inner_frame: ResolvedFrame,
    /// `ld.symMatrix`; any present array applies, even the identity.
    pub(crate) sym_matrix: Option<[f64; 6]>,
    /// `cameraMatrixAtFrame(sym.cameraKeys, ii, w, h)`; `None` for no keys.
    pub(crate) camera_matrix: Option<[f64; 6]>,
}

impl ResolvedComponentPlacement {
    /// Placement whose camera matrix is the existing `camera_matrix_at_frame`
    /// of the component's own keys (`None`/empty keys give no camera).
    pub(crate) fn from_camera_keys(
        inner_frame: ResolvedFrame,
        sym_matrix: Option<[f64; 6]>,
        camera_keys: Option<&[CameraKey]>,
        canvas_w: f64,
        canvas_h: f64,
    ) -> Self {
        let camera_matrix =
            camera_matrix_at_frame(camera_keys, inner_frame.raw(), canvas_w, canvas_h);
        ResolvedComponentPlacement {
            inner_frame,
            sym_matrix,
            camera_matrix,
        }
    }

    /// The matrices in application order: component camera, then `symMatrix`.
    pub(crate) fn chain(&self) -> Vec<Affine> {
        [self.camera_matrix, self.sym_matrix]
            .into_iter()
            .flatten()
            .map(Affine)
            .collect()
    }
}

/// Where an evaluated shape came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeOrigin {
    /// A stored component shape, transformed.
    Stored,
    /// A synthetic mograph duplicator copy (`isDuplicatorCopy`, `dupIndex`):
    /// materialised at load time and never written back to the frame.
    DuplicatorCopy { dup_index: u32 },
}

/// A detached evaluated shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EvaluatedShape {
    data: ShapeData,
    origin: ShapeOrigin,
}

impl EvaluatedShape {
    pub(crate) fn data(&self) -> &ShapeData {
        &self.data
    }

    pub(crate) fn origin(&self) -> ShapeOrigin {
        self.origin
    }

    /// `false` for a duplicator copy, whether typed here or tagged in the dict
    /// (`saveActiveLayerFrame` / `saveAllLayerFrames` must skip them).
    pub(crate) fn is_persistable(&self) -> bool {
        self.origin == ShapeOrigin::Stored
            && !self
                .data
                .extras()
                .get("isDuplicatorCopy")
                .is_some_and(truthy)
    }

    /// The stroke dict the baseline would hold, duplicator tags included.
    pub(crate) fn to_value(&self) -> Value {
        let mut value = self.data.to_value();
        if let (ShapeOrigin::DuplicatorCopy { dup_index }, Some(map)) =
            (self.origin, value.as_object_mut())
        {
            map.insert("isDuplicatorCopy".to_string(), Value::Bool(true));
            map.insert("dupIndex".to_string(), Value::from(dup_index));
        }
        value
    }
}

/// The inline lines of `applyLayerDuplicator`: transform the clone, multiply
/// its opacity by the copy's factor, then tag it a duplicator copy.
pub(crate) fn duplicator_copy(
    source: &ShapeData,
    m: &Affine,
    opacity_factor: f64,
    dup_index: u32,
) -> Result<EvaluatedShape, ShapeError> {
    let mut data = source.apply_affine(m)?;
    data.scale_opacity(opacity_factor)?;
    Ok(EvaluatedShape {
        data,
        origin: ShapeOrigin::DuplicatorCopy { dup_index },
    })
}

/// A shape failed, so nothing is published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EvaluateError {
    pub(crate) shape_index: usize,
    pub(crate) cause: ShapeError,
}

/// All shapes of one component instance at one internal frame, in source order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EvaluatedComponent {
    pub(crate) inner_frame: ResolvedFrame,
    pub(crate) shapes: Vec<EvaluatedShape>,
}

impl EvaluatedComponent {
    pub(crate) fn to_values(&self) -> Vec<Value> {
        self.shapes.iter().map(EvaluatedShape::to_value).collect()
    }
}

/// The `symbolId` branch tail of `getEffectiveStrokes`: every shape through the
/// placement chain (camera, then `symMatrix`). The result is always a set of
/// detached copies (the baseline returned the live stored dicts when neither
/// matrix applied; the content is identical).
pub(crate) fn evaluate_component_shapes(
    source: &[ShapeData],
    placement: &ResolvedComponentPlacement,
) -> Result<EvaluatedComponent, EvaluateError> {
    let chain = placement.chain();
    let mut shapes = Vec::with_capacity(source.len());
    for (shape_index, shape) in source.iter().enumerate() {
        let mut data = shape.clone();
        for m in &chain {
            data = data
                .apply_affine(m)
                .map_err(|cause| EvaluateError { shape_index, cause })?;
        }
        shapes.push(EvaluatedShape {
            data,
            origin: ShapeOrigin::Stored,
        });
    }
    Ok(EvaluatedComponent {
        inner_frame: placement.inner_frame,
        shapes,
    })
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_component_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component.rs"]
mod tests;
