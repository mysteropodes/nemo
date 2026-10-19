//! Container-only controls over the real OrderTrack DTO; codec/owner consumers are separate tests.
use super::{EffectKeyTracks, EffectKeyTracksError};
use crate::object_document::{KeyLockMode, OrderTrack};
use serde_json::{json, Value};

fn decode(raw: &str) -> EffectKeyTracks {
    serde_json::from_str(raw).unwrap()
}
fn encoded(store: &EffectKeyTracks) -> String {
    serde_json::to_string(store).unwrap()
}
fn names(store: &EffectKeyTracks) -> Vec<&str> {
    store
        .entries()
        .iter()
        .map(|(key, _)| key.as_str())
        .collect()
}
fn reject(raw: &str) {
    assert!(
        serde_json::from_str::<EffectKeyTracks>(raw).is_err(),
        "malformed effect key map accepted"
    );
}
fn null_map(keys: &[String]) -> String {
    format!(
        "{{{}}}",
        keys.iter()
            .map(|key| format!("{}:null", serde_json::to_string(key).unwrap()))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[test]
fn empty_map_and_optional_null_are_distinct_and_parameter_null_entries_stay_present() {
    let empty = decode("{}");
    assert!(empty.entries().is_empty());
    assert_eq!(encoded(&empty), "{}");
    let null: Option<EffectKeyTracks> = serde_json::from_str("null").unwrap();
    assert!(null.is_none());
    let present: Option<EffectKeyTracks> = serde_json::from_str("{}").unwrap();
    assert!(present.unwrap().entries().is_empty());
    reject("null"); // Parent Presence<Option<_>>, not this map type, owns field nullability.
    let store = decode(r#"{"p1":null,"unknown":null,"":{"keys":[]}}"#);
    assert_eq!(store.entries().len(), 3);
    assert!(store.entries()[0].1.is_none());
    assert!(store.entries()[1].1.is_none());
    assert!(store.entries()[2].1.as_ref().unwrap().keys.is_empty());
    assert_eq!(
        encoded(&store),
        r#"{"p1":null,"unknown":null,"":{"keys":[]}}"#
    );
}

#[test]
fn original_parameter_insertion_order_empty_unknown_and_escaped_utf8_names_are_not_filtered() {
    let raw = r#"{"zeta":{"keys":[]},"p9":null,"":null,"p1":{"keys":[]},"é:\"\\":null}"#;
    let store = decode(raw);
    assert_eq!(names(&store), vec!["zeta", "p9", "", "p1", "é:\"\\"]);
    assert_eq!(encoded(&store), raw);
    assert_eq!(decode(&encoded(&store)), store);
    assert_eq!(encoded(&store.clone()), raw);
}

#[test]
fn duplicate_raw_keys_including_escaped_aliases_and_duplicates_with_null_refuse_before_overwrite() {
    for raw in [
        r#"{"p1":null,"p1":{"keys":[]}}"#,
        r#"{"p1":{"keys":[]},"p1":null}"#,
        r#"{"p1":null,"p\u0031":null}"#,
        r#"{"":null,"":null}"#,
        r#"{"p1":null,"p1":{"unrecognized":"not even a track"}}"#,
    ] {
        reject(raw);
    }
}

#[test]
fn maps_and_actual_order_track_records_are_closed_map_only_and_reject_wrong_scalar_shapes() {
    for raw in [
        "[]",
        "0",
        "false",
        "\"map\"",
        r#"[["p1",null]]"#,
        r#"{"p1":[]}"#,
        r#"{"p1":false}"#,
        r#"{"p1":3}"#,
        r#"{"p1":{}}"#,
        r#"{"p1":{"keys":null}}"#,
        r#"{"p1":{"keys":[],"extra":1}}"#,
        r#"{"p1":{"keys":[],"keys":[]}}"#,
        r#"{"p1":{"keys":[[0,[1]]]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"frame":1,"v":[1]}]}}"#,
        r#"{"p1":{"keys":[{"frame":null,"v":[1]}]}}"#,
        r#"{"p1":{"keys":[{"frame":"0","v":[1]}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":null}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":[true]}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":[1],"extra":0}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":[1],"lockTo":false}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":[1],"lockTo":{"in":{}}}]}}"#,
        r#"{"p1":{"keys":[{"frame":1e400,"v":[1]}]}}"#,
        r#"{"p1":{"keys":[{"frame":0,"v":[1e400]}]}}"#,
    ] {
        reject(raw);
    }
}

#[test]
fn actual_track_preserves_fractional_frames_locks_curves_handles_values_and_optional_presence() {
    let raw = r#"{"p7":{"keys":[{"frame":-2.5,"v":[0.125],"hold":false,"lockTo":"in","curvePoints":[{"x":0,"y":0,"tx":-1,"ty":2},{"x":1,"y":1}],"hIn":[-3.25,4],"hOut":[5,6]},{"frame":3.0,"v":[7],"lockTo":null,"curvePoints":null},{"frame":8,"v":[9]}]},"custom-param":null}"#;
    let store = decode(raw);
    let track: &OrderTrack = store.entries()[0].1.as_ref().unwrap();
    assert_eq!(track.keys.len(), 3);
    assert_eq!(track.keys[0].frame.as_f64(), Some(-2.5));
    assert_eq!(track.keys[0].lock_to.value(), Some(&Some(KeyLockMode::In)));
    assert_eq!(track.keys[1].lock_to.value(), Some(&None));
    assert!(track.keys[2].lock_to.missing());
    assert!(track.keys[2].curve_points.missing());
    assert_eq!(encoded(&store), raw);
    assert_eq!(
        serde_json::to_value(&store).unwrap(),
        serde_json::from_str::<Value>(raw).unwrap()
    );
    store.checked().unwrap();
}

#[test]
fn key_utf8_limit_is_inclusive_and_uses_decoded_bytes_not_character_count_or_escaped_length() {
    let exact = "é".repeat(512);
    assert_eq!(exact.len(), 1024);
    let raw = null_map(&[exact.clone()]);
    assert_eq!(encoded(&decode(&raw)), raw);
    reject(&null_map(&[exact + "x"]));
    let quotes = "\"".repeat(1024);
    assert_eq!(quotes.len(), 1024);
    let raw = null_map(&[quotes]);
    assert_eq!(
        encoded(&decode(&raw)),
        raw,
        "escaped JSON bytes count toward store budget, not key UTF8 bound"
    );
}

#[test]
fn entry_count_4096_is_admitted_and_4097_is_refused_without_aliasing_null_entries() {
    let keys: Vec<String> = (0..4096).map(|i| format!("parameter-{i}")).collect();
    let raw = null_map(&keys);
    let store = decode(&raw);
    assert_eq!(store.entries().len(), 4096);
    assert_eq!(names(&store).first().copied(), Some("parameter-0"));
    assert_eq!(names(&store).last().copied(), Some("parameter-4095"));
    assert_eq!(encoded(&store), raw);
    let mut over = keys;
    over.push("overflow".into());
    reject(&null_map(&over));
}

fn exact_budget_keys() -> Vec<String> {
    (0..4096)
        .map(|i| format!("{i:04}{}", "k".repeat(if i == 0 { 1011 } else { 1012 })))
        .collect()
}
#[test]
fn encoded_map_budget_is_inclusive_at_four_mib_and_counts_null_braces_colons_and_commas() {
    let keys = exact_budget_keys();
    let raw = null_map(&keys);
    assert_eq!(raw.len(), 4 * 1024 * 1024);
    let store = decode(&raw);
    assert_eq!(encoded(&store), raw);
    store.checked().unwrap();
    let mut keys = keys;
    keys[0].push('x');
    let over = null_map(&keys);
    assert_eq!(over.len(), raw.len() + 1);
    assert!(keys.iter().all(|key| key.len() <= 1024));
    reject(&over);
}

#[test]
fn encoded_map_budget_counts_json_escaping_even_when_decoded_key_lengths_do_not_change() {
    let mut keys = exact_budget_keys();
    let raw = null_map(&keys);
    assert_eq!(raw.len(), 4 * 1024 * 1024);
    let before = keys[0].len();
    keys[0].replace_range(4..5, "\"");
    assert_eq!(keys[0].len(), before);
    let escaped = null_map(&keys);
    assert_eq!(escaped.len(), raw.len() + 1);
    reject(&escaped);
}

#[test]
fn typed_mutation_keeps_order_and_null_slots_and_revalidates_before_serialization() {
    let original =
        decode(r#"{"second":{"keys":[{"frame":1.5,"v":[3],"lockTo":"out"}]},"first":null}"#);
    let mut candidate = original.clone();
    let before = encoded(&original);
    let track = candidate.entries_mut()[0].1.as_mut().unwrap();
    track.keys[0].frame = serde_json::Number::from(4);
    assert_eq!(names(&candidate), vec!["second", "first"]);
    assert!(candidate.entries()[1].1.is_none());
    assert_eq!(
        encoded(&candidate),
        r#"{"second":{"keys":[{"frame":4,"v":[3],"lockTo":"out"}]},"first":null}"#
    );
    assert_eq!(encoded(&original), before);
    candidate.entries_mut()[1].0 = "second".into();
    assert_eq!(candidate.checked(), Err(EffectKeyTracksError::Invalid));
    let mut output = Vec::new();
    assert!(serde_json::to_writer(&mut output, &candidate).is_err());
    assert!(
        output.is_empty(),
        "map validation precedes any serialized publication"
    );
    assert_eq!(encoded(&original), before);
    let mut candidate = original.clone();
    candidate.entries_mut()[0].0 = "x".repeat(1025);
    assert_eq!(candidate.checked(), Err(EffectKeyTracksError::Invalid));
}

#[test]
fn candidate_growth_in_key_or_track_bytes_refuses_while_original_typed_store_stays_exact() {
    let original = decode(&null_map(&exact_budget_keys()));
    let before = encoded(&original);
    let mut candidate = original.clone();
    candidate.entries_mut()[0].0.push('x');
    assert_eq!(candidate.checked(), Err(EffectKeyTracksError::Invalid));
    assert!(serde_json::to_string(&candidate).is_err());
    assert_eq!(encoded(&original), before);
    let mut candidate = original.clone();
    candidate.entries_mut()[0].1 = Some(
        serde_json::from_value(json!({"keys":[{"frame":4,"v":[1],"lockTo":"layer"}]})).unwrap(),
    );
    assert_eq!(candidate.checked(), Err(EffectKeyTracksError::Invalid));
    assert!(serde_json::to_string(&candidate).is_err());
    assert_eq!(encoded(&original), before);
}
