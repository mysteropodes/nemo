//! U36 (#2362): the Labs generators (speed lines, boil, follow-path) as one native transaction each. The engine reads
//! the shown drawing of the target cell itself and runs the baseline algorithms of `src/js/labs/speed-lines.js`,
//! `boil-effect.js` and `follow-path.js` @1065e9d8 (`object_labs_generate_kernels.rs`): the same seeded RNG and call
//! order, the same frame growth and, like Repeat, the work-area end extended with it. Every generator writes whole
//! cells through the frame-paste kernel, so composition order and timing follow the ordinary paste rules: speed lines
//! rewrite the target cell as its shown drawing plus the new lines (the baseline `ensureKeyframe` then append), boil
//! and follow-path replace each written cell. One commit is one history entry; a refusal publishes nothing.
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    finish, layer_position, paste_cells, PasteCell,
};
use crate::commands::object_fill_owner::object_frame_clipboard_repeat::repeat_project_metadata;
use crate::commands::object_fill_owner::object_frame_edit_document::DocumentEdit;
use crate::commands::ObjectFillOwner;
use crate::commands::PreparedBooleanEdit;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::{ObjectDocument, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Value;

#[path = "object_labs_timing_actions.rs"]
mod actions;
#[path = "object_labs_generate_kernels.rs"]
mod kernels;
#[path = "object_labs_move.rs"]
mod moves;
pub(crate) use kernels::Ease;

#[path = "object_labs_request.rs"]
mod request;
pub(crate) use request::{parse_labs_request, LabsRequest, OPS};

const CONTEXT: &str = "scene-root";

/// The target layer of every generator: an unlocked, timed, non-component scene-root drawing layer (the baseline
/// refuses `locked || symbolId` with one toast; a layer without timing has no cells to write).
pub(super) fn check_layer(
    document: &ObjectDocument,
    uid: &str,
    frame: u32,
) -> Result<usize, DispatchErrorCode> {
    let index = layer_position(document, uid).ok_or(DispatchErrorCode::NotFound)?;
    if frame >= document.total_frames {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let component = document
        .component_set()
        .is_some_and(|set| set.instance(CONTEXT, uid).is_some());
    if document.layer_is_locked(uid) || component || document.layers[index].timing.value().is_none()
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(index)
}

/// Grow the timeline to `needed` frames when it is shorter (the baseline pushes plain frames on every layer; sparse
/// timing needs no change), then write the cells at `frame` on the layer.
pub(super) fn write_cells(
    document: &ObjectDocument,
    layer: usize,
    frame: u32,
    needed: u64,
    cells: &[PasteCell],
) -> Result<DocumentEdit, DispatchErrorCode> {
    let needed = u32::try_from(needed).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    let grown = if needed > document.total_frames {
        let mut grown = document.clone();
        grown.total_frames = needed;
        match finish(grown)? {
            DocumentEdit::Changed(next) => next,
            DocumentEdit::NoChange => return Err(DispatchErrorCode::Internal),
        }
    } else {
        document.clone()
    };
    paste_cells(&grown, layer, frame, cells)
}

/// A prepared Labs operation.
pub(crate) enum PreparedLabs {
    Generate(PreparedLabsGenerate),
    Fill(PreparedBooleanEdit),
}

/// The prepared transaction: the drawing and, only when the timeline grew, its work-area end.
pub(crate) struct PreparedLabsGenerate {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    metadata: Option<ProjectMetadata>,
}

impl ObjectFillOwner {
    /// Parse and prepare one Labs operation: a generator, or the reference fill, which is the ordinary bucket fill
    /// with other layers' walls and is committed as the boolean edit it is.
    pub(crate) fn prepare_labs(
        &self,
        operation: &str,
        payload: &Value,
    ) -> Result<PreparedLabs, DispatchErrorCode> {
        let request = parse_labs_request(operation, payload)?;
        if let LabsRequest::ReferenceFill {
            layer_uid,
            frame,
            point,
            color,
            opacity,
        } = &request
        {
            let origin = self.current_snapshot();
            let document = origin.document();
            layer_position(document, layer_uid).ok_or(DispatchErrorCode::NotFound)?;
            let instance = document
                .component_set()
                .is_some_and(|set| set.instance(CONTEXT, layer_uid).is_some());
            if *frame >= document.total_frames {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            if document.layer_is_locked(layer_uid) || instance {
                return Err(DispatchErrorCode::Unavailable);
            }
            let scope = Scope {
                context_id: CONTEXT.to_owned(),
                layer_uid: layer_uid.clone(),
                frame: *frame,
            };
            return self
                .prepare_fill_trace_at(scope, *point, color, *opacity, false, true)
                .map(PreparedLabs::Fill);
        }
        self.prepare_labs_generate(&request)
            .map(PreparedLabs::Generate)
    }

    fn prepare_labs_generate(
        &self,
        request: &LabsRequest,
    ) -> Result<PreparedLabsGenerate, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let edit = match request {
            LabsRequest::Pingpong {
                from_layer,
                to_layer,
                first,
                last,
                times,
            } => actions::pingpong(&origin, from_layer, to_layer, *first, *last, *times),
            LabsRequest::Retime {
                from_layer,
                to_layer,
                first,
                last,
                spacing,
            } => actions::retime(&origin, from_layer, to_layer, *first, *last, *spacing),
            LabsRequest::MoveToLayer {
                from_layer,
                to_layer,
                frame,
                stroke_ids,
            } => moves::move_to_layer(&origin, from_layer, to_layer, *frame, stroke_ids),
            LabsRequest::Interval {
                layer_uid,
                first,
                last,
                count,
                ease,
            } => actions::interval(&origin, layer_uid, *first, *last, *count, *ease),
            _ => kernels::generate(&origin, request),
        };
        let document = match edit? {
            DocumentEdit::Changed(next) => Some(next),
            DocumentEdit::NoChange => None,
        };
        let metadata = repeat_project_metadata(&origin, document.as_ref())?;
        Ok(PreparedLabsGenerate {
            origin,
            document,
            metadata,
        })
    }

    /// Publish the prepared generation if instance, document, snapshot and revision are still the candidate's.
    pub(crate) fn commit_labs_generate(
        &mut self,
        candidate: PreparedLabsGenerate,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish_frame_repeat(document, candidate.metadata)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_labs_generate.rs"]
mod tests;
