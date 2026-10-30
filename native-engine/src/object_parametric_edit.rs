//! L01-G (G22): sealed parameter edit of a dynamic shape through the one revision owner. Strict original-byte
//! payload, one detached candidate document sealed to the immutable origin snapshot, full storage admission
//! before anything can publish, and publication only through the existing `ObjectRevisionOwner` (history,
//! request replay and dispatch live in their existing owners). Nothing here is an authority of its own.
//!
//! Wire (operation `command.document.object.parametric.edit`, member `command` selects the action; every
//! action names its record in `stableTarget`):
//! - `object.parametric.set`: `paramShape` is the complete declared intent of the SAME kind the record already
//!   holds (`rect` tl/tr/br/bl, `ellipse` startAngle/sweep/innerRadius, `star` pointCount/innerRatio/
//!   cornerRadius; an absent member stays absent, a stored 0 stays 0) and carries no `box`. The stored baked
//!   anchors are rebuilt from it through the L01-B kernel and written together with the intent (a panel commit:
//!   `ps.x = v; applyParamShape*(p)` at tools.js 6789/6942/6952); a closed path results, as in the baseline.
//! - `object.parametric.translate`: `delta` `[dx, dy]`; anchors move and the stored box follows
//!   (`p.translate(delta)` + `syncParamShapeBoxOnTranslate`).
//! - `object.parametric.scale`: `scale` `[sx, sy]` about `anchor` `[x, y]`; anchors scale, the box is scaled and
//!   re-ordered (a negative factor keeps `x1 <= x2`) and a rect is rebuilt from it with its unchanged radii
//!   (`p.scale(sx, sy, anchor)` + `syncParamShapeBoxOnScale`).
//!
//! Same lock and path-effect guard as every other Drawing edit (`check_drawing_layer`), no pressure ribbon
//! (its centreline would go stale), no reference scope. An edit that changes nothing preserves bytes, history
//! and redo. The kernels are reached through the read-queries facade, so a build without the evaluation layer
//! refuses with `Unavailable` instead of guessing.
use crate::commands::object_fill_owner::vector_edit::{
    check_drawing_layer, check_target, is_vector_record, same_record,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, ObjectDocument, ObjectPoint, ObjectRecord, ObjectSegment, ObjectTarget,
    ParamShape, Presence,
};
use crate::object_snapshot::object_parametric_edit_read::{
    self as shape_kernels, Anchor, Frame, Intent, Moved,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{Map, Number, Value};

const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug)]
struct ParametricPayload {
    command: String,
    stable_target: ObjectTarget,
    param_shape: Presence<ParamShape>,
    delta: Presence<[Number; 2]>,
    scale: Presence<[Number; 2]>,
    anchor: Presence<[Number; 2]>,
}
object_deserialize!(ParametricPayload {
    command: String,
    stable_target: ObjectTarget,
    #[serde(default)] param_shape: Presence<ParamShape>,
    #[serde(default)] delta: Presence<[Number; 2]>,
    #[serde(default)] scale: Presence<[Number; 2]>,
    #[serde(default)] anchor: Presence<[Number; 2]>
});

/// Detached candidate sealed to the origin snapshot; its fields are private to this module, so a caller
/// cannot forge an origin or a document. It is not an authoritative snapshot.
#[derive(Debug)]
pub(crate) struct PreparedParametricEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

fn finite(number: &Number) -> Result<f64, DispatchErrorCode> {
    number
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(DispatchErrorCode::InvalidRequest)
}

fn number(value: f64) -> Result<Number, DispatchErrorCode> {
    Number::from_f64(value).ok_or(DispatchErrorCode::InvalidRequest)
}

fn member(slot: &Presence<Number>) -> Result<Option<f64>, DispatchErrorCode> {
    match slot.value() {
        None => Ok(None),
        Some(stored) => finite(stored).map(Some),
    }
}

fn pair(slot: &Presence<[Number; 2]>) -> Result<[f64; 2], DispatchErrorCode> {
    let values = slot.value().ok_or(DispatchErrorCode::InvalidRequest)?;
    Ok([finite(&values[0])?, finite(&values[1])?])
}

fn intent_of(shape: &ParamShape) -> Result<Intent, DispatchErrorCode> {
    match shape.kind.as_str() {
        "rect" => Ok(Intent::Rect {
            tl: member(&shape.tl)?,
            tr: member(&shape.tr)?,
            br: member(&shape.br)?,
            bl: member(&shape.bl)?,
        }),
        "ellipse" => Ok(Intent::Ellipse {
            start_angle: member(&shape.start_angle)?,
            sweep: member(&shape.sweep)?,
            inner_radius: member(&shape.inner_radius)?,
        }),
        "star" => Ok(Intent::Star {
            point_count: member(&shape.point_count)?,
            inner_ratio: member(&shape.inner_ratio)?,
            corner_radius: member(&shape.corner_radius)?,
        }),
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

fn frame_of(shape: &ParamShape) -> Result<Option<Frame>, DispatchErrorCode> {
    match shape.frame.value() {
        None => Ok(None),
        Some(stored) => Ok(Some(Frame {
            x1: finite(&stored.x1)?,
            y1: finite(&stored.y1)?,
            x2: finite(&stored.x2)?,
            y2: finite(&stored.y2)?,
        })),
    }
}

fn point_of(point: &ObjectPoint) -> Result<[f64; 2], DispatchErrorCode> {
    Ok([finite(&point.x)?, finite(&point.y)?])
}

fn anchors_of(record: &ObjectRecord) -> Result<Vec<Anchor>, DispatchErrorCode> {
    record
        .geometry
        .segments
        .iter()
        .map(|segment| {
            Ok(Anchor {
                point: point_of(&segment.point)?,
                handle_in: point_of(&segment.handle_in)?,
                handle_out: point_of(&segment.handle_out)?,
            })
        })
        .collect()
}

fn object_point(point: [f64; 2]) -> Result<ObjectPoint, DispatchErrorCode> {
    Ok(ObjectPoint {
        x: number(point[0])?,
        y: number(point[1])?,
    })
}

fn segments_of(anchors: &[Anchor]) -> Result<Vec<ObjectSegment>, DispatchErrorCode> {
    anchors
        .iter()
        .map(|anchor| {
            Ok(ObjectSegment {
                point: object_point(anchor.point)?,
                handle_in: object_point(anchor.handle_in)?,
                handle_out: object_point(anchor.handle_out)?,
            })
        })
        .collect()
}

/// The stored shape with `frame` as its box. The box record type is private to the document layer, so it is
/// decoded from its own strict wire shape.
fn with_frame(mut shape: ParamShape, frame: Frame) -> Result<ParamShape, DispatchErrorCode> {
    let mut wire = Map::new();
    wire.insert("x1".to_owned(), Value::Number(number(frame.x1)?));
    wire.insert("y1".to_owned(), Value::Number(number(frame.y1)?));
    wire.insert("x2".to_owned(), Value::Number(number(frame.x2)?));
    wire.insert("y2".to_owned(), Value::Number(number(frame.y2)?));
    shape.frame = Presence::Present(
        serde_json::from_value(Value::Object(wire))
            .map_err(|_| DispatchErrorCode::InvalidRequest)?,
    );
    Ok(shape)
}

/// What one action computed: the anchors to store, whether the path is closed, and the shape to store.
struct Outcome {
    anchors: Vec<Anchor>,
    closed: bool,
    shape: ParamShape,
}

fn moved_outcome(
    moved: Moved,
    stored: &ParamShape,
    closed: bool,
) -> Result<Outcome, DispatchErrorCode> {
    let shape = match moved.frame {
        Some(frame) => with_frame(stored.clone(), frame)?,
        None => stored.clone(),
    };
    Ok(Outcome {
        anchors: moved.anchors,
        closed: closed || moved.rebuilt,
        shape,
    })
}

/// Exactly the members the action reads: anything else is a malformed request.
fn only(payload: &ParametricPayload, shape: bool, delta: bool, scale: bool, anchor: bool) -> bool {
    payload.param_shape.value().is_some() == shape
        && payload.delta.value().is_some() == delta
        && payload.scale.value().is_some() == scale
        && payload.anchor.value().is_some() == anchor
}

fn compute(
    payload: &ParametricPayload,
    record: &ObjectRecord,
    stored: &ParamShape,
    angle: f64,
) -> Result<Outcome, DispatchErrorCode> {
    let anchors = anchors_of(record)?;
    let frame = frame_of(stored)?;
    let refuse = |why: shape_kernels::ShapeRefusal| why.code();
    match payload.command.as_str() {
        "object.parametric.set" => {
            if !only(payload, true, false, false, false) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let wanted = payload
                .param_shape
                .value()
                .ok_or(DispatchErrorCode::InvalidRequest)?;
            if wanted.kind != stored.kind || wanted.frame.value().is_some() {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let rebuilt = shape_kernels::rebuild(&intent_of(wanted)?, frame, &anchors, angle)
                .map_err(refuse)?;
            Ok(Outcome {
                anchors: rebuilt.anchors,
                closed: true,
                shape: with_frame(wanted.clone(), rebuilt.frame)?,
            })
        }
        "object.parametric.translate" => {
            if !only(payload, false, true, false, false) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let delta = pair(&payload.delta)?;
            let moved =
                shape_kernels::translate(&anchors, frame, delta[0], delta[1]).map_err(refuse)?;
            moved_outcome(moved, stored, record.geometry.closed)
        }
        "object.parametric.scale" => {
            if !only(payload, false, false, true, true) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let factors = pair(&payload.scale)?;
            let pivot = pair(&payload.anchor)?;
            let moved =
                shape_kernels::scale(&intent_of(stored)?, &anchors, frame, angle, factors, pivot)
                    .map_err(refuse)?;
            moved_outcome(moved, stored, record.geometry.closed)
        }
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

/// #2372 Select on a dynamic shape (batch `reshape` with a `matrix`, record space): a move, an axis-aligned resize or a
/// turn of one shape through the same kernels the translate and scale actions use, so the box and the rect radii stay
/// consistent. A turn moves the baked anchors and adds to the stored `boxAngle` (the box stays the un-rotated frame);
/// a skew or a resize of a turned shape has no meaning for a dynamic shape and is refused.
pub(crate) fn transformed(record: &ObjectRecord, m: [f64; 6]) -> Result<ObjectRecord, DispatchErrorCode> {
    const EPS: f64 = 1e-9;
    let ext = record.vector_ext.value().ok_or(DispatchErrorCode::Unavailable)?;
    let stored = ext.param_shape.value().ok_or(DispatchErrorCode::Unavailable)?;
    let angle = member(&ext.box_angle)?.unwrap_or(0.0);
    let (mut anchors, mut frame, mut rebuilt, mut turned) = (anchors_of(record)?, frame_of(stored)?, false, 0.0);
    let refuse = |why: shape_kernels::ShapeRefusal| why.code();
    let [a, b, c, d, e, f] = m;
    if b.abs() < EPS && c.abs() < EPS {
        if (a - 1.0).abs() > EPS || (d - 1.0).abs() > EPS {
            let moved = shape_kernels::scale(&intent_of(stored)?, &anchors, frame, angle, [a, d], [0.0, 0.0]).map_err(refuse)?;
            (anchors, frame, rebuilt) = (moved.anchors, moved.frame, moved.rebuilt);
        }
        if e.abs() > 0.0 || f.abs() > 0.0 {
            let moved = shape_kernels::translate(&anchors, frame, e, f).map_err(refuse)?;
            (anchors, frame) = (moved.anchors, moved.frame);
        }
    } else if (a - d).abs() < EPS && (b + c).abs() < EPS && (a.hypot(b) - 1.0).abs() < 1e-6 {
        let at = |p: [f64; 2]| [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f];
        let lin = |p: [f64; 2]| [a * p[0] + c * p[1], b * p[0] + d * p[1]];
        anchors = anchors.iter().map(|k| Anchor { point: at(k.point), handle_in: lin(k.handle_in), handle_out: lin(k.handle_out) }).collect();
        turned = b.atan2(a).to_degrees();
    } else {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let mut next = record.clone();
    next.geometry.segments = segments_of(&anchors)?;
    next.geometry.closed = record.geometry.closed || rebuilt;
    if let Presence::Present(next_ext) = &mut next.vector_ext {
        next_ext.param_shape = Presence::Present(match frame {
            Some(frame) => with_frame(stored.clone(), frame)?,
            None => stored.clone(),
        });
        if turned != 0.0 {
            next_ext.box_angle = Presence::Present(number((angle + turned) % 360.0)?);
        }
    }
    Ok(next)
}

impl ObjectFillOwner {
    /// Strict payload preparation directly from the original bytes (a `Value` would erase duplicate members).
    /// The returned candidate is sealed to this owner's current immutable snapshot and already passed the
    /// complete storage admission of the edited document; nothing is published.
    pub(crate) fn prepare_parametric_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedParametricEdit, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: ParametricPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        let current = origin.document();
        check_target(current, &payload.stable_target)?;
        let index = current
            .objects
            .iter()
            .position(|existing| existing.target == payload.stable_target)
            .ok_or(DispatchErrorCode::NotFound)?;
        let record = &current.objects[index];
        if !is_vector_record(record) {
            return Err(DispatchErrorCode::Unavailable);
        }
        check_drawing_layer(current, &record.target)?;
        let ext = record
            .vector_ext
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        let stored = ext
            .param_shape
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        if ext.pressure.value().is_some() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let angle = match ext.box_angle.value() {
            None => 0.0,
            Some(stored_angle) => finite(stored_angle)?,
        };
        let outcome = compute(&payload, record, stored, angle)?;
        let mut next = record.clone();
        next.geometry.segments = segments_of(&outcome.anchors)?;
        next.geometry.closed = outcome.closed;
        if let Presence::Present(next_ext) = &mut next.vector_ext {
            next_ext.param_shape = Presence::Present(outcome.shape);
            // Baseline transformFillGradient applies the same gesture to its document-world endpoints.
            if let Some(gradient) = next_ext.fill_gradient.value().cloned() {
                let mut moved = gradient;
                for point in [&mut moved.from, &mut moved.to] {
                    let [x, y] = [finite(&point[0])?, finite(&point[1])?];
                    let values = match payload.command.as_str() {
                        "object.parametric.translate" => {
                            let delta = pair(&payload.delta)?;
                            [x + delta[0], y + delta[1]]
                        }
                        "object.parametric.scale" => {
                            let scale = pair(&payload.scale)?;
                            let anchor = pair(&payload.anchor)?;
                            [
                                anchor[0] + (x - anchor[0]) * scale[0],
                                anchor[1] + (y - anchor[1]) * scale[1],
                            ]
                        }
                        _ => [x, y],
                    };
                    *point = [number(values[0])?, number(values[1])?];
                }
                next_ext.fill_gradient = Presence::Present(moved);
            }
        }
        let changed = !same_record(record, &next);
        let mut document = current.clone();
        if changed {
            document.objects[index] = next;
        }
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedParametricEdit {
            origin,
            document,
            changed,
        })
    }

    /// Publish a sealed candidate if and only if the origin pins (instance, document, snapshot, content
    /// revision) still match the current head. An exact no-op returns the unchanged head revision.
    pub(crate) fn commit_parametric_edit(
        &mut self,
        candidate: PreparedParametricEdit,
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
        if !candidate.changed {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
