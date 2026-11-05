//! Project length (F04/F11/U13): the Frames field and Trim to Work Area as one document-plus-settings candidate.
//! Behavior source: src/js/timeline.js `setTotalFrames` and `trimToWorkArea` at 1065e9d8.
//!
//! The Frames field clamps to 1..=999. Growing only lengthens the timeline (the last exposure holds, as the
//! baseline's empty padded frames do); shrinking drops the cells at or beyond the new end through the same
//! frame-removal arithmetic F5/Remove use, because native storage admits no record past the last frame. The work
//! area follows the baseline clamps: the end to the new last frame, then the start below the end. Layer in/out
//! points, layer markers, composition markers and Motion keys are not touched by the Frames field.
//!
//! Trim keeps `in..=out` of the work area: cells outside it go, everything after the start re-bases on frame 0,
//! and layer in/out points, layer and composition markers, camera keys and Motion keys shift by the start. A work
//! area shorter than two frames or one that already covers the whole timeline changes nothing.
use crate::commands::object_fill_owner::{object_frame_edit_document::remap, retime_root_layer_motion};
use crate::object_document::{
    LayerRange, MarkerRecord, ObjectDocument, Presence, ProjectCameraKey, ProjectMetadata,
    TimingEdit,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

const MAX_FRAMES: f64 = 999.0;

/// The edited drawing (absent when only the settings change) and the settings that go with it.
pub(super) struct LengthEdit {
    pub(super) document: Option<ObjectDocument>,
    pub(super) metadata: ProjectMetadata,
}

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}

/// Drop frames `min..=max` (`0 < count < total`) from every layer; later cells move down. Layers with no timing
/// record own no cells. A record this edit must not move (Reference scope, another context) refuses it.
fn cut(document: &ObjectDocument, min: u32, max: u32) -> Result<ObjectDocument, DispatchErrorCode> {
    let total = document.total_frames;
    let count = max - min + 1;
    let shrunk = total - count;
    let outside = |frame: u32| frame >= shrunk;
    let stuck = document.objects.iter().any(|record| {
        (record.target.context_id != "scene-root"
            || record.target.frame_scope.kind != crate::object_document::FrameScopeKind::Authored)
            && outside(record.target.frame_scope.frame)
    }) || document.composition_order.value().is_some_and(|plan| {
        plan.contexts
            .iter()
            .filter(|context| context.context_id != "scene-root")
            .flat_map(|context| context.scopes.iter())
            .any(|scope| outside(scope.frame_scope.frame))
    });
    if stuck {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut next = document.clone();
    for layer in &mut next.layers {
        let Some(timing) = layer.timing.value() else {
            continue;
        };
        if let TimingEdit::Changed(edited) =
            timing.remove_span(min, max, total).map_err(|_| invalid())?
        {
            layer.timing = Presence::Present(edited);
        }
    }
    next.total_frames = shrunk;
    let shift = |frame: u32| {
        if frame < min {
            Some(frame)
        } else if frame <= max {
            None
        } else {
            Some(frame - count)
        }
    };
    remap(&mut next, &|_, frame| shift(frame), &shift)
        .map_err(|_| DispatchErrorCode::Unavailable)?;
    Ok(next)
}

fn admitted(document: ObjectDocument) -> Result<ObjectDocument, DispatchErrorCode> {
    ObjectSnapshot::validate_drawing(&document).map_err(|_| invalid())?;
    Ok(document)
}

/// The Frames field. `frames` is the raw field value; non-finite input is invalid.
pub(super) fn set_total_frames(
    document: &ObjectDocument,
    metadata: &ProjectMetadata,
    frames: Number,
) -> Result<LengthEdit, DispatchErrorCode> {
    let value = frames.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)?;
    let wanted = value.floor().clamp(1.0, MAX_FRAMES) as u32;
    let total = document.total_frames;
    let next = if wanted == total {
        None
    } else if wanted > total {
        let mut grown = document.clone();
        grown.total_frames = wanted;
        Some(grown)
    } else {
        Some(admitted(cut(document, wanted, total - 1)?)?)
    };
    // waOut >= v -> v - 1, then waIn >= waOut -> max(0, waOut - 1); an unset end already means the last frame.
    let last = f64::from(wanted) - 1.0;
    let (wa_in, wa_out) = metadata.work_area();
    let out = wa_out.map(|end| if end >= f64::from(wanted) { last } else { end });
    let ceiling = out.unwrap_or(last);
    let start = wa_in.map(|begin| {
        if begin >= ceiling {
            (ceiling - 1.0).max(0.0)
        } else {
            begin
        }
    });
    let metadata = metadata
        .clone()
        .with_work_area(start, out)
        .map_err(|_| invalid())?;
    Ok(LengthEdit {
        document: next,
        metadata,
    })
}

fn shifted_range(range: &LayerRange, start: i64, length: i64) -> LayerRange {
    let mut next = range.clone();
    if let Presence::Present(Some(frame)) = &range.in_point {
        next.in_point = Presence::Present(Some((frame - start).max(0)));
    }
    if let Presence::Present(Some(frame)) = &range.out_point {
        next.out_point = Presence::Present(Some((frame - start).min(length - 1).max(0)));
    }
    if let Presence::Present(markers) = &range.markers {
        next.markers = Presence::Present(
            markers
                .iter()
                .filter(|m| m.frame - start >= 0 && m.frame - start < length)
                .map(|m| MarkerRecord {
                    frame: m.frame - start,
                    name: m.name.clone(),
                    color: m.color.clone(),
                })
                .collect(),
        );
    }
    next
}

/// Trim Comp to Work Area. Refuses nothing for an empty or whole-timeline work area; it changes nothing.
pub(super) fn trim_to_work_area(
    document: &ObjectDocument,
    metadata: &ProjectMetadata,
) -> Result<LengthEdit, DispatchErrorCode> {
    let total = document.total_frames;
    let (wa_in, wa_out) = metadata.work_area();
    let start = wa_in.unwrap_or(0.0).max(0.0).floor();
    let end = wa_out
        .unwrap_or(f64::from(total) - 1.0)
        .min(f64::from(total) - 1.0)
        .floor();
    if end <= start || (start == 0.0 && end == f64::from(total) - 1.0) {
        return Ok(LengthEdit {
            document: None,
            metadata: metadata.clone(),
        });
    }
    let (start, end) = (start as u32, end as u32);
    let length = end - start + 1;
    let mut next = document.clone();
    if start > 0 {
        // Motion keys re-base against the length they had before the trim, as the baseline does.
        let uids: Vec<String> = next.layers.iter().map(|l| l.layer_uid.clone()).collect();
        for uid in &uids {
            retime_root_layer_motion(&mut next, uid, -i64::from(start))?;
        }
    }
    for layer in &mut next.layers {
        if let Presence::Present(range) = &layer.range {
            layer.range = Presence::Present(shifted_range(range, i64::from(start), i64::from(length)));
        }
    }
    if end < total - 1 {
        next = cut(&next, end + 1, total - 1)?;
    }
    if start > 0 {
        next = cut(&next, 0, start - 1)?;
    }
    let document = admitted(next)?;
    let shift = f64::from(start);
    let span = f64::from(length);
    let keep = |frame: f64| frame - shift >= 0.0 && frame - shift < span;
    let markers = metadata.composition_markers().map(|list| {
        list.iter()
            .filter(|(frame, _, _)| keep(*frame))
            .map(|(frame, name, color)| (frame - shift, name.clone(), color.clone()))
            .collect::<Vec<_>>()
    });
    let (bpm, bpm_offset, bpm_show) = metadata.composition_bpm();
    let keys = metadata.camera_keys().map(|list| {
        list.iter()
            .filter(|key| keep(key.frame))
            .map(|key| ProjectCameraKey {
                frame: key.frame - shift,
                ..key.clone()
            })
            .collect::<Vec<_>>()
    });
    let mut edited = metadata.clone();
    if markers.is_some() {
        edited = edited
            .with_composition_metadata(markers, bpm, bpm_offset, bpm_show)
            .map_err(|_| invalid())?;
    }
    if keys.is_some() {
        edited = edited
            .with_camera(keys, metadata.camera_layer_on())
            .map_err(|_| invalid())?;
    }
    let metadata = edited
        .with_work_area(Some(0.0), Some(span - 1.0))
        .map_err(|_| invalid())?;
    Ok(LengthEdit {
        document: Some(document),
        metadata,
    })
}
