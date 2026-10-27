//! Held editing copies the baseline cell through existing native owners in one transaction.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use support::{bytes, dispatch, document, owner_of, record_at, state, target};
fn fixture(version: u32) -> Value {
    let mut a = record_at("A", "L1", 0, "s-a");
    a["vectorExt"] =
        json!({"paramShape":{"kind":"rect","tl":0,"tr":0,"br":0,"bl":0},"boxAngle":17});
    let mut d = document(
        version,
        vec![
            a,
            record_at("B", "L1", 0, "s-b"),
            record_at("A", "L1", 3, "sample"),
            record_at("C", "L1", 10, "later"),
            record_at("C", "L2", 0, "unrelated"),
        ],
        None,
    );
    d["totalFrames"] = json!(12);
    d["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0},{"frame":10}],"samples":[{"frame":3}]});
    d
}
fn request(o: &crate::history::NativeObjectHistory, id: &str, frame: u32, identity: bool) -> Value {
    let mut r = support::request(
        o,
        id,
        json!({"command":"object.subselect.edit","stableTarget":target("L1",0,"s-a"),"atFrame":frame,"intent":{"kind":"move","indices":[0],"delta":if identity{[0,0]}else{[7,9]}}}),
    );
    r["operation"] = json!("command.document.object.subselect.edit");
    r
}
#[test]
fn subselect_held_edit_and_identity_promotion_copy_siblings_with_one_undo_replay_and_reopen() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/held.json")).unwrap();
    let row = &oracle["cases"][2];
    assert_eq!(row["frame"], 5);
    for version in [1, 2] {
        for identity in [false, true] {
            let mut o = owner_of(&fixture(version));
            let before = bytes(&o);
            let initial = state(&o);
            let q = request(&o, "held-edit", 5, identity);
            let r = dispatch(&mut o, &q);
            assert_eq!(r["ok"], true, "{r}");
            assert_eq!(r["result"], json!({"applied":true,"historyEntriesAdded":1}));
            assert_eq!(o.history_depths(), (1, 0));
            let after = bytes(&o);
            let value = state(&o);
            let keys: Vec<_> = value["layers"][0]["timing"]["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k["frame"].as_u64().unwrap() as usize)
                .collect();
            let expected_keys: Vec<_> = row["expected"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, f)| f["isKeyframe"] == true)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(keys, expected_keys);
            let copied: Vec<_> = value["objects"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["target"]["frameScope"]["frame"] == 5)
                .collect();
            let ids: Vec<_> = copied
                .iter()
                .map(|r| r["target"]["strokeId"].as_str().unwrap())
                .collect();
            let expected_ids: Vec<_> = row["expected"][5]["strokes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["strokeId"].as_str().unwrap())
                .collect();
            assert_eq!(ids, expected_ids);
            for old in initial["objects"].as_array().unwrap() {
                assert!(value["objects"].as_array().unwrap().contains(old));
            }
            for (i, copy) in copied.iter().enumerate() {
                let mut expected = initial["objects"][i].clone();
                expected["target"]["frameScope"]["frame"] = json!(5);
                if i == 0 && !identity {
                    for (axis, delta) in [("x", 7.), ("y", 9.)] {
                        let p = &mut expected["geometry"]["segments"][0]["point"][axis];
                        *p = json!(p.as_f64().unwrap() + delta);
                    }
                }
                assert_eq!(**copy, expected);
            }
            if version == 2 {
                let scopes = value["compositionOrder"]["contexts"][0]["scopes"]
                    .as_array()
                    .unwrap();
                let copied_order = scopes
                    .iter()
                    .find(|s| s["frameScope"]["frame"] == 5)
                    .unwrap();
                let source = scopes
                    .iter()
                    .find(|s| s["frameScope"]["frame"] == 0)
                    .unwrap();
                let expected: Vec<_> = source["elements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["layerUid"] == "L1")
                    .cloned()
                    .collect();
                assert_eq!(copied_order["elements"], json!(expected));
            }
            assert_eq!(dispatch(&mut o, &q), r);
            assert_eq!(bytes(&o), after);
            o.undo(o.content_revision()).unwrap();
            assert_eq!(bytes(&o), before);
            o.redo(o.content_revision()).unwrap();
            assert_eq!(bytes(&o), after);
            assert_eq!(bytes(&owner_of(&value)), after);
        }
    }
}
#[test]
fn subselect_identity_on_existing_key_or_interpolated_sample_does_not_promote() {
    for (frame, id) in [(0, "s-a"), (3, "sample")] {
        let mut o = owner_of(&fixture(2));
        let before = bytes(&o);
        let mut q = request(&o, "existing", frame, true);
        q["payload"]["stableTarget"] = target("L1", frame, id);
        let r = dispatch(&mut o, &q);
        assert_eq!(
            r["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "{r}"
        );
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
fn add_item(d: &mut Value, version: u32, kind: &str, frame: u32) {
    let t = target("L1", frame, "item");
    if kind == "raster" {
        d["items"] = json!({"raster":[{"target":t,"item":{"assetId":"image-one","x":0,"y":0,"width":20,"height":10,"opacity":1}}]});
    } else {
        let row = include_str!("fixtures/object-text-layout-commands/oracle.txt")
            .lines()
            .find(|l| l.starts_with("CREATE "))
            .unwrap();
        let row: Value = serde_json::from_str(&row[7..]).unwrap();
        d["items"] = json!({"text":[{"target":t,"block":row["record"]}]});
    }
    if version == 2 {
        let scopes = d["compositionOrder"]["contexts"][0]["scopes"]
            .as_array_mut()
            .unwrap();
        if !scopes.iter().any(|s| s["frameScope"]["frame"] == frame) {
            scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":[]}));
            scopes.sort_by_key(|s| s["frameScope"]["frame"].as_u64().unwrap());
        }
        let s = scopes
            .iter_mut()
            .find(|s| s["frameScope"]["frame"] == frame)
            .unwrap();
        let position = s["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["layerUid"] == "L1")
            .count();
        s["elements"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"L1","strokeId":"item","naturalPosition":position,"order":{}}));
    }
}
#[test]
fn subselect_held_mixed_cells_source_mismatch_range_and_locked_refuse_atomically() {
    for version in [1, 2] {
        for identity in [false, true] {
            for kind in [
                "raster",
                "text",
                "destination-raster",
                "destination-text",
                "range",
                "locked",
                "source",
                "outside",
                "null",
            ] {
                let mut d = fixture(version);
                match kind {
                    "raster" | "text" => add_item(&mut d, version, kind, 0),
                    "destination-raster" => add_item(&mut d, version, "raster", 5),
                    "destination-text" => add_item(&mut d, version, "text", 5),
                    "range" => d["layers"][0]["range"] = json!({"outPoint":2}),
                    "locked" => d["layers"][0]["locked"] = json!(true),
                    _ => {}
                }
                let mut o = owner_of(&d);
                let before = bytes(&o);
                let mut q = request(
                    &o,
                    "held-refusal",
                    if kind == "source" {
                        11
                    } else if kind == "outside" {
                        12
                    } else {
                        5
                    },
                    identity,
                );
                if kind == "null" {
                    q["payload"]["atFrame"] = Value::Null;
                }
                let r = dispatch(&mut o, &q);
                assert_eq!(r["ok"], false, "{kind}: {r}");
                assert_eq!(bytes(&o), before, "{kind}");
                assert_eq!(o.history_depths(), (0, 0));
            }
        }
    }
}
