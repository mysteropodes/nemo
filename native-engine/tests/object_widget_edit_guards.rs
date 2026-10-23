//! Envelope, sealing and discrimination guards of the widget transactions: dispatch refusals and replay,
//! sealed candidates, oracle non-vacuity and the read-queries facade.
use super::object_widget_edit_rows::*;
use super::*;
#[cfg(feature = "evaluation")]
use crate::object_snapshot::widget_read::{WidgetIntent, WidgetWriteRefusal};
use serde_json::{json, Value};

#[test]
fn every_operation_refuses_cancel_stale_foreign_and_replays_without_a_second_execution() {
    let mut cases: Vec<(&str, Value, bool)> = vec![
        (
            "create",
            create_payload("slider", "Slider 1", [960., 540.]),
            true,
        ),
        (
            "settings",
            settings_payload(UID, json!({"size":[200,200]})),
            false,
        ),
    ];
    if cfg!(feature = "evaluation") {
        cases.push((
            "write",
            write_payload(UID, json!(0), local([50., -50.])),
            false,
        ));
    }
    for (op, payload, fresh) in cases {
        let make = || {
            if fresh {
                owner_of(&base(1))
            } else {
                joystick(None)
            }
        };
        let mut owner = make();
        let original = state(&owner);
        for (id, patch) in [
            ("cancel", json!({"cancelledBeforeDispatch":true})),
            ("stale", json!({"expectedRevision":99})),
            ("foreign-instance", json!({"instanceId":"foreign"})),
            ("foreign-document", json!({"documentId":"foreign"})),
            ("wrong-api", json!({"apiVersion":1})),
        ] {
            let mut req = request(&owner, id, op, payload.clone());
            for (member, value) in patch.as_object().unwrap() {
                req[member] = value.clone();
            }
            let reply = dispatch(&mut owner, &req);
            let want = match id {
                "cancel" => "cancelled_before_dispatch",
                "stale" => "stale_revision",
                "foreign-instance" => "wrong_instance",
                "foreign-document" => "wrong_document",
                _ => "invalid_request",
            };
            assert_eq!(code(&reply), want, "{op} {id}");
            assert_eq!(state(&owner), original, "{op} {id}");
        }
        let mut req = request(&owner, "missing", op, payload.clone());
        req.as_object_mut().unwrap().remove("expectedRevision");
        assert_eq!(dispatch(&mut owner, &req)["ok"], false, "{op}");
        assert_eq!(state(&owner), original, "{op}");
        let req = request(&owner, "replay", op, payload.clone());
        let first = dispatch(&mut owner, &req);
        assert_eq!(first["ok"], true, "{op}: {first}");
        let applied = state(&owner);
        assert_eq!(
            dispatch(&mut owner, &req),
            first,
            "{op}: same body, same receipt"
        );
        assert_eq!(state(&owner), applied, "{op}: the replay executed nothing");
        let altered = with(req.clone(), |r| r["payload"]["extra"] = json!(1));
        assert_eq!(
            dispatch(&mut owner, &altered)["ok"],
            false,
            "{op}: reused id, changed body"
        );
        assert_eq!(state(&owner), applied, "{op}");
        owner.undo(owner.content_revision()).unwrap();
        let undone = state(&owner);
        assert_eq!(dispatch(&mut owner, &req), first, "{op}: replay after undo");
        assert_eq!(state(&owner), undone, "{op}: replay cannot run twice");
    }
}

#[test]
fn prepared_candidates_are_sealed_to_their_origin_owner_and_revision() {
    let bytes = settings_payload(UID, json!({"size":[200,200]})).to_string();
    let mut source = joystick(None);
    let candidate = source
        .prepare_widget_settings_json(bytes.as_bytes())
        .unwrap();
    let mut other = joystick(None);
    let other_before = state(&other);
    assert_eq!(
        other.commit_widget_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        crate::object_codec::decode_storage_project(base(1).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign.commit_widget_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(state(&other), other_before);
    assert_eq!(foreign.history_depths(), (0, 0));
    assert_eq!(source.commit_widget_edit(candidate.clone()).unwrap(), 2);
    let applied = state(&source);
    assert_eq!(
        source.commit_widget_edit(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision,
        "a candidate commits once"
    );
    assert_eq!(state(&source), applied);
    let create = source
        .prepare_widget_create_json(
            create_payload("slider", "S", [1., 2.])
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let mut third = joystick(None);
    assert_eq!(
        third.commit_layer_create(create).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
}

#[test]
fn the_oracle_discriminates_the_flaws_the_rows_exist_for() {
    let wr = rows("wr");
    let writes = |row: &Vec<&str>| -> Vec<(char, f64)> {
        row[4]
            .split(';')
            .map(|part| (part.chars().next().unwrap(), f(&part[2..])))
            .collect()
    };
    let nonzero_y = wr
        .iter()
        .filter(|r| writes(*r).iter().any(|(a, v)| *a == 'y' && *v != 0.))
        .count();
    let nonzero_x = wr
        .iter()
        .filter(|r| writes(*r).iter().any(|(a, v)| *a == 'x' && *v != 0.))
        .count();
    assert!(
        nonzero_y >= 100 && nonzero_x >= 150,
        "a sign or axis mix-up flips {nonzero_y}/{nonzero_x} rows"
    );
    for tag in ["L:", "W:", "R"] {
        assert!(
            wr.iter().filter(|r| r[3].starts_with(tag)).count() >= 10,
            "{tag}"
        );
    }
    let angles: std::collections::BTreeSet<_> =
        wr.iter().map(|r| r[2].split('|').nth(3).unwrap()).collect();
    assert!(angles.len() >= 6, "rotations covered: {}", angles.len());
    assert!(
        wr.iter().any(|r| r[2].starts_with("slider"))
            && wr.iter().any(|r| r[2].starts_with("joystick"))
    );
    let clamped = rows("st")
        .iter()
        .chain(rows("sm").iter())
        .filter(|r| r[5].contains("\"size\":[8,") || r[5].contains(",8],"))
        .count();
    assert!(
        clamped >= 8,
        "the size floor of 8 is exercised by {clamped} rows"
    );
    let kinds: std::collections::BTreeSet<_> = rows("cr").iter().map(|r| r[2]).collect();
    assert!(kinds.contains("\"joystick\"") && kinds.contains("\"slider\"") && kinds.len() == 4);
}

#[cfg(not(feature = "evaluation"))]
#[test]
fn without_the_geometry_kernel_a_write_is_a_typed_unavailable_never_a_guess() {
    let mut owner = joystick(None);
    let before = state(&owner);
    let reply = run(
        &mut owner,
        "w",
        "write",
        write_payload(UID, json!(0), local([1., 2.])),
    );
    assert_eq!(code(&reply), "unavailable");
    assert_eq!(state(&owner), before);
}

#[cfg(feature = "evaluation")]
#[test]
fn the_facade_reports_typed_refusals_and_maps_them_to_stable_codes() {
    let owner = joystick(None);
    let snapshot = current(&owner);
    let rest = WidgetIntent::Rest;
    assert_eq!(
        snapshot.widget_write_values("nope", 0., &rest),
        Err(WidgetWriteRefusal::LayerAbsent)
    );
    assert_eq!(
        snapshot.widget_write_values("L0", 0., &rest),
        Err(WidgetWriteRefusal::NotAWidget)
    );
    let writes = snapshot.widget_write_values(UID, 0., &rest).unwrap();
    assert_eq!(
        writes
            .iter()
            .map(|w| (w.key.as_str(), w.value))
            .collect::<Vec<_>>(),
        vec![(key(UID, "x").as_str(), 0.), (key(UID, "y").as_str(), 0.)]
    );
    let mut keyless = broken_document(json!([
        number_control("kx", "Turn"),
        number_control("ky", "Lift")
    ]));
    keyless["layers"][0]["widget"]["y"]["key"] = json!("");
    let snapshot = current(&owner_of(&keyless));
    assert_eq!(
        snapshot.widget_write_values("W", 0., &rest),
        Err(WidgetWriteRefusal::AxisWithoutKey)
    );
    let mut overflow = broken_document(json!([
        number_control("kx", "Turn"),
        number_control("ky", "Lift")
    ]));
    overflow["layers"][0]["widget"]["x"] = json!({"key":"kx","min":-1e308,"max":1e308});
    let snapshot = current(&owner_of(&overflow));
    assert_eq!(
        snapshot.widget_write_values("W", 0., &rest),
        Err(WidgetWriteRefusal::NonFiniteValue)
    );
    for (refusal, code) in [
        (WidgetWriteRefusal::LayerAbsent, "not_found"),
        (WidgetWriteRefusal::NotAWidget, "not_found"),
        (WidgetWriteRefusal::AxisWithoutKey, "unavailable"),
        (WidgetWriteRefusal::KernelUnavailable, "unavailable"),
        (WidgetWriteRefusal::NonFiniteValue, "invalid_request"),
    ] {
        assert_eq!(serde_json::to_value(refusal.code()).unwrap(), json!(code));
    }
}
