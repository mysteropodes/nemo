//! #2256: a layer that hosts a stored duplicator is reported with its source mode; every other layer carries neither member.
use super::layer_members;
use serde_json::json;

#[path = "object_component_convert_fixture.rs"]
mod fixture;

#[test]
fn a_duplicator_host_is_reported_with_its_source_mode() { // #2256: the Motion duplicator column reads this; nothing else changes
    let layers = [fixture::layer("host", "Host", &[(0, "s1")]), fixture::layer("pool", "Pool", &[(0, "s2")]), fixture::layer("plain", "Plain", &[(0, "s3")])];
    let table = fixture::duplicator_table(&[("host", json!({"mode":"grid"})), ("pool", json!({"sourceMode":"objects","sourceLayerUids":["plain"]}))]);
    let rows = layer_members(&fixture::decode(&fixture::document(&layers, json!({"componentTables":{"duplicators":table}}))));
    assert_eq!((&rows[0]["hasDuplicator"], &rows[0]["dupSourceMode"]), (&json!(true), &json!("shape")));
    assert_eq!((&rows[1]["hasDuplicator"], &rows[1]["dupSourceMode"]), (&json!(true), &json!("objects")));
    assert!(rows[2].get("hasDuplicator").is_none() && rows[2].get("dupSourceMode").is_none());
}
