//! #2256 Q5: the `instance` selector of the component read. The panel shows the stored playback record with the baseline
//! display defaults (`updateCompInstancePanel`, 1065e9d8: `symPlayMode||'loop'`, `symSpeed||1`, `symPlacedAt||0`), the
//! resolved component frame the playback kernel answers at the asked outer frame (`resolveSymbolFrameIdx`), and the
//! component's length for the frame strip. Expected frames are the baseline arithmetic worked by hand.
use super::fixture::{decode, document, layer};
use super::*;
use serde_json::{json, Value};

fn snapshot(instance: Value) -> ObjectSnapshot {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0},{"frame":6}],"samples":[]});
    let layers = [
        layer("ly_inst", "Inst", &[]).with(json!({"timing":timing})),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let mut record = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x"});
    for (key, member) in instance.as_object().expect("object") {
        record[key] = member.clone();
    }
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"sym_x","totalFrames":24,"layers":[{"layerUid":"xi"}],"objects":[]}],
        "instances":[record]});
    let document = decode(&document(
        &layers,
        json!({"componentTables":{"components":components}}),
    ));
    ObjectSnapshot::new("inst-read", document).expect("snapshot")
}
fn selector(layer_uid: &str, frame: u64) -> Value {
    json!({"kind":"instance","layerUid":layer_uid,"frame":frame})
}
fn ask_with(snapshot: &ObjectSnapshot, selector: Value) -> ResponseEnvelope {
    let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"inst-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object.component.read",
        "payload":{"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
            "selector":selector}}))
    .unwrap();
    snapshot.dispatch(request).unwrap()
}
fn ask(snapshot: &ObjectSnapshot, frame: u64) -> Value {
    let response = ask_with(snapshot, selector("ly_inst", frame));
    assert!(response.ok, "{:?}", response.error);
    response.result.unwrap()
}

#[test]
fn a_stored_record_is_shown_with_the_component_length_and_head() {
    let snap = snapshot(
        json!({"symPlayMode":"pingpong","symSpeed":2.5,"symPlacedAt":-4,"symSingleFrame":9}),
    );
    let result = ask(&snap, 10);
    assert_eq!(result["kind"], "instance");
    assert_eq!(result["atRevision"], snap.content_revision());
    assert_eq!(result["documentSnapshotId"], snap.snapshot_id());
    let expected = json!({"layerUid":"ly_inst","componentId":"sym_x","mode":"pingpong","singleFrame":9,
        "speed":2.5,"placedAt":-4,"totalFrames":24,"explicit":false});
    for (key, want) in expected.as_object().unwrap() {
        assert_eq!(&result[key], want, "{key}");
    }
}

#[test]
fn an_unauthored_record_shows_the_baseline_display_defaults() {
    let result = ask(&snapshot(json!({})), 0);
    let expected = json!({"mode":"loop","singleFrame":0,"speed":1,"placedAt":0});
    for (key, want) in expected.as_object().unwrap() {
        assert_eq!(&result[key], want, "{key}");
    }
    // `ld.symSpeed||1` and `ld.symPlacedAt||0`: a stored zero shows the fallback, as the baseline field does.
    let zero = ask(&snapshot(json!({"symSpeed":0,"symPlayMode":null})), 0);
    assert_eq!((&zero["speed"], &zero["mode"]), (&json!(1), &json!("loop")));
}

/// Placed at 4 and speed 2 over a 24-frame component: `elapsed = max(0, f - 4) * 2`, then the baseline mode arithmetic.
#[test]
fn the_resolved_frame_follows_the_baseline_arithmetic_of_every_mode() {
    let base = |mode: &str| {
        snapshot(
            json!({"symPlayMode":mode,"symSpeed":2,"symPlacedAt":4,"symSingleFrame":5,
            "symTrimIn":0,"symTrimOut":23}),
        )
    };
    // (outer frame, loop, once, pingpong, single)
    let rows = [
        (2_u64, 0, 0, 0, 5),
        (10, 12, 12, 12, 5),
        (20, 8, 23, 14, 5),
        (4, 0, 0, 0, 5),
    ];
    for (frame, looped, once, pingpong, single) in rows {
        for (mode, want) in [
            ("loop", looped),
            ("once", once),
            ("pingpong", pingpong),
            ("single", single),
        ] {
            assert_eq!(
                ask(&base(mode), frame)["resolvedFrame"],
                json!(want),
                "{mode} at {frame}"
            );
        }
    }
    // A single frame past the end is clamped to the last frame of the component.
    let past = snapshot(json!({"symPlayMode":"single","symSingleFrame":40}));
    assert_eq!(ask(&past, 3)["resolvedFrame"], json!(23));
}

#[test]
fn an_outer_key_that_picks_a_frame_wins_and_is_reported_explicit_and_a_held_blank_resolves_nothing()
{
    let picked = snapshot(json!({"symPlayMode":"loop","symSpeed":1,
        "frameKeys":[{"frame":6,"componentFrame":17}]}));
    let at_key = ask(&picked, 6);
    assert_eq!(
        (&at_key["resolvedFrame"], &at_key["explicit"]),
        (&json!(17), &json!(true))
    );
    let held = ask(&picked, 9);
    assert_eq!(
        (&held["resolvedFrame"], &held["explicit"]),
        (&json!(17), &json!(false))
    );
    let before = ask(&picked, 3);
    assert_eq!(
        before["resolvedFrame"],
        json!(3),
        "before the key the play mode decides"
    );
    let blank = snapshot(json!({"frameKeys":[{"frame":6,"blankOverride":true}]}));
    assert_eq!(ask(&blank, 8)["resolvedFrame"], Value::Null);
}

#[test]
fn a_layer_that_is_not_an_instance_or_an_unknown_layer_is_not_found_and_a_bad_selector_is_refused()
{
    let snap = snapshot(json!({}));
    for uid in ["ly_plain", "xi", "nope"] {
        let response = ask_with(&snap, selector(uid, 0));
        assert!(!response.ok && response.result.is_none(), "{uid}");
        assert_eq!(
            response.error.unwrap().code(),
            crate::request_receipts::DispatchErrorCode::NotFound,
            "{uid}"
        );
    }
    for bad in [
        json!({"kind":"instance","layerUid":"ly_inst"}),
        json!({"kind":"instance","frame":0}),
        json!({"kind":"instance","layerUid":"","frame":0}),
        json!({"kind":"instance","layerUid":"ly_inst","frame":-1}),
        json!({"kind":"instance","layerUid":"ly_inst","frame":1.5}),
        json!({"kind":"instance","layerUid":"ly_inst","frame":0,"cursor":0}),
    ] {
        let response = ask_with(&snap, bad.clone());
        assert_eq!(
            response.error.unwrap().code(),
            crate::request_receipts::DispatchErrorCode::InvalidRequest,
            "{bad}"
        );
    }
}

#[test]
fn the_answer_fits_the_response_bound_with_the_longest_identities() {
    let long = "u".repeat(128);
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let layers = [layer("ly_long", "Inst", &[]).with(json!({"timing":timing}))];
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":long,"totalFrames":24,"layers":[{"layerUid":"xi"}],"objects":[]}],
        "instances":[{"contextId":"scene-root","layerUid":"ly_long","symbolId":long,"symPlayMode":"pingpong",
            "symSpeed":0.123456789012345,"symPlacedAt":-2147483648,"symSingleFrame":4294967295_u64}]});
    let doc = decode(&document(
        &layers,
        json!({"componentTables":{"components":components}}),
    ));
    let snap = ObjectSnapshot::new("inst-read", doc).expect("snapshot");
    let response = ask_with(&snap, selector("ly_long", 9_007_199_254_740_991));
    assert!(response.ok, "{:?}", response.error);
    assert!(serde_json::to_vec(&response).unwrap().len() <= MAX_MESSAGE_BYTES);
}
