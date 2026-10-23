//! Oracle tests of the duplicator setup plans (L04-M, I10). Every expected value is
//! a baseline row executed from the verbatim `toggleLayerDuplicator`,
//! `convertSelectionToObjectDuplicator` and `_resolveDuplicatorPath`.
use super::fixture::{
    decode, document, duplicator_oracle, duplicator_table, layer, uids, wire, DUPLICATOR_ORACLE,
};
use super::*;
use serde_json::{json, Value};

fn pool_document() -> Value {
    let layers = [
        layer("ly_a", "A", &[(0, "a1")]),
        layer("ly_b", "B", &[(0, "b1")]),
        layer("ly_c", "C", &[(0, "c1")]),
        layer("ly_d", "D", &[(0, "d1")]),
        layer("ly_old", "Duplicator", &[(0, "o1")]),
    ];
    let tables = json!({"duplicators":duplicator_table(&[("ly_c", json!({"mode":"grid"}))])});
    document(&layers, json!({"componentTables":tables}))
}

fn path_document() -> Value {
    let layers = [
        layer("ly_dup", "Host", &[(0, "h1")]),
        layer("ly_other_dup", "Other", &[(0, "x1")]),
        layer("ly_plain", "Plain", &[(0, "p1")]),
    ];
    let hosts = [
        ("ly_dup", json!({"mode":"path"})),
        ("ly_other_dup", json!({"mode":"grid"})),
    ];
    document(
        &layers,
        json!({"componentTables":{"duplicators":duplicator_table(&hosts)}}),
    )
}

fn hosts(after: &Value) -> Vec<Value> {
    after["componentTables"]["duplicators"]["duplicators"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn record_of<'a>(list: &'a [Value], uid: &str) -> &'a Value {
    list.iter()
        .find(|r| r["layerUid"] == uid)
        .expect("duplicator record")
}

fn keys_of(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[test]
fn the_oracle_is_pinned_to_the_baseline_commit_and_its_row_count() {
    assert!(DUPLICATOR_ORACLE.contains("fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert_eq!(duplicator_oracle().rows(), 24);
}

#[test]
fn enabling_writes_the_baseline_default_descriptor_and_locks_the_layer() {
    let o = duplicator_oracle();
    let doc = decode(&document(
        &[layer("ly_t", "Toggle", &[(0, "t1")])],
        json!({}),
    ));
    let plan = plan_duplicator_set(&doc, "ly_t", true, Some(123_456)).unwrap();
    assert!(plan.changed);
    let after = wire(&plan.document);
    let list = hosts(&after);
    let written = &record_of(&list, "ly_t")["duplicator"];
    assert_eq!(written, o.get("D1.descriptor"));
    let mut expected_keys: Vec<String> = o
        .get("D1.keys")
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    expected_keys.sort();
    assert_eq!(
        keys_of(written),
        expected_keys,
        "no member beyond the baseline literal"
    );
    assert_eq!(&after["layers"][0]["locked"], o.get("D1.locked"));
    assert_eq!(decode(&after), plan.document, "storage round trip");
}

#[test]
fn disabling_removes_the_descriptor_and_unlocks_the_layer() {
    let o = duplicator_oracle();
    let doc = decode(&document(
        &[layer("ly_t", "Toggle", &[(0, "t1")])],
        json!({}),
    ));
    let on = plan_duplicator_set(&doc, "ly_t", true, Some(7)).unwrap();
    let off = plan_duplicator_set(&on.document, "ly_t", false, None).unwrap();
    assert!(off.changed);
    let after = wire(&off.document);
    assert_eq!(o.get("D2.duplicator"), &Value::Null);
    assert!(hosts(&after).is_empty(), "the record is gone");
    assert_eq!(
        after["componentTables"]["duplicators"]["duplicators"],
        json!([]),
        "the table stays authored"
    );
    assert_eq!(&after["layers"][0]["locked"], o.get("D2.locked"));
}

#[test]
fn asking_for_the_state_a_layer_already_has_changes_nothing() {
    let doc = decode(&document(
        &[layer("ly_t", "Toggle", &[(0, "t1")])],
        json!({}),
    ));
    let off = plan_duplicator_set(&doc, "ly_t", false, None).unwrap();
    assert!(!off.changed);
    assert_eq!(off.document, doc);
    let on = plan_duplicator_set(&doc, "ly_t", true, Some(1)).unwrap();
    let again = plan_duplicator_set(&on.document, "ly_t", true, Some(2)).unwrap();
    assert!(!again.changed);
    assert_eq!(
        again.document, on.document,
        "the seed of a second request is not applied"
    );
}

#[test]
fn the_seed_is_the_baseline_random_range() {
    let doc = decode(&document(
        &[layer("ly_t", "Toggle", &[(0, "t1")])],
        json!({}),
    ));
    for seed in [None, Some(1_000_000), Some(u32::MAX)] {
        assert_eq!(
            plan_duplicator_set(&doc, "ly_t", true, seed).unwrap_err(),
            SetupError::SeedRange,
            "{seed:?}"
        );
    }
    for seed in [0, 999_999] {
        assert!(
            plan_duplicator_set(&doc, "ly_t", true, Some(seed)).is_ok(),
            "{seed}"
        );
    }
    assert_eq!(
        plan_duplicator_set(&doc, "nope", true, Some(1)).unwrap_err(),
        SetupError::UnknownLayer("nope".into())
    );
}

#[test]
fn the_object_duplicator_pools_the_selection_like_the_baseline_rows() {
    let o = duplicator_oracle();
    let doc = decode(&pool_document());
    let selection = uids(&["ly_d", "ly_b", "ly_b", "ly_a", "ly_c"]);
    let plan = plan_object_duplicator(&doc, 9, &selection, Some(654_321)).unwrap();
    assert_eq!(plan.created.as_deref(), Some("native-layer:9:0"));
    let after = wire(&plan.document);
    let layers = after["layers"].as_array().unwrap();
    assert_eq!(&json!(layers.len()), o.get("D3.layerCount"));
    let host = &layers[o.get("D3.return").as_u64().unwrap() as usize];
    assert_eq!(host["layerUid"], "native-layer:9:0");
    assert_eq!(&host["name"], o.get("D3.host.name"));
    assert_eq!(&host["locked"], o.get("D3.host.locked"));
    assert_eq!(&host["visible"], o.get("D3.host.visible"));
    let keys: Vec<Value> = host["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(&Value::Array(keys), o.get("D3.host.keyFrames"));
    let on_host = after["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["target"]["layerUid"] == "native-layer:9:0")
        .count();
    assert_eq!(&json!(on_host), o.get("D3.host.strokeTotal"));
    let list = hosts(&after);
    assert_eq!(
        &record_of(&list, "native-layer:9:0")["duplicator"],
        o.get("D3.descriptor")
    );
    let visible: Vec<Value> = layers[..5].iter().map(|l| l["visible"].clone()).collect();
    assert_eq!(&Value::Array(visible), o.get("D3.poolVisible"));
    assert_eq!(
        &record_of(&list, "native-layer:9:0")["duplicator"]["sourceLayerUids"],
        o.get("D3.poolUids")
    );
    assert_eq!(decode(&after), plan.document, "storage round trip");
}

#[test]
fn a_pool_of_fewer_than_two_layers_is_refused_and_nothing_changes() {
    let o = duplicator_oracle();
    let doc = decode(&pool_document());
    let error = plan_object_duplicator(&doc, 9, &uids(&["ly_a", "ly_c"]), Some(1)).unwrap_err();
    assert_eq!(error, SetupError::TooFewLayers);
    assert_eq!(o.get("D4.return"), &json!(-1));
    assert_eq!(o.get("D4.layerCount"), &json!(2));
    assert_eq!(wire(&doc)["layers"].as_array().unwrap().len(), 5);
    // The baseline skips an index that names no layer; the native request names
    // layers by identity, so a stale uid is refused instead of shrinking the pool.
    assert_eq!(
        plan_object_duplicator(&doc, 9, &uids(&["ly_a", "ly_b", "nope"]), Some(1)).unwrap_err(),
        SetupError::UnknownLayer("nope".into())
    );
    assert_eq!(
        plan_object_duplicator(&doc, 9, &uids(&["ly_a", "ly_b"]), None).unwrap_err(),
        SetupError::SeedRange
    );
}

#[test]
fn the_path_layer_refuses_what_the_baseline_resolver_answers_null_for() {
    let o = duplicator_oracle();
    let doc = decode(&path_document());
    let refused = [
        (
            "D5.pathOnDuplicator",
            "ly_other_dup",
            SetupError::PathChained("ly_other_dup".into()),
        ),
        (
            "D5.pathOnHost",
            "ly_dup",
            SetupError::PathSelf("ly_dup".into()),
        ),
        (
            "D5.pathOnUnknown",
            "ly_missing",
            SetupError::PathUnknown("ly_missing".into()),
        ),
    ];
    for (row, path, error) in refused {
        assert_eq!(o.get(row), &Value::Null, "{row}");
        assert_eq!(
            plan_duplicator_path(&doc, "ly_dup", Some(path)).unwrap_err(),
            error,
            "{row}"
        );
    }
    assert_ne!(o.get("D5.pathOnPlain"), &Value::Null);
    let plan = plan_duplicator_path(&doc, "ly_dup", Some("ly_plain")).unwrap();
    assert!(plan.changed);
    let list = hosts(&wire(&plan.document));
    assert_eq!(
        record_of(&list, "ly_dup")["duplicator"]["pathLayerUid"],
        "ly_plain"
    );
    assert_eq!(
        decode(&wire(&plan.document)),
        plan.document,
        "storage round trip"
    );
}

#[test]
fn clearing_the_path_writes_an_explicit_null_once() {
    let o = duplicator_oracle();
    assert_eq!(o.get("D5.pathNull"), &Value::Null);
    let doc = decode(&path_document());
    let cleared = plan_duplicator_path(&doc, "ly_dup", None).unwrap();
    assert!(
        cleared.changed,
        "absent becomes an explicit null, as `value || null` writes"
    );
    let list = hosts(&wire(&cleared.document));
    assert_eq!(
        record_of(&list, "ly_dup")["duplicator"]["pathLayerUid"],
        Value::Null
    );
    let empty = plan_duplicator_path(&cleared.document, "ly_dup", Some("")).unwrap();
    assert!(!empty.changed, "an empty select value is the same null");
    assert_eq!(empty.document, cleared.document);
    let set = plan_duplicator_path(&cleared.document, "ly_dup", Some("ly_plain")).unwrap();
    let same = plan_duplicator_path(&set.document, "ly_dup", Some("ly_plain")).unwrap();
    assert!(!same.changed);
}

#[test]
fn only_a_duplicator_host_has_a_path_layer() {
    let doc = decode(&path_document());
    assert_eq!(
        plan_duplicator_path(&doc, "ly_plain", Some("ly_dup")).unwrap_err(),
        SetupError::NotADuplicator("ly_plain".into())
    );
    assert_eq!(
        plan_duplicator_path(&doc, "nope", None).unwrap_err(),
        SetupError::UnknownLayer("nope".into())
    );
}

#[test]
fn the_descriptor_comparison_notices_absent_versus_null_and_every_field() {
    let o = duplicator_oracle();
    let doc = decode(&document(
        &[layer("ly_t", "Toggle", &[(0, "t1")])],
        json!({}),
    ));
    let plan = plan_duplicator_set(&doc, "ly_t", true, Some(123_456)).unwrap();
    let list = hosts(&wire(&plan.document));
    let written = record_of(&list, "ly_t")["duplicator"].clone();
    assert_eq!(&written, o.get("D1.descriptor"));
    type Mutation = (&'static str, fn(&mut Value));
    let mutations: [Mutation; 7] = [
        ("rows", |d| d["rows"] = json!(3)),
        ("mode", |d| d["mode"] = json!("radial")),
        ("seed", |d| d["seed"] = json!(123_457)),
        ("end angle absent", |d| {
            d.as_object_mut().unwrap().remove("endAngle");
        }),
        ("hue", |d| d["staggerRandom"]["hue"] = json!(true)),
        ("direction", |d| {
            d["timeOffset"]["direction"] = json!("backward")
        }),
        ("path layer", |d| d["pathLayerUid"] = json!("x")),
    ];
    for (name, mutate) in mutations {
        let mut changed = written.clone();
        mutate(&mut changed);
        assert_ne!(&changed, o.get("D1.descriptor"), "{name} went unnoticed");
    }
}
