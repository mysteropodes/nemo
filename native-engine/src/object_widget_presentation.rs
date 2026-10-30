//! #2255: the on-canvas facts of every widget layer at one frame, from one immutable snapshot.
//!
//! A widget layer is never rendered, so the presented scene knows nothing about it. The hit-test and the overlay of the
//! ordinary joystick / slider need exactly what `geomFor`, `puckLocal` and `restLocal` of `src/js/rig-widget.js` computed:
//! the composed world anchor and angle, the pad size, the puck and rest offsets and each axis. This module returns them,
//! through the same pad kernel the write transaction uses (`widget_read.rs`), so the drag and the overlay cannot disagree.
//! Nothing is written, cached or kept; without the `evaluation` feature the read is a typed unavailable.
//!
//! The placement is `layerMotionAt` (the layer's own position and rotation, a flat offset with no pivot) then the parent
//! chain, exactly `geomFor`. A parent that pivots on its content bounds is reported as `parent_geometry`: only the
//! presented scene can know those bounds and a widget has none, so the read never guesses a pivot (a Null parent pivots on
//! `nullPos`, which is stored). Control values come from the raw sampler, like the write: an enabled expression on a
//! transform property is `expression`, never silently ignored.
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::Value;

impl ObjectSnapshot {
    /// One JSON item per widget layer, in document order.
    pub(super) fn widget_presentation_items(
        &self,
        frame: u32,
    ) -> Result<Vec<Value>, DispatchErrorCode> {
        #[cfg(feature = "evaluation")]
        {
            Ok(self
                .document()
                .layers()
                .iter()
                .enumerate()
                .filter(|(_, layer)| layer.widget().is_some())
                .map(|(index, _)| kernels::item(self, index, frame))
                .collect())
        }
        #[cfg(not(feature = "evaluation"))]
        {
            let _ = frame;
            Err(DispatchErrorCode::Unavailable)
        }
    }
}

#[cfg(feature = "evaluation")]
mod kernels {
    use crate::evaluation::motion_parent_chain::{
        motion_mat_from_values, transform_segment, MotionMat, Segment, CHAIN_NODE_LIMIT,
    };
    use crate::evaluation::object_image_mesh_kernels::object_widget_geometry::{
        axis_value, geom_for, puck_local, rest_local, AxisRest, AxisSample, AxisSpec,
        AxisValueSource, Placement, WidgetKind, WidgetSpec,
    };
    use crate::object_codec::{Presence, PropertyHolderTarget};
    use crate::revision::ObjectSnapshot;
    use serde_json::{json, Value};

    // The read-queries layer never names a document type: the stored axis and widget are reached by inference.
    macro_rules! axis_spec {
        ($axis:expr) => {
            AxisSpec {
                key: Some($axis.key.clone()),
                min: $axis.min_value(),
                max: $axis.max_value(),
                rest: match $axis.rest.value() {
                    None => AxisRest::Absent,
                    Some(None) => AxisRest::Null,
                    Some(Some(number)) => AxisRest::Value(number.as_f64().unwrap_or(f64::NAN)),
                },
            }
        };
    }
    macro_rules! axis_json {
        ($snapshot:expr, $layer:expr, $axis:expr, $spec:expr, $value:expr) => {{
            let rest = match $spec.rest {
                AxisRest::Absent => $spec.min,
                AxisRest::Null => 0.0,
                AxisRest::Value(v) => v,
            };
            let holders = $snapshot.document().property_holders().unwrap_or(&[]);
            json!({"key":$axis.key,"min":$spec.min,"max":$spec.max,"rest":rest,
                "label":$axis.display_label($layer.layer_uid(), holders),"value":$value})
        }};
    }

    fn layer_target(layer_uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: "scene-root".into(),
            layer_uid: layer_uid.into(),
            stroke_id: Presence::Missing,
        }
    }

    /// The raw sampler of one layer's controls, the port the kernel reads values through.
    struct Controls<'a> {
        snapshot: &'a ObjectSnapshot,
        target: PropertyHolderTarget,
    }
    impl AxisValueSource for Controls<'_> {
        fn sample(&self, key: &str, frame: f64) -> AxisSample {
            match self.snapshot.sample_property_raw(&self.target, key, frame) {
                Ok(values) => AxisSample::Values(values),
                Err(_) => AxisSample::Absent,
            }
        }
    }

    /// `computeMotionMat` of one layer at `frame`, from the raw sampler. An enabled expression is not evaluated here.
    fn layer_mat(
        snapshot: &ObjectSnapshot,
        layer_uid: &str,
        frame: f64,
    ) -> Result<Option<MotionMat>, &'static str> {
        let target = layer_target(layer_uid);
        let read = |property: &str, count: usize| -> Result<Vec<f64>, &'static str> {
            if snapshot
                .property_expression_source(&target, property)
                .ok()
                .flatten()
                .is_some_and(|source| source.enabled)
            {
                return Err("expression");
            }
            match snapshot.sample_property_raw(&target, property, frame) {
                Ok(values) if values.len() >= count && values.iter().all(|v| v.is_finite()) => {
                    Ok(values)
                }
                _ => Err("sample"),
            }
        };
        let position = read("position", 2)?;
        let anchor = read("anchor", 2)?;
        let rotation = read("rotation", 1)?;
        let scale = read("scale", 2)?;
        Ok(motion_mat_from_values(
            [position[0], position[1]],
            [anchor[0], anchor[1]],
            rotation[0],
            [scale[0], scale[1]],
            100.0,
        ))
    }

    /// `geomFor`'s anchor and accumulated angle: the stored anchor (or the canvas centre), the layer's own flat offset and
    /// rotation, then each parent that has a motion mat about its Null pivot.
    fn placement(
        snapshot: &ObjectSnapshot,
        layer_uid: &str,
        pos: Option<[f64; 2]>,
        frame: f64,
    ) -> Result<Placement, &'static str> {
        let canvas = snapshot
            .project()
            .map(|project| project.project().canvas_dimensions());
        let base = match (pos, canvas) {
            (Some(pos), _) => pos,
            (None, Some((w, h))) => [f64::from(w) / 2.0, f64::from(h) / 2.0],
            (None, None) => return Err("canvas"),
        };
        let own = layer_mat(snapshot, layer_uid, frame)?;
        let mut point = [
            base[0] + own.map_or(0.0, |m| m.dx),
            base[1] + own.map_or(0.0, |m| m.dy),
        ];
        let mut angle = own.map_or(0.0, |m| m.rot);
        let layers = snapshot.document().layers();
        let mut seen = vec![layer_uid];
        let mut next = layers
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .and_then(|layer| layer.parent_layer_uid.value().and_then(Option::as_deref));
        while let Some(uid) = next.filter(|_| seen.len() <= CHAIN_NODE_LIMIT) {
            if seen.contains(&uid) {
                break;
            }
            let parent = layers
                .iter()
                .find(|candidate| candidate.layer_uid() == uid)
                .ok_or("parent")?;
            seen.push(uid);
            if let Some(mat) = layer_mat(snapshot, uid, frame)? {
                let centre = parent
                    .null_pivot_base(canvas)
                    .map_err(|_| "canvas")?
                    .ok_or("parent_geometry")?;
                let pivot = [centre[0] + mat.ax, centre[1] + mat.ay];
                let moved = transform_segment(
                    &Segment {
                        point,
                        handle_in: None,
                        handle_out: None,
                    },
                    pivot,
                    &mat,
                );
                point = moved.point;
                angle += mat.rot;
            }
            next = parent.parent_layer_uid.value().and_then(Option::as_deref);
        }
        Ok(Placement {
            anchor: point,
            angle_degrees: angle,
        })
    }

    pub(super) fn item(snapshot: &ObjectSnapshot, index: usize, frame: u32) -> Value {
        let layer = &snapshot.document().layers()[index];
        let widget = layer.widget().expect("a widget layer");
        let kind = WidgetKind::parse(widget.kind.as_str()).expect("an admitted widget kind");
        let x = axis_spec!(widget.x);
        let y = widget.y_axis().map(|axis| axis_spec!(axis));
        let spec = WidgetSpec {
            kind,
            size: widget.size_value(),
            x,
            y,
        };
        let at = f64::from(frame);
        let source = Controls {
            snapshot,
            target: layer_target(layer.layer_uid()),
        };
        let value = |axis: Option<&AxisSpec>| axis_value(axis, at, &source).ok();
        let (vx, vy) = (value(Some(&spec.x)), value(spec.y.as_ref()));
        let y_json = widget
            .y_axis()
            .zip(spec.y.as_ref())
            .map(|(axis, y)| axis_json!(snapshot, layer, axis, y, vy));
        let mut out = json!({"layerUid":layer.layer_uid(),"visible":layer.visible(),
            "locked":layer.locked.value().copied().unwrap_or(false),"kind":widget.kind.as_str(),
            "x":axis_json!(snapshot, layer, widget.x, spec.x, vx),"y":y_json,
            "center":null,"angle":null,"size":null,"puck":null,"rest":null,"unavailable":null});
        match placement(snapshot, layer.layer_uid(), widget.pos_value(), at) {
            Err(reason) => out["unavailable"] = json!(reason),
            Ok(placed) => {
                let geometry = geom_for(&spec, &placed, at);
                out["center"] = json!([geometry.cx, geometry.cy]);
                out["angle"] = json!(geometry.angle);
                out["size"] = json!([geometry.w, geometry.h]);
                out["rest"] = json!(rest_local(&spec, &geometry));
                match puck_local(&spec, &geometry, &source) {
                    Ok(puck) => out["puck"] = json!(puck),
                    Err(_) => out["unavailable"] = json!("control"),
                }
            }
        }
        out
    }
}
