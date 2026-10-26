//! Atomic refusal controls for the tween family and its sealed revision candidate.
use super::*;
use crate::request_receipts::DispatchErrorCode;
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        o.content_revision(),
        o.history_depths(),
        o.acquire_snapshot(o.content_revision())
            .unwrap()
            .snapshot_id()
            .into(),
        bytes(o),
    )
}
#[test]
fn native_tween_refusals_preserve_revision_bytes_and_redo() {
    let c = fixture_cases()
        .into_iter()
        .find(|c| c["input"]["name"] == "move")
        .unwrap();
    let mut o =
        NativeObjectHistory::new("refusals", document(&c["input"]["frames"], true)).unwrap();
    let payload = json!({"command":"object.tween.generate","layerUid":"L0"});
    let q = request(&o, "seed", "tween.generate", payload.clone());
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    let q = request(&o, "undo", "undo", json!({"command":"object.undo"}));
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    assert_eq!(o.history_depths(), (0, 1));
    let before = state(&o);
    for (i,bad)in [json!({"command":"object.tween.generate","layerUid":"missing"}),
        json!({"command":"object.tween.split","layerUid":"L0"}),
        json!({"command":"object.tween.generate","layerUid":"L0","unexpected":true}),
        json!({"command":"object.tween.generate","layerUid":"L0","starts":[999]}),
        json!({"command":"object.tween.generate","layerUid":"L0","starts":[0],"selection":[0]}),
        json!({"command":"object.tween.generate","layerUid":"L0","options":{"resamplePoints":1,"step":1,"skipManual":true,"easing":[]}})].into_iter().enumerate(){
        let q=request(&o,&format!("invalid-{i}"),"tween.generate",bad);assert_eq!(dispatch(&mut o,&q)["ok"],false);assert_eq!(state(&o),before);
    }
    for i in 0..5 {
        let mut q = request(
            &o,
            &format!("envelope-{i}"),
            "tween.generate",
            payload.clone(),
        );
        match i {
            0 => q.instance_id = "wrong".into(),
            1 => q.document_id = "wrong".into(),
            2 => q.expected_revision = Some(999),
            3 => q.cancelled_before_dispatch = true,
            _ => q.api_version = 1,
        }
        assert_eq!(dispatch(&mut o, &q)["ok"], false);
        assert_eq!(state(&o), before);
    }
    assert!(o
        .owner
        .prepare_tween_json(
            br#"{"command":"object.tween.generate","layerUid":"L0","layerUid":"L0"}"#
        )
        .is_err());
    assert_eq!(state(&o), before);
    let candidate = o
        .owner
        .prepare_tween_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    let q = request(&o, "fresh", "tween.generate", payload);
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    let after = state(&o);
    assert_eq!(
        o.owner.commit_tween(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), after);
}
#[test]
fn native_tween_store_and_texture_unknown_fields_are_refused_on_reopen() {
    let c = fixture_cases()
        .into_iter()
        .find(|c| c["input"]["name"] == "texture-both")
        .unwrap();
    let mut o = NativeObjectHistory::new("strict", document(&c["input"]["frames"], true)).unwrap();
    let q = request(
        &o,
        "generate",
        "tween.generate",
        json!({"command":"object.tween.generate","layerUid":"L0"}),
    );
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    let saved = bytes(&o);
    let wire: Value = serde_json::from_slice(&saved).unwrap();
    let mut bad = wire.clone();
    bad["tweens"]["layers"][0]["options"]["step"] = json!(0);
    assert!(decode_storage_project(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = wire.clone();
    bad["tweens"]["unexpected"] = json!(true);
    assert!(decode_storage_project(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = wire;
    bad["objects"][0]["vectorExt"]["texture"]["unexpected"] = json!(true);
    assert!(decode_storage_project(&serde_json::to_vec(&bad).unwrap()).is_err());
    assert_eq!(bytes(&o), saved);
}

#[test]
fn native_tween_d13_retime_preserves_options_and_rekeys_only_original_motion_arcs() {
    let c = fixture_cases()
        .into_iter()
        .find(|c| c["input"]["name"] == "force-arc")
        .unwrap();
    let mut doc = document(&c["input"]["frames"], true);
    doc.motion_arcs = crate::object_document::Presence::Present(
        serde_json::from_value(c["input"]["motionArcs"].clone()).unwrap(),
    );
    let mut owner = NativeObjectHistory::new("tween-retime", doc).unwrap();
    let q = request(
        &owner,
        "generate",
        "tween.reassign",
        json!({"command":"object.tween.reassign","layerUid":"L0","options":c["input"]["options"],"span":c["input"]["span"]}),
    );
    assert_eq!(dispatch(&mut owner, &q)["ok"], true);
    let before = bytes(&owner);
    let settings = head(&owner).tweens.clone();
    let q = request(
        &owner,
        "move",
        "layer.key.move",
        json!({"command":"object.layer.key.move","layerUid":"L0","activeLayerUid":"L0","from":5,"to":4}),
    );
    assert_eq!(dispatch(&mut owner, &q)["ok"], true);
    assert_eq!(head(&owner).tweens, settings);
    let value = serde_json::to_value(head(&owner).motion_arcs.value().unwrap()).unwrap();
    assert!(value.get("L0:0-5--1").is_none());
    assert_eq!(value["L0:0-4--1"], c["input"]["motionArcs"]["L0:0-5--1"]);
    let saved = bytes(&owner);
    assert_eq!(decode_storage_project(&saved).unwrap().tweens, settings);
    let q = request(
        &owner,
        "undo-move",
        "undo",
        json!({"command":"object.undo"}),
    );
    assert_eq!(dispatch(&mut owner, &q)["ok"], true);
    assert_eq!(bytes(&owner), before);
}

#[test]
fn native_tween_pressure_output_capacity_is_unavailable_without_mutation() {
    let c = fixture_cases()
        .into_iter()
        .find(|c| c["input"]["name"] == "pressure-pair")
        .unwrap();
    let mut doc = document(&c["input"]["frames"], true);
    for r in &mut doc.objects {
        if let crate::object_document::Presence::Present(ext) = &mut r.vector_ext {
            if let crate::object_document::Presence::Present(pressure) = &mut ext.pressure {
                if let crate::object_document::Presence::Present(center) =
                    &mut pressure.center_segments
                {
                    for p in center {
                        p.point.x = serde_json::Number::from_f64(p.point.x.as_f64().unwrap() * 20.)
                            .unwrap();
                    }
                }
            }
        }
    }
    let mut owner = NativeObjectHistory::new("pressure-capacity", doc).unwrap();
    let before = bytes(&owner);
    let q = request(
        &owner,
        "oversize",
        "tween.generate",
        json!({"command":"object.tween.generate","layerUid":"L0"}),
    );
    let reply = dispatch(&mut owner, &q);
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "unavailable", "{reply}");
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    assert_eq!(bytes(&owner), before);
}
