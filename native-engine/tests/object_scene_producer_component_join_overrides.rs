//! Override and paint tests of the component producer join (L07-K): the exposed
//! properties through the L04-E kernel and the property-runtime port, and the typed
//! paint of the source record. Expected values come from the baseline oracle.

use super::rows::{base, diff, entry_of, oracle, outcome_of, Prepared};
use super::support::{component_mut, FixturePorts, Runtime};
use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedError, ExposedValue,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    ComponentUnavailable, ShapePaint, NO_FILL, VECTOR_BODY,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::ScenePaintError;
use crate::object_document::SolidFill;
use serde_json::json;

fn fill(r: u8, g: u8, b: u8) -> ExposedValue {
    let solid: SolidFill = serde_json::from_value(
        json!({"kind":"solid","r":f64::from(r),"g":f64::from(g),"b":f64::from(b),"a":1}),
    )
    .unwrap();
    ExposedValue::Paint(Some(solid))
}

fn channels(paint: &ShapePaint) -> [f64; 4] {
    match paint {
        ShapePaint::Solid(solid) => solid.channels().map(|n| n.as_f64().unwrap()),
        other => panic!("expected a solid paint, got {other:?}"),
    }
}

#[test]
fn exposed_overrides_use_the_property_port_and_stay_per_instance() {
    let set = oracle().remove(2); // camera set: loop + ping (symMatrix), camera keys
    let mut components = set.components.clone();
    component_mut(&mut components, "sym_comp")["exposedProps"] = json!([
        {"key":"ep_fill","label":"A fill","targetStrokeId":"s_comp_a_0","targetField":"fillColor"},
        {"key":"ep_vis","label":"B visible","targetStrokeId":"s_comp_b_0","targetField":"__visible"}
    ]);
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    assert_eq!(
        prepared
            .table
            .validate(prepared.snapshot.document().layers()),
        Ok(())
    );
    let stored = prepared.table.clone();
    let packet = prepared.packet(0);
    let mut runtime = Runtime {
        wired: true,
        ..Runtime::default()
    };
    for (uid, color, visible) in [
        ("ly_comp_loop", fill(0, 1, 0), 100.0),
        ("ly_comp_ping", fill(0, 0, 1), 10.0),
    ] {
        runtime
            .values
            .insert((uid.to_owned(), "ep_fill".to_owned()), color);
        runtime.values.insert(
            (uid.to_owned(), "ep_vis".to_owned()),
            ExposedValue::Number(visible),
        );
    }
    let production = prepared
        .produce_with(&packet, &mut prepared.ports(), &mut runtime)
        .unwrap();
    let scene = production
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap();
    let find = |uid: &str| match &entry_of(&scene, uid).outcome {
        ComponentOutcome::Shapes(placed) => placed.shapes.clone(),
        other => panic!("{uid}: {other:?}"),
    };
    let (loop_shapes, ping_shapes) = (find("ly_comp_loop"), find("ly_comp_ping"));
    // 100 keeps B visible, 10 (< 50) hides it; the fill override is per instance.
    assert_eq!(loop_shapes.len(), 2);
    assert_eq!(ping_shapes.len(), 1);
    assert_eq!(channels(&loop_shapes[0].paint), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(channels(&ping_shapes[0].paint), [0.0, 0.0, 1.0, 1.0]);
    // The un-overridden shape keeps the stored fill (#5856d6).
    assert_eq!(
        channels(&loop_shapes[1].paint),
        [
            0.34509803921568627,
            0.33725490196078434,
            0.8392156862745098,
            1.0
        ]
    );
    // Geometry is the baseline's, camera and matrix included; ping lost only B.
    assert_eq!(
        diff(entry_of(&scene, "ly_comp_loop"), &set.expects[0].want),
        None
    );
    let mut ping_want = set.expects[24].want.clone();
    assert_eq!(set.expects[24].uid, "ly_comp_ping");
    ping_want["shapes"].as_array_mut().unwrap().truncate(1);
    assert_eq!(diff(entry_of(&scene, "ly_comp_ping"), &ping_want), None);
    // The stored table and the stored fills were never edited.
    assert_eq!(prepared.table, stored);
    assert_eq!(
        runtime.reads,
        [
            ("ly_comp_loop".to_owned(), "ep_fill".to_owned(), 0),
            ("ly_comp_loop".to_owned(), "ep_vis".to_owned(), 0),
            ("ly_comp_ping".to_owned(), "ep_fill".to_owned(), 0),
            ("ly_comp_ping".to_owned(), "ep_vis".to_owned(), 0),
        ]
    );
    assert_eq!(runtime.availability_asks, 2);
}

#[test]
fn an_unwired_property_runtime_or_a_missing_value_is_a_typed_unavailable_slot() {
    let set = oracle().remove(2);
    let mut components = set.components.clone();
    component_mut(&mut components, "sym_comp")["exposedProps"] = json!([
        {"key":"ep_fill","label":"A fill","targetStrokeId":"s_comp_a_0","targetField":"fillColor"}
    ]);
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let packet = prepared.packet(0);
    let scene = |runtime: &mut Runtime| {
        prepared
            .produce_with(&packet, &mut prepared.ports(), runtime)
            .unwrap()
            .join(&packet, &ScenePin::of_packet(&packet))
            .unwrap()
    };
    let mut unwired = Runtime::default();
    let joined = scene(&mut unwired);
    assert_eq!(unwired.availability_asks, 2);
    assert!(unwired.reads.is_empty());
    for uid in ["ly_comp_loop", "ly_comp_ping"] {
        assert_eq!(
            entry_of(&joined, uid).outcome,
            ComponentOutcome::Unavailable(ComponentUnavailable::OverridesUnavailable)
        );
    }
    assert_eq!(joined.unavailable().count(), 2);
    let mut empty = Runtime {
        wired: true,
        ..Runtime::default()
    };
    let joined = scene(&mut empty);
    assert_eq!(
        entry_of(&joined, "ly_comp_loop").outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::Overrides(ExposedError::Port {
            key: "ep_fill".to_owned(),
            message: "no value for ly_comp_loop/ep_fill".to_owned()
        }))
    );
    // Components that declare nothing never ask the runtime.
    let plain = base();
    let packet = plain.packet(0);
    let mut runtime = Runtime {
        wired: true,
        ..Runtime::default()
    };
    plain
        .produce_with(&packet, &mut FixturePorts::default(), &mut runtime)
        .unwrap();
    assert_eq!(runtime.availability_asks, 0);
    assert!(runtime.reads.is_empty());
}

#[test]
fn paint_is_the_typed_fill_of_the_source_record_and_pending_never_drops_the_shape() {
    let prepared = base();
    let ComponentOutcome::Shapes(placed) = outcome_of(&prepared, 0, "ly_comp_loop") else {
        panic!("shapes");
    };
    // Literals: #ff9500 and #5856d6 as 8-bit channels over 255.
    assert_eq!(
        channels(&placed.shapes[0].paint),
        [1.0, 0.5843137254901961, 0.0, 1.0]
    );
    assert_eq!(
        channels(&placed.shapes[1].paint),
        [
            0.34509803921568627,
            0.33725490196078434,
            0.8392156862745098,
            1.0
        ]
    );
    // Identity is the stable component target, with the authored frame it was read from.
    let target = &placed.shapes[1].target;
    assert_eq!(
        (
            target.context_id(),
            target.layer_uid(),
            target.stroke_id(),
            target.frame()
        ),
        ("sym_comp", "ly_comp_ib", "s_comp_b_0", 0)
    );
    // An unrepresentable fill keeps the geometry and reports the paint as pending.
    let set = oracle().remove(0);
    let mut components = set.components.clone();
    component_mut(&mut components, "sym_comp")["objects"][0]["fill"]["r"] = json!(2.0);
    let pending = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let ComponentOutcome::Shapes(placed) = outcome_of(&pending, 0, "ly_comp_loop") else {
        panic!("shapes");
    };
    assert_eq!(
        placed.shapes[0].paint,
        ShapePaint::Pending(ScenePaintError::UnrepresentableStraightRgba.message())
    );
    assert_eq!(placed.shapes.len(), 2);
}

#[test]
fn a_vector_body_or_an_absent_fill_keeps_the_shape_with_a_pending_paint() {
    let set = oracle().remove(0);
    let mut components = set.components.clone();
    let objects = component_mut(&mut components, "sym_comp")["objects"]
        .as_array_mut()
        .unwrap();
    // s_comp_a_0 becomes an editable-vector-path record (its stroke and gradient drawing
    // belong to the style consumer); s_comp_b_0 keeps the legacy family with fill null.
    assert_eq!(objects[0]["target"]["strokeId"], "s_comp_a_0");
    objects[0]["family"] = json!("editable-vector-path");
    objects[0]["vector"] = json!({"stroke": null, "hasRealStroke": false, "strokeWidth": 3,
        "strokeCap": "round", "strokeJoin": "round", "opacity": 1});
    assert_eq!(objects[12]["target"]["strokeId"], "s_comp_b_0");
    objects[12]["fill"] = json!(null);
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let packet = prepared.packet(0);
    let scene = prepared
        .production(&packet)
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap();
    assert_eq!(scene.unavailable().count(), 0);
    let entry = entry_of(&scene, "ly_comp_loop");
    let ComponentOutcome::Shapes(placed) = &entry.outcome else {
        panic!("shapes");
    };
    assert_eq!(placed.shapes.len(), 2);
    assert_eq!(placed.shapes[0].paint, ShapePaint::Pending(VECTOR_BODY));
    assert_eq!(placed.shapes[1].paint, ShapePaint::Pending(NO_FILL));
    // The geometry is never dropped because a paint is pending: it equals the baseline row.
    let row = set
        .expects
        .iter()
        .find(|e| e.uid == "ly_comp_loop" && e.frame == 0)
        .unwrap();
    assert_eq!(diff(entry, &row.want), None);
}
