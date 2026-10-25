//! Same original project owner: public split, exact pending proof and actual prepared preview.
use super::support::{Fixture, Pins};
use crate::application_mcp::revision_sync::{RevisionBinding, RevisionControl};
use crate::native_application_contract::host_error;
use crate::native_object_desktop_preview::{admit_request, preview_with_target};
use serde_json::{json, Value};

fn expected_split(before: &Value) -> Value {
    let mut expected = before.clone();
    let uids = ["native-layer:1:0", "native-layer:1:1", "L0"];
    let source = before["drawing"]["layers"][0].clone();
    expected["drawing"]["layers"] = json!(uids
        .iter()
        .enumerate()
        .map(|(i, uid)| {
            let mut layer = source.clone();
            layer["layerUid"] = json!(uid);
            layer["name"] = json!(format!("Layer — Forme {}", i + 1));
            layer["visible"] = json!(true);
            layer["locked"] = json!(false);
            layer["color"] = json!("#B49BE6");
            layer.as_object_mut().unwrap().remove("groups");
            layer
        })
        .collect::<Vec<_>>());
    for record in expected["drawing"]["objects"].as_array_mut().unwrap() {
        let piece = match record["target"]["strokeId"].as_str().unwrap() {
            "a" => 0,
            "b" | "d" => 1,
            "survivor" => 2,
            _ => unreachable!(),
        };
        record["target"]["layerUid"] = json!(uids[piece]);
    }
    for (i, holder) in expected["drawing"]["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        holder["target"]["layerUid"] = json!(uids[i]);
        holder["target"].as_object_mut().unwrap().remove("strokeId");
    }
    let context = &mut expected["drawing"]["compositionOrder"]["contexts"][0];
    context["layers"] = json!(uids
        .iter()
        .enumerate()
        .map(|(i, uid)| json!({"layerUid":uid,"naturalPosition":i,"order":{}}))
        .collect::<Vec<_>>());
    context["scopes"] = json!([
        {"frameScope":{"kind":"authored","frame":0},"elements":[
            {"layerUid":uids[0],"strokeId":"a","naturalPosition":0,"order":{}},
            {"layerUid":uids[1],"strokeId":"b","naturalPosition":0,"order":{}},
            {"layerUid":uids[2],"strokeId":"survivor","naturalPosition":0,"order":{}}]},
        {"frameScope":{"kind":"authored","frame":3},"elements":[
            {"layerUid":uids[0],"strokeId":"a","naturalPosition":0,"order":{}},
            {"layerUid":uids[1],"strokeId":"d","naturalPosition":0,"order":{}}]}]);
    expected
}

fn prepared_preview(
    f: &mut Fixture,
    pins: &Pins,
    frame: u32,
    proof: Option<(&str, &Value)>,
) -> Result<(), String> {
    let mut raw = json!({"apiVersion":2,"requestId":"split-preview","instanceId":pins.instance,"documentId":pins.document,
        "contentRevision":pins.revision,"documentSnapshotId":pins.snapshot,"lifecycleGeneration":pins.generation,
        "contextId":"scene-root","scopeKind":"authored","frame":frame,
        "outputSpec":{"kind":"frame","format":"rgba8","width":64,"height":64,"colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":64,"height":64},"physicalExtent":{"width":64,"height":64},
            "compositionExtent":{"width":64,"height":64},"reportedDpr":1}});
    if let Some((subscription, event)) = proof {
        raw["revisionSync"] = json!({"subscriptionId":subscription,"event":event});
    }
    let request =
        admit_request(&raw.to_string(), f.state.instance_id()).map_err(|e| format!("{e:?}"))?;
    let receipt = preview_with_target(&f.state, &request, |consumer| {
        consumer
            .present(&mut f.gpu, &mut f.view)
            .map_err(|e| host_error("unavailable", e))
    })
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(receipt.preview.status, "presented");
    Ok(())
}

#[test]
fn gpu_same_project_owner_split_mixed_authored_frames_requires_exact_preview_ack() {
    let mut f = Fixture::split_project();
    let original = f.pins.clone();
    let before = f.saved_project();
    prepared_preview(&mut f, &original, 0, None).unwrap();
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
    let split = f.command_named("same-owner-split", "command.document.object.layer.split",
        json!({"command":"object.layer.split","contextId":"scene-root","layerUid":"L0","frame":0,"labels":{"shape":"Forme","stroke":"Trait"}}), true);
    assert!(split.response.ok);
    assert_eq!(split.response.content_revision, 1);
    let result = split.response.result.unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["historyEntriesAdded"], 1);
    let (event, mut receiver) = split.notification.unwrap();
    let event_json = serde_json::to_value(&event).unwrap();
    f.refresh_for(json!({"contextId":"scene-root","layerUid":"native-layer:1:0","strokeId":"a","frameScope":{"kind":"authored","frame":0}}));
    let pins = f.pins.clone();
    assert_eq!(
        (&pins.instance, &pins.document, pins.generation),
        (&original.instance, &original.document, original.generation)
    );
    assert_eq!(pins.revision, 1);
    assert_ne!(pins.snapshot, original.snapshot);
    let expected = expected_split(&before);
    assert_eq!(f.saved_project(), expected);
    let history = f.history();
    assert_eq!(history["undoDepth"], 1);
    assert_eq!(history["redoDepth"], 0);
    assert!(prepared_preview(&mut f, &pins, 0, None)
        .unwrap_err()
        .contains("pending"));
    f.assert_invalid_cut_proofs(&binding.subscription_id, &event_json);
    assert_eq!(f.calls.get(), 1);
    let proof = Some((binding.subscription_id.as_str(), &event_json));
    for (frame, pixel) in [
        (0, [255, 0, 0, 255]),
        (3, [255, 0, 0, 255]),
        (0, [255, 0, 0, 255]),
    ] {
        f.expected_pixel.set(pixel);
        prepared_preview(&mut f, &pins, frame, proof).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(f.saved_project(), expected);
        assert_eq!(f.history(), history);
    }
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Acknowledge {
                subscription_id: binding.subscription_id.clone(),
                event: event.clone(),
            },
        )
        .unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(f.state.revisions.settle(&f.state.native, &event, true));
    assert!(prepared_preview(&mut f, &pins, 0, proof)
        .unwrap_err()
        .contains("no pending"));
    prepared_preview(&mut f, &pins, 0, None).unwrap();
    assert_eq!(f.saved_project(), expected);
    assert_eq!(f.history(), history);
    f.release();
}
