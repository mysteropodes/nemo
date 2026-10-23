//! L02-H atomicity controls: a payload with one unresolvable reference, a
//! malformed table, a provider that runs dry midway, a refused promotion or a
//! locked destination publishes nothing - document bytes, revision, BOTH history
//! stacks and the request receipts stay exactly as they were - and the Animation
//! 2D hold promotion happens exactly once, inside the same candidate.
use super::object_selection_edit_clipboard::{copy, paste, target, two_tables};
use super::object_selection_edit_tests::{
    apply, base, current, duplicate, frozen, owner_of, strokes_at,
};
use super::*;
use crate::object_codec::encode_storage_project;
use crate::object_document::object_group_kernels::object_clipboard_plan::{
    plan_paste, ClipPayload, PasteRequest,
};
use crate::object_document::object_group_kernels::object_clipboard_remap::{
    IdKind, IdSource, RemapError,
};
use crate::object_document::object_group_kernels::object_selection_edit_clones::RevisionIds;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    EditMode, HeldFramePromotion, PromotionRefusal, UnavailablePromotion,
};
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    plan_delete, SelectionError, SelectionRequest,
};
use crate::object_document::{ObjectDocument, Presence, TimingEdit};
use serde_json::{json, Value};

/// Grants the scripted identities in order; `None` is a provider that cannot supply one.
struct Scripted(Vec<Option<&'static str>>, usize);
impl IdSource for Scripted {
    fn fresh(&mut self, _kind: IdKind, _replaces: Option<&str>) -> Option<String> {
        let next = self.0.get(self.1).copied().flatten();
        self.1 += 1;
        next.map(str::to_string)
    }
}

/// The observable contract of the L05 `promote_cells` kernel for a hold on a
/// document without an order plan: the frame becomes a key and holds a copy of
/// what the key before it shows, with the same stroke identities. The real kernel
/// lives above the commands layer, so this stands in for it ONLY to prove the plan
/// calls the port once and composes the promotion with the edit.
struct CellPromotion {
    calls: std::cell::Cell<u32>,
}
impl HeldFramePromotion for CellPromotion {
    fn promote(
        &self,
        document: &ObjectDocument,
        layer_uid: &str,
        frame: u32,
    ) -> Result<ObjectDocument, PromotionRefusal> {
        self.calls.set(self.calls.get() + 1);
        let mut next = document.clone();
        let index = next
            .layers
            .iter()
            .position(|layer| layer.layer_uid == layer_uid)
            .ok_or(PromotionRefusal::Invalid)?;
        let timing = next.layers[index]
            .timing
            .value()
            .ok_or(PromotionRefusal::Invalid)?;
        let source = timing.exposure(frame).1;
        let promoted = match timing.promote_cell(frame, next.total_frames) {
            Ok(TimingEdit::Changed(promoted)) => promoted,
            _ => return Err(PromotionRefusal::Invalid),
        };
        if let Some(from) = source {
            let copies: Vec<_> = next
                .objects
                .iter()
                .filter(|r| r.target.layer_uid == layer_uid && r.target.frame_scope.frame == from)
                .cloned()
                .map(|mut record| {
                    record.target.frame_scope.frame = frame;
                    record
                })
                .collect();
            next.objects.extend(copies);
        }
        next.layers[index].timing = Presence::Present(promoted);
        Ok(next)
    }
}

fn good_payload(clip: &ClipPayload) -> Value {
    json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L1",
        "frame":0,"mode":"motion","clip":serde_json::to_value(clip).unwrap()})
}
fn paste_json(owner: &mut NativeObjectHistory, payload: &Value) -> Result<bool, DispatchErrorCode> {
    let candidate = owner.prepare_clipboard_paste_json(payload.to_string().as_bytes())?;
    let changed = candidate.changed();
    owner.commit_selection_edit(candidate)?;
    Ok(changed)
}

#[test]
fn a_clip_with_one_unresolvable_reference_publishes_nothing() {
    let mut owner = owner_of(&two_tables(2));
    let clip = copy(&owner, "L0", 0, &["a", "b"]).unwrap().unwrap();
    // A redo entry that no refusal may clear.
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L1", 0, "motion", &["x"]),
    )
    .unwrap();
    owner.undo(1).unwrap();
    let before = frozen(&owner);
    assert_eq!(before.1, (0, 1));
    assert_eq!(
        paste_json(&mut owner, &good_payload(&clip)),
        Ok(true),
        "the control works"
    );
    owner.undo(3).unwrap();
    let good = good_payload(&clip);
    let mut cases: Vec<(&str, Value)> = Vec::new();
    for member in [
        "meshId",
        "rigBind",
        "symbolId",
        "componentId",
        "imageMeshId",
    ] {
        let mut bad = good.clone();
        bad["clip"]["records"][1][member] = json!("m1");
        cases.push((member, bad));
    }
    let mut other_layer = good.clone();
    other_layer["clip"]["records"][1]["target"]["layerUid"] = json!("L1");
    cases.push(("a record of another layer", other_layer));
    let mut other_context = good.clone();
    other_context["clip"]["records"][1]["target"]["contextId"] = json!("comp");
    cases.push(("a record of another context", other_context));
    let mut reference = good.clone();
    reference["clip"]["records"][1]["target"]["frameScope"]["kind"] = json!("reference");
    cases.push(("a reference-scope record", reference));
    let mut twice = good.clone();
    twice["clip"]["records"][1] = good["clip"]["records"][0].clone();
    cases.push(("one stroke twice", twice));
    let mut cyclic = good.clone();
    cyclic["clip"]["groups"]["entries"] = json!([
        {"gid":"g1","order":["g2","a"]},{"gid":"g2","order":["g1","b"]}]);
    cases.push(("a cyclic group table", cyclic));
    let mut unreadable = good.clone();
    unreadable["clip"]["groups"]["entries"][0]["combineMode"] = json!("weird");
    cases.push(("an unreadable combine mode", unreadable));
    let mut beyond = good.clone();
    beyond["frame"] = json!(6);
    cases.push(("a frame beyond the document", beyond));
    let mut mode = good.clone();
    mode["mode"] = json!("zoom");
    cases.push(("an unknown mode", mode));
    let mut extra = good.clone();
    extra["extra"] = json!(1);
    cases.push(("an unknown payload member", extra));
    let mut command = good.clone();
    command["command"] = json!("object.clipboard.pasteAll");
    cases.push(("another command", command));
    let before = frozen(&owner);
    for (label, payload) in cases {
        assert_eq!(
            paste_json(&mut owner, &payload),
            Err(DispatchErrorCode::InvalidRequest),
            "{label}"
        );
        assert_eq!(frozen(&owner), before, "{label}");
    }
}

#[test]
fn an_unknown_or_locked_destination_is_refused_and_leaves_state_alone() {
    let mut owner = owner_of(&two_tables(1));
    let clip = copy(&owner, "L0", 0, &["a"]).unwrap().unwrap();
    let before = frozen(&owner);
    assert_eq!(
        paste(&mut owner, &clip, &target("nope", 0, EditMode::Motion)),
        Err(DispatchErrorCode::NotFound)
    );
    let mut source = two_tables(1);
    source["layers"][1]["locked"] = json!(true);
    let mut locked = owner_of(&source);
    let kept = frozen(&locked);
    assert_eq!(
        paste(&mut locked, &clip, &target("L1", 0, EditMode::Motion)),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(frozen(&locked), kept);
    // A context the document does not have: the copies would fail storage admission.
    let mut plain = owner_of(&base(1));
    let loose = copy(&plain, "L0", 0, &["a"]).unwrap().unwrap();
    let mut elsewhere = target("L1", 0, EditMode::Motion);
    elsewhere.context_id = "comp".into();
    let kept = frozen(&plain);
    assert_eq!(
        paste(&mut plain, &loose, &elsewhere),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(frozen(&plain), kept);
    assert_eq!(frozen(&owner), before);
}

#[test]
fn a_provider_that_runs_dry_midway_leaves_no_candidate_and_the_document_untouched() {
    let owner = owner_of(&two_tables(2));
    let clip = copy(&owner, "L0", 0, &["a", "b"]).unwrap().unwrap();
    let snapshot = current(&owner);
    let document = snapshot.document();
    let frozen_bytes = encode_storage_project(document).unwrap();
    let request = PasteRequest {
        context_id: "scene-root",
        layer_uid: "L1",
        frame: 0,
        mode: EditMode::Motion,
    };
    let plan = |source: &mut Scripted| {
        plan_paste(document, &clip, &request, &UnavailablePromotion, source).map(|p| p.created)
    };
    assert_eq!(
        plan(&mut Scripted(vec![Some("n1"), None], 0)),
        Err(SelectionError::Remap(RemapError::IdUnavailable(
            IdKind::Stroke
        )))
    );
    assert_eq!(
        plan(&mut Scripted(vec![Some("n1"), Some("a")], 0)),
        Err(SelectionError::Remap(RemapError::IdCollision("a".into()))),
        "an id already in the plan's world is refused"
    );
    assert_eq!(
        plan(&mut Scripted(vec![Some("n1"), Some("")], 0)),
        Err(SelectionError::Remap(RemapError::EmptyFreshId(
            IdKind::Stroke
        )))
    );
    assert_eq!(
        plan(&mut Scripted(vec![Some("n1"), Some("n2"), None], 0)),
        Err(SelectionError::Remap(RemapError::IdUnavailable(
            IdKind::CombineGroup
        ))),
        "a failure while recreating the group discards the strokes already issued"
    );
    assert_eq!(
        plan(&mut Scripted(vec![Some("n1"), Some("n2"), Some("g9")], 0)),
        Ok(vec!["n1".to_string(), "n2".to_string()])
    );
    assert_eq!(encode_storage_project(document).unwrap(), frozen_bytes);
}

#[test]
fn an_animation_2d_hold_is_promoted_once_inside_the_same_candidate() {
    let owner = owner_of(&base(1));
    let snapshot = current(&owner);
    let document = snapshot.document();
    let clip = copy(&owner, "L0", 0, &["b"]).unwrap().unwrap();
    let request = PasteRequest {
        context_id: "scene-root",
        layer_uid: "L0",
        frame: 5,
        mode: EditMode::Animation2d,
    };
    let mut ids = RevisionIds::new(1, document);
    let port = CellPromotion {
        calls: std::cell::Cell::new(0),
    };
    let plan = plan_paste(document, &clip, &request, &port, &mut ids).unwrap();
    assert_eq!(port.calls.get(), 1, "exactly one promotion");
    assert!(plan.promoted && plan.changed);
    assert_eq!(plan.content_frame, 5);
    let after: Value =
        serde_json::from_slice(&encode_storage_project(&plan.document).unwrap()).unwrap();
    assert_eq!(
        strokes_at(&after, "L0", 5),
        ["a", "d", "native-stroke:1:0"],
        "the hold's copy of key 3 plus the pasted shape"
    );
    assert_eq!(
        strokes_at(&after, "L0", 3),
        ["a", "d"],
        "key 3 is not touched"
    );
    assert_eq!(
        after["layers"][0]["timing"]["keys"],
        json!([{"frame":0},{"frame":3},{"frame":5}])
    );
    // The same promotion carries a delete, and a refused port publishes nothing.
    let stroke_ids = vec!["d".to_string()];
    let delete_request = SelectionRequest {
        context_id: "scene-root",
        layer_uid: "L0",
        frame: 5,
        mode: EditMode::Animation2d,
        stroke_ids: &stroke_ids,
    };
    let port = CellPromotion {
        calls: std::cell::Cell::new(0),
    };
    let deleted = plan_delete(document, &delete_request, &port).unwrap();
    assert_eq!(port.calls.get(), 1);
    let after: Value =
        serde_json::from_slice(&encode_storage_project(&deleted.document).unwrap()).unwrap();
    assert_eq!(strokes_at(&after, "L0", 5), ["a"]);
    assert_eq!(
        strokes_at(&after, "L0", 3),
        ["a", "d"],
        "`d` still exists in key 3"
    );
    assert_eq!(
        plan_delete(document, &delete_request, &UnavailablePromotion).unwrap_err(),
        SelectionError::Promotion(PromotionRefusal::Unavailable)
    );
    // A key or an in-between never asks the port.
    let port = CellPromotion {
        calls: std::cell::Cell::new(0),
    };
    let keyed = SelectionRequest {
        frame: 3,
        ..delete_request
    };
    plan_delete(document, &keyed, &port).unwrap();
    assert_eq!(port.calls.get(), 0);
}

#[test]
fn a_real_held_cut_is_sealed_atomic_and_one_undo_restores_timing_and_content() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let before = frozen(&owner);
        let payload = super::object_selection_edit_tests::selection(
            "object.clipboard.cut",
            "L0",
            5,
            "animation2d",
            &["d"],
        );
        let raw = payload.to_string();
        let (clip, candidate) = owner.prepare_clipboard_cut_json(raw.as_bytes()).unwrap();
        assert!(candidate.promoted() && candidate.changed());
        assert_eq!(candidate.content_frame(), 5);
        let clip = clip.unwrap();
        assert_eq!(clip.records.len(), 1);
        assert_eq!(clip.records[0].target.stroke_id, "d");
        assert_eq!(frozen(&owner), before);
        owner.commit_selection_edit(candidate.clone()).unwrap();
        let after = frozen(&owner);
        let drawing = super::object_selection_edit_tests::value(&owner);
        assert_eq!(strokes_at(&drawing, "L0", 5), ["a"]);
        assert_eq!(strokes_at(&drawing, "L0", 3), ["a", "d"]);
        assert_eq!(
            drawing["layers"][0]["timing"]["keys"],
            json!([{"frame":0},{"frame":3},{"frame":5}])
        );
        assert_eq!((after.1, after.2), ((1, 0), 1));
        assert_eq!(
            owner.commit_selection_edit(candidate),
            Err(DispatchErrorCode::StaleRevision)
        );
        assert_eq!(frozen(&owner), after);
        let reopened = owner_of(&drawing);
        assert_eq!(frozen(&reopened).0, after.0);
        owner.undo(1).unwrap();
        assert_eq!(frozen(&owner).0, before.0);
        assert_eq!(owner.history_depths(), (0, 1));
        let (_, stale) = owner.prepare_clipboard_cut_json(raw.as_bytes()).unwrap();
        owner.redo(2).unwrap();
        let kept = frozen(&owner);
        assert_eq!(kept.0, after.0);
        assert_eq!(
            owner.commit_selection_edit(stale),
            Err(DispatchErrorCode::StaleRevision)
        );
        assert_eq!(frozen(&owner), kept);
    }
}
