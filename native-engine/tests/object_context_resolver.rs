//! Oracle tests of the nested-context resolver (L04-J, F14).
//!
//! The world is the baseline components fixture (`sym_comp`, two inner layers, 13 strokes, its
//! clone `sym_comp_b` with the SAME stroke ids, seven instance layers) written as a native
//! document; the expected tokens come from the oracle rows the verbatim baseline functions
//! produced (see `object_context_resolver_world.rs`). Literals below are hand-typed from the
//! baseline fixture.
use super::world::{count, fill_hex, rows, world};
use super::*;
use std::collections::BTreeSet;

fn target(
    context: &str,
    kind: FrameScopeKind,
    frame: u32,
    layer: &str,
    stroke: &str,
) -> ObjectTarget {
    ObjectTarget {
        context_id: context.to_string(),
        frame_scope: FrameScope { kind, frame },
        layer_uid: layer.to_string(),
        stroke_id: stroke.to_string(),
    }
}

fn authored(context: &str, frame: u32, layer: &str, stroke: &str) -> ObjectTarget {
    target(context, FrameScopeKind::Authored, frame, layer, stroke)
}

#[test]
fn fixture_world_and_row_counts_are_pinned() {
    let w = world("D");
    assert_eq!(w.total_frames, 24);
    assert_eq!((w.layers.len(), count("sceneLayers")), (11, 11));
    assert_eq!((w.set.components.len(), count("components")), (2, 2));
    assert_eq!((w.set.instances.len(), count("instances")), (9, 9));
    assert_eq!(count("contexts"), 3);
    assert_eq!(count("strokesSymComp"), 13);
    assert_eq!(count("strokesSymCompB"), 13);
    assert_eq!(rows("S").len(), 26);
    assert_eq!(rows("H").len(), 11);
    assert_eq!(rows("A").len(), 24);
    assert_eq!(rows("P").len(), 26);
    assert_eq!(
        rows("M"),
        vec![vec!["M", "sym_comp", "sym_1788000123456_5000"]]
    );
    for component in &w.set.components {
        assert_eq!(component.total_frames(), 12);
        assert_eq!(component.layers().len(), 2);
        assert_eq!(component.objects().len(), 13);
    }
}

#[test]
fn declared_contexts_are_the_scene_then_the_components_in_table_order() {
    let w = world("D");
    assert_eq!(
        w.source().declared_contexts(),
        vec!["scene-root", "sym_comp", "sym_comp_b"]
    );
    let scene = w.source().context("scene-root").unwrap();
    assert_eq!(scene.kind(), ContextKind::SceneRoot);
    assert_eq!(scene.id(), "scene-root");
    assert_eq!(scene.total_frames(), 24);
    assert_eq!(scene.layers().len(), 11);
    assert!(scene.component().is_none());
    let comp = w.source().context("sym_comp_b").unwrap();
    assert_eq!(comp.kind(), ContextKind::Component);
    assert_eq!(comp.id(), "sym_comp_b");
    assert_eq!(comp.total_frames(), 12);
    assert_eq!(comp.component().unwrap().symbol_id(), "sym_comp_b");
}

#[test]
fn a_source_without_components_declares_only_the_scene() {
    let w = world("D");
    let bare = ContextSource::new(24, &w.layers, &w.objects, None);
    assert_eq!(bare.declared_contexts(), vec!["scene-root"]);
    assert_eq!(
        bare.context("sym_comp").err(),
        Some(ResolveError::UnknownContext)
    );
    assert!(bare.components().is_empty());
    assert!(bare.instance("ly_comp_loop").is_none());
}

#[test]
fn every_baseline_stroke_resolves_to_its_own_record() {
    let w = world("D");
    let source = w.source();
    let mut seen = 0;
    for row in rows("S") {
        let (sym, layer, frame, stroke, fill) = (
            row[1],
            row[2],
            row[3].parse::<u32>().unwrap(),
            row[4],
            row[5],
        );
        let context = source.context(sym).unwrap();
        let identity = ElementIdentity::new(sym, layer, stroke).unwrap();
        let hit = context.element_at(&identity, frame).unwrap();
        assert_eq!(hit.record.target.context_id, sym);
        assert_eq!(hit.record.target.layer_uid, layer);
        assert_eq!(hit.record.target.stroke_id, stroke);
        assert_eq!(hit.record.target.frame_scope.frame, frame);
        assert_eq!(hit.layer.layer_uid(), layer);
        assert_eq!(fill_hex(hit.record), fill);
        assert!(std::ptr::eq(
            hit.record,
            &context.objects()[hit.object_index]
        ));
        assert!(std::ptr::eq(hit.layer, &context.layers()[hit.layer_index]));
        // The full native target resolves to the very same record.
        let again = context
            .resolve_target(&authored(sym, frame, layer, stroke))
            .unwrap();
        assert_eq!(again.object_index, hit.object_index);
        seen += 1;
    }
    assert_eq!(seen, 26);
}

#[test]
fn the_same_stroke_id_in_two_components_is_two_distinct_targets() {
    let w = world("D");
    let source = w.source();
    let a = source.context("sym_comp").unwrap();
    let b = source.context("sym_comp_b").unwrap();
    let in_a = ElementIdentity::new("sym_comp", "ly_comp_ia", "s_comp_a_1").unwrap();
    let in_b = ElementIdentity::new("sym_comp_b", "ly_comp_ia", "s_comp_a_1").unwrap();
    assert_ne!(in_a, in_b);
    let ra = a.element_at(&in_a, 1).unwrap();
    let rb = b.element_at(&in_b, 1).unwrap();
    assert_eq!(ra.record.target.stroke_id, rb.record.target.stroke_id);
    assert_eq!(ra.record.target.layer_uid, rb.record.target.layer_uid);
    assert_eq!(ra.record.target.frame_scope, rb.record.target.frame_scope);
    assert_ne!(ra.record.target, rb.record.target);
    assert_eq!(ra.record.target.context_id, "sym_comp");
    assert_eq!(rb.record.target.context_id, "sym_comp_b");
    assert!(!std::ptr::eq(ra.record, rb.record));
    // Each context refuses the other's identity instead of serving a look-alike.
    assert_eq!(
        a.element_at(&in_b, 1).err(),
        Some(ResolveError::WrongContext)
    );
    assert_eq!(
        b.element_at(&in_a, 1).err(),
        Some(ResolveError::WrongContext)
    );
    // Every one of the 13 stroke ids exists in both components, as 13 distinct pairs.
    let ids_a: BTreeSet<_> = a
        .identities()
        .iter()
        .map(|i| i.stroke_id().to_string())
        .collect();
    let ids_b: BTreeSet<_> = b
        .identities()
        .iter()
        .map(|i| i.stroke_id().to_string())
        .collect();
    assert_eq!(ids_a.len(), 13);
    assert_eq!(ids_a, ids_b);
}

#[test]
fn unknown_contexts_layers_strokes_frames_and_scopes_are_typed_errors() {
    let w = world("D");
    let source = w.source();
    for (context, expected) in [
        ("", ResolveError::EmptyId),
        ("sym_missing", ResolveError::UnknownContext),
        ("Scene-root", ResolveError::UnknownContext),
        ("sym_comp ", ResolveError::UnknownContext),
        ("0", ResolveError::UnknownContext),
    ] {
        assert_eq!(source.context(context).err(), Some(expected), "{context:?}");
    }
    let comp = source.context("sym_comp").unwrap();
    let cases = [
        (
            authored("sym_comp", 1, "ly_missing", "s_comp_a_1"),
            ResolveError::UnknownLayer,
        ),
        (
            authored("sym_comp", 1, "0", "s_comp_a_1"),
            ResolveError::UnknownLayer,
        ),
        (
            authored("sym_comp", 1, "", "s_comp_a_1"),
            ResolveError::EmptyId,
        ),
        (
            authored("sym_comp", 1, "ly_comp_ia", ""),
            ResolveError::EmptyId,
        ),
        (
            authored("", 1, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::EmptyId,
        ),
        (
            authored("sym_comp", 1, "ly_comp_ia", "s_absent"),
            ResolveError::UnknownStroke,
        ),
        // A stroke of the other inner layer is not found through this layer.
        (
            authored("sym_comp", 0, "ly_comp_ib", "s_comp_a_0"),
            ResolveError::UnknownStroke,
        ),
        // The element exists, but its keyed drawing is at frame 1.
        (
            authored("sym_comp", 0, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::NotAuthoredAtFrame,
        ),
        (
            authored("sym_comp", 12, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::FrameOutOfRange,
        ),
        (
            authored("sym_comp", u32::MAX, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::FrameOutOfRange,
        ),
        (
            target(
                "sym_comp",
                FrameScopeKind::Reference,
                1,
                "ly_comp_ia",
                "s_comp_a_1",
            ),
            ResolveError::ReferenceUnavailable,
        ),
        (
            authored("sym_comp_b", 1, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::WrongContext,
        ),
        (
            authored("scene-root", 1, "ly_comp_ia", "s_comp_a_1"),
            ResolveError::WrongContext,
        ),
    ];
    for (target, expected) in cases {
        assert_eq!(
            comp.resolve_target(&target).err(),
            Some(expected),
            "{target:?}"
        );
    }
    // The scene is another context: an inner target is not a scene target.
    let scene = source.context("scene-root").unwrap();
    assert_eq!(
        scene
            .resolve_target(&authored("sym_comp", 1, "ly_comp_ia", "s_comp_a_1"))
            .err(),
        Some(ResolveError::WrongContext)
    );
    assert_eq!(
        scene
            .resolve_target(&authored("scene-root", 1, "ly_comp_ia", "s_comp_a_1"))
            .err(),
        Some(ResolveError::UnknownLayer)
    );
}

#[test]
fn the_scene_context_resolves_scene_records_and_keeps_reference_scope() {
    let w = world("D");
    let mut record = serde_json::to_value(&w.set.components[0].objects()[0]).unwrap();
    record["target"]["contextId"] = serde_json::json!("scene-root");
    record["target"]["layerUid"] = serde_json::json!("ly_plain");
    record["target"]["strokeId"] = serde_json::json!("s_scene_0");
    let authored_record: ObjectRecord = serde_json::from_value(record.clone()).unwrap();
    record["target"]["frameScope"] = serde_json::json!({"kind": "reference", "frame": 2});
    let reference_record: ObjectRecord = serde_json::from_value(record).unwrap();
    let objects = vec![authored_record, reference_record];
    let source = ContextSource::new(24, &w.layers, &objects, Some(&w.set));
    let scene = source.context("scene-root").unwrap();
    let hit = scene
        .resolve_target(&authored("scene-root", 0, "ly_plain", "s_scene_0"))
        .unwrap();
    assert_eq!((hit.layer_index, hit.object_index), (9, 0));
    let reference = scene
        .resolve_target(&target(
            "scene-root",
            FrameScopeKind::Reference,
            2,
            "ly_plain",
            "s_scene_0",
        ))
        .unwrap();
    assert_eq!(reference.object_index, 1);
    assert_eq!(
        scene
            .resolve_target(&authored("scene-root", 24, "ly_plain", "s_scene_0"))
            .err(),
        Some(ResolveError::FrameOutOfRange)
    );
    assert_eq!(
        scene
            .resolve_target(&authored("scene-root", 2, "ly_plain", "s_scene_0"))
            .err(),
        Some(ResolveError::NotAuthoredAtFrame)
    );
    // Reference-scoped records are not elements of the authored identity space.
    assert_eq!(scene.identities().len(), 1);
    // The scene stroke does not exist inside a component, whatever its id.
    let comp = source.context("sym_comp").unwrap();
    assert_eq!(
        comp.resolve_target(&authored("sym_comp", 0, "ly_plain", "s_scene_0"))
            .err(),
        Some(ResolveError::UnknownLayer)
    );
}

#[test]
fn an_instance_layer_is_found_in_the_scene_and_never_inside_a_component() {
    let w = world("D");
    let source = w.source();
    for row in rows("H") {
        let found = source.instance(row[1]);
        assert_eq!(
            found.map(|i| i.symbol_id.as_str()),
            (row[2] != "-").then_some(row[2])
        );
    }
    assert!(source.instance("ly_comp_ia").is_none());
    let comp = source.context("sym_comp").unwrap();
    assert!(comp.layer("ly_comp_loop").is_none());
}
