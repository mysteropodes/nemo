//! nemo-test-harness: test-evaluation => property_expression_views_controls_tests
//! Controls for the expression context views: mutation detection, identity,
//! detachment and the contexts this provider refuses to describe.

use crate::evaluation::property_expression_views::{
    bind_for_test, ExpressionContextView, ViewUnavailable,
};
use crate::object_document::{OrderContext, OrderPlan, Presence, ProjectMetadata};
use crate::property_expression_views_tests::{
    document, encode, holder, layer, parse, run, target, Case, ORACLE,
};
use crate::revision::ObjectSnapshot;

fn rows() -> Vec<Case> {
    ORACLE.lines().map(parse).collect()
}

/// Each plausible misreading of a Motion rule disagrees with some oracle row.
#[test]
fn oracle_rejects_rule_misreadings() {
    let cases = rows();
    for c in &cases {
        assert_eq!(run(c), c.expected);
    }
    let field = |g: &str, i: usize| g.split(',').nth(i).unwrap().to_owned();
    let bits = |x: f64| format!("{:016x}", x.to_bits());
    let stored_parent = |c: &Case| {
        let me = c
            .document
            .layers()
            .iter()
            .find(|l| l.layer_uid() == c.target.layer_uid)?;
        me.parent_layer_uid.value().cloned().flatten()
    };
    // Parent found by uid instead of by the parent's first name.
    let by_uid = cases.iter().filter(|c| {
        let uid = stored_parent(c);
        let at = c
            .document
            .layers()
            .iter()
            .position(|l| Some(l.layer_uid()) == uid.as_deref());
        at.is_some_and(|i| c.expected[1] != "-" && !c.expected[1].ends_with(&bits(i as f64)))
    });
    assert!(by_uid.count() > 0);
    // duration without `|| 1`.
    let raw = cases.iter().filter(|c| {
        let fps = c.metadata.fps();
        field(&c.expected[0], 7) != bits(f64::from(c.document.total_frames()) / fps)
    });
    assert!(raw.count() > 0);
    // hasParent as "a parent uid is stored", counting "".
    let stored = cases
        .iter()
        .filter(|c| stored_parent(c).is_some() && field(&c.expected[1], 4) == "F");
    assert!(stored.count() > 0);
    // An unresolved holder still answering with a default layer view.
    assert!(cases
        .iter()
        .any(|c| c.expected[2] == "-" && field(&c.expected[1], 2) == bits(-1.0)));
}

#[test]
fn bare_drawing_has_no_comp_but_keeps_self() {
    let doc = document(
        12,
        vec![layer("a", Presence::Present("A".into()), Presence::Missing)],
        vec![],
    );
    let snapshot = ObjectSnapshot::from_admitted("inst".into(), doc).unwrap();
    let view = ExpressionContextView::bind(&snapshot, &target("a", None), "opacity").unwrap();
    assert_eq!(view.comp(), Err(ViewUnavailable::ProjectMetadata));
    let me = view.self_view().expect("layer resolves");
    assert_eq!(
        (me.name(), me.index(), me.is_shape()),
        (Some("A"), 0, false)
    );
    assert_eq!(view.property(), "opacity");
}

#[test]
fn identity_is_strict_and_views_are_detached() {
    let doc = document(
        12,
        vec![layer("a", Presence::Present("A".into()), Presence::Missing)],
        vec![],
    );
    let first = ObjectSnapshot::from_admitted("inst".into(), doc.clone()).unwrap();
    let other = ObjectSnapshot::from_admitted("inst".into(), doc).unwrap();
    let view = ExpressionContextView::bind(&first, &target("a", None), "position").unwrap();
    assert!(view.is_bound_to(&first) && view.is_bound_to(&first.clone()));
    assert!(
        !view.is_bound_to(&other),
        "same content, different revision identity"
    );
    assert_eq!(view.identity(), view.clone().identity());
    fn owned<T: 'static>(_: &T) {}
    owned(&view);
    drop(first);
    drop(other);
    assert_eq!(view.self_view().unwrap().name(), Some("A"));
}

#[test]
fn later_document_edits_do_not_reach_a_bound_view() {
    let mut case = parse(ORACLE.lines().next().unwrap());
    let view = bind_for_test(
        &case.document,
        Some(&case.metadata),
        &case.target,
        &case.property,
    )
    .unwrap();
    let before = encode(&view);
    case.document.layers[0].name = Presence::Present("Renamed".into());
    case.document.layers.clear();
    case.document.total_frames = 1;
    assert_eq!(encode(&view), before);
    assert_eq!(before, case.expected);
}

#[test]
fn element_targets_need_their_own_holder() {
    let layers = vec![
        layer("a", Presence::Present("A".into()), Presence::Missing),
        layer("b", Presence::Present("B".into()), Presence::Missing),
    ];
    let meta = ProjectMetadata::new(10, 10, 24.0).unwrap();
    let bind = |holders, t| {
        bind_for_test(&document(4, layers.clone(), holders), Some(&meta), &t, "p").unwrap()
    };
    let own = bind(
        vec![holder(target("b", Some("s1")))],
        target("b", Some("s1")),
    );
    assert!(own
        .self_view()
        .is_some_and(|v| v.is_shape() && v.stroke_id() == Some("s1")));
    assert!(bind(vec![], target("b", Some("s1"))).self_view().is_none());
    assert!(bind(
        vec![holder(target("a", Some("s1")))],
        target("b", Some("s1"))
    )
    .self_view()
    .is_none());
    assert!(bind(
        vec![holder(target("b", Some("s2")))],
        target("b", Some("s1"))
    )
    .self_view()
    .is_none());
    let layer_view = bind(vec![], target("b", None));
    assert!(layer_view
        .self_view()
        .is_some_and(|v| !v.is_shape() && v.layer_uid() == "b"));
}

#[test]
fn unsupported_contexts_are_refused_not_guessed() {
    let meta = ProjectMetadata::new(10, 10, 24.0).unwrap();
    let mut doc = document(
        4,
        vec![layer("a", Presence::Missing, Presence::Missing)],
        vec![],
    );
    let mut t = target("a", None);
    t.context_id = "component-1".into();
    assert_eq!(
        bind_for_test(&doc, Some(&meta), &t, "p"),
        Err(ViewUnavailable::ComponentContext)
    );
    let context = |id: &str| OrderContext {
        context_id: id.into(),
        layers: vec![],
        scopes: vec![],
    };
    let plan = |contexts| OrderPlan {
        format: String::new(),
        format_version: 0,
        document_format_version: 0,
        contexts,
        migration: String::new(),
        reference_policy: String::new(),
    };
    doc.composition_order = Presence::Present(plan(vec![context("scene-root"), context("c")]));
    assert_eq!(
        bind_for_test(&doc, Some(&meta), &target("a", None), "p"),
        Err(ViewUnavailable::ComponentLayers)
    );
    doc.composition_order = Presence::Present(plan(vec![context("scene-root")]));
    let view = bind_for_test(&doc, Some(&meta), &target("a", None), "p").unwrap();
    assert_eq!(view.self_view().map(|v| v.name()), Some(None));
    assert_eq!(
        view.comp().map(|c| (c.layers(), c.frames(), c.name())),
        Ok((1, 4, "Scene"))
    );
}
