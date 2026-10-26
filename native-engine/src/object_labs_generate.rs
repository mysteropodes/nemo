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
use serde_json::{Map, Value};

#[path = "object_labs_generate_kernels.rs"]
mod kernels;
pub(crate) use kernels::Ease;

/// One public operation per generator; the payload `command` repeats the operation suffix.
pub(crate) const OPS: [&str; 4] = [
    "command.document.object.labs.speedLines",
    "command.document.object.labs.boil",
    "command.document.object.labs.followPath",
    "command.document.object.labs.referenceFill",
];
const CONTEXT: &str = "scene-root";

/// One typed generator request; every command admits exactly its own fields.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LabsRequest {
    SpeedLines {
        layer_uid: String,
        frame: u32,
        center: [f64; 2],
        count: u32,
        r0: f64,
        r1: f64,
        jitter: f64,
        seed: u32,
        color: [f64; 4],
        brush_size: f64,
        opacity: f64,
    },
    Boil {
        layer_uid: String,
        frame: u32,
        frames: u32,
        amplitude: f64,
        seed_step: i64,
    },
    FollowPath {
        layer_uid: String,
        frame: u32,
        path_id: String,
        stroke_ids: Vec<String>,
        frames: u32,
        ease: Ease,
        remove_trajectory: bool,
    },
    ReferenceFill {
        layer_uid: String,
        frame: u32,
        point: [f64; 2],
        color: String,
        opacity: f64,
    },
}

fn fields<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, DispatchErrorCode> {
    let map = value.as_object().ok_or(DispatchErrorCode::InvalidRequest)?;
    if map.len() != keys.len() || !keys.iter().all(|key| map.contains_key(*key)) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(map)
}
fn number(
    map: &Map<String, Value>,
    key: &str,
    min: f64,
    max: f64,
) -> Result<f64, DispatchErrorCode> {
    map[key]
        .as_f64()
        .filter(|v| v.is_finite() && (min..=max).contains(v))
        .ok_or(DispatchErrorCode::InvalidRequest)
}
fn integer(
    map: &Map<String, Value>,
    key: &str,
    min: u64,
    max: u64,
) -> Result<u32, DispatchErrorCode> {
    map[key]
        .as_u64()
        .filter(|v| (min..=max).contains(v))
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(DispatchErrorCode::InvalidRequest)
}
fn text(map: &Map<String, Value>, key: &str) -> Result<String, DispatchErrorCode> {
    map[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .map(str::to_owned)
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// Strict payload: the baseline clamps happen in the caller, so an out-of-range value here is refused, not clamped.
pub(crate) fn parse_labs_request(
    operation: &str,
    payload: &Value,
) -> Result<LabsRequest, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .ok_or(InvalidRequest)?;
    let suffix = operation
        .strip_prefix("command.document.")
        .ok_or(InvalidRequest)?;
    if !OPS.contains(&operation) || command != suffix {
        return Err(InvalidRequest);
    }
    match command {
        "object.labs.speedLines" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "cx",
                    "cy",
                    "count",
                    "r0",
                    "r1",
                    "jitter",
                    "seed",
                    "color",
                    "brushSize",
                    "opacity",
                ],
            )?;
            let color = m["color"]
                .as_array()
                .filter(|c| c.len() == 4)
                .ok_or(InvalidRequest)?;
            let mut rgba = [0.0; 4];
            for (slot, value) in rgba.iter_mut().zip(color) {
                *slot = value
                    .as_f64()
                    .filter(|v| (0.0..=1.0).contains(v))
                    .ok_or(InvalidRequest)?;
            }
            const SPAN: f64 = 1e7;
            Ok(LabsRequest::SpeedLines {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                center: [number(m, "cx", -SPAN, SPAN)?, number(m, "cy", -SPAN, SPAN)?],
                count: integer(m, "count", 3, 400)?,
                r0: number(m, "r0", -SPAN, SPAN)?,
                r1: number(m, "r1", -SPAN, SPAN)?,
                jitter: number(m, "jitter", 0.0, 1.0)?,
                seed: integer(m, "seed", 0, u64::from(u32::MAX))?,
                color: rgba,
                brush_size: number(m, "brushSize", 0.0, 10_000.0)?,
                opacity: number(m, "opacity", 0.0, 1.0)?,
            })
        }
        "object.labs.boil" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "frames",
                    "amplitude",
                    "seedStep",
                ],
            )?;
            let seed_step = m["seedStep"]
                .as_i64()
                .filter(|v| v.unsigned_abs() <= u64::from(u32::MAX))
                .ok_or(InvalidRequest)?;
            Ok(LabsRequest::Boil {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                frames: integer(m, "frames", 1, 24)?,
                amplitude: number(m, "amplitude", 0.2, 1e6)?,
                seed_step,
            })
        }
        "object.labs.followPath" => {
            let m = fields(
                payload,
                &[
                    "command",
                    "layerUid",
                    "frame",
                    "pathId",
                    "strokeIds",
                    "frames",
                    "ease",
                    "removeTrajectory",
                ],
            )?;
            let ids = m["strokeIds"]
                .as_array()
                .filter(|ids| (1..=256).contains(&ids.len()))
                .ok_or(InvalidRequest)?;
            let mut stroke_ids = Vec::with_capacity(ids.len());
            for id in ids {
                let id = id
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or(InvalidRequest)?;
                if stroke_ids.iter().any(|seen: &String| seen == id) {
                    return Err(InvalidRequest);
                }
                stroke_ids.push(id.to_owned());
            }
            Ok(LabsRequest::FollowPath {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                path_id: text(m, "pathId")?,
                stroke_ids,
                frames: integer(m, "frames", 2, 120)?,
                ease: Ease::parse(m["ease"].as_str().ok_or(InvalidRequest)?)?,
                remove_trajectory: m["removeTrajectory"].as_bool().ok_or(InvalidRequest)?,
            })
        }
        "object.labs.referenceFill" => {
            let m = fields(
                payload,
                &["command", "layerUid", "frame", "point", "color", "opacity"],
            )?;
            let point = m["point"]
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or(InvalidRequest)?;
            let at = |i: usize| {
                point[i]
                    .as_f64()
                    .filter(|v| v.is_finite() && v.abs() <= 1e12)
                    .ok_or(InvalidRequest)
            };
            Ok(LabsRequest::ReferenceFill {
                layer_uid: text(m, "layerUid")?,
                frame: integer(m, "frame", 0, u64::from(u32::MAX))?,
                point: [at(0)?, at(1)?],
                color: text(m, "color")?,
                opacity: number(m, "opacity", 0.0, 1.0)?,
            })
        }
        _ => Err(InvalidRequest),
    }
}

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
        let document = match kernels::generate(&origin, request)? {
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
