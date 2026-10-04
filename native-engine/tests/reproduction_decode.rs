//! Raw-byte adversarial inputs must fail before any native authority is made.
use super::tests::BUNDLE;
use super::*;
use serde_json::Value;

fn reject(bytes: &[u8], expected: ReproductionReplayError) {
    assert_eq!(
        replay_with_factory(bytes, || panic!("invalid bytes constructed an authority")),
        Err(expected)
    );
    assert_eq!(replay_reproduction_bundle(bytes), Err(expected));
}

fn mutation(path: &str, value: Value) -> Vec<u8> {
    let mut bundle: Value = serde_json::from_str(BUNDLE).unwrap();
    *bundle.pointer_mut(path).unwrap() = value;
    serde_json::to_vec(&bundle).unwrap()
}

#[test]
fn required_objects_reject_unknown_missing_duplicate_and_array_shaped_fields() {
    let baseline: Value = serde_json::from_str(BUNDLE).unwrap();
    for path in [
        "",
        "/fixture",
        "/stableTarget",
        "/versions",
        "/commands/0",
        "/commands/2",
    ] {
        let object = baseline.pointer(path).unwrap().as_object().unwrap();
        for key in object.keys() {
            let mut missing = baseline.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            reject(
                &serde_json::to_vec(&missing).unwrap(),
                ReproductionReplayError::InvalidBundle,
            );
            // Duplicate raw spelling and an escaped equivalent both target the
            // typed decoder directly, before any Value could erase duplicates.
            let encoded = serde_json::to_string(object).unwrap();
            let value = serde_json::to_string(&object[key]).unwrap();
            let original = serde_json::to_string(&baseline).unwrap();
            for duplicate_key in [
                format!("\"{key}\""),
                format!("\"\\u{:04x}{}\"", key.as_bytes()[0], &key[1..]),
            ] {
                let duplicated = format!("{{{duplicate_key}:{value},{}", &encoded[1..]);
                let raw = original.replacen(&encoded, &duplicated, 1);
                assert_ne!(raw, original);
                reject(raw.as_bytes(), ReproductionReplayError::InvalidBundle);
            }
        }
        let mut unknown = baseline.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("private/path".into(), json!("secret"));
        reject(
            &serde_json::to_vec(&unknown).unwrap(),
            ReproductionReplayError::InvalidBundle,
        );
        let array = object.values().cloned().collect::<Vec<_>>();
        for value in [
            json!(array),
            json!([]),
            json!(null),
            json!("private/path"),
            json!(true),
            json!(1),
        ] {
            reject(
                &mutation(path, value),
                ReproductionReplayError::InvalidBundle,
            );
        }
    }
    // Exact declaration-order array forms could otherwise deserialize as structs.
    for (path, array) in [
        (
            "/fixture",
            json!([
                "native-opacity-static",
                1,
                "895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050"
            ]),
        ),
        ("/stableTarget", json!(["r08_curve_layer"])),
        ("/versions", json!(["0.1.0"])),
        ("/commands/0", json!([1, 0, 40, 1, true])),
    ] {
        reject(
            &mutation(path, array),
            ReproductionReplayError::InvalidBundle,
        );
    }
}

#[test]
fn wrong_typed_fields_and_non_null_clock_seed_are_rejected() {
    for path in [
        "/format",
        "/fixture/id",
        "/fixture/sha256",
        "/command",
        "/stableTarget/layerUid",
        "/versions/nativeEngine",
    ] {
        for value in [json!(null), json!(true), json!(1), json!([]), json!({})] {
            reject(
                &mutation(path, value),
                ReproductionReplayError::InvalidBundle,
            );
        }
    }
    for path in [
        "/formatVersion",
        "/apiVersion",
        "/fixture/version",
        "/commands/0/id",
        "/commands/0/expectedRevision",
        "/commands/0/revision",
    ] {
        for value in [
            json!(null),
            json!("1"),
            json!(true),
            json!(1.0),
            json!(-1),
            json!([]),
            json!({}),
        ] {
            reject(
                &mutation(path, value),
                ReproductionReplayError::InvalidBundle,
            );
        }
    }
    for path in ["/clock", "/seed"] {
        for value in [json!(0), json!(false), json!(""), json!([]), json!({})] {
            reject(
                &mutation(path, value),
                ReproductionReplayError::InvalidBundle,
            );
        }
    }
    for value in [
        json!(null),
        json!("40"),
        json!(true),
        json!([]),
        json!({}),
        json!(-0.01),
        json!(100.01),
    ] {
        reject(
            &mutation("/commands/0/value", value),
            ReproductionReplayError::InvalidBundle,
        );
    }
    for value in [
        json!(null),
        json!("true"),
        json!(0),
        json!(1),
        json!([]),
        json!({}),
    ] {
        reject(
            &mutation("/commands/0/applied", value),
            ReproductionReplayError::InvalidBundle,
        );
    }
    for value in [json!(null), json!({}), json!(true), json!("commands")] {
        reject(
            &mutation("/commands", value),
            ReproductionReplayError::InvalidBundle,
        );
    }
}

#[test]
fn selectors_versions_bounds_and_sequence_dispositions_are_checked_before_creation() {
    for (path, value) in [
        ("/format", json!("private/format")),
        ("/formatVersion", json!(2)),
        ("/apiVersion", json!(1)),
        ("/fixture/id", json!("private-fixture")),
        ("/fixture/version", json!(2)),
        ("/fixture/sha256", json!("0".repeat(64))),
        ("/command", json!("job.export.png.begin")),
        ("/stableTarget/layerUid", json!("private-layer")),
        ("/versions/nativeEngine", json!("9.9.9")),
    ] {
        reject(
            &mutation(path, value),
            ReproductionReplayError::IncompatibleBundle,
        );
    }
    for (path, value) in [
        ("/commands", json!([])),
        ("/commands/0/id", json!(0)),
        ("/commands/1/id", json!(1)),
        ("/commands/2/id", json!(4)),
        ("/commands/0/expectedRevision", json!(1)),
        ("/commands/1/expectedRevision", json!(0)),
        ("/commands/2/expectedRevision", json!(u64::MAX)),
        ("/commands/0/revision", json!(0)),
        ("/commands/1/revision", json!(u64::MAX)),
        ("/commands/2/revision", json!(3)),
        ("/commands/0/applied", json!(false)),
        ("/commands/2/applied", json!(true)),
        ("/commands/0/value", json!(25)),
        ("/commands/1/value", json!(40)),
        ("/commands/2/value", json!(61)),
    ] {
        reject(
            &mutation(path, value),
            ReproductionReplayError::InvalidSequence,
        );
    }
    let mut too_many: Value = serde_json::from_str(BUNDLE).unwrap();
    too_many["commands"] = json!((1..=33)
        .map(|id| json!({"id":id,"expectedRevision":0,"value":25,"revision":0,"applied":false}))
        .collect::<Vec<_>>());
    let bytes = serde_json::to_vec(&too_many).unwrap();
    assert!(bytes.len() <= 3072);
    reject(&bytes, ReproductionReplayError::InvalidSequence);
}

#[test]
fn all_malformed_raw_json_tail_and_size_failures_are_fixed_privacy_safe_errors() {
    for end in 0..BUNDLE.len() {
        reject(
            &BUNDLE.as_bytes()[..end],
            ReproductionReplayError::InvalidBundle,
        );
    }
    for raw in [
        "",
        "null",
        "[]",
        "true",
        "{",
        "{\"private/path\":\"secret\"}",
        "{\"format\":NaN}",
    ] {
        reject(raw.as_bytes(), ReproductionReplayError::InvalidBundle);
    }
    for raw in [
        format!("{BUNDLE} {{}}"),
        format!("{BUNDLE} trailing-private-path"),
        BUNDLE.replace("\"value\":40", "\"value\":1e999"),
        BUNDLE.replace("\"value\":40", "\"value\":NaN"),
        BUNDLE.replace("\"id\":1", "\"id\":256"),
        BUNDLE.replace(
            "\"expectedRevision\":0",
            "\"expectedRevision\":18446744073709551616",
        ),
    ] {
        reject(raw.as_bytes(), ReproductionReplayError::InvalidBundle);
    }
    let mut invalid_utf8 = BUNDLE.as_bytes().to_vec();
    invalid_utf8.push(255);
    reject(&invalid_utf8, ReproductionReplayError::InvalidBundle);
    // Whitespace counts against the raw-byte bound, independently of decoded size.
    let mut exact = BUNDLE.as_bytes().to_vec();
    exact.resize(3072, b' ');
    assert!(replay_reproduction_bundle(&exact).is_ok());
    exact.push(b' ');
    reject(&exact, ReproductionReplayError::ByteLimit);
    for error in [
        ReproductionReplayError::ByteLimit,
        ReproductionReplayError::InvalidBundle,
        ReproductionReplayError::IncompatibleBundle,
        ReproductionReplayError::InvalidSequence,
        ReproductionReplayError::CatalogUnavailable,
        ReproductionReplayError::ReplayMismatch,
    ] {
        let encoded = serde_json::to_string(&error).unwrap();
        assert!(!encoded.contains("private") && !encoded.contains("secret") && encoded.len() < 30);
    }
}
