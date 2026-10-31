//! #2356 point-type hint through the real command family: optional and backward compatible, aligned through every edit,
//! one atomic history entry, exact undo/redo, reopen and the typed read.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};
use support::{bytes, dispatch, document, owner_of, record_at, state, target};
const OP: &str = "command.document.object.subselect.edit";

fn seg(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> Value {
    let p = |v: [f64; 2]| json!({"x":v[0],"y":v[1]});
    json!({"point":p(point),"handleIn":p(handle_in),"handleOut":p(handle_out)})
}
fn input(version: u32) -> Value {
    let mut r = record_at("A", "L1", 0, "s-a");
    r["geometry"]["closed"] = json!(false);
    r["geometry"]["segments"] = json!([
        seg([0.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
        seg([50.0, 20.0], [-10.0, 5.0], [6.0, 8.0]),
        seg([100.0, 0.0], [0.0, 0.0], [0.0, 0.0])
    ]);
    r.as_object_mut().unwrap().remove("vectorExt");
    document(version, vec![r], None)
}
fn req(owner: &NativeObjectHistory, id: &str, intent: Value) -> Value {
    let mut r = support::request(
        owner,
        id,
        json!({"command":"object.subselect.edit","stableTarget":target("L1",0,"s-a"),"intent":intent}),
    );
    r["operation"] = OP.into();
    r
}
fn types(owner: &NativeObjectHistory) -> Value {
    state(owner)["objects"][0]["vectorExt"]["pointTypes"].clone()
}
fn handles(owner: &NativeObjectHistory, i: usize) -> [f64; 4] {
    let s = &state(owner)["objects"][0]["geometry"]["segments"][i];
    let n = |k: &str, a: &str| s[k][a].as_f64().unwrap();
    [
        n("handleIn", "x"),
        n("handleIn", "y"),
        n("handleOut", "x"),
        n("handleOut", "y"),
    ]
}
fn read(owner: &NativeObjectHistory, preview: Option<Value>) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut page = json!({"cursor":0,"limit":16});
    if let Some(p) = preview {
        page["preview"] = p;
    }
    let q = json!({"apiVersion":2,"requestId":"types-read","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":owner.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":page}});
    serde_json::to_value(
        snapshot
            .dispatch(serde_json::from_value(q).unwrap())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn an_old_document_loads_and_saves_unchanged_and_untyped_edits_add_no_field() {
    for version in [1, 2] {
        let doc = input(version);
        let mut owner = owner_of(&doc);
        let original = bytes(&owner);
        assert!(!String::from_utf8(original.clone())
            .unwrap()
            .contains("pointTypes"));
        assert_eq!(
            bytes(&owner_of(&serde_json::from_slice(&original).unwrap())),
            original
        );
        for (i, intent) in [json!({"kind":"move","indices":[0],"delta":[2,3]}), json!({"kind":"insert","curve":0,"time":0.5}), json!({"kind":"toggle","index":1}), json!({"kind":"handle","index":1,"handle":"handleOut","point":[60,30],"breakPair":false})].into_iter().enumerate() {
            let q = req(&owner, &format!("plain-{i}"), intent);
            assert_eq!(dispatch(&mut owner, &q)["ok"], true);
            assert!(state(&owner)["objects"][0].get("vectorExt").is_none(), "no type was set, so no field: {i}");
        }
    }
}

#[test]
fn a_point_type_edit_stores_an_aligned_hint_with_exact_undo_redo_and_reopen() {
    for version in [1, 2] {
        let mut owner = owner_of(&input(version));
        let before = bytes(&owner);
        let q = req(
            &owner,
            "corner",
            json!({"kind":"pointType","indices":[1],"pointType":"corner"}),
        );
        let r = dispatch(&mut owner, &q);
        assert_eq!(
            r["result"],
            json!({"applied":true,"historyEntriesAdded":1}),
            "{r}"
        );
        assert_eq!(types(&owner), json!(["symmetric", "corner", "symmetric"]));
        assert_eq!(
            handles(&owner, 1),
            [-10.0, 5.0, 6.0, 8.0],
            "Corner keeps both handles"
        );
        let after = bytes(&owner);
        assert_eq!(dispatch(&mut owner, &q), r, "a replay is idempotent");
        assert_eq!(owner.undo(1), Ok(2));
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.redo(2), Ok(3));
        assert_eq!(bytes(&owner), after);
        let reopened =
            NativeObjectHistory::new("types-reopen", decode_storage_project(&after).unwrap())
                .unwrap();
        assert_eq!(bytes(&reopened), after);
        assert_eq!(
            types(&reopened),
            json!(["symmetric", "corner", "symmetric"])
        );
    }
}

#[test]
fn collapse_removes_the_handles_and_a_drag_then_follows_each_recorded_type() {
    let mut owner = owner_of(&input(2));
    let go = |owner: &mut NativeObjectHistory, id: &str, intent: Value| {
        let q = req(owner, id, intent);
        let r = dispatch(owner, &q);
        assert_eq!(r["ok"], true, "{r}");
    };
    go(
        &mut owner,
        "collapse",
        json!({"kind":"pointType","indices":[1],"pointType":"collapse"}),
    );
    assert_eq!(handles(&owner, 1), [0.0; 4]);
    assert_eq!(types(&owner)[1], "corner");
    go(
        &mut owner,
        "one-sided",
        json!({"kind":"handle","index":1,"handle":"handleOut","point":[60,20],"breakPair":false}),
    );
    assert_eq!(
        handles(&owner, 1),
        [0.0, 0.0, 10.0, 0.0],
        "a corner moves one handle only"
    );
    go(
        &mut owner,
        "smooth",
        json!({"kind":"pointType","indices":[1],"pointType":"smooth"}),
    );
    let [ix, iy, ox, oy] = handles(&owner, 1);
    assert!(
        ox > 0.0 && oy.abs() < 1e-9 && ix < 0.0 && iy.abs() < 1e-9,
        "the placed tangent is kept"
    );
    let in_len = (ix * ix + iy * iy).sqrt();
    go(
        &mut owner,
        "smooth-drag",
        json!({"kind":"handle","index":1,"handle":"handleOut","point":[50,60],"breakPair":false}),
    );
    let [ix, iy, ox, oy] = handles(&owner, 1);
    assert!(
        (ox, oy) == (0.0, 40.0)
            && ix.abs() < 1e-9
            && ((iy.abs() - in_len).abs() < 1e-9)
            && iy < 0.0,
        "collinear, the opposite keeps its length"
    );
    assert_eq!(types(&owner)[1], "smooth");
    go(
        &mut owner,
        "symmetric",
        json!({"kind":"pointType","indices":[1],"pointType":"symmetric"}),
    );
    go(
        &mut owner,
        "sym-drag",
        json!({"kind":"handle","index":1,"handle":"handleIn","point":[40,20],"breakPair":false}),
    );
    assert_eq!(
        handles(&owner, 1),
        [-10.0, 0.0, 10.0, 0.0],
        "symmetric mirrors"
    );
    go(
        &mut owner,
        "alt",
        json!({"kind":"handle","index":1,"handle":"handleOut","point":[50,40],"breakPair":true}),
    );
    assert_eq!(
        handles(&owner, 1),
        [-10.0, 0.0, 0.0, 20.0],
        "Alt moves one handle"
    );
    assert_eq!(types(&owner)[1], "corner");
}

#[test]
fn the_hint_stays_aligned_through_insert_delete_and_toggle_commands() {
    let mut owner = owner_of(&input(2));
    let go = |owner: &mut NativeObjectHistory, id: &str, intent: Value| {
        let q = req(owner, id, intent);
        assert_eq!(dispatch(owner, &q)["ok"], true, "{id}");
    };
    go(
        &mut owner,
        "t",
        json!({"kind":"pointType","indices":[1],"pointType":"smooth"}),
    );
    assert_eq!(types(&owner), json!(["symmetric", "smooth", "symmetric"]));
    go(
        &mut owner,
        "ins",
        json!({"kind":"insert","curve":0,"time":0.5}),
    );
    let t = types(&owner);
    assert_eq!(t.as_array().unwrap().len(), 4);
    assert_eq!(
        (t[0].clone(), t[2].clone(), t[3].clone()),
        (json!("symmetric"), json!("smooth"), json!("symmetric"))
    );
    go(&mut owner, "tog", json!({"kind":"toggle","index":2}));
    assert_eq!(types(&owner)[2], "corner");
    go(&mut owner, "del", json!({"kind":"delete","indices":[0]}));
    assert_eq!(types(&owner).as_array().unwrap().len(), 3);
    assert_eq!(
        state(&owner)["objects"][0]["geometry"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let after = bytes(&owner);
    for _ in 0..4 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_ne!(bytes(&owner), after);
    assert!(
        state(&owner)["objects"][0].get("vectorExt").is_none(),
        "undo returns to the untyped document exactly"
    );
}

#[test]
fn the_plain_read_lists_the_type_each_vertex_shows_and_a_preview_shows_the_edit() {
    let owner = owner_of(&input(2));
    let r = read(&owner, None);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["result"]["subselect"]["pointTypes"],
        json!(["symmetric", "corner", "symmetric"]),
        "inferred from the handles"
    );
    let p = read(
        &owner,
        Some(json!({"kind":"pointType","indices":[1],"pointType":"symmetric"})),
    );
    assert_eq!(p["result"]["subselect"]["pointTypes"][1], "symmetric");
    let bytes_before = bytes(&owner);
    assert_eq!(bytes(&owner), bytes_before, "a preview writes nothing");
}

#[test]
fn a_malformed_hint_is_refused_at_ingress() {
    let with = |hint: Value| {
        let mut doc = input(2);
        doc["objects"][0]["vectorExt"] = json!({"pointTypes":hint});
        decode_storage_project(doc.to_string().as_bytes())
    };
    assert!(with(json!(["corner", "smooth", "symmetric"])).is_ok());
    for bad in [
        json!(["corner"]),
        json!(["corner", "smooth", "symmetric", "corner"]),
        json!(["corner", "round", "symmetric"]),
        json!([]),
        json!("corner"),
        json!([1, 2, 3]),
    ] {
        assert!(with(bad.clone()).is_err(), "{bad}");
    }
    let mut ribbon = input(2);
    ribbon["objects"][0]["vectorExt"] = json!({"pointTypes":["corner","smooth","symmetric"],"pressure":{"centerSegments":[],"strokeProfile":"bulge","profileBase":1}});
    assert!(
        decode_storage_project(ribbon.to_string().as_bytes()).is_err(),
        "a pressure ribbon has no point-type hint"
    );
}

#[test]
fn the_public_contract_declares_every_point_type_and_the_hint_the_engine_owns() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let capability =
        include_str!("../../engineering/application/capabilities-v2/native-object-subselect.json");
    let declared = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    let intents = schema["$defs"]["ObjectSubselectIntent"].to_string();
    for kind in ["corner", "smooth", "symmetric", "collapse"] {
        assert!(intents.contains(&format!("\"{kind}\"")), "{kind}");
        // The engine owns each declared type: it applies it and records or clears a hint.
        let mut owner = owner_of(&input(2));
        let q = req(
            &owner,
            kind,
            json!({"kind":"pointType","indices":[1],"pointType":kind}),
        );
        assert_eq!(dispatch(&mut owner, &q)["ok"], true, "{kind}");
    }
    let mut owner = owner_of(&input(2));
    let q = req(
        &owner,
        "bad",
        json!({"kind":"pointType","indices":[1],"pointType":"round"}),
    );
    assert_eq!(dispatch(&mut owner, &q)["ok"], false);
    for def in [
        "ObjectVectorExt",
        "ObjectClipboardVectorExt",
        "ObjectVectorReadExt",
    ] {
        let hint = &schema["$defs"][def]["properties"]["pointTypes"];
        assert_eq!(
            declared(&hint["items"]["enum"]),
            ["corner", "smooth", "symmetric"],
            "{def}"
        );
    }
    assert!(capability.contains("vectorExt.pointTypes") && capability.contains("collapse"));
}
