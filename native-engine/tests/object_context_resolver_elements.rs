//! Element-level resolver tests (L04-J, F14): frame-local presentation ids, exposed property
//! targets and the identity of keyed drawings. Sibling of `object_context_resolver.rs`; same
//! world and oracle rows (`P` = verbatim `layerElements`/`elementLabel`, `A` = verbatim
//! `applyOverrides`).
use super::world::{rows, world};
use super::*;
use std::collections::BTreeSet;

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
fn frame_local_presentation_ids_never_resolve() {
    let w = world("D");
    let source = w.source();
    let mut labels: Vec<(&str, &str)> = Vec::new();
    for row in rows("P") {
        let (sym, layer, frame, index, label, stroke) = (
            row[1],
            row[2],
            row[3].parse::<u32>().unwrap(),
            row[4],
            row[5],
            row[6],
        );
        let context = source.context(sym).unwrap();
        // The positional label and the positional index are not identities...
        for presentation in [label, index] {
            let id = ElementIdentity::new(sym, layer, presentation).unwrap();
            assert_eq!(
                context.element_at(&id, frame).err(),
                Some(ResolveError::UnknownStroke)
            );
            assert!(context.authored_frames(&id).is_empty());
        }
        assert!(context.layer(index).is_none());
        // ...while the stable stroke id is, at the frame it is drawn on.
        let id = ElementIdentity::new(sym, layer, stroke).unwrap();
        assert_eq!(
            context
                .element_at(&id, frame)
                .unwrap()
                .record
                .target
                .stroke_id,
            stroke
        );
        if sym == "sym_comp" {
            labels.push((label, stroke));
        }
    }
    // The baseline label is NOT stable: one label ("Shape 1") names 13 different strokes.
    let shape_1: BTreeSet<&str> = labels
        .iter()
        .filter(|(l, _)| *l == "Shape 1")
        .map(|(_, s)| *s)
        .collect();
    assert_eq!(shape_1.len(), 13);
    assert!(shape_1.contains("s_comp_a_0") && shape_1.contains("s_comp_a_11"));
    assert!(shape_1.contains("s_comp_b_0"));
}

#[test]
fn exposed_properties_resolve_to_elements_of_their_own_component_only() {
    let w = world("D");
    let source = w.source();
    let mut checked = 0;
    for row in rows("A") {
        let (sym, frame, csv) = (row[1], row[2].parse::<u32>().unwrap(), row[3]);
        let context = source.context(sym).unwrap();
        let mut reached = BTreeSet::new();
        for prop in context.component().unwrap().exposed_props() {
            for identity in context.exposed_targets(prop) {
                assert_eq!(identity.context_id(), sym);
                if context.element_at(&identity, frame).is_ok() {
                    reached.insert(identity.stroke_id().to_string());
                }
            }
        }
        let expected: BTreeSet<String> = if csv == "-" {
            BTreeSet::new()
        } else {
            csv.split(',').map(str::to_string).collect()
        };
        assert_eq!(reached, expected, "{sym} frame {frame}");
        checked += 1;
    }
    assert_eq!(checked, 24);
    // Hand-typed from the fixture: the same stroke ids are exposed differently per component.
    let a = source.context("sym_comp").unwrap();
    let b = source.context("sym_comp_b").unwrap();
    let a0 = a.exposed_targets(&a.component().unwrap().exposed_props()[0]);
    assert_eq!(
        a0,
        vec![ElementIdentity::new("sym_comp", "ly_comp_ia", "s_comp_a_0").unwrap()]
    );
    assert_eq!(b.component().unwrap().exposed_props().len(), 1);
    assert!(
        a.exposed_targets(&b.component().unwrap().exposed_props()[0])
            .len()
            == 1
    );
    // A scene context has no exposed targets.
    let scene = source.context("scene-root").unwrap();
    assert!(scene
        .exposed_targets(&a.component().unwrap().exposed_props()[0])
        .is_empty());
}

#[test]
fn identities_and_authored_frames_follow_the_keyed_drawings() {
    let w = world("D");
    let source = w.source();
    let a = source.context("sym_comp").unwrap();
    let identities = a.identities();
    assert_eq!(identities.len(), 13);
    assert_eq!(
        identities[0],
        ElementIdentity::new("sym_comp", "ly_comp_ia", "s_comp_a_0").unwrap()
    );
    assert_eq!(
        identities[12],
        ElementIdentity::new("sym_comp", "ly_comp_ib", "s_comp_b_0").unwrap()
    );
    let a3 = ElementIdentity::new("sym_comp", "ly_comp_ia", "s_comp_a_3").unwrap();
    assert_eq!(a.authored_frames(&a3), vec![3]);
    assert_eq!(
        a.authored_frames(&ElementIdentity::new("sym_comp", "ly_comp_ia", "s_none").unwrap()),
        Vec::<u32>::new()
    );
    assert_eq!(
        a3.target_at(3),
        authored("sym_comp", 3, "ly_comp_ia", "s_comp_a_3")
    );
    assert_eq!(ElementIdentity::of_target(&a3.target_at(7)).unwrap(), a3);
    assert_eq!(a3.in_context("sym_x").context_id(), "sym_x");
    assert_eq!(a3.in_context("sym_x").stroke_id(), "s_comp_a_3");
    for (c, l, s) in [("", "l", "s"), ("c", "", "s"), ("c", "l", "")] {
        assert_eq!(
            ElementIdentity::new(c, l, s).err(),
            Some(ResolveError::EmptyId)
        );
    }
}

#[test]
fn holder_targets_resolve_per_context_like_the_scene_holder_check() {
    let w = world("D");
    let source = w.source();
    let comp = source.context("sym_comp").unwrap();
    assert_eq!(comp.resolve_holder("ly_comp_ia", None), Ok(()));
    assert_eq!(
        comp.resolve_holder("ly_comp_ia", Some("s_comp_a_3")),
        Ok(())
    );
    assert_eq!(
        comp.resolve_holder("ly_comp_ib", Some("s_comp_b_0")),
        Ok(())
    );
    let refusals = [
        ("", None, ResolveError::EmptyId),
        ("ly_comp_ia", Some(""), ResolveError::EmptyId),
        ("ly_missing", None, ResolveError::UnknownLayer),
        ("ly_comp_loop", None, ResolveError::UnknownLayer),
        ("ly_comp_ia", Some("s_none"), ResolveError::UnknownStroke),
        ("ly_comp_ia", Some("Shape 1"), ResolveError::UnknownStroke),
        (
            "ly_comp_ib",
            Some("s_comp_a_0"),
            ResolveError::UnknownStroke,
        ),
    ];
    for (layer, stroke, expected) in refusals {
        assert_eq!(
            comp.resolve_holder(layer, stroke),
            Err(expected),
            "{layer:?} {stroke:?}"
        );
    }
    // The scene context knows its own layers and none of the component's.
    let scene = source.context("scene-root").unwrap();
    assert_eq!(scene.resolve_holder("ly_plain", None), Ok(()));
    assert_eq!(
        scene.resolve_holder("ly_comp_ia", None),
        Err(ResolveError::UnknownLayer)
    );
    assert_eq!(
        scene.resolve_holder("ly_plain", Some("s_comp_a_0")),
        Err(ResolveError::UnknownStroke)
    );
}
