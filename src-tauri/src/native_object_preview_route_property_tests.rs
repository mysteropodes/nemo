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

fn project_of(drawing: Value) -> Fixture {
    Fixture::from_project(
        json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"drawing":drawing,"viewportReference":null,"assets":[]}),
    )
}

/// One vector layer whose only record carries a real black stroke: the default look of a drawn line.
fn stroked_project() -> Fixture {
    let mut drawing = document();
    drawing["objects"][0]["family"] = json!("editable-vector-path");
    drawing["objects"][0]["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,
        "strokeWidth":3,"strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1});
    project_of(drawing)
}

fn set_key(layer: &str, property: &str, frame: u32, values: Value) -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":layer},
        "property":property,"action":{"kind":"setKey","frame":frame,"values":values}})
}

// Rotation, Scale and Anchor of a layer that paints are applied by the common vector batch (the whole-layer pivot is the
// union of its cubic bounds); the pixels checked here stay inside the square for each of these, so a refusal is the only
// way these can fail.
#[test]
fn gpu_rotation_scale_anchor_edits_on_a_vector_layer_present_under_their_exact_proof() {
    for (property, values) in [
        ("rotation", json!([30])),
        ("scale", json!([150, 150])),
        ("anchor", json!([3, 4])),
    ] {
        acknowledged_preview(
            Fixture::cut_project(),
            BLUE,
            PROPERTY,
            set("L0", property, values),
        );
    }
}

#[test]
fn gpu_scale_edits_on_a_stroked_vector_layer_present_with_the_baseline_stroke_width() {
    for values in [json!([150, 150]), json!([100, 150])] {
        acknowledged_preview(stroked_project(), BLUE, PROPERTY, set("L0", "scale", values));
    }
}

#[test]
fn gpu_a_keyed_rotation_edit_on_a_vector_layer_presents_under_its_exact_proof() {
    let f = Fixture::cut_project();
    let seeded = f.command_named("seed-key", PROPERTY, set_key("L0", "rotation", 3, json!([60])), false);
    assert!(seeded.response.ok, "{:?}", seeded.response.error);
    let mut f = f;
    f.refresh();
    acknowledged_preview(f, BLUE, PROPERTY, set_key("L0", "rotation", 0, json!([30])));
}

/// One layer that paints a vector square AND a native Text block: the layer a drawing app produces after the first label.
fn text_and_vector_project() -> Fixture {
    let mut drawing = document();
    drawing["items"] = json!({"text":[{
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"label"},
        "block":{"text":"I","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":12,"color":"#112233",
            "align":"left","anchorTopLeft":{"x":46,"y":46}}}]});
    drawing["totalFrames"] = json!(4);
    drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}},
        {"layerUid":"L0","strokeId":"label","naturalPosition":1,"order":{}}]);
    project_of(drawing)
}

#[test]
fn gpu_a_keyed_position_edit_on_a_layer_of_vectors_and_text_presents_under_its_exact_proof() {
    // The pivot of a mixed layer is the union of its vector and its Text, so the probes stay inside the square for these.
    for property in ["position", "rotation", "scale", "anchor"] {
        let f = text_and_vector_project();
        let (key, value, edit) = match property {
            "position" => ("position", json!([3, 3]), json!([1, 1])),
            "rotation" => ("rotation", json!([8]), json!([5])),
            "scale" => ("scale", json!([120, 120]), json!([150, 150])),
            _ => ("anchor", json!([9, 9]), json!([3, 4])),
        };
        let seeded = f.command_named("seed-key", PROPERTY, set_key("L0", key, 3, value), false);
        assert!(seeded.response.ok, "{property}: {:?}", seeded.response.error);
        let mut f = f;
        f.refresh();
        acknowledged_preview(f, BLUE, PROPERTY, set_key("L0", key, 0, edit));
    }
}

/// The image is a 24 x 24 raster of a two-pixel PNG, clear of both probes. On its own layer L1 it is an image layer;
/// on L0 it shares the layer of the vector square.
fn image_project(layer: &str) -> Fixture {
    let mut drawing = document();
    drawing["totalFrames"] = json!(4);
    let timing = drawing["layers"][0]["timing"].clone();
    if layer == "L1" {
        drawing["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"L1","timing":timing}));
        drawing["compositionOrder"]["contexts"][0]["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"L1","naturalPosition":1,"order":{}}));
    }
    drawing["items"] = json!({"raster":[{
        "target":{"contextId":"scene-root","layerUid":layer,"strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
        "item":{"assetId":"ink","x":36,"y":4,"width":24,"height":24,"opacity":1}}]});
    drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":layer,"strokeId":"ink","naturalPosition":usize::from(layer == "L0"),"order":{}}));
    Fixture::from_project(json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,
        "assets":[{"id":"ink","name":"Authored ink","kind":"image","source":{"kind":"embedded","mediaType":"image/png",
            "encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}}],
        "assetInventory":{"assetIds":["ink"],"placements":[]},"drawing":drawing}))
}

#[test]
fn gpu_a_keyed_position_edit_on_an_image_layer_presents_under_its_exact_proof() {
    for (layer, property, seed, edit) in [
        ("L1", "position", json!([3, 3]), json!([1, 1])),
        ("L1", "rotation", json!([8]), json!([5])),
        ("L1", "scale", json!([120, 120]), json!([150, 150])),
        // The image shares its layer with the vector square: both move with the union pivot.
        ("L0", "position", json!([3, 3]), json!([1, 1])),
        ("L0", "rotation", json!([8]), json!([5])),
        ("L0", "scale", json!([120, 120]), json!([150, 150])),
    ] {
        let mut f = image_project(layer);
        let seeded = f.command_named("seed-key", PROPERTY, set_key(layer, property, 3, seed), false);
        assert!(seeded.response.ok, "{property}: {:?}", seeded.response.error);
        f.refresh();
        acknowledged_preview(f, BLUE, PROPERTY, set_key(layer, property, 0, edit));
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
