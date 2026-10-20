//! Authenticated native Perspective projection/presence and imported-lock preservation.
use super::{
    change, document, edit, history, open, project, read, receipt, round_trip, saved, subscribe,
};
use crate::application_mcp::ApplicationMcp;
use serde_json::{json, Value};

#[tokio::test]
async fn perspective_effective_defaults_keep_raw_presence_mode_density_and_native_dimensions() {
    for (fields, expected) in [
        (
            json!({}),
            json!([
                {"ordinal":1,"x":-15.0,"y":40.0,"locked":false},
                {"ordinal":2,"x":115.0,"y":40.0,"locked":false}
            ]),
        ),
        (
            json!({"perspectiveVPs":null,"perspectiveMode":"fisheye","perspectiveDensity":-3.5}),
            json!([{"ordinal":1,"x":50.0,"y":40.0,"locked":false}]),
        ),
        (
            json!({"perspectiveVPs":[],"perspectiveMode":"3pt","perspectiveEnabled":false}),
            json!([
                {"ordinal":1,"x":-10.0,"y":28.0,"locked":false},
                {"ordinal":2,"x":110.0,"y":28.0,"locked":false},
                {"ordinal":3,"x":50.0,"y":144.0,"locked":false}
            ]),
        ),
        (
            json!({"canvasW":200,"canvasH":120,"perspectiveVPs":null,
            "perspectiveMode":"1pt","perspectiveDensity":-9.25}),
            json!([{"ordinal":1,"x":100.0,"y":60.0,"locked":false}]),
        ),
        (
            json!({"perspectiveMode":"unknown-import","perspectiveVPs":[]}),
            json!([
                {"ordinal":1,"x":-15.0,"y":40.0,"locked":false},
                {"ordinal":2,"x":115.0,"y":40.0,"locked":false}
            ]),
        ),
    ] {
        let state = ApplicationMcp::default();
        let id = open(&state, document(fields.clone()));
        let initial = saved(&state, &id);
        let initial_info = state.native_project_info(&id).unwrap();
        let (settings, events) = round_trip(
            &state,
            read(&state, &id, "raw-settings", json!({"kind":"settings"})),
            None,
        )
        .await;
        assert!(settings.ok, "{settings:?}");
        assert_eq!(events, 0);
        let raw = settings.result.unwrap()["result"].clone();
        assert_eq!(raw["vpSource"], "default");
        assert_eq!(raw["perspectiveVPCount"], 0);
        assert_eq!(raw["effectiveVPCount"], expected.as_array().unwrap().len());
        assert_eq!(
            raw["perspectiveVPsPresent"],
            fields.get("perspectiveVPs").is_some()
        );
        assert_eq!(
            raw["perspectiveVPsNull"],
            fields.get("perspectiveVPs").is_some_and(Value::is_null)
        );
        for field in [
            "perspectiveEnabled",
            "perspectiveMode",
            "perspectiveDensity",
        ] {
            assert_eq!(raw.get(field), fields.get(field));
        }
        let (page, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "raw-page",
                json!({"kind":"vpPage","cursor":0,"limit":16}),
            ),
            None,
        )
        .await;
        assert!(page.ok, "{page:?}");
        assert_eq!(events, 0);
        let body = page.result.unwrap()["result"].clone();
        assert_eq!(body["vpSource"], "default");
        assert_eq!(body["perspectiveVPCount"], 0);
        assert_eq!(body["effectiveVPCount"], expected.as_array().unwrap().len());
        assert_eq!(body["items"], expected);
        assert!(body["nextCursor"].is_null());
        assert_eq!(saved(&state, &id), initial);
        assert_eq!(state.native_project_info(&id).unwrap(), initial_info);
        let binding = subscribe(&state);
        let disable_applied = fields.get("perspectiveEnabled") != Some(&json!(false));
        let (disabled, events) = round_trip(
            &state,
            edit(
                &state,
                &id,
                "disable-without-defaults",
                0,
                json!({"kind":"setEnabled","enabled":false}),
            ),
            Some(&binding),
        )
        .await;
        let before_enable_revision = u64::from(disable_applied);
        receipt(&disabled, before_enable_revision, disable_applied);
        assert_eq!(events, usize::from(disable_applied));
        assert_eq!(
            project(&state, &id).get("perspectiveVPs"),
            fields.get("perspectiveVPs")
        );
        let disabled_bytes = saved(&state, &id);
        change(
            &state,
            &id,
            &binding,
            before_enable_revision,
            json!({"kind":"setEnabled","enabled":true}),
        )
        .await;
        let changed = saved(&state, &id);
        let mut expected_points = expected;
        for point in expected_points.as_array_mut().unwrap() {
            point.as_object_mut().unwrap().remove("ordinal");
        }
        assert_eq!(project(&state, &id)["perspectiveVPs"], expected_points);
        assert_eq!(project(&state, &id)["perspectiveEnabled"], true);
        for field in ["perspectiveMode", "perspectiveDensity"] {
            assert_eq!(project(&state, &id).get(field), fields.get(field));
        }
        assert!(
            history(
                &state,
                &id,
                "projection-undo",
                before_enable_revision + 1,
                false
            )
            .ok
        );
        assert_eq!(saved(&state, &id), disabled_bytes);
        assert!(
            history(
                &state,
                &id,
                "projection-redo",
                before_enable_revision + 2,
                true
            )
            .ok
        );
        assert_eq!(saved(&state, &id), changed);
        let fresh = ApplicationMcp::default();
        let next = open(&fresh, String::from_utf8(changed.clone()).unwrap());
        assert_eq!(saved(&fresh, &next), changed);
    }
}

#[tokio::test]
async fn perspective_stored_pages_and_moves_keep_optional_locks_raw_mode_and_redo() {
    let state = ApplicationMcp::default();
    let id = open(
        &state,
        document(json!({
            "perspectiveMode":"raw-custom","perspectiveDensity":-9.75,
            "perspectiveVPs":[{"x":-12.25,"y":8.5},{"x":100.5,"y":-8.25,"locked":true},
                {"x":-12.25,"y":8.5,"locked":false}]
        })),
    );
    let initial = saved(&state, &id);
    let mut collected = Vec::new();
    let mut cursor = 0;
    loop {
        let (response, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                &format!("stored-page-{cursor}"),
                json!({"kind":"vpPage","cursor":cursor,"limit":2}),
            ),
            None,
        )
        .await;
        assert!(response.ok, "{response:?}");
        assert_eq!(events, 0);
        let page = response.result.unwrap()["result"].clone();
        assert_eq!(page["vpSource"], "stored");
        assert_eq!(page["perspectiveVPsPresent"], true);
        assert_eq!(page["perspectiveVPsNull"], false);
        assert_eq!(page["perspectiveVPCount"], 3);
        assert_eq!(page["effectiveVPCount"], 3);
        collected.extend(page["items"].as_array().unwrap().iter().cloned());
        if page["nextCursor"].is_null() {
            break;
        }
        let next = page["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(
        collected,
        vec![
            json!({"ordinal":1,"x":-12.25,"y":8.5}),
            json!({"ordinal":2,"x":100.5,"y":-8.25,"locked":true}),
            json!({"ordinal":3,"x":-12.25,"y":8.5,"locked":false})
        ]
    );
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    change(
        &state,
        &id,
        &binding,
        0,
        json!({"kind":"move","ordinal":1,"x":-0.5,"y":2.25}),
    )
    .await;
    let moved = saved(&state, &id);
    change(
        &state,
        &id,
        &binding,
        1,
        json!({"kind":"translate","dx":-3.25,"dy":4.5}),
    )
    .await;
    let changed = saved(&state, &id);
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(
        next["project"]["perspectiveVPs"],
        json!([
            {"x":-3.75,"y":6.75},{"x":100.5,"y":-8.25,"locked":true},
            {"x":-15.5,"y":13.0,"locked":false}
        ])
    );
    let original: Value = serde_json::from_slice(&initial).unwrap();
    next["project"]["perspectiveVPs"] = original["project"]["perspectiveVPs"].clone();
    assert_eq!(next, original);
    assert!(history(&state, &id, "stored-undo-translate", 2, false).ok);
    assert_eq!(saved(&state, &id), moved);
    let (noop, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "stored-locked-noop",
            3,
            json!({"kind":"move","ordinal":2,"x":0,"y":0}),
        ),
        None,
    )
    .await;
    receipt(&noop, 3, false);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), moved);
    assert!(history(&state, &id, "stored-redo-translate", 3, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let fresh = ApplicationMcp::default();
    let next = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &next), changed);
}
