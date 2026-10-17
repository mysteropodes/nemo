//! Component evaluation join (L04-K; I15, D33): one admitted frame query of the one native document
//! becomes the pinned component output every consumer reads.
//!
//! A frame query is `(instance, document, content revision, snapshot, context, scope, frame)`.
//! `evaluate_component_frame` admits it with the nested-context admission (`object_context_admission`,
//! L04-J: the four snapshot pins, the context against the declared contexts, the scope, the frame
//! against THAT context's own `totalFrames`) and then produces, per context kind:
//!
//! * `scene-root`: the packet is prepared by the existing frame-packet owner (`prepare_object_frame`;
//!   a document with Motion property holders the evaluation does not consume stays `Unavailable`, it is
//!   not painted flat), the component table is read from the pinned snapshot's document (L04-I,
//!   `ObjectDocument::component_set`) and the instances placed on the packet's layers are produced by
//!   the component producer join (L07-K: held outer key, playback and Time Remap of L04-A, inner layer
//!   records, exposed overrides of L04-E, component camera then `symMatrix` of L07-H) and joined in the
//!   packet's order plan under the CONSUMER's expected pin. A document without the table reports
//!   `NoComponents`, which is not an empty table.
//! * a declared component (the context entered through the workspace, `enterSymbol` of the baseline):
//!   the records its visible layers show at that frame of the component (`select_records`, the same
//!   selection the placed instance uses), detached copies with their stable `(context, layer, frame,
//!   stroke)` targets. No instance placement, playback or override applies inside: the baseline shows
//!   the symbol's own layers as ordinary layers, and ordinary layers obey their own time range
//!   (`getEffectiveStrokes` -> `layerHasTimeRange`). The entered view does not evaluate in points, out
//!   points or time links yet (D27), so a VISIBLE inner layer whose stored range gates frames
//!   (`ObjectLayer::range_gates_frames`) makes the whole query a typed `Inside(InnerFeature)`
//!   refusal, never every frame of that layer painted. The placed instance is unaffected: the
//!   baseline's symbol branch never reads an inner layer's range.
//!
//! `evaluate_packet` is the scene-root half for a caller that already holds a packet: the viewport
//! (`prepare_object_frame`) and the export (`prepare_object_exposure_frame`) both end here, so there is
//! one producer. Nothing is mutated: the snapshot is only read, the output owns every value, and a
//! missing provider is a typed outcome, never a default or an old JS/Paper path: the Time Remap
//! sampler and the component camera keys are the join's ports, the exposed-property runtime is the
//! `OverridePort`, and the canvas size the camera is sampled against is the caller's (an unknown canvas
//! with camera keys makes that instance's slot unavailable instead of guessing a size).

use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::evaluation::object_component_kernels::object_context_admission::admit_frame_context;
use crate::evaluation::object_component_kernels::object_context_resolver::ContextKind;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::{
    produce, ComponentInputs, ComponentPorts, ComponentProduceError,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentJoinError, ComponentScene,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    select_records, ComponentUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_document::ObjectRecord;
use crate::object_frame_packet::{
    prepare_object_frame, ObjectFrameError, ObjectFramePacket, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;

/// The reason an instance with camera keys is unavailable when the caller gave no canvas size.
pub(crate) const CANVAS_UNAVAILABLE: &str = "canvas size is not supplied";

/// The records an entered component shows at one of its own frames.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsideFrame {
    /// The consumer's pin of the query (context is the component's symbol id).
    pub(crate) pin: ScenePin,
    pub(crate) symbol_id: String,
    pub(crate) frame: u32,
    /// Visible layers in component order, record order as stored; detached copies.
    pub(crate) records: Vec<ObjectRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentFrame {
    /// A scene-root query on a document that declares no component table.
    NoComponents,
    /// Component slots joined in the packet's order plan.
    Scene(ComponentScene),
    /// A query inside a declared component.
    Inside(InsideFrame),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentFrameError {
    /// Pins, context, scope, frame range or packet availability, in the frame packet's own taxonomy.
    Frame(ObjectFrameError),
    /// The table is inadmissible against the packet layers, or an instance is not placed on one.
    Produce(ComponentProduceError),
    /// A pin differs (consumer against packet or production) or the order plan disagrees.
    Join(ComponentJoinError),
    /// The entered component's records cannot be selected (path effects, missing inner timing).
    Inside(ComponentUnavailable),
}

/// The canvas size is known to the consumer that holds the project, not to the drawing document.
struct CanvasGuard<'a> {
    inner: &'a mut dyn ComponentPorts,
    canvas_known: bool,
}

impl ComponentPorts for CanvasGuard<'_> {
    fn time_remap(&mut self, layer_uid: &str, frame: u32) -> Result<Option<f64>, &'static str> {
        self.inner.time_remap(layer_uid, frame)
    }
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        let keys = self.inner.camera_keys(symbol_id)?;
        if !self.canvas_known && keys.is_some_and(|keys| !keys.is_empty()) {
            return Err(CANVAS_UNAVAILABLE);
        }
        Ok(keys)
    }
}

/// The pin the consumer asked for, built from its own selector and not from the packet it got.
fn pin_of_selector(selector: &ObjectFrameSelector<'_>) -> ScenePin {
    ScenePin {
        instance_id: selector.instance_id.to_owned(),
        document_id: selector.document_id.to_owned(),
        content_revision: selector.content_revision,
        document_snapshot_id: selector.document_snapshot_id.to_owned(),
        context_id: selector.context_id.to_owned(),
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
    }
}

/// The scene-root half over a packet the caller already prepared. `None` is a document without the
/// component table. The packet must carry the pin the caller expects, table or not.
pub(crate) fn evaluate_packet(
    packet: &ObjectFramePacket,
    expected: &ScenePin,
    canvas: Option<(f64, f64)>,
    ports: &mut dyn ComponentPorts,
    overrides: &mut dyn OverridePort,
) -> Result<Option<ComponentScene>, ComponentFrameError> {
    if let Some(field) = ScenePin::of_packet(packet).first_mismatch(expected) {
        return Err(ComponentFrameError::Join(ComponentJoinError::WrongPin {
            producer: None,
            field,
        }));
    }
    let Some(components) = packet.snapshot().document().component_set() else {
        return Ok(None);
    };
    let mut guarded = CanvasGuard {
        inner: ports,
        canvas_known: canvas.is_some(),
    };
    let inputs = ComponentInputs {
        components,
        canvas: canvas.unwrap_or((0.0, 0.0)),
    };
    let production =
        produce(packet, &inputs, &mut guarded, overrides).map_err(ComponentFrameError::Produce)?;
    production
        .join(packet, expected)
        .map(Some)
        .map_err(ComponentFrameError::Join)
}

/// One admitted frame query in, one pinned component output out; see the module comment.
pub(crate) fn evaluate_component_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    canvas: Option<(f64, f64)>,
    ports: &mut dyn ComponentPorts,
    overrides: &mut dyn OverridePort,
) -> Result<ComponentFrame, ComponentFrameError> {
    let admitted = admit_frame_context(snapshot, selector).map_err(ComponentFrameError::Frame)?;
    let expected = pin_of_selector(selector);
    match admitted.context.kind() {
        ContextKind::SceneRoot => {
            let packet =
                prepare_object_frame(snapshot, selector).map_err(ComponentFrameError::Frame)?;
            Ok(
                match evaluate_packet(&packet, &expected, canvas, ports, overrides)? {
                    Some(scene) => ComponentFrame::Scene(scene),
                    None => ComponentFrame::NoComponents,
                },
            )
        }
        ContextKind::Component => {
            let Some(component) = admitted.context.component() else {
                return Err(ComponentFrameError::Frame(ObjectFrameError::InvalidContext));
            };
            if let Some(layer) = component
                .layers()
                .iter()
                .find(|layer| layer.visible() && layer.range_gates_frames())
            {
                return Err(ComponentFrameError::Inside(
                    ComponentUnavailable::InnerFeature {
                        layer_uid: layer.layer_uid().to_owned(),
                        feature: "layer range",
                    },
                ));
            }
            let records =
                select_records(component, admitted.frame).map_err(ComponentFrameError::Inside)?;
            Ok(ComponentFrame::Inside(InsideFrame {
                pin: expected,
                symbol_id: component.symbol_id().to_owned(),
                frame: admitted.frame,
                records,
            }))
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_component_evaluate_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_evaluate_frames.rs"]
mod frames_tests;
#[cfg(test)]
#[path = "../tests/object_component_evaluate_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_component_evaluate_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/object_component_evaluate.rs"]
mod tests;
