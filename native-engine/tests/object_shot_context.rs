//! Tests for the montage view context (L10-I, I18/I19/I20/I36/I42). The scenario replay compares the
//! production kernel, driven through the real `StoryboardRecord` / `ComponentSet` join, with the
//! verbatim baseline `enterMontageView` / `exitMontageView` (see `rows`); the remaining tests pin
//! literals read from that oracle and the typed refusals the baseline never had.
use super::rows::{js_number, mismatches, rows, segments_of, worlds, Deviation, ORACLE};
use super::{
    admit_shot_source, enter_from_records, EnterOutcome, ExitOutcome, OwnedFields,
    ShotContextError, ShotRefusal, ShotSource, ShotWorkspace, StoredOwned, ViewScalars, WriteBack,
};
use crate::evaluation::object_montage_layer::MontageLayerError;
use crate::evaluation::object_montage_timing::{
    ChainMember, MontageTimingError, Retime, SymbolView,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use serde_json::{json, Value};

fn scene_view() -> ViewScalars {
    ViewScalars {
        current_frame: 7,
        total_frames: 24,
        wa_in: 2,
        wa_out: 20,
        active_layer: 1,
        fps: 12.0,
    }
}

fn scene_owned() -> OwnedFields {
    OwnedFields {
        markers: json!(["m_a", "m_b"]),
        motion_arcs: json!({"z": 1}),
        tween_overrides: json!({"y": 1}),
        tween_easing: json!({"x": 1}),
        camera_keys: json!(["c_a"]),
    }
}

fn workspace() -> ShotWorkspace {
    ShotWorkspace::new(scene_view(), scene_owned())
}

fn enter(
    ws: &mut ShotWorkspace,
    component_open: bool,
    world: &str,
    stored: &StoredOwned,
    id: &str,
) -> EnterOutcome {
    let all = worlds();
    let w = all.iter().find(|w| w.id == world).expect("world");
    enter_from_records(ws, component_open, &w.storyboard, &w.components, stored, id)
        .expect("admissible")
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(
        ORACLE.contains("app.js 7898f76780c2f335f8a4a1d56f6383ef57db01b71e4ef876eabf09c8188163ca")
    );
    assert_eq!(rows("W").len(), 9);
    assert_eq!(rows("X").len(), 472);
    let mut scenarios: Vec<&str> = rows("X").iter().map(|r| r[1]).collect();
    scenarios.dedup();
    assert_eq!(scenarios.len(), 92);
    assert_eq!(rows("X").iter().filter(|r| r[3] == "0").count(), 92);
}

#[test]
fn every_baseline_scenario_step_is_reproduced() {
    let bad = mismatches(Deviation::default());
    assert!(
        bad.is_empty(),
        "{} mismatching steps, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn entering_swaps_in_the_montage_view_and_exiting_restores_the_scene_exactly() {
    let mut ws = workspace();
    let outcome = enter(&mut ws, false, "plain", &StoredOwned::default(), "mt");
    let entered = match outcome {
        EnterOutcome::Entered(entered) => entered,
        other => panic!("expected the view, got {other:?}"),
    };
    assert_eq!(entered.montage_id, "mt");
    assert_eq!(entered.spec.total_frames, 35);
    let seen: Vec<(String, String, String, i64, i64, i64, i64)> = entered
        .spec
        .segments
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                s.symbol_id.clone(),
                js_number(s.sym_speed),
                s.sym_placed_at,
                s.sym_trim_in,
                s.sym_trim_out,
                s.out_point,
            )
        })
        .collect();
    let expected = [
        ("Walk \u{2014} montage", "sa", "1", 0, 0, 11, 11),
        ("Run \u{2014} montage", "sb", "1", 12, 2, 9, 19),
        ("Idle \u{2014} montage", "sc", "0.4", 20, 0, 3, 29),
        ("Run \u{2014} montage", "sb", "2.5", 30, 0, 9, 33),
        ("Walk \u{2014} montage", "sa", "1", 34, 3, 3, 34),
    ];
    assert_eq!(seen.len(), expected.len());
    for (got, want) in seen.iter().zip(expected) {
        assert_eq!(
            (
                got.0.as_str(),
                got.1.as_str(),
                got.2.as_str(),
                got.3,
                got.4,
                got.5,
                got.6
            ),
            want
        );
    }
    assert!(entered
        .spec
        .segments
        .iter()
        .all(|s| s.locked && s.visible && s.sym_play_mode == "once" && s.sym_single_frame == 0));
    assert_eq!(
        *ws.view(),
        ViewScalars {
            current_frame: 0,
            total_frames: 35,
            wa_in: 0,
            wa_out: 34,
            active_layer: 0,
            fps: 12.0
        }
    );
    assert_eq!(*ws.owned(), OwnedFields::empty());
    assert_eq!(ws.active_montage_view(), Some("mt"));
    ws.view_mut().current_frame = 9;
    ws.view_mut().fps = 30.0;
    ws.owned_mut()
        .markers
        .as_array_mut()
        .expect("markers")
        .push(json!("inside"));
    let exit = ws.exit(false, true);
    assert_eq!(
        exit,
        ExitOutcome::Exited {
            write_back: Some(WriteBack {
                montage_id: "mt".to_string(),
                owned: OwnedFields {
                    markers: json!(["inside"]),
                    ..OwnedFields::empty()
                },
            })
        }
    );
    assert_eq!(*ws.view(), scene_view());
    assert_eq!(*ws.owned(), scene_owned());
    assert_eq!(ws.active_montage_view(), None);
}

#[test]
fn stored_montage_fields_are_kept_when_truthy_and_defaulted_when_falsy() {
    let stored = StoredOwned {
        markers: Some(json!([])),
        motion_arcs: Some(json!(false)),
        tween_overrides: Some(json!(0)),
        tween_easing: Some(json!("")),
        camera_keys: Some(Value::Null),
    };
    assert_eq!(stored.resolved(), OwnedFields::empty());
    let kept = StoredOwned {
        markers: Some(json!(["a"])),
        motion_arcs: Some(json!({"k": 1})),
        tween_overrides: Some(json!(1)),
        tween_easing: Some(json!("x")),
        camera_keys: Some(json!([])),
    };
    let resolved = kept.resolved();
    assert_eq!(resolved.markers, json!(["a"]));
    assert_eq!(resolved.motion_arcs, json!({"k": 1}));
    assert_eq!(resolved.tween_overrides, json!(1));
    assert_eq!(resolved.tween_easing, json!("x"));
    assert_eq!(resolved.camera_keys, json!([]));
    assert_eq!(StoredOwned::default().resolved(), OwnedFields::empty());
}

#[test]
fn what_is_written_inside_the_view_never_reaches_the_scene_fields() {
    let mut ws = workspace();
    let stored = StoredOwned {
        camera_keys: Some(json!(["mk_1"])),
        ..StoredOwned::default()
    };
    enter(&mut ws, false, "plain", &stored, "mt");
    assert_eq!(ws.owned().camera_keys, json!(["mk_1"]));
    ws.owned_mut().camera_keys = json!(["replaced"]);
    ws.owned_mut()
        .markers
        .as_array_mut()
        .expect("markers")
        .push(json!("x"));
    assert_eq!(
        ws.exit(false, true),
        ExitOutcome::Exited {
            write_back: Some(WriteBack {
                montage_id: "mt".to_string(),
                owned: OwnedFields {
                    markers: json!(["x"]),
                    camera_keys: json!(["replaced"]),
                    ..OwnedFields::empty()
                },
            }),
        }
    );
    assert_eq!(*ws.owned(), scene_owned());
}

#[test]
fn a_deleted_montage_gets_no_write_back_but_the_scene_is_restored() {
    let mut ws = workspace();
    enter(&mut ws, false, "plain", &StoredOwned::default(), "mt");
    ws.owned_mut().markers = json!(["lost"]);
    assert_eq!(
        ws.exit(false, false),
        ExitOutcome::Exited { write_back: None }
    );
    assert_eq!(*ws.owned(), scene_owned());
    assert_eq!(*ws.view(), scene_view());
}

fn untouched(ws: &ShotWorkspace, inside: Option<&str>) {
    assert_eq!(ws.active_montage_view(), inside);
    if inside.is_none() {
        assert_eq!(*ws.view(), scene_view());
        assert_eq!(*ws.owned(), scene_owned());
    }
}

#[test]
fn the_guards_run_in_the_baseline_order_and_refusals_change_nothing() {
    let none = StoredOwned::default();
    let mut ws = workspace();
    // A component first, even before the montage lookup that would be a silent no-op.
    assert!(matches!(
        enter(&mut ws, true, "plain", &none, "zzz"),
        EnterOutcome::Refused(ShotRefusal::CloseComponentFirst)
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "zzz"),
        EnterOutcome::NoOp
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "i1"),
        EnterOutcome::NoOp
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "mt3"),
        EnterOutcome::Refused(ShotRefusal::EmptyMontage)
    ));
    assert!(matches!(
        enter(&mut ws, false, "nochain", &none, "mt"),
        EnterOutcome::Refused(ShotRefusal::EmptyMontage)
    ));
    assert!(matches!(
        enter(&mut ws, false, "dangling", &none, "mt2"),
        EnterOutcome::Refused(ShotRefusal::EmptyMontage)
    ));
    untouched(&ws, None);
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "mt"),
        EnterOutcome::Entered(_)
    ));
    let inside_view = ws.view().clone();
    let inside_owned = ws.owned().clone();
    // Inside: the component flag still wins, then the same view is a no-op, another view a refusal.
    assert!(matches!(
        enter(&mut ws, true, "plain", &none, "mt"),
        EnterOutcome::Refused(ShotRefusal::CloseComponentFirst)
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "mt"),
        EnterOutcome::NoOp
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "mt2"),
        EnterOutcome::Refused(ShotRefusal::CloseMontageFirst)
    ));
    assert!(matches!(
        enter(&mut ws, false, "plain", &none, "zzz"),
        EnterOutcome::Refused(ShotRefusal::CloseMontageFirst)
    ));
    assert_eq!(
        ws.exit(true, true),
        ExitOutcome::Refused(ShotRefusal::CloseComponentFirst)
    );
    untouched(&ws, Some("mt"));
    assert_eq!(*ws.view(), inside_view);
    assert_eq!(*ws.owned(), inside_owned);
    assert!(matches!(ws.exit(false, true), ExitOutcome::Exited { .. }));
    assert_eq!(ws.exit(false, true), ExitOutcome::NoOp);
    assert_eq!(ws.exit(true, true), ExitOutcome::NoOp);
    untouched(&ws, None);
}

#[test]
fn refusal_keys_are_the_baseline_message_keys() {
    assert_eq!(
        ShotRefusal::CloseComponentFirst.toast_key(),
        Some("toastCloseComponentFirst")
    );
    assert_eq!(
        ShotRefusal::CloseMontageFirst.toast_key(),
        Some("toastCloseMontageFirst")
    );
    assert_eq!(
        ShotRefusal::EmptyMontage.toast_key(),
        Some("hsMontageEmpty")
    );
    assert_eq!(ShotRefusal::NotAComponent.toast_key(), None);
    assert_eq!(ShotRefusal::FlatLayer.toast_key(), None);
}

fn big_member(id: &str, duration: i64) -> ChainMember {
    ChainMember {
        module_id: id.to_string(),
        symbol_id: "sa".to_string(),
        retime: Retime {
            trim_in: 0,
            trim_out: 0,
            duration,
        },
        symbol: SymbolView::Missing,
    }
}

#[test]
fn a_timing_overflow_is_typed_and_leaves_the_workspace_untouched() {
    let mut ws = workspace();
    let members = [big_member("a", i64::MAX), big_member("b", 1)];
    let stored = StoredOwned::default();
    let got = ws.enter(false, "mt", Some((&members[..], &stored)));
    assert!(matches!(got, Err(MontageTimingError::Overflow)));
    untouched(&ws, None);
    // A one-frame view is the smallest legal one: the work area is [0, 0].
    let one = [big_member("a", 1)];
    assert!(matches!(
        ws.enter(false, "mt", Some((&one[..], &stored))),
        Ok(EnterOutcome::Entered(_))
    ));
    assert_eq!(
        *ws.view(),
        ViewScalars {
            current_frame: 0,
            total_frames: 1,
            wa_in: 0,
            wa_out: 0,
            active_layer: 0,
            fps: 12.0
        }
    );
}

#[test]
fn a_record_the_join_cannot_read_is_a_typed_error_after_the_guards() {
    let all = worlds();
    let w = all.iter().find(|w| w.id == "plain").expect("world");
    let mut value = serde_json::to_value(&w.storyboard).expect("encode");
    let at = value["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .position(|m| m["id"] == "i2")
        .expect("i2");
    value["modules"][at]["duration"] = json!(2.5);
    let storyboard = serde_json::from_value(value).expect("decode");
    let mut ws = workspace();
    let none = StoredOwned::default();
    let guarded = enter_from_records(&mut ws, true, &storyboard, &w.components, &none, "mt");
    assert!(matches!(
        guarded,
        Ok(EnterOutcome::Refused(ShotRefusal::CloseComponentFirst))
    ));
    let got = enter_from_records(&mut ws, false, &storyboard, &w.components, &none, "mt");
    assert_eq!(
        got.err(),
        Some(ShotContextError::Montage(MontageLayerError::NonInteger(
            "duration"
        )))
    );
    untouched(&ws, None);
    let other = enter_from_records(&mut ws, false, &storyboard, &w.components, &none, "mt2");
    assert!(matches!(other, Ok(EnterOutcome::Entered(_))));
}

#[test]
fn segment_text_is_rendered_from_the_spec_only() {
    let mut ws = workspace();
    let outcome = enter(&mut ws, false, "zerodur", &StoredOwned::default(), "mt");
    let entered = match outcome {
        EnterOutcome::Entered(entered) => entered,
        other => panic!("expected the view, got {other:?}"),
    };
    let text = segments_of(&entered);
    assert_eq!(text.split(';').count(), 3);
    // The zero-duration member keeps the baseline's Infinity speed and its empty window [10, 9].
    assert!(text
        .split(';')
        .nth(1)
        .expect("segment")
        .contains(",once,Infinity,10,0,4,0,10,9,1,1,20,1"));
}

fn components_with_instance(base: &ComponentSet, layer_uid: &str, symbol: &str) -> ComponentSet {
    let mut value = serde_json::to_value(base).expect("encode");
    value["instances"] =
        json!([{"contextId": "scene-root", "layerUid": layer_uid, "symbolId": symbol}]);
    serde_json::from_value(value).expect("decode")
}

#[test]
fn a_shot_is_a_component_and_a_flat_layer_is_refused() {
    let all = worlds();
    let w = all.iter().find(|w| w.id == "plain").expect("world");
    assert_eq!(
        admit_shot_source(&w.components, ShotSource::Symbol("sa")),
        Ok("sa".to_string())
    );
    assert_eq!(
        admit_shot_source(&w.components, ShotSource::Symbol("flat-layer")),
        Err(ShotRefusal::NotAComponent)
    );
    let flat = ShotSource::InstanceLayer {
        context_id: "scene-root",
        layer_uid: "ly_1",
    };
    assert_eq!(
        admit_shot_source(&w.components, flat),
        Err(ShotRefusal::FlatLayer)
    );
    let converted = components_with_instance(&w.components, "ly_1", "sb");
    assert_eq!(admit_shot_source(&converted, flat), Ok("sb".to_string()));
    let elsewhere = ShotSource::InstanceLayer {
        context_id: "scene-root",
        layer_uid: "ly_2",
    };
    assert_eq!(
        admit_shot_source(&converted, elsewhere),
        Err(ShotRefusal::FlatLayer)
    );
}
