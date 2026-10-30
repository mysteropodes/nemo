//! Property and widget revisions: the exact-proof preview that gates the subscriber ACK must present.
use super::super::support::{document, Fixture, Pins};
use crate::application_mcp::revision_sync::{RevisionBinding, RevisionControl};
use crate::native_application_contract::host_error;
use crate::native_object_desktop_preview::{admit_request, preview_with_target};
use serde_json::{json, Value};

fn preview(f: &mut Fixture, pins: &Pins, proof: Option<(&str, &Value)>) -> Result<(), String> {
    let mut raw = json!({"apiVersion":2,"requestId":"property-preview","instanceId":pins.instance,"documentId":pins.document,
        "contentRevision":pins.revision,"documentSnapshotId":pins.snapshot,"lifecycleGeneration":pins.generation,
        "contextId":"scene-root","scopeKind":"authored","frame":0,
        "outputSpec":{"kind":"frame","format":"rgba8","width":64,"height":64,"colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":64,"height":64},"physicalExtent":{"width":64,"height":64},
            "compositionExtent":{"width":64,"height":64},"reportedDpr":1}});
    if let Some((subscription, event)) = proof {
        raw["revisionSync"] = json!({"subscriptionId":subscription,"event":event});
    }
    let request =
        admit_request(&raw.to_string(), f.state.instance_id()).map_err(|e| format!("{e:?}"))?;
    preview_with_target(&f.state, &request, |consumer| {
        consumer
            .present(&mut f.gpu, &mut f.view)
            .map_err(|e| host_error("unavailable", e))
    })
    .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn proof_preview(
    mut f: Fixture,
    pixel: [u8; 4],
    command: &str,
    payload: Value,
) -> Result<(), String> {
    f.expected_pixel.set(pixel);
    let original = f.pins.clone();
    preview(&mut f, &original, None).unwrap();
    let binding: RevisionBinding = serde_json::from_value(
        f.state
            .revisions
            .control(&f.state.native, RevisionControl::Binding)
            .unwrap(),
    )
    .unwrap();
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Subscribe {
                binding: binding.clone(),
            },
        )
        .unwrap();
    let delivery = f.command_named("property-preview-edit", command, payload, true);
    assert!(delivery.response.ok, "{:?}", delivery.response.error);
    let (event, mut receiver) = delivery.notification.unwrap();
    let event_json = serde_json::to_value(&event).unwrap();
    f.refresh();
    let pins = f.pins.clone();
    preview(
        &mut f,
        &pins,
        Some((binding.subscription_id.as_str(), &event_json)),
    )?;
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    Ok(())
}
fn acknowledged_preview(f: Fixture, pixel: [u8; 4], command: &str, payload: Value) {
    proof_preview(f, pixel, command, payload).unwrap();
}

/// L1 paints nothing and no layer reads it (a fresh layer before its first stroke); L0 paints the stroke.
/// With `child`, layer L2 is parented to L1 so L1's pose reaches the frame through its child.
fn empty_layer_project(child: bool) -> Fixture {
    let mut drawing = document();
    let empty = json!({"layerUid":"L1","timing":drawing["layers"][0]["timing"].clone()});
    let mut layers = vec![json!({"layerUid":"L1","naturalPosition":1,"order":{}})];
    drawing["layers"]
        .as_array_mut()
        .unwrap()
        .push(empty.clone());
    if child {
        let mut layer = empty;
        layer["layerUid"] = json!("L2");
        layer["parentLayerUid"] = json!("L1");
        drawing["layers"].as_array_mut().unwrap().push(layer);
        layers.push(json!({"layerUid":"L2","naturalPosition":2,"order":{}}));
    }
    let context = drawing["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap();
    context.extend(layers);
    Fixture::from_project(
        json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"drawing":drawing,"viewportReference":null,"assets":[]}),
    )
}

const PROPERTY: &str = "command.document.object.property.edit";
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn set(layer: &str, property: &str, values: Value) -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
        "property":property,"action":{"kind":"setValue","frame":0,"values":values}})
}

#[test]
fn gpu_position_edit_on_a_vector_layer_presents_under_its_exact_proof() {
    acknowledged_preview(
        Fixture::cut_project(),
        BLUE,
        PROPERTY,
        set("L0", "position", json!([1, 1])),
    );
}

#[test]
fn gpu_transform_edits_on_an_empty_layer_present_under_their_exact_proof() {
    for (property, values) in [
        ("position", json!([10, 20])),
        ("anchor", json!([3, 4])),
        ("rotation", json!([30])),
        ("scale", json!([50, 50])),
    ] {
        acknowledged_preview(
            empty_layer_project(false),
            BLUE,
            PROPERTY,
            set("L1", property, values),
        );
    }
}

#[test]
fn gpu_widget_create_revision_presents_under_its_exact_proof() {
    acknowledged_preview(
        Fixture::cut_project(),
        BLUE,
        "command.document.object.widget.create",
        json!({"command":"object.widget.create","kind":"joystick","name":"Joystick 1","pos":[10,10]}),
    );
}

#[test]
fn gpu_a_parent_layer_pose_stays_refused_until_a_consumer_applies_it_to_the_child() {
    let refusal = proof_preview(
        empty_layer_project(true),
        BLUE,
        PROPERTY,
        set("L1", "position", json!([10, 20])),
    )
    .unwrap_err();
    assert!(refusal.contains("Unavailable"), "{refusal}");
}
