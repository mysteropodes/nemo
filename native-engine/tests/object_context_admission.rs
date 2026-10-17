//! The resolver over real admitted documents and pinned snapshots (L04-J, F14): the world of the
//! oracle goes through the strict v2 codec (the optional `componentTables` member of L04-I), the
//! snapshot pins are checked in the frame packet's order, and the frame-query admission of a
//! component context is exercised next to the unchanged `prepare_object_frame` answer.
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::{
    fill_hex, json, rows,
};
use crate::object_codec::{decode_project, decode_storage_project};
use crate::object_document::FrameScope;
use crate::object_frame_packet::prepare_object_frame;

const V1: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L"}],"objects":[]}"#;

fn snapshot(kind: &str) -> ObjectSnapshot {
    let document = decode_storage_project(json(kind).as_bytes()).expect("admitted v2 world");
    ObjectSnapshot::new("ctx-instance", document).expect("snapshot")
}

fn selector<'a>(
    snapshot: &'a ObjectSnapshot,
    context_id: &'a str,
    scope_kind: FrameScopeKind,
    frame: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id,
        scope_kind,
        frame,
    }
}

fn authored(context: &str, frame: u32, layer: &str, stroke: &str) -> ObjectTarget {
    ObjectTarget {
        context_id: context.to_string(),
        frame_scope: FrameScope {
            kind: FrameScopeKind::Authored,
            frame,
        },
        layer_uid: layer.to_string(),
        stroke_id: stroke.to_string(),
    }
}

#[test]
fn an_admitted_document_declares_the_scene_and_its_components() {
    let snap = snapshot("D");
    let source = ContextSource::of_document(snap.document());
    assert_eq!(
        source.declared_contexts(),
        vec!["scene-root", "sym_comp", "sym_comp_b"]
    );
    assert_eq!(source.context("scene-root").unwrap().layers().len(), 11);
    assert_eq!(source.context("sym_comp").unwrap().objects().len(), 13);
    let imported = snapshot("D2");
    let declared = ContextSource::of_document(imported.document()).declared_contexts();
    assert_eq!(
        declared,
        vec![
            "scene-root",
            "sym_comp",
            "sym_comp_b",
            "sym_1788000123456_5000"
        ]
    );
}

#[test]
fn a_document_without_the_member_declares_only_the_scene() {
    let snap = ObjectSnapshot::new("v1", decode_project(V1.as_bytes()).unwrap()).unwrap();
    let source = ContextSource::of_document(snap.document());
    assert_eq!(source.declared_contexts(), vec!["scene-root"]);
    assert_eq!(
        source.context("sym_comp").err(),
        Some(ResolveError::UnknownContext)
    );
    let admitted = admit_frame_context(
        &snap,
        &selector(&snap, "scene-root", FrameScopeKind::Authored, 2),
    );
    assert_eq!(admitted.unwrap().context.total_frames(), 3);
    assert_eq!(
        admit_frame_context(
            &snap,
            &selector(&snap, "sym_comp", FrameScopeKind::Authored, 0)
        )
        .err(),
        Some(ObjectFrameError::InvalidContext)
    );
}

#[test]
fn targets_resolve_inside_the_pinned_snapshot_to_borrowed_records() {
    let snap = snapshot("D");
    let pins = ContextPins::of_snapshot(&snap);
    for row in rows("S") {
        let frame: u32 = row[3].parse().unwrap();
        let hit =
            resolve_in_snapshot(&snap, &pins, &authored(row[1], frame, row[2], row[4])).unwrap();
        assert_eq!(fill_hex(hit.record), row[5]);
        let set = snap.document().component_set().expect("component table");
        let component = set.component(row[1]).unwrap();
        assert!(std::ptr::eq(
            hit.record,
            &component.objects()[hit.object_index]
        ));
    }
    // Resolving twice is the same borrow of the same immutable record.
    let target = authored("sym_comp_b", 1, "ly_comp_ia", "s_comp_a_1");
    let a = resolve_in_snapshot(&snap, &pins, &target).unwrap();
    let b = resolve_in_snapshot(&snap, &pins, &target).unwrap();
    assert!(std::ptr::eq(a.record, b.record));
    assert_eq!(a.record.target.context_id, "sym_comp_b");
}

#[test]
fn each_wrong_pin_refuses_before_any_lookup() {
    let snap = snapshot("D");
    let good = ContextPins::of_snapshot(&snap);
    let valid = authored("sym_comp", 1, "ly_comp_ia", "s_comp_a_1");
    assert!(resolve_in_snapshot(&snap, &good, &valid).is_ok());
    let mut wrong_instance = good;
    wrong_instance.instance_id = "other-instance";
    let mut wrong_document = good;
    wrong_document.document_id = "old-document";
    let mut wrong_revision = good;
    wrong_revision.content_revision = 1;
    let mut wrong_snapshot = good;
    wrong_snapshot.document_snapshot_id = "other-snapshot";
    // An unknown context in the target must not mask a wrong pin.
    let unknown = authored("sym_missing", 1, "ly_comp_ia", "s_comp_a_1");
    for (pins, expected) in [
        (wrong_instance, ObjectFrameError::WrongInstance),
        (wrong_document, ObjectFrameError::WrongDocument),
        (wrong_revision, ObjectFrameError::WrongRevision),
        (wrong_snapshot, ObjectFrameError::WrongSnapshot),
    ] {
        for target in [&valid, &unknown] {
            assert_eq!(
                resolve_in_snapshot(&snap, &pins, target).err(),
                Some(SnapshotResolveError::Pin(expected))
            );
        }
    }
    // Several wrong pins report the first in the packet's order.
    let mut all_wrong = wrong_snapshot;
    all_wrong.instance_id = "x";
    all_wrong.content_revision = 9;
    assert_eq!(
        resolve_in_snapshot(&snap, &all_wrong, &valid).err(),
        Some(SnapshotResolveError::Pin(ObjectFrameError::WrongInstance))
    );
    // The snapshot is unchanged by any refusal.
    assert!(resolve_in_snapshot(&snap, &good, &valid).is_ok());
}

#[test]
fn an_unknown_context_or_a_malformed_target_is_a_typed_resolve_error() {
    let snap = snapshot("D");
    let pins = ContextPins::of_snapshot(&snap);
    for (target, expected) in [
        (
            authored("sym_missing", 1, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::UnknownContext,
        ),
        (
            authored("", 1, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::EmptyId,
        ),
        (
            authored("sym_comp", 12, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::FrameOutOfRange,
        ),
        (
            authored("sym_comp", 0, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::NotAuthoredAtFrame,
        ),
        (
            authored("scene-root", 0, "ly_comp_loop", "s_comp_a_1"),
            ResolveError::UnknownStroke,
        ),
    ] {
        assert_eq!(
            resolve_in_snapshot(&snap, &pins, &target).err(),
            Some(SnapshotResolveError::Resolve(expected)),
            "{target:?}"
        );
    }
}

#[test]
fn frame_query_admission_uses_each_contexts_own_frame_count_and_scope() {
    let snap = snapshot("D");
    let ok = |context: &str, kind: FrameScopeKind, frame: u32| {
        admit_frame_context(&snap, &selector(&snap, context, kind, frame))
            .map(|a| (a.context.id().to_string(), a.frame))
    };
    let authored_kind = || FrameScopeKind::Authored;
    assert_eq!(
        ok("scene-root", authored_kind(), 23),
        Ok(("scene-root".to_string(), 23))
    );
    assert_eq!(
        ok("scene-root", authored_kind(), 24),
        Err(ObjectFrameError::FrameOutOfRange)
    );
    // The component has 12 frames although the scene has 24.
    assert_eq!(
        ok("sym_comp", authored_kind(), 11),
        Ok(("sym_comp".to_string(), 11))
    );
    assert_eq!(
        ok("sym_comp", authored_kind(), 12),
        Err(ObjectFrameError::FrameOutOfRange)
    );
    assert_eq!(
        ok("sym_comp_b", authored_kind(), 0),
        Ok(("sym_comp_b".to_string(), 0))
    );
    for context in ["scene-root", "sym_comp"] {
        assert_eq!(
            ok(context, FrameScopeKind::Reference, 0),
            Err(ObjectFrameError::Unavailable)
        );
    }
    for context in ["", "symbol-A", "Scene-root", "sym_comp\0", "sym_comp "] {
        assert_eq!(
            ok(context, authored_kind(), 0),
            Err(ObjectFrameError::InvalidContext),
            "{context:?}"
        );
    }
    let admitted = admit_frame_context(
        &snap,
        &selector(&snap, "sym_comp", FrameScopeKind::Authored, 3),
    )
    .unwrap();
    assert_eq!(admitted.context.kind(), ContextKind::Component);
    assert_eq!(admitted.scope_kind, FrameScopeKind::Authored);
}

#[test]
fn frame_query_admission_checks_the_pins_first() {
    let snap = snapshot("D");
    let mut bad = selector(&snap, "sym_missing", FrameScopeKind::Authored, 99);
    bad.document_snapshot_id = "other";
    assert_eq!(
        admit_frame_context(&snap, &bad).err(),
        Some(ObjectFrameError::WrongSnapshot)
    );
    bad.content_revision = 4;
    assert_eq!(
        admit_frame_context(&snap, &bad).err(),
        Some(ObjectFrameError::WrongRevision)
    );
    bad.document_id = "other";
    assert_eq!(
        admit_frame_context(&snap, &bad).err(),
        Some(ObjectFrameError::WrongDocument)
    );
    bad.instance_id = "other";
    assert_eq!(
        admit_frame_context(&snap, &bad).err(),
        Some(ObjectFrameError::WrongInstance)
    );
}

/// Ordering tripwire. The frame packet is NOT routed through the resolver in this leaf: it keeps
/// answering `Unavailable` for every non-root context of a v2 document (pinned today by
/// `object_exposure.rs`, `object_order_evaluation.rs`), including a context the resolver now
/// declares. When the evaluation join (L04-K) routes component contexts, flip this assertion in
/// the same change; that is the recorded order.
#[test]
fn the_frame_packet_still_answers_unavailable_for_a_declared_component_context() {
    let snap = snapshot("D");
    for context in ["sym_comp", "sym_comp_b", "symbol-A"] {
        assert_eq!(
            prepare_object_frame(
                &snap,
                &selector(&snap, context, FrameScopeKind::Authored, 0)
            )
            .err(),
            Some(ObjectFrameError::Unavailable),
            "{context}"
        );
    }
    // The resolver admits the declared one and refuses the undeclared one.
    assert!(admit_frame_context(
        &snap,
        &selector(&snap, "sym_comp", FrameScopeKind::Authored, 0)
    )
    .is_ok());
    assert!(admit_frame_context(
        &snap,
        &selector(&snap, "symbol-A", FrameScopeKind::Authored, 0)
    )
    .is_err());
}
