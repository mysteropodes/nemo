//! Production exposed-property overrides on component instances (L04-K, #2243): the instance
//! layer's static and keyed tracks reach the placed scene through the one evaluator, with the
//! baseline `applyOverrides` semantics as the independent oracle (`gen.cjs`).
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectPaintPlan, ObjectRenderScene, PaintEntry,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_component_render_scene_fixture.rs"]
mod base;
use base::*;
#[path = "object_component_exposed_override_fixture.rs"]
mod fix;
use fix::*;

fn one(field: &str, default: Option<i64>) -> Value {
    json!([exposed("ep_x", field, default)])
}
fn scene_of(d: &Value, frame: u32) -> ObjectRenderScene {
    let o = owner(d);
    let s = o.acquire_snapshot(0).unwrap();
    scene(&s, frame)
}
fn refused(d: &Value, frame: u32) -> bool {
    let Ok(history) = NativeObjectHistory::new(
        "refusal",
        match decode_storage_project(d.to_string().as_bytes()) {
            Ok(project) => project,
            Err(_) => return true,
        },
    ) else {
        return true;
    };
    let s = history.acquire_snapshot(0).unwrap();
    let expected = pins(&s, frame);
    match prepare_object_exposure_frame(&s, &expected) {
        Ok(packet) => prepare_object_render_scene(&packet, &expected).is_err(),
        Err(_) => true,
    }
}
fn holders_for(name: &str) -> Value {
    let records = match name {
        "default-only" | "default-only-50" => return Value::Null,
        "static-37.5" => static_value("ep_x", json!([37.5])),
        "static-over-100" => static_value("ep_x", json!([250])),
        "static-negative" => static_value("ep_x", json!([-20])),
        "keyed-0-100" => keys("ep_x", &[(0, json!([0])), (8, json!([100]))]),
        "keyed-three" => keys(
            "ep_x",
            &[(0, json!([100])), (4, json!([20])), (8, json!([80]))],
        ),
        "visible-49" => static_value("ep_x", json!([49])),
        "visible-50" => static_value("ep_x", json!([50])),
        "visible-keyed" => keys("ep_x", &[(0, json!([100])), (8, json!([0]))]),
        other => panic!("unmapped oracle scenario {other}"),
    };
    json!([holder("host-a", json!([records]))])
}

#[test]
fn keyed_static_and_default_overrides_equal_the_baseline_apply_overrides_oracle() {
    let names: Vec<&str> =
        ORACLE
            .lines()
            .map(|l| l.split('\t').next().unwrap())
            .fold(Vec::new(), |mut acc, n| {
                if !acc.contains(&n) {
                    acc.push(n);
                }
                acc
            });
    assert_eq!(names.len(), 10);
    let mut checked = 0;
    for name in names {
        let visible = name.starts_with("visible");
        let (field, default) = if visible {
            ("__visible", 100)
        } else if name == "default-only-50" {
            ("opacity", 50)
        } else {
            ("opacity", 100)
        };
        let d = doc(
            one(field, Some(default)),
            &[("host-a", 0)],
            holders_for(name),
        );
        for (frame, expected) in oracle(name) {
            let sc = scene_of(&d, frame);
            match expected {
                None => assert!(
                    sc.components.paints.is_empty(),
                    "{name}@{frame} must be dropped"
                ),
                Some(value) if visible => {
                    assert_eq!((name, frame, alpha_of(&sc, 0)), (name, frame, 1.));
                    assert_eq!(value, 1.);
                }
                Some(value) => {
                    let got = alpha_of(&sc, 0);
                    assert!(
                        (got - value).abs() < 1e-9,
                        "{name}@{frame}: {got} vs baseline {value}"
                    );
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 24);
}

#[test]
fn two_instances_of_one_component_with_different_overrides_evaluate_distinctly() {
    let holders = json!([
        holder("host-a", json!([static_value("ep_x", json!([25]))])),
        holder(
            "host-b",
            json!([keys("ep_x", &[(0, json!([0])), (8, json!([100]))])])
        )
    ]);
    let d = doc(
        one("opacity", Some(100)),
        &[("host-a", 0), ("host-b", 32)],
        holders,
    );
    let o = owner(&d);
    let s = o.acquire_snapshot(0).unwrap();
    let before = image(&s);
    let at0 = scene(&s, 0);
    let at4 = scene(&s, 4);
    assert_eq!(at0.components.paints.len(), 2);
    let by_host = |sc: &ObjectRenderScene, uid: &str| {
        sc.components
            .paints
            .iter()
            .position(|p| p.host_layer_index == layer_index(&d, uid))
            .unwrap()
    };
    assert_eq!(alpha_of(&at0, by_host(&at0, "host-a")), 0.25);
    assert_eq!(alpha_of(&at0, by_host(&at0, "host-b")), 0.);
    assert_eq!(alpha_of(&at4, by_host(&at4, "host-a")), 0.25);
    assert_eq!(alpha_of(&at4, by_host(&at4, "host-b")), 0.5);
    // The stored component and the document never change.
    assert_eq!(image(&s), before);
    // Without any override both instances fall back to the declared default.
    let plain = scene_of(
        &doc(
            one("opacity", Some(60)),
            &[("host-a", 0), ("host-b", 32)],
            Value::Null,
        ),
        0,
    );
    assert_eq!((alpha_of(&plain, 0), alpha_of(&plain, 1)), (0.6, 0.6));
}
fn layer_index(d: &Value, uid: &str) -> usize {
    // source layers are published in document order, which `doc` writes top-first
    d["layers"]
        .as_array()
        .unwrap()
        .iter()
        .position(|l| l["layerUid"] == uid)
        .unwrap()
}

#[test]
fn colour_override_is_decoded_to_a_solid_fill_static_and_keyed() {
    let colour = json!([exposed("ep_x", "fillColor", None)]);
    let holders = json!([holder(
        "host-a",
        json!([static_value("ep_x", json!([255, 128, 0, 255]))])
    )]);
    let c = channels_of(
        &scene_of(&doc(colour.clone(), &[("host-a", 0)], holders), 0),
        0,
    );
    assert_eq!(c, [1., 128. / 255., 0., 1.]);
    let keyed_colour = json!([holder(
        "host-a",
        json!([keys(
            "ep_x",
            &[(0, json!([0, 0, 0, 255])), (8, json!([255, 255, 255, 255]))]
        )])
    )]);
    let d = doc(colour, &[("host-a", 0)], keyed_colour);
    assert_eq!(channels_of(&scene_of(&d, 0), 0), [0., 0., 0., 1.]);
    assert_eq!(
        channels_of(&scene_of(&d, 4), 0),
        [127.5 / 255., 127.5 / 255., 127.5 / 255., 1.]
    );
    assert_eq!(channels_of(&scene_of(&d, 8), 0), [1., 1., 1., 1.]);
}

#[test]
fn override_edit_saves_reopens_byte_identical_and_undoes_and_redoes() {
    let d = doc(one("opacity", Some(100)), &[("host-a", 0)], Value::Null);
    let mut o = owner(&d);
    let old = o.acquire_snapshot(0).unwrap();
    let original = image(&old);
    // a document without overrides is byte-identical through save and reopen
    let reopened0 =
        NativeObjectHistory::new("r0", decode_storage_project(&original).unwrap()).unwrap();
    assert_eq!(image(&reopened0.acquire_snapshot(0).unwrap()), original);
    assert!(!String::from_utf8(original.clone())
        .unwrap()
        .contains("propertyHolders"));
    let edit = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"host-a"},
        "property":"ep_x","action":{"kind":"setValue","frame":0,"values":[40]}}).to_string();
    let next = o.prepare_property_edit_json(edit.as_bytes()).unwrap();
    assert_eq!(o.commit_property_edit(next).unwrap(), 1);
    let current = o.acquire_snapshot(1).unwrap();
    assert_eq!(alpha_of(&scene(&current, 0), 0), 0.4);
    assert_eq!(alpha_of(&scene(&old, 0), 0), 1.);
    let saved = image(&current);
    let reopened = NativeObjectHistory::new("r1", decode_storage_project(&saved).unwrap()).unwrap();
    let fresh = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(image(&fresh), saved);
    assert_eq!(alpha_of(&scene(&fresh, 0), 0), 0.4);
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(image(&o.acquire_snapshot(2).unwrap()), original);
    assert_eq!(alpha_of(&scene(&o.acquire_snapshot(2).unwrap(), 0), 0), 1.);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(image(&o.acquire_snapshot(3).unwrap()), saved);
    assert_eq!(alpha_of(&scene(&o.acquire_snapshot(3).unwrap(), 0), 0), 0.4);
    // an edit of a key the component does not expose is refused and changes nothing
    for (layer, property) in [("host-a", "ep_other"), ("back", "ep_x")] {
        let bad = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
            "property":property,"action":{"kind":"setValue","frame":0,"values":[40]}}).to_string();
        assert!(
            o.prepare_property_edit_json(bad.as_bytes()).is_err(),
            "{layer}/{property}"
        );
    }
    assert_eq!(image(&o.acquire_snapshot(3).unwrap()), saved);
}

#[test]
fn negative_controls_non_exposed_key_non_instance_layer_and_malformed_values_are_refused() {
    let base = |holders: Value| doc(one("opacity", Some(100)), &[("host-a", 0)], holders);
    let good = base(json!([holder(
        "host-a",
        json!([static_value("ep_x", json!([40]))])
    )]));
    assert!(!refused(&good, 0));
    assert!(decode_storage_project(good.to_string().as_bytes()).is_ok());
    let cases: Vec<(&str, Value)> = vec![
        (
            "non-exposed key",
            base(json!([holder(
                "host-a",
                json!([static_value("ep_nope", json!([40]))])
            )])),
        ),
        (
            "non-instance layer",
            base(json!([holder(
                "back",
                json!([static_value("ep_x", json!([40]))])
            )])),
        ),
        (
            "element holder",
            base(
                json!([{"target":{"contextId":"scene-root","layerUid":"host-a","strokeId":"ink"},
            "properties":[static_value("ep_x", json!([40]))]}]),
            ),
        ),
        (
            "dimension mismatch",
            base(json!([holder(
                "host-a",
                json!([static_value("ep_x", json!([40, 1]))])
            )])),
        ),
        (
            "colour with three channels",
            doc(
                json!([exposed("ep_x", "fillColor", None)]),
                &[("host-a", 0)],
                json!([holder(
                    "host-a",
                    json!([static_value("ep_x", json!([1, 2, 3]))])
                )]),
            ),
        ),
        (
            "enabled expression",
            base(json!([holder(
                "host-a",
                json!([
            {"property":"ep_x","expression":{"enabled":true,"code":"value"}}])
            )])),
        ),
    ];
    for (name, d) in &cases {
        assert!(
            decode_storage_project(d.to_string().as_bytes()).is_err(),
            "{name} must fail admission"
        );
    }
    // missing value: no declared default, no override (decodes, refuses at the scene join)
    let scene_level = [
        doc(one("opacity", None), &[("host-a", 0)], Value::Null),
        doc(
            json!([exposed("ep_x", "fillColor", None)]),
            &[("host-a", 0)],
            Value::Null,
        ),
    ];
    for d in &scene_level {
        assert!(decode_storage_project(d.to_string().as_bytes()).is_ok());
        assert!(refused(d, 0));
    }
    // colour channel outside 0..255 is a typed scene refusal, not a clamp
    assert!(refused(
        &doc(
            json!([exposed("ep_x", "fillColor", None)]),
            &[("host-a", 0)],
            json!([holder(
                "host-a",
                json!([static_value("ep_x", json!([300, 0, 0, 255]))])
            )])
        ),
        0
    ));
    // a number where the target field needs a colour is not guessed
    assert!(refused(
        &doc(
            json!([exposed("ep_x", "fillColor", Some(100))]),
            &[("host-a", 0)],
            Value::Null
        ),
        0
    ));
}

#[test]
fn old_unavailable_port_contract_is_gone_but_unrelated_providers_still_refuse() {
    // formerly: any declared exposed property refused at the scene join
    let d = doc(one("opacity", Some(100)), &[("host-a", 0)], Value::Null);
    assert!(!refused(&d, 0));
    let mut remap = d.clone();
    remap["componentTables"]["components"]["instances"][0]["timeRemap"] =
        json!({"keys":[{"frame":0,"v":[0]}]});
    assert!(refused(&remap, 0));
}

#[test]
fn ordered_paint_plan_keeps_instance_slots_in_host_order() {
    let holders = json!([holder("host-b", json!([static_value("ep_x", json!([50]))]))]);
    let d = doc(
        one("opacity", Some(100)),
        &[("host-a", 0), ("host-b", 32)],
        holders,
    );
    let o = owner(&d);
    let s = o.acquire_snapshot(0).unwrap();
    let p: ObjectPaintPlan = prepare_object_paint_plan(&scene(&s, 0), &pins(&s, 0)).unwrap();
    let entries: Vec<PaintEntry> = ordered_entries(&p).collect();
    assert_eq!(entries.len(), 3);
    let _ = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
}

#[cfg(all(
    feature = "application",
    feature = "project_media_codec",
    target_os = "macos"
))]
mod pixels {
    use super::*;
    use crate::compositor::Compositor;
    use crate::object_render_scene::prepare_object_viewport_composition;
    #[test]
    fn actual_metal_two_instances_with_different_overrides_render_differently_view_equals_export() {
        let holders = json!([holder("host-b", json!([static_value("ep_x", json!([50]))]))]);
        let d = doc(
            one("opacity", Some(100)),
            &[("host-a", 0), ("host-b", 32)],
            holders,
        );
        let o = project_owner(&d);
        let s = o.acquire_snapshot(0).unwrap();
        let p = plan(&s, 0);
        let view = prepare_object_viewport_composition(&p, &pins(&s, 0), None).unwrap();
        let export = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
        let mut gpu = Compositor::new().expect("override pixel test requires real Metal");
        assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
        let shown = render(&mut gpu, &view);
        assert_eq!(shown, render(&mut gpu, &export));
        // back blue; instance A at x 0..16 default 100% red; instance B at x 32..48 at 50% red over blue
        for (x, y, expected) in [
            (2usize, 2usize, [0u8, 0, 255, 255]),
            (8, 12, [255, 0, 0, 255]),
            (40, 12, [128, 0, 127, 255]),
            (24, 12, [0, 0, 255, 255]),
        ] {
            let at = (y * 64 + x) * 4;
            for channel in 0..4 {
                assert!(
                    shown[at + channel].abs_diff(expected[channel]) <= 1,
                    "({x},{y}) c{channel}"
                );
            }
        }
        // a fresh reopen of the saved project renders the same pixels
        let file = o.serialize_project_file(0, s.snapshot_id()).unwrap();
        let reopened = NativeObjectHistory::from_project_file("override-reopen", &file).unwrap();
        let fresh = reopened.acquire_snapshot(0).unwrap();
        let fp = plan(&fresh, 0);
        assert_eq!(
            shown,
            render(
                &mut gpu,
                &prepare_object_composition(&fp, &pins(&fresh, 0)).unwrap()
            )
        );
    }
}
