//! L09-K: closed payloads of the native SVG and media export jobs and their unavailable registration.
use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

const SNAPSHOT: &str = "native-object:native-document-1:0";

fn spec() -> Value {
    json!({"kind":"frame","format":"rgba8","width":64,"height":36,
        "colorInterpretation":"srgb","alphaMode":"straight"})
}
fn svg() -> Value {
    json!({"documentSnapshotId":SNAPSHOT,"contextId":"scene-root",
        "outputHandle":"output/a","frames":[0,5,4294967295u32]})
}
fn media(kind: &str, format: &str, range: Value, extra: Value) -> Value {
    let mut payload = json!({"documentSnapshotId":SNAPSHOT,"contextId":"scene-root",
        "outputHandle":"output/a","output":{"kind":kind,"frameFormat":format},"range":range});
    for (key, value) in extra.as_object().unwrap() {
        payload[key] = value.clone();
    }
    payload
}
fn png_sequence() -> Value {
    media(
        "sequence",
        "png",
        json!({"start":0,"end":3}),
        json!({"outputSpec":spec()}),
    )
}
fn envelope(operation: &str, payload: Value, revision: Option<u64>) -> NativeApplicationRequest {
    let mut value = json!({"apiVersion":2,"requestId":"export","instanceId":"instance",
        "documentId":"document","operation":operation,"payload":payload});
    if let Some(revision) = revision {
        value["expectedRevision"] = json!(revision);
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn every_valid_shape_is_admitted() {
    let valid_media = [
        png_sequence(),
        media(
            "still",
            "png",
            json!({"start":4,"end":4}),
            json!({"outputSpec":spec()}),
        ),
        media("sequence", "svg", json!({}), json!({})),
        media("still", "svg", json!({"start":0,"end":0}), json!({})),
        media(
            "video",
            "png",
            json!({"end":9}),
            json!({"outputSpec":spec()}),
        ),
        media(
            "video",
            "png",
            json!({"start":0,"end":4294967295u64}),
            json!({"outputSpec":spec(),"video":{"fps":0,"quality":""}}),
        ),
        media(
            "video",
            "png",
            json!({"start":2}),
            json!({"outputSpec":spec(),"video":{"fps":29.97}}),
        ),
    ];
    assert!(request(SVG_BEGIN, &svg()));
    let mut one = svg();
    one["frames"] = json!([7]);
    assert!(request(SVG_BEGIN, &one));
    for payload in &valid_media {
        assert!(request(MEDIA_BEGIN, payload), "{payload}");
    }
}

#[test]
fn every_member_is_required_closed_and_typed() {
    for (operation, valid, required) in [
        (
            SVG_BEGIN,
            svg(),
            vec!["documentSnapshotId", "contextId", "outputHandle", "frames"],
        ),
        (
            MEDIA_BEGIN,
            png_sequence(),
            vec![
                "documentSnapshotId",
                "contextId",
                "outputHandle",
                "output",
                "range",
            ],
        ),
    ] {
        for key in required {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!request(operation, &missing), "without {key}");
            let mut null = valid.clone();
            null[key] = Value::Null;
            assert!(!request(operation, &null), "null {key}");
        }
        let mut extra = valid.clone();
        extra["extra"] = json!(1);
        assert!(!request(operation, &extra));
        let mut context = valid.clone();
        context["contextId"] = json!("component-1");
        assert!(!request(operation, &context));
        let mut handle = valid.clone();
        handle["outputHandle"] = json!("a b");
        assert!(!request(operation, &handle));
        assert!(!request(operation, &json!([])));
        assert!(!request(operation, &Value::Null));
    }
}

#[test]
fn svg_frames_are_bounded_u32_and_strictly_ascending() {
    let wide: Vec<u32> = (0..33).collect();
    for frames in [
        json!([]),
        json!(wide),
        json!([3, 2]),
        json!([1, 1]),
        json!([-1]),
        json!([1.5]),
        json!([4294967296u64]),
        json!(["1"]),
        json!("abc"),
        Value::Null,
    ] {
        let mut payload = svg();
        payload["frames"] = frames.clone();
        assert!(!request(SVG_BEGIN, &payload), "{frames}");
    }
    let thirty_two: Vec<u32> = (0..32).map(|i| 1000 + i * 3).collect();
    let mut payload = svg();
    payload["frames"] = json!(thirty_two);
    assert!(request(SVG_BEGIN, &payload));
}

#[test]
fn media_shapes_follow_the_plan_rules() {
    let png = |kind: &str, range: Value| media(kind, "png", range, json!({"outputSpec":spec()}));
    let svg_only = |kind: &str, range: Value, extra: Value| media(kind, "svg", range, extra);
    for (label, payload) in [
        (
            "video of SVG frames",
            svg_only("video", json!({}), json!({})),
        ),
        (
            "PNG without outputSpec",
            media("sequence", "png", json!({}), json!({})),
        ),
        (
            "SVG with outputSpec",
            svg_only("sequence", json!({}), json!({"outputSpec":spec()})),
        ),
        (
            "video options on a sequence",
            media(
                "sequence",
                "png",
                json!({}),
                json!({"outputSpec":spec(),"video":{}}),
            ),
        ),
        (
            "video options on SVG",
            svg_only("sequence", json!({}), json!({"video":{}})),
        ),
        (
            "still of two frames",
            png("still", json!({"start":4,"end":5})),
        ),
        ("still without a frame", png("still", json!({}))),
        ("still with one member", png("still", json!({"start":4}))),
        (
            "end before start",
            png("sequence", json!({"start":5,"end":4})),
        ),
        (
            "121 frames in one receipt",
            png("sequence", json!({"start":0,"end":120})),
        ),
        ("negative start", png("sequence", json!({"start":-1}))),
        ("fractional end", png("sequence", json!({"end":1.5}))),
        (
            "start above u32",
            png("sequence", json!({"start":4294967296u64})),
        ),
        ("unknown range member", png("sequence", json!({"first":1}))),
        ("string start", png("sequence", json!({"start":"1"}))),
        ("unknown kind", png("movie", json!({}))),
        (
            "unknown format",
            media("sequence", "gif", json!({}), json!({"outputSpec":spec()})),
        ),
    ] {
        assert!(!request(MEDIA_BEGIN, &payload), "{label}");
    }
    assert!(request(
        MEDIA_BEGIN,
        &png("sequence", json!({"start":0,"end":119}))
    ));
    assert!(!request(
        MEDIA_BEGIN,
        &png("sequence", json!({"start":0,"end":120}))
    ));
    assert!(request(
        MEDIA_BEGIN,
        &png("video", json!({"start":0,"end":4294967295u64}))
    ));
    let mut output = png_sequence();
    output["output"]["audio"] = json!(true);
    assert!(
        !request(MEDIA_BEGIN, &output),
        "exports are silent: no audio member"
    );
}

#[test]
fn output_spec_is_the_fixed_straight_srgb_frame() {
    for (key, value) in [
        ("width", json!(0)),
        ("width", json!(4097)),
        ("height", json!(0)),
        ("height", json!(4097)),
        ("width", json!(1.5)),
        ("width", json!("64")),
        ("format", json!("rgba16")),
        ("colorInterpretation", json!("display-p3")),
        ("alphaMode", json!("premultiplied")),
        ("kind", json!("preview")),
        ("depth", json!(8)),
    ] {
        let mut payload = png_sequence();
        payload["outputSpec"][key] = value.clone();
        assert!(!request(MEDIA_BEGIN, &payload), "{key}={value}");
    }
    let mut widest = png_sequence();
    widest["outputSpec"]["width"] = json!(4096);
    widest["outputSpec"]["height"] = json!(1);
    assert!(request(MEDIA_BEGIN, &widest));
}

#[test]
fn video_options_keep_the_baseline_fallbacks() {
    let with = |video: Value| {
        media(
            "video",
            "png",
            json!({}),
            json!({"outputSpec":spec(),"video":video}),
        )
    };
    for video in [
        json!({"fps":0}),
        json!({"fps":24.5,"quality":"best"}),
        json!({"quality":""}),
        json!({"quality":"x".repeat(32)}),
        json!({}),
    ] {
        assert!(request(MEDIA_BEGIN, &with(video.clone())), "{video}");
    }
    for video in [
        json!({"fps":-1}),
        json!({"fps":"24"}),
        json!({"fps":null}),
        json!({"quality":"a b"}),
        json!({"quality":"x".repeat(33)}),
        json!({"quality":3}),
        json!({"codec":"h264"}),
        Value::Null,
    ] {
        assert!(!request(MEDIA_BEGIN, &with(video.clone())), "{video}");
    }
}

#[test]
fn status_and_cancel_name_only_the_job() {
    for operation in CONTROLS {
        assert!(request(operation, &json!({"jobId":"native-export-job-1"})));
        for payload in [
            json!({}),
            json!({"jobId":"a b"}),
            json!({"jobId":7}),
            json!({"jobId":"job","extra":1}),
            json!([]),
        ] {
            assert!(!request(operation, &payload), "{operation} {payload}");
        }
    }
    assert!(owns(SVG_BEGIN) && owns(MEDIA_BEGIN));
    assert!(CONTROLS.iter().all(|operation| owns(operation)));
    assert!(!owns("job.export.png.begin") && !owns("job.export.png.status"));
}

#[test]
fn both_capabilities_are_registered_unavailable_and_still_admit_input() {
    for (operation, id) in [
        (SVG_BEGIN, "native.vector.export"),
        ("job.export.svg.status", "native.vector.export"),
        ("job.export.svg.cancel", "native.vector.export"),
        (MEDIA_BEGIN, "native.media.export"),
        ("job.export.media.status", "native.media.export"),
        ("job.export.media.cancel", "native.media.export"),
    ] {
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(operation)
            .unwrap();
        assert_eq!(descriptor["id"], id);
        assert_eq!(descriptor["availability"]["state"], "unavailable");
        assert_eq!(descriptor["authority"]["fallback"], "none");
        assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
    }
    for (operation, payload, revision) in [
        (SVG_BEGIN, svg(), Some(0)),
        (MEDIA_BEGIN, png_sequence(), Some(0)),
        ("job.export.svg.status", json!({"jobId":"job"}), None),
        ("job.export.media.cancel", json!({"jobId":"job"}), None),
    ] {
        let admitted = envelope(operation, payload, revision);
        assert!(admitted.validate_input().is_ok(), "{operation}");
        assert_eq!(admitted.validate().unwrap_err().code(), "unavailable");
    }
    let mut malformed = svg();
    malformed["frames"] = json!([2, 1]);
    let refused = envelope(SVG_BEGIN, malformed, Some(0));
    assert_eq!(
        refused.validate_input().unwrap_err().code(),
        "malformed_payload"
    );
}

#[test]
fn the_shared_dispatcher_routes_these_operations_to_this_module() {
    let result = json!({"jobId":"native-export-job-1"});
    assert!(crate::native_contract::validate_request(SVG_BEGIN, &svg()));
    assert!(crate::native_contract::validate_result(
        SVG_BEGIN,
        "document",
        &svg(),
        &result
    ));
    let mut flawed = svg();
    flawed["frames"] = json!([]);
    assert!(!crate::native_contract::validate_request(
        SVG_BEGIN, &flawed
    ));
    assert!(!crate::native_contract::validate_result(
        SVG_BEGIN, "document", &flawed, &result
    ));
    assert!(!crate::native_contract::validate_request(
        MEDIA_BEGIN,
        &svg()
    ));
    assert!(!crate::native_contract::validate_request(
        "job.export.media.status",
        &json!({})
    ));
}
