//! Pinned Perspective presence, effective VP pages and raw-read refusal oracles.
use super::{default_points, edit, owner, pin, points, project, query, saved, Error};
use serde_json::{json, Value};

#[test]
fn raw_missing_null_empty_and_unknown_values_have_pure_native_default_projection() {
    for supplied in [
        json!({}),
        json!({"perspectiveVPs":null}),
        json!({"perspectiveVPs":[]}),
        json!({"perspectiveVPs":[],"perspectiveEnabled":false,
            "perspectiveMode":"unknown-import","perspectiveDensity":-3.5}),
    ] {
        let mut o = owner(supplied.clone());
        let before = saved(&o);
        let p = pin(&o);
        let settings = query(&p, json!({"kind":"settings"}));
        let raw = &settings["result"]["result"];
        assert_eq!(
            raw["perspectiveVPsPresent"],
            supplied.get("perspectiveVPs").is_some()
        );
        assert_eq!(
            raw["perspectiveVPsNull"],
            supplied.get("perspectiveVPs").is_some_and(Value::is_null)
        );
        assert_eq!(raw["perspectiveVPCount"], 0);
        assert_eq!(raw["effectiveVPCount"], 2);
        assert_eq!(raw["vpSource"], "default");
        for field in [
            "perspectiveEnabled",
            "perspectiveMode",
            "perspectiveDensity",
        ] {
            assert_eq!(raw.get(field), supplied.get(field));
        }
        let page = query(&p, json!({"kind":"vpPage","cursor":0,"limit":16}));
        assert_eq!(
            page["result"]["result"]["items"],
            json!([
                {"ordinal":1,"x":-15.0,"y":40.0,"locked":false},
                {"ordinal":2,"x":115.0,"y":40.0,"locked":false}
            ])
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
        edit(
            &mut o,
            "disable",
            json!({"kind":"setEnabled","enabled":false}),
        );
        assert_eq!(
            project(&o).get("perspectiveVPs"),
            supplied.get("perspectiveVPs")
        );
        let revision = o.content_revision();
        edit(
            &mut o,
            "enable",
            json!({"kind":"setEnabled","enabled":true}),
        );
        assert_eq!(o.content_revision(), revision + 1);
        assert_eq!(points(&o), default_points("2pt"));
        for field in ["perspectiveMode", "perspectiveDensity"] {
            assert_eq!(project(&o).get(field), supplied.get(field));
        }
    }
}
#[test]
fn pages_preserve_stored_order_optional_lock_fields_and_native_default_modes_without_writes() {
    let source: Vec<Value> = (0..35)
        .map(|i| {
            let mut point = json!({"x":i as f64 - 4.25,"y":-3.5});
            if i % 2 == 0 {
                point["locked"] = json!(i % 4 == 0);
            }
            point
        })
        .collect();
    let o = owner(json!({"perspectiveVPs":source}));
    let before = saved(&o);
    let p = pin(&o);
    let mut collected = Vec::new();
    let mut cursor = 0;
    loop {
        let response = query(&p, json!({"kind":"vpPage","cursor":cursor,"limit":16}));
        assert_eq!(response["ok"], true);
        let page = &response["result"]["result"];
        assert_eq!(page["vpSource"], "stored");
        assert_eq!(page["perspectiveVPCount"], 35);
        assert_eq!(page["effectiveVPCount"], 35);
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty() && items.len() <= 16);
        for item in items {
            assert_eq!(item["ordinal"].as_u64(), Some(collected.len() as u64 + 1));
            let mut point = item.clone();
            point.as_object_mut().unwrap().remove("ordinal");
            collected.push(point);
        }
        if page["nextCursor"].is_null() {
            break;
        }
        let next = page["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(collected, source);
    assert_eq!(saved(&o), before);
    for mode in ["1pt", "2pt", "3pt", "fisheye"] {
        let o = owner(json!({"perspectiveMode":mode,"perspectiveVPs":null}));
        let before = saved(&o);
        let response = query(&pin(&o), json!({"kind":"vpPage","cursor":0,"limit":16}));
        let mut expected = default_points(mode);
        for (i, point) in expected.as_array_mut().unwrap().iter_mut().enumerate() {
            point["ordinal"] = json!(i + 1);
        }
        assert_eq!(response["result"]["result"]["items"], expected);
        assert_eq!(saved(&o), before);
    }
}
#[test]
fn raw_read_fences_and_complete_response_budget_preserve_imported_metadata() {
    let o = owner(json!({"perspectiveMode":"\u{0001}".repeat(1024)}));
    let before = saved(&o);
    let p = pin(&o);
    assert_eq!(
        query(&p, json!({"kind":"settings"}))["error"]["code"],
        "unavailable"
    );
    let mut query = json!({"command":"object.project.perspective.read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":{"kind":"vpPage","cursor":0,"limit":1}});
    assert_eq!(
        p.query_perspective_metadata_json(&serde_json::to_vec(&query).unwrap())
            .unwrap()["result"]["effectiveVPCount"],
        2
    );
    for (field, value, code) in [
        (
            "command",
            json!("object.project.shadow.read"),
            Error::InvalidRequest,
        ),
        ("instanceId", json!("foreign"), Error::WrongInstance),
        ("documentId", json!("foreign"), Error::WrongDocument),
        ("atRevision", json!(1), Error::StaleRevision),
        ("documentSnapshotId", json!("wrong"), Error::StaleRevision),
        ("contextId", json!("nested"), Error::Unavailable),
    ] {
        let mut wrong = query.clone();
        wrong[field] = value;
        assert_eq!(
            p.query_perspective_metadata_json(&serde_json::to_vec(&wrong).unwrap()),
            Err(code)
        );
    }
    query["selector"] = json!({"kind":"settings"});
    let duplicate = query.to_string().replace(
        "\"kind\":\"settings\"",
        "\"kind\":\"settings\",\"kind\":\"settings\"",
    );
    assert_eq!(
        p.query_perspective_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let oversized = vec![b' '; 4097];
    for bytes in [b"[]".as_slice(), b"{".as_slice(), oversized.as_slice()] {
        assert_eq!(
            p.query_perspective_metadata_json(bytes),
            Err(Error::InvalidRequest)
        );
    }
    assert_eq!(saved(&o), before);
}
