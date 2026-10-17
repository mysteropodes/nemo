//! L02-H split-layer-into-elements replay: 48 scenarios generated from the
//! verbatim `splitLayerIntoElementsCore` / `layerElements` / `elementLabel` of
//! nemo @ fbe4b8a3 (fixtures/object-selection-edit/oracle.txt) are rebuilt as
//! native documents, split through the actual owner and compared with the
//! baseline's layers, names, colors, per-frame stroke partition, kept groups,
//! promoted motion, copied expressions and carried flags.
use super::object_selection_edit_tests::{apply, bytes, split, strokes_at, value};
use super::*;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/object-selection-edit/oracle.txt");

fn dimension(property: &str) -> usize {
    match property {
        "opacity" | "rotation" => 1,
        _ => 2,
    }
}
fn ones(property: &str) -> Vec<u32> {
    vec![1; dimension(property)]
}
fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

/// The native document of one scenario: filled shapes are the legacy family,
/// unfilled ones open vector strokes, the layer's motion, expressions, controls
/// and group table become its holders and its table.
fn native_document(sc: &Value, version: u32) -> Value {
    let mut keys: Vec<u64> = Vec::new();
    let mut samples: Vec<u64> = Vec::new();
    let mut objects: Vec<Value> = Vec::new();
    for (frame, spec) in sc["frames"].as_array().unwrap().iter().enumerate() {
        match spec["kind"].as_str().unwrap() {
            "key" => keys.push(frame as u64),
            "interp" => samples.push(frame as u64),
            _ => {}
        }
        for (rank, stroke) in spec["strokes"].as_array().unwrap().iter().enumerate() {
            let id = stroke["id"].as_str().unwrap();
            let x = 10 * rank as i64;
            objects.push(if stroke["fill"] == true {
                super::object_selection_edit_tests::shape("T", id, frame as u32, x)
            } else {
                super::object_selection_edit_tests::line("T", id, frame as u32, x)
            });
        }
    }
    let mut target = json!({"layerUid":"T","visible":false,
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root",
            "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),
            "samples":samples.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>()},
        "folderId":sc["folderId"]});
    if let Some(name) = sc["name"].as_str() {
        target["name"] = json!(name);
    }
    if !sc["shy"].is_null() {
        target["shy"] = sc["shy"].clone();
    }
    if sc["pathFx"] == true {
        target["pathFx"] = json!([{"type":"zigzag"}]);
    }
    if let Some(groups) = sc["groups"].as_array() {
        let entries: Vec<Value> = groups
            .iter()
            .map(|group| {
                let mut entry = json!({"gid":group["gid"],"order":group["order"],
                    "combineMode":"none"});
                if let Some(name) = group["name"].as_str() {
                    entry["name"] = json!(name);
                }
                entry
            })
            .collect();
        let mut members: Vec<Value> = Vec::new();
        for spec in sc["frames"].as_array().unwrap() {
            for stroke in spec["strokes"].as_array().unwrap() {
                if let Some(gid) = stroke["gid"].as_str() {
                    members.push(json!({"strokeId":stroke["id"],"groupId":gid}));
                }
            }
        }
        target["groups"] = json!({"contextId":"scene-root","entries":entries,"members":members});
    }
    let mut layers: Vec<Value> = sc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"layerUid":format!("O{i}"),"name":name,"visible":true}))
        .collect();
    layers.insert(sc["at"].as_u64().unwrap() as usize, target);
    let mut document = super::object_selection_edit_tests::document(version, layers, objects);
    if sc["folderId"] == "folder1" {
        document["layerFolders"] = json!({"folder1":{"name":"F"}});
    }
    let mut holders: Vec<Value> = Vec::new();
    for (stroke, motion) in sc["motion"].as_object().unwrap() {
        let track = names(&motion["track"]);
        let fixed = names(&motion["static"]);
        let mut properties: Vec<Value> = Vec::new();
        let mut order: Vec<String> = track.clone();
        order.extend(fixed.iter().filter(|p| !track.contains(p)).cloned());
        for property in &order {
            let mut record = json!({"property":property});
            if track.contains(property) {
                record["track"] = json!({"keys":[{"frame":0,"v":ones(property)}]});
            }
            if fixed.contains(property) {
                let values: Vec<u32> = [1, 2][..dimension(property)].to_vec();
                record["static"] = json!(values);
            }
            properties.push(record);
        }
        holders.push(
            json!({"target":{"contextId":"scene-root","layerUid":"T","strokeId":stroke},
            "properties":properties}),
        );
    }
    // The layer's own motion (dropped by the baseline) plus its expressions and controls.
    let mut layer_properties = vec![
        json!({"property":"position","track":{"keys":[{"frame":0,"v":[9,9]}]}}),
        json!({"property":"scale","static":[50,50]}),
    ];
    for property in names(&sc["layerExpressions"]) {
        let expression = json!({"enabled":true,"code":"value"});
        match layer_properties
            .iter_mut()
            .find(|r| r["property"] == property.as_str())
        {
            Some(record) => record["expression"] = expression,
            None => layer_properties.push(json!({"property":property,"expression":expression})),
        }
    }
    let mut layer_holder = json!({"target":{"contextId":"scene-root","layerUid":"T"},
        "properties":layer_properties});
    let controls = sc["controls"].as_u64().unwrap();
    if controls > 0 {
        layer_holder["controls"] = Value::Array(
            (0..controls)
                .map(|c| json!({"key":format!("xc_{c}"),"name":format!("Ctl {c}"),"type":"number"}))
                .collect(),
        );
    }
    holders.push(layer_holder);
    document["propertyHolders"] = Value::Array(holders);
    document
}

/// Properties of the layer-level holder of `uid` as `name -> (static, track, expression)`.
fn flags(document: &Value, uid: &str) -> (Vec<(String, bool, bool, bool)>, usize) {
    let holder = document["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == uid && h["target"].get("strokeId").is_none());
    let Some(holder) = holder else {
        return (Vec::new(), 0);
    };
    let mut rows: Vec<(String, bool, bool, bool)> = holder["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["property"].as_str().unwrap().to_string(),
                p.get("static").is_some(),
                p.get("track").is_some(),
                p.get("expression").is_some(),
            )
        })
        .collect();
    rows.sort();
    let controls = holder
        .get("controls")
        .map_or(0, |c| c.as_array().unwrap().len());
    (rows, controls)
}
fn expected_flags(piece: &Value) -> (Vec<(String, bool, bool, bool)>, usize) {
    let (motion, fixed, expressions) = (
        names(&piece["motion"]),
        names(&piece["motionStatic"]),
        names(&piece["expressions"]),
    );
    let mut all: Vec<String> = motion.clone();
    all.extend(fixed.clone());
    all.extend(expressions.clone());
    all.sort();
    all.dedup();
    let rows = all
        .iter()
        .map(|p| {
            (
                p.clone(),
                fixed.contains(p),
                motion.contains(p),
                expressions.contains(p),
            )
        })
        .collect();
    (rows, piece["controls"].as_u64().unwrap() as usize)
}

fn check_piece(sc: &Value, document: &Value, index: usize, count: usize, piece: &Value) {
    let layer = &document["layers"][index];
    let last = index == sc["at"].as_u64().unwrap() as usize + count - 1;
    let id = sc["id"].as_u64().unwrap();
    assert_eq!(layer["name"], piece["name"], "#{id} name");
    assert_eq!(layer["color"], piece["color"], "#{id} color");
    assert_eq!(layer["visible"], true, "#{id}");
    assert_eq!(layer["locked"], false, "#{id}");
    let uid = layer["layerUid"].as_str().unwrap();
    assert_eq!(
        uid == "T",
        piece["keepsUid"] == true,
        "#{id} uid kept by the last piece only"
    );
    assert_eq!(uid == "T", last, "#{id}");
    for (frame, expected) in piece["frames"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            strokes_at(document, uid, frame as u32),
            names(&expected["ids"]),
            "#{id} piece {index} frame {frame}"
        );
    }
    let timing = &layer["timing"];
    let flagged = |flag: &str| -> Vec<u64> {
        piece["frames"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, f)| f[flag] == true)
            .map(|(i, _)| i as u64)
            .collect()
    };
    let keyed: Vec<u64> = timing["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect();
    let sampled: Vec<u64> = timing["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect();
    assert_eq!(keyed, flagged("key"), "#{id} exposure keys");
    assert_eq!(sampled, flagged("interp"), "#{id} exposure in-betweens");
    let kept: Option<Vec<String>> = layer.get("groups").map(|g| {
        let mut gids: Vec<String> = g["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["gid"].as_str().unwrap().to_string())
            .collect();
        gids.sort();
        gids
    });
    let wanted: Option<Vec<String>> = piece["groups"].as_array().and_then(|g| {
        let mut list = names(&Value::Array(g.clone()));
        list.sort();
        if list.is_empty() {
            None
        } else {
            Some(list)
        }
    });
    assert_eq!(kept, wanted, "#{id} piece {index} groups");
    assert_eq!(
        flags(document, uid),
        expected_flags(piece),
        "#{id} piece {index} holder"
    );
    match piece["shy"].as_bool() {
        Some(shy) => assert_eq!(layer["shy"], shy, "#{id}"),
        None => assert!(layer.get("shy").is_none(), "#{id}"),
    }
    match piece["folderId"].as_str() {
        Some(folder) => assert_eq!(layer["folderId"], folder, "#{id}"),
        None => assert!(
            layer.get("folderId").is_none(),
            "#{id} an explicit null is not carried"
        ),
    }
    assert_eq!(
        layer.get("pathFx").is_some(),
        piece["pathFx"] == true,
        "#{id}"
    );
}

fn check_order(sc: &Value, document: &Value, count: usize) {
    let plan = &document["compositionOrder"]["contexts"][0];
    let uids: Vec<&str> = document["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap())
        .collect();
    let stack: Vec<(&str, u64)> = plan["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["layerUid"].as_str().unwrap(),
                l["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect();
    let at = sc["at"].as_u64().unwrap() as usize;
    for (position, uid) in uids.iter().enumerate() {
        assert!(
            stack.contains(&(*uid, position as u64)),
            "#{} {uid} sits at slot {position}",
            sc["id"]
        );
    }
    assert_eq!(
        uids.len(),
        sc["layers"].as_array().unwrap().len() + count,
        "layers after the split, at {at}"
    );
}

#[test]
fn every_baseline_scenario_replays_on_v1_and_v2_documents() {
    let rows: Vec<Value> = ORACLE
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 48);
    let (mut split_rows, mut refused_rows, mut grouped, mut motioned) = (0, 0, 0, 0);
    for version in [1, 2] {
        for row in &rows {
            let (sc, expect) = (&row["scenario"], &row["expect"]);
            let mut owner = owner_for(sc, version);
            let before = bytes(&owner);
            let payload = split("T", sc["current"].as_u64().unwrap() as u32);
            let outcome = apply(&mut owner, "split", &payload);
            if expect["ok"] != true {
                assert_eq!(
                    outcome,
                    Err(DispatchErrorCode::InvalidRequest),
                    "#{}",
                    sc["id"]
                );
                assert_eq!(bytes(&owner), before);
                assert_eq!(owner.history_depths(), (0, 0));
                refused_rows += 1;
                continue;
            }
            assert_eq!(outcome, Ok(true), "#{} v{version}", sc["id"]);
            let document = value(&owner);
            let pieces = expect["pieces"].as_array().unwrap();
            let at = sc["at"].as_u64().unwrap() as usize;
            let listed: Vec<Value> = document["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.get("name").cloned().unwrap_or(Value::Null))
                .collect();
            assert_eq!(
                Value::Array(listed),
                expect["names"],
                "#{} layer list",
                sc["id"]
            );
            for (offset, piece) in pieces.iter().enumerate() {
                check_piece(sc, &document, at + offset, pieces.len(), piece);
            }
            if version == 2 {
                check_order(sc, &document, pieces.len());
            }
            assert_eq!(
                (owner.content_revision(), owner.history_depths()),
                (1, (1, 0))
            );
            owner.undo(1).unwrap();
            assert_eq!(
                bytes(&owner),
                before,
                "#{} one undo restores the single layer",
                sc["id"]
            );
            split_rows += 1;
            grouped += usize::from(pieces[pieces.len() - 1]["groups"].is_array());
            motioned += usize::from(
                pieces
                    .iter()
                    .any(|p| !p["motion"].as_array().unwrap().is_empty()),
            );
        }
    }
    assert_eq!(
        (split_rows, refused_rows),
        (88, 8),
        "44 splits and 4 refusals per version"
    );
    assert_eq!(
        (grouped, motioned),
        (56, 72),
        "28 grouped and 36 animated splits per version"
    );
}

fn owner_for(sc: &Value, version: u32) -> NativeObjectHistory {
    super::object_selection_edit_tests::owner_of(&native_document(sc, version))
}

#[test]
fn a_corrupted_expectation_is_caught() {
    let first: Value = serde_json::from_str(ORACLE.lines().next().unwrap()).unwrap();
    let (sc, expect) = (&first["scenario"], &first["expect"]);
    let mut owner = owner_for(sc, 2);
    apply(
        &mut owner,
        "split",
        &split("T", sc["current"].as_u64().unwrap() as u32),
    )
    .unwrap();
    let document = value(&owner);
    let piece = &expect["pieces"][0];
    let at = sc["at"].as_u64().unwrap() as usize;
    let count = expect["pieces"].as_array().unwrap().len();
    check_piece(sc, &document, at, count, piece);
    let mut wrong = piece.clone();
    wrong["name"] = json!("Hero - Forme 1");
    let caught = std::panic::catch_unwind(|| check_piece(sc, &document, at, count, &wrong));
    assert!(
        caught.is_err(),
        "the em dash in the baseline name is compared exactly"
    );
    let mut moved = piece.clone();
    moved["frames"][0]["ids"] = json!(["s0_1"]);
    let caught = std::panic::catch_unwind(|| check_piece(sc, &document, at, count, &moved));
    assert!(caught.is_err(), "a different stroke partition is caught");
}
