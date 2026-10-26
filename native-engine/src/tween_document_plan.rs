//! Detached tween command planner. No publication, history stack or UI mirror lives here.
use super::*;
use crate::object_document::{
    object_deserialize,
    tween::{TweenOptions, TweenSpan, TweenStore},
    ObjectDocument, Presence, TimingEdit,
};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
#[derive(Debug)]
pub(crate) struct TweenRequest {
    pub(crate) command: String,
    pub(crate) layer_uid: String,
    pub(crate) options: Presence<TweenOptions>,
    pub(crate) starts: Presence<Vec<u32>>,
    pub(crate) selection: Presence<Vec<u32>>,
    pub(crate) span: Presence<TweenSpan>,
    pub(crate) frame: Presence<u32>,
}
object_deserialize!(TweenRequest{command:String,layer_uid:String,#[serde(default)] options:Presence<TweenOptions>,
    #[serde(default)] starts:Presence<Vec<u32>>,#[serde(default)] selection:Presence<Vec<u32>>,
    #[serde(default)] span:Presence<TweenSpan>,#[serde(default)] frame:Presence<u32>});
pub(crate) struct TweenPlan {
    pub(crate) document: ObjectDocument,
    pub(crate) bakes: Vec<BitmapTweenBake>,
}
pub(crate) fn plan_tween(
    document: &ObjectDocument,
    request: &TweenRequest,
    custom: Option<&crate::object_document::ProjectPresetCatalog>,
) -> R<Option<TweenPlan>> {
    let invalid = DispatchErrorCode::InvalidRequest;
    let operation = request.command.as_str();
    if ![
        "object.tween.generate",
        "object.tween.regenerate",
        "object.tween.split",
        "object.tween.reassign",
    ]
    .contains(&operation)
        || request.layer_uid.is_empty()
        || request.layer_uid.len() > 255
        || request.options.value().is_some_and(|o| !o.valid())
        || request.span.value().is_some_and(|s| !s.valid())
        || request.starts.value().is_some_and(|s| s.len() > 4096)
        || request.selection.value().is_some_and(|s| s.len() > 4096)
        || (!request.starts.missing() && !request.selection.missing())
        || (operation == "object.tween.split") != !request.frame.missing()
        || (operation == "object.tween.reassign") != !request.span.missing()
    {
        return Err(invalid);
    }
    let index = document
        .layers
        .iter()
        .position(|l| l.layer_uid == request.layer_uid)
        .ok_or(DispatchErrorCode::NotFound)?;
    if document.layer_is_locked(&request.layer_uid) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let timing = document.layers[index]
        .timing
        .value()
        .ok_or(DispatchErrorCode::Unavailable)?;
    let drawn_keys = |doc: &ObjectDocument| -> Vec<u32> {
        doc.layers[index]
            .timing
            .value()
            .unwrap()
            .key_frames()
            .filter(|&f| has_frame_content(doc, &request.layer_uid, f))
            .collect()
    };
    if drawn_keys(document).len() < 2 {
        return Ok(None);
    }
    let mut next = document.clone();
    let mut store = next.tweens.value().cloned().unwrap_or_default();
    let settings = store.layer_mut(&request.layer_uid);
    if let Some(options) = request.options.value() {
        settings.options = options.clone();
    }
    if let Some(span) = request.span.value() {
        let keys = drawn_keys(document);
        if !keys.windows(2).any(|p| p == [span.from, span.to]) {
            return Err(invalid);
        }
        if let Some(old) = settings
            .spans
            .iter_mut()
            .find(|s| s.from == span.from && s.to == span.to)
        {
            *old = span.clone();
        } else {
            settings.spans.push(span.clone());
        }
    }
    let mut starts = request.starts.value().cloned();
    if let Some(selection) = request.selection.value() {
        if selection.iter().any(|&f| f >= document.total_frames) {
            return Err(invalid);
        }
        if !selection.is_empty() {
            starts = Some(
                selection
                    .iter()
                    .filter_map(|&f| timing.key_frames().take_while(|&k| k <= f).last())
                    .collect(),
            );
        }
    }
    if let Some(span) = request.span.value() {
        starts = Some(vec![span.from]);
    }
    if let Some(&frame) = request.frame.value() {
        if frame >= document.total_frames || timing.exposure(frame).0 != "interpolated" {
            return Err(invalid);
        }
        let keys = drawn_keys(document);
        let Some(previous) = keys.iter().copied().take_while(|&k| k < frame).last() else {
            return Err(invalid);
        };
        let TimingEdit::Changed(promoted) = timing
            .promote_cell(frame, document.total_frames)
            .map_err(|_| invalid)?
        else {
            return Err(invalid);
        };
        next.layers[index].timing = Presence::Present(promoted);
        starts = Some(vec![previous, frame]);
    }
    if starts
        .as_ref()
        .is_some_and(|s| s.iter().any(|&f| f >= document.total_frames))
    {
        return Err(invalid);
    }
    let keys = drawn_keys(&next);
    let options = settings.options.clone();
    let spans = settings.spans.clone();
    let mut bakes = vec![];
    for endpoints in keys.windows(2) {
        let (from, to) = (endpoints[0], endpoints[1]);
        if starts.as_ref().is_some_and(|s| !s.contains(&from)) {
            continue;
        }
        generate_span(
            &mut next,
            &request.layer_uid,
            from,
            to,
            &options,
            spans.iter().find(|s| s.from == from && s.to == to),
            custom,
            &mut bakes,
        )?;
    }
    next.tweens = Presence::Present(store);
    crate::revision::ObjectSnapshot::validate_drawing(&next).map_err(|_| invalid)?;
    Ok(Some(TweenPlan {
        document: next,
        bakes,
    }))
}
