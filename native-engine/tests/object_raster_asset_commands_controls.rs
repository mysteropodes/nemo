//! Oracle-discrimination controls: single-flaw variants of a test-local model of the edit must each be caught by the
//! baseline oracle rows, so the rows cannot be satisfied by a plausible but wrong implementation. They do NOT run the
//! production transaction; its sensitivity to the same flaws comes from the real-code oracle test in
//! `object_raster_asset_commands.rs`, which compares the production result to these same rows.
use super::super::support::rows;
use super::{canonical, with_asset, ORACLE};
use serde_json::{json, Map, Value};

/// A test-local model of the edit on a `before` record (the production semantic, restated), with an optional flaw.
fn edit(row: &Value, flaw: Option<&str>) -> String {
    let mut members: Map<String, Value> = with_asset(row["before"].clone())
        .as_object()
        .unwrap()
        .clone();
    let args = &row["args"];
    match row["op"].as_str().unwrap() {
        "move" => {
            let (mut x, mut y) = (args["x"].clone(), args["y"].clone());
            if flaw == Some("rounds") {
                x = json!(x.as_f64().unwrap().round());
                y = json!(y.as_f64().unwrap().round());
            }
            members.insert("x".into(), x);
            members.insert("y".into(), y);
            if flaw == Some("drops-rotation-on-move") {
                members.remove("rotation");
            }
        }
        "resize" => {
            members.insert("width".into(), args["width"].clone());
            members.insert("height".into(), args["height"].clone());
        }
        "rotate" => {
            let mut value = args["rotation"].clone();
            let mut rotation = value.as_f64().unwrap();
            if flaw == Some("normalizes-rotation") {
                rotation = (rotation + 180.0).rem_euclid(360.0) - 180.0;
                value = json!(rotation);
            }
            if rotation == 0.0 && flaw != Some("keeps-zero-rotation") {
                members.remove("rotation");
            } else {
                members.insert("rotation".into(), value);
            }
        }
        "opacity" => {
            members.insert("opacity".into(), args["opacity"].clone());
        }
        other => panic!("unknown op {other}"),
    }
    if flaw == Some("resets-opacity") {
        members.insert("opacity".into(), json!(1));
    }
    canonical(&Value::Object(members))
}

fn expected(row: &Value) -> String {
    canonical(&with_asset(row["expected"].clone()))
}

#[test]
fn the_faithful_edit_matches_every_oracle_row_and_each_single_flaw_is_caught() {
    let all = rows(ORACLE, None);
    for row in &all {
        assert_eq!(edit(row, None), expected(row), "{}", row["name"]);
    }
    for (flaw, caught_by) in [
        ("rounds", "move-fractional"),
        ("drops-rotation-on-move", "move-rotated-keeps-rotation"),
        ("normalizes-rotation", "rotate-to-190"),
        ("keeps-zero-rotation", "rotate-back-to-zero"),
        ("resets-opacity", "opacity-0.25"),
    ] {
        let caught: Vec<&str> = all
            .iter()
            .filter(|row| edit(row, Some(flaw)) != expected(row))
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert!(
            caught.contains(&caught_by),
            "{flaw}: caught by {caught:?}, expected {caught_by}"
        );
    }
}

#[test]
fn the_oracle_has_not_been_hollowed_out() {
    let all = rows(ORACLE, None);
    let count = |op: &str| all.iter().filter(|row| row["op"] == op).count();
    assert_eq!(
        (
            count("move"),
            count("resize"),
            count("rotate"),
            count("opacity")
        ),
        (4, 3, 9, 3)
    );
    let rotated_before = all
        .iter()
        .filter(|row| row["before"].get("rotation").is_some())
        .count();
    assert_eq!(rotated_before, 6, "rotated starting points are exercised");
    assert!(all.iter().any(|row| row["class"] == "normalized"));
    assert!(all.iter().any(|row| row["class"] == "noise"));
}
