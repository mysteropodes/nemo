//! Read-queries facade for the widget write kernel (F19); no state, no publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the widget transaction reaches the L03-B pad geometry and
//! value mapping through this one method on the immutable snapshot, exactly like
//! `ObjectSnapshot::sample_property_raw` reaches the raw sampler. Without the `evaluation` feature the
//! method reports `KernelUnavailable`; nothing is defaulted or guessed.
//!
//! What it does: take the stored widget of one layer, build the kernel's `WidgetSpec` from it, map a
//! drag intent to a pad-local point (`toLocal` for a world point, `restLocal` for the rest return,
//! the point itself otherwise) and return the control writes `writeFromLocal` asks for, X first, then Y
//! (joystick with a Y axis only). Placement (anchor and accumulated angle) is supplied by the caller
//! because its composition belongs to the transform provider; a local or rest intent never reads it.
//!
//! Deliberate differences from the baseline `writeFromLocal`: an axis without a control key is a typed
//! refusal instead of a write to a null key, and a non-finite result is refused instead of written.
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

/// Where a widget drag lands, before the pad geometry maps it to control values.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WidgetIntent {
    /// A point already in pad-local coordinates (pad centre at the origin, Y down).
    Local { point: [f64; 2] },
    /// A world point plus the composed placement of the pad.
    World {
        point: [f64; 2],
        anchor: [f64; 2],
        angle_degrees: f64,
    },
    /// The puck returns to the rest marker (the baseline double-click).
    Rest,
}

/// One control value the drag asks for.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "evaluation"), allow(dead_code))]
pub(crate) struct WidgetAxisWrite {
    pub(crate) key: String,
    pub(crate) value: f64,
}

/// Why a write set could not be planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "evaluation"), allow(dead_code))]
pub(crate) enum WidgetWriteRefusal {
    LayerAbsent,
    NotAWidget,
    /// An axis has no usable control key.
    AxisWithoutKey,
    /// The mapped value is NaN or infinite.
    NonFiniteValue,
    /// The geometry kernel is not part of this build.
    KernelUnavailable,
}

impl WidgetWriteRefusal {
    pub(crate) fn code(self) -> DispatchErrorCode {
        match self {
            WidgetWriteRefusal::LayerAbsent | WidgetWriteRefusal::NotAWidget => {
                DispatchErrorCode::NotFound
            }
            WidgetWriteRefusal::AxisWithoutKey | WidgetWriteRefusal::KernelUnavailable => {
                DispatchErrorCode::Unavailable
            }
            WidgetWriteRefusal::NonFiniteValue => DispatchErrorCode::InvalidRequest,
        }
    }
}

impl ObjectSnapshot {
    /// The control writes the drag `intent` asks of the widget on `layer_uid`, sampled from this exact
    /// retained snapshot. Nothing is written; the caller issues each write through the property owner.
    pub(crate) fn widget_write_values(
        &self,
        layer_uid: &str,
        frame: f64,
        intent: &WidgetIntent,
    ) -> Result<Vec<WidgetAxisWrite>, WidgetWriteRefusal> {
        #[cfg(feature = "evaluation")]
        {
            plan(self, layer_uid, frame, intent)
        }
        #[cfg(not(feature = "evaluation"))]
        {
            let _ = (layer_uid, frame, intent);
            Err(WidgetWriteRefusal::KernelUnavailable)
        }
    }
}

#[cfg(feature = "evaluation")]
fn axis_spec(
    key: &str,
    min: f64,
    max: f64,
    rest: Option<&Option<serde_json::Number>>,
) -> crate::evaluation::object_image_mesh_kernels::object_widget_geometry::AxisSpec {
    use crate::evaluation::object_image_mesh_kernels::object_widget_geometry::{
        AxisRest, AxisSpec,
    };
    let rest = match rest {
        None => AxisRest::Absent,
        Some(None) => AxisRest::Null,
        Some(Some(number)) => AxisRest::Value(number.as_f64().unwrap_or(f64::NAN)),
    };
    AxisSpec {
        key: Some(key.to_owned()),
        min,
        max,
        rest,
    }
}

#[cfg(feature = "evaluation")]
fn plan(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: f64,
    intent: &WidgetIntent,
) -> Result<Vec<WidgetAxisWrite>, WidgetWriteRefusal> {
    use crate::evaluation::object_image_mesh_kernels::object_widget_geometry::{
        geom_for, rest_local, to_local, write_from_local, Placement, WidgetGeometryError,
        WidgetKind, WidgetSpec,
    };
    let layer = snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == layer_uid)
        .ok_or(WidgetWriteRefusal::LayerAbsent)?;
    let widget = layer.widget().ok_or(WidgetWriteRefusal::NotAWidget)?;
    let kind =
        WidgetKind::parse(widget.kind.as_str()).map_err(|_| WidgetWriteRefusal::NotAWidget)?;
    let spec = WidgetSpec {
        kind,
        size: widget.size_value(),
        x: axis_spec(
            &widget.x.key,
            widget.x.min_value(),
            widget.x.max_value(),
            widget.x.rest.value(),
        ),
        y: widget
            .y_axis()
            .map(|y| axis_spec(&y.key, y.min_value(), y.max_value(), y.rest.value())),
    };
    let neutral = Placement {
        anchor: [0.0, 0.0],
        angle_degrees: 0.0,
    };
    let (geometry, local) = match intent {
        WidgetIntent::Local { point } => (geom_for(&spec, &neutral, frame), *point),
        WidgetIntent::World {
            point,
            anchor,
            angle_degrees,
        } => {
            let placement = Placement {
                anchor: *anchor,
                angle_degrees: *angle_degrees,
            };
            let geometry = geom_for(&spec, &placement, frame);
            let local = to_local(&geometry, point[0], point[1]);
            (geometry, local)
        }
        WidgetIntent::Rest => {
            let geometry = geom_for(&spec, &neutral, frame);
            let local = rest_local(&spec, &geometry);
            (geometry, local)
        }
    };
    let writes =
        write_from_local(&spec, &geometry, local[0], local[1]).map_err(|error| match error {
            WidgetGeometryError::InvalidKind => WidgetWriteRefusal::NotAWidget,
            WidgetGeometryError::AxisUnavailable { .. } => WidgetWriteRefusal::AxisWithoutKey,
        })?;
    let mut out = vec![WidgetAxisWrite {
        key: writes.x.key,
        value: writes.x.value,
    }];
    if let Some(y) = writes.y {
        out.push(WidgetAxisWrite {
            key: y.key,
            value: y.value,
        });
    }
    if out.iter().any(|write| !write.value.is_finite()) {
        return Err(WidgetWriteRefusal::NonFiniteValue);
    }
    Ok(out)
}
