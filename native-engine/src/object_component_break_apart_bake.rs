//! The evaluation half of breaking a component apart: the instance of one scene layer, evaluated by the one component join at
//! every frame of the scene (held outer key, playback, Time Remap refusal, exposed overrides, component camera, `symMatrix`),
//! with identical consecutive frames collapsed into one key plus holds, exactly as `convertComponentToLayer` collapses the
//! JSON of `getEffectiveStrokes`. The records come back with the placement chain applied to their geometry (points through the
//! matrix, handles through its linear part, the way `applyMatrixToStrokeData` does) and everything else untouched; the plan
//! half (`object_component_break_apart.rs`) writes them into the layer. Nothing here mutates the snapshot.
use crate::evaluation::object_component_kernels::object_component_camera_ports::RecordPorts;
use crate::evaluation::object_component_kernels::object_component_override_port::PacketOverrides;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::ResolvedComponentPlacement;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::{
    evaluate_instance, ComponentInputs,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentUnavailable;
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::{FrameScopeKind, ObjectPoint, ObjectRecord};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

/// Why the instance could not be evaluated into records: a stable, user-facing sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BakeError(pub(crate) &'static str);

const NOT_EVALUABLE: BakeError =
    BakeError("component-break-apart: the instance could not be evaluated at every frame.");

fn why(reason: &ComponentUnavailable) -> BakeError {
    match reason {
        ComponentUnavailable::TimeRemap(_) => {
            BakeError("component-break-apart: Time Remap on the instance cannot be baked yet.")
        }
        ComponentUnavailable::Camera(_) => {
            BakeError("component-break-apart: the component camera needs the project canvas.")
        }
        ComponentUnavailable::InnerFeature { .. } => BakeError(
            "component-break-apart: an inner layer has a feature a flat layer cannot hold yet.",
        ),
        _ => NOT_EVALUABLE,
    }
}

fn through(chain: &[[f64; 6]], x: f64, y: f64) -> [f64; 2] {
    chain.iter().fold([x, y], |[x, y], [a, b, c, d, tx, ty]| {
        [x * a + y * c + tx, x * b + y * d + ty]
    })
}

fn number(value: f64) -> Result<Number, BakeError> {
    Number::from_f64(value).ok_or(NOT_EVALUABLE)
}

fn pair(point: &ObjectPoint) -> Result<[f64; 2], BakeError> {
    Ok([
        point.x.as_f64().ok_or(NOT_EVALUABLE)?,
        point.y.as_f64().ok_or(NOT_EVALUABLE)?,
    ])
}

/// `applyMatrixToStrokeData` over the stored geometry: points through every matrix of the chain, handles through its linear part.
fn place(record: &ObjectRecord, chain: &[[f64; 6]]) -> Result<ObjectRecord, BakeError> {
    let mut placed = record.clone();
    if chain.is_empty() {
        return Ok(placed);
    }
    let origin = through(chain, 0.0, 0.0);
    for segment in &mut placed.geometry.segments {
        let [x, y] = pair(&segment.point)?;
        let moved = through(chain, x, y);
        segment.point = ObjectPoint {
            x: number(moved[0])?,
            y: number(moved[1])?,
        };
        for handle in [&mut segment.handle_in, &mut segment.handle_out] {
            let [hx, hy] = pair(handle)?;
            let moved = through(chain, hx, hy);
            *handle = ObjectPoint {
                x: number(moved[0] - origin[0])?,
                y: number(moved[1] - origin[1])?,
            };
        }
    }
    Ok(placed)
}

fn chain_of(placement: &ResolvedComponentPlacement) -> Vec<[f64; 6]> {
    placement.chain().into_iter().map(|m| m.0).collect()
}

impl ObjectSnapshot {
    /// The key frames of the layer's instance, each with the records it shows (`convertComponentToLayer`'s `frames`).
    pub(crate) fn bake_component_instance(
        &self,
        layer_uid: &str,
    ) -> Result<Vec<(u32, Vec<ObjectRecord>)>, BakeError> {
        let document = self.document();
        let set = document.component_set().ok_or(NOT_EVALUABLE)?;
        let instance = set
            .instance(SCENE_CONTEXT, layer_uid)
            .ok_or(NOT_EVALUABLE)?;
        let host = document
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(NOT_EVALUABLE)?;
        let canvas = self.project().map(|p| {
            let (w, h) = p.project().canvas_dimensions();
            (f64::from(w), f64::from(h))
        });
        let has_camera = set
            .component(instance.symbol_id())
            .and_then(|c| c.camera_keys())
            .is_some_and(|keys| !keys.is_empty());
        if has_camera && canvas.is_none() {
            return Err(why(&ComponentUnavailable::Camera(
                "canvas size is not supplied",
            )));
        }
        let inputs = ComponentInputs {
            components: set,
            canvas: canvas.unwrap_or((0.0, 0.0)),
        };
        let mut ports = RecordPorts::new(set);
        let mut frames: Vec<(u32, Vec<ObjectRecord>)> = Vec::new();
        for frame in 0..document.total_frames {
            let selector = ObjectFrameSelector {
                instance_id: self.instance_id(),
                document_id: self.document_id(),
                content_revision: self.content_revision(),
                document_snapshot_id: self.snapshot_id(),
                context_id: SCENE_CONTEXT,
                scope_kind: FrameScopeKind::Authored,
                frame,
            };
            let packet =
                prepare_object_exposure_frame(self, &selector).map_err(|_| NOT_EVALUABLE)?;
            let outcome = evaluate_instance(
                &packet,
                &inputs,
                &mut ports,
                &mut PacketOverrides::new(&packet),
                instance,
                host,
            )
            .map_err(|reason| why(&reason))?;
            let records = match outcome {
                ComponentOutcome::Shapes(placed) => {
                    let chain = placed.placement.as_ref().map(chain_of).unwrap_or_default();
                    placed
                        .shapes
                        .iter()
                        .map(|shape| place(&shape.source, &chain))
                        .collect::<Result<Vec<_>, _>>()?
                }
                ComponentOutcome::Blank => Vec::new(),
                ComponentOutcome::Unavailable(reason) => return Err(why(&reason)),
            };
            if frames.last().is_none_or(|last| last.1 != records) {
                frames.push((frame, records));
            }
        }
        Ok(frames)
    }
}
