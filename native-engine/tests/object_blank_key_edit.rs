//! Real sealed F7 owner tests; every expected cell/timing/outcome is hand-written.
use super::*;
use crate::commands::ObjectFillOwner;
use crate::object_document::Presence;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_blank_key_edit_build.rs"]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
use fixture::{admit, bytes, document, expected_blank, head, owner, payload, value};

fn blank(owner: &mut ObjectFillOwner, layer: &str, cell: u32) -> Result<u64, DispatchErrorCode> {
    let candidate = owner.prepare_blank_key_json(&payload(layer, cell))?;
    assert_eq!(candidate.command(), "object.layer.key.blank");
    owner.commit_blank_key(candidate)
}

#[test]
fn key_held_sample_manual_unexposed_and_blank_cells_become_empty_keys_in_all_four_forms() {
    let cases = [
        ("K=a.1.10;M=m.2.20;.;K=b.3.30", 0, "K;M=m.2.20;.;K=b.3.30"),
        ("K=a.1.10;M=m.2.20;.;K=b.3.30", 1, "K=a.1.10;K;.;K=b.3.30"),
        ("K=a.1.10;I=m.2.20;.;K=b.3.30", 1, "K=a.1.10;K;.;K=b.3.30"),
        (
            "K=a.1.10;M=m.2.20;.;K=b.3.30",
            2,
            "K=a.1.10;M=m.2.20;K;K=b.3.30",
        ),
        (".;.;K=a.1.10;.", 0, "K;.;K=a.1.10;."),
        ("K;.;.;K=b.3.30", 0, "K;.;.;K=b.3.30"),
    ];
    for form in build::Build::ALL {
        for (source, cell, expected) in cases {
            let mut input = document(&format!("{source}|K=z.4.40;.;.;K=q.5.50"), form);
            input.property_holders = Presence::Present(
                serde_json::from_value(json!([
                {"target":{"contextId":"scene-root","layerUid":"L0"},
                    "properties":[{"property":"rotation","static":[17]}]}]))
                .unwrap(),
            );
            let mut o = owner(input.clone());
            let before = bytes(&o);
            let retained = o.acquire_snapshot(0).unwrap();
            let prepared = o.prepare_blank_key_json(&payload("L0", cell)).unwrap();
            assert_eq!(bytes(&o), before, "prepare is detached");
            assert_eq!(o.commit_blank_key(prepared).unwrap(), 1);
            assert_eq!(
                value(&head(&o)),
                expected_blank(&input, 0, cell, expected),
                "{source}/{cell}/{form:?}"
            );
            assert_eq!(build::render_layer(&head(&o), 0), expected);
            assert_eq!(
                crate::object_codec::encode_storage_project(retained.document()).unwrap(),
                before
            );
            let saved = bytes(&o);
            assert_eq!(
                value(&admit(serde_json::from_slice(&saved).unwrap())),
                value(&head(&o))
            );
        }
    }
}

#[test]
fn repeated_success_on_an_already_blank_key_still_publishes_each_checkpoint() {
    let mut o = owner(document("K;.;.;K|K=x.1.10;.;.;.", build::Build::V2Extras));
    let before = bytes(&o);
    assert_eq!(blank(&mut o, "L0", 0), Ok(1));
    assert_eq!(bytes(&o), before);
    assert_eq!(blank(&mut o, "L0", 0), Ok(2));
    assert_eq!(bytes(&o), before);
}

#[test]
fn malformed_wire_and_exact_payload_budget_refuse_without_any_publication() {
    let o = owner(document("K=a.1.10;.;.;K|K;.;.;.", build::Build::V1));
    let good = String::from_utf8(payload("L0", 1)).unwrap();
    let before = bytes(&o);
    let mut invalid = vec![
        "null".into(),
        "[]".into(),
        "{}".into(),
        good.replace("object.layer.key.blank", "object.layer.key.promote"),
        good.replace("\"cell\":1", "\"cell\":1,\"cell\":2"),
        good.replace(
            "\"layerUid\":\"L0\"",
            "\"layerUid\":\"L0\",\"layerUid\":\"L1\"",
        ),
        good.replace(
            "\"command\":\"object.layer.key.blank\"",
            "\"command\":\"object.layer.key.blank\",\"command\":\"object.layer.key.blank\"",
        ),
        good.replace("\"cell\":1", "\"cell\":null"),
        good.replace("\"cell\":1", "\"cell\":1.5"),
        good.replace("\"cell\":1", "\"cell\":-1"),
        good.replace("\"cell\":1", "\"cell\":4294967296"),
        good.replace("\"L0\"", "\"\""),
        good.replace("\"L0\"", "null"),
        good.replace('}', ",\"layerUids\":[\"L0\"]}"),
        good.replace('}', ",\"extra\":true}"),
    ];
    for field in ["command", "cell", "layerUid"] {
        let mut v: Value = serde_json::from_str(&good).unwrap();
        v.as_object_mut().unwrap().remove(field);
        invalid.push(v.to_string());
    }
    let mut at_limit = good.clone();
    at_limit.push_str(&" ".repeat(MAX_PAYLOAD_BYTES - at_limit.len()));
    assert!(o.prepare_blank_key_json(at_limit.as_bytes()).is_ok());
    invalid.push(at_limit + " ");
    for raw in invalid {
        assert_eq!(
            o.prepare_blank_key_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{raw}"
        );
    }
    assert_eq!(bytes(&o), before);
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn locked_missing_timing_absent_layer_and_out_of_range_refuse_without_changes() {
    let base = document("K=a.1.10;.;.;K|K;.;.;.", build::Build::V2);
    for (change, layer, cell, error) in [
        ("locked", "L0", 1, DispatchErrorCode::Unavailable),
        ("untimed", "L0", 1, DispatchErrorCode::Unavailable),
        ("none", "missing", 1, DispatchErrorCode::NotFound),
        ("none", "L0", 4, DispatchErrorCode::InvalidRequest),
    ] {
        let mut input = base.clone();
        if change == "locked" {
            input.layers[0].locked = Presence::Present(true);
        }
        if change == "untimed" {
            input.layers[0].timing = Presence::Missing;
        }
        let mut o = owner(input);
        let before = bytes(&o);
        assert_eq!(blank(&mut o, layer, cell), Err(error));
        assert_eq!(bytes(&o), before);
        assert_eq!(o.content_revision(), 0);
    }
    let mut unsupported = value(&base);
    unsupported["layers"][0]["timing"]["family"] = json!("unsupported-family");
    assert!(
        crate::object_codec::decode_storage_project(unsupported.to_string().as_bytes()).is_err(),
        "unsupported timing cannot enter the owner as an admitted baseline"
    );
}

#[test]
fn prepared_candidates_refuse_other_instance_document_and_stale_revision_without_publication() {
    let input = document("K=a.1.10;.;.;K|K;.;.;.", build::Build::V1Ref);
    let mut a = owner(input.clone());
    let mut b = owner(input.clone());
    let prepared = a.prepare_blank_key_json(&payload("L0", 1)).unwrap();
    let before = bytes(&b);
    assert_eq!(
        b.commit_blank_key(prepared).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(bytes(&b), before);
    let prepared = a.prepare_blank_key_json(&payload("L0", 1)).unwrap();
    let mut foreign = ObjectFillOwner::new("another-instance", input).unwrap();
    assert_eq!(
        foreign.commit_blank_key(prepared).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    let first = a.prepare_blank_key_json(&payload("L0", 1)).unwrap();
    let stale = a.prepare_blank_key_json(&payload("L0", 2)).unwrap();
    a.commit_blank_key(first).unwrap();
    let after = bytes(&a);
    assert_eq!(
        a.commit_blank_key(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(bytes(&a), after);
}

#[test]
fn real_locked_component_blank_override_is_held_then_next_normal_key_resumes() {
    for cell in [0, 1] {
        let input = fixture::component(true);
        let mut o = owner(input.clone());
        assert_eq!(blank(&mut o, "L1", cell), Ok(1));
        let out = head(&o);
        let mut expected = expected_blank(
            &input,
            1,
            cell,
            if cell == 0 { "K;.;.;K;K" } else { "K;K;.;K;K" },
        );
        expected["componentTables"]["components"]["instances"][0]["frameKeys"] = if cell == 0 {
            json!([{"frame":0,"blankOverride":true},{"frame":4,"componentFrame":2,"blankOverride":false}])
        } else {
            json!([{"frame":0,"componentFrame":1},{"frame":1,"blankOverride":true},
            {"frame":4,"componentFrame":2,"blankOverride":false}])
        };
        assert_eq!(value(&out), expected);
        #[cfg(feature = "evaluation")]
        {
            use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::playback_input;
            use crate::evaluation::object_component_kernels::object_component_playback::{evaluate_component_playback,PlaybackOutcome};
            let instance = out
                .component_set()
                .unwrap()
                .instance("scene-root", "L1")
                .unwrap();
            for frame in 0..5 {
                let playback = playback_input(instance, &out.layers[1], frame).unwrap();
                assert_eq!(
                    matches!(
                        evaluate_component_playback(&playback, 3.0, i64::from(frame), None),
                        PlaybackOutcome::Blank
                    ),
                    frame >= cell && frame < 3,
                    "actual held-outer-key consumer at {frame}"
                );
            }
        }
    }
}

#[test]
fn a_full_valid_component_key_table_allows_replacement_but_refuses_a_new_entry() {
    let mut v = value(&fixture::component(false));
    v["totalFrames"] = json!(8192);
    v["layers"][1]["timing"]["keys"] =
        Value::Array((0..4096).map(|i| json!({"frame":i*2})).collect());
    v["componentTables"]["components"]["instances"][0]["frameKeys"] = Value::Array(
        (0..4096)
            .map(|i| json!({"frame":i*2,"componentFrame":0}))
            .collect(),
    );
    let mut o = owner(admit(v));
    let before = bytes(&o);
    assert_eq!(
        blank(&mut o, "L1", 1),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(bytes(&o), before);
    assert_eq!(blank(&mut o, "L1", 0), Ok(1));
    let out = value(&head(&o));
    let keys = out["componentTables"]["components"]["instances"][0]["frameKeys"]
        .as_array()
        .unwrap();
    assert_eq!(keys.len(), 4096);
    assert_eq!(keys[0], json!({"frame":0,"blankOverride":true}));
}

#[test]
fn targeted_raster_text_and_order_leave_together_while_other_items_and_contexts_survive() {
    let input = fixture::mixed_items();
    let mut o = owner(input.clone());
    assert_eq!(blank(&mut o, "L0", 0), Ok(1));
    let out = head(&o);
    let mut expected = expected_blank(&input, 0, 0, "K;.;.;K=b.2.20");
    expected["items"]["raster"] = json!([value(&input)["items"]["raster"][1]]);
    expected["items"]["text"] = json!([value(&input)["items"]["text"][1]]);
    assert_eq!(value(&out), expected);
    assert_eq!(blank(&mut o, "L1", 3), Ok(2));
    let mut final_expected = expected_blank(&out, 1, 3, "K=c.3.30;.;.;K");
    final_expected.as_object_mut().unwrap().remove("items");
    assert_eq!(
        value(&head(&o)),
        final_expected,
        "last item normalizes the whole container to absence"
    );
}

#[test]
fn an_element_holder_survives_when_another_authored_frame_still_owns_the_stroke() {
    let mut input = document(
        "K=a.1.10;.;.;K=a.1.10|K=z.2.20;.;.;.",
        build::Build::V2Extras,
    );
    input.property_holders=Presence::Present(serde_json::from_value(json!([
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
            "properties":[{"property":"rotation","track":{"keys":[{"frame":0,"v":[11]},{"frame":3,"v":[22]}]}}]}
    ])).unwrap());
    let mut o = owner(input.clone());
    assert_eq!(blank(&mut o, "L0", 0), Ok(1));
    assert_eq!(
        value(&head(&o)),
        expected_blank(&input, 0, 0, "K;.;.;K=a.1.10")
    );
    assert_eq!(head(&o).property_holders, input.property_holders);
}

#[test]
fn blanking_the_last_vector_record_with_an_element_holder_refuses_without_orphaning_it() {
    let mut input = document(
        "K=a.1.10;.;.;K=b.2.20|K=z.2.20;.;.;.",
        build::Build::V2Extras,
    );
    input.property_holders = Presence::Present(
        serde_json::from_value(json!([
            {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
                "properties":[{"property":"rotation","static":[17]}]}
        ]))
        .unwrap(),
    );
    let mut o = owner(input);
    let before = bytes(&o);
    let retained = o.acquire_snapshot(0).unwrap();
    assert_eq!(blank(&mut o, "L0", 0), Err(DispatchErrorCode::Unavailable));
    assert_eq!(bytes(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert!(o.acquire_snapshot(1).is_none());
    assert_eq!(
        crate::object_codec::encode_storage_project(retained.document()).unwrap(),
        before
    );
}

#[test]
fn genuine_no_timing_components_author_outer_timing_only_on_the_f7_command() {
    for locked in [false, true] {
        for empty_keys in [false, true] {
            for cell in [0, 2] {
                let mut source = value(&fixture::component(locked));
                source["layers"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("timing");
                let instance = &mut source["componentTables"]["components"]["instances"][0];
                if empty_keys {
                    instance["frameKeys"] = json!([]);
                } else {
                    instance.as_object_mut().unwrap().remove("frameKeys");
                }
                let input = admit(source);
                let mut o = owner(input.clone());
                let before = bytes(&o);
                let prepared = o.prepare_blank_key_json(&payload("L1", cell)).unwrap();
                assert_eq!(
                    bytes(&o),
                    before,
                    "prepare cannot seed the source's missing timing"
                );
                assert_eq!(o.commit_blank_key(prepared).unwrap(), 1);
                let out = head(&o);
                let mut expected = value(&input);
                expected["layers"][1]["timing"] = build::timing_json(
                    &rows::parse_layers(if cell == 0 { "K;.;.;.;." } else { "K;.;K;.;." })[0],
                );
                expected["componentTables"]["components"]["instances"][0]["frameKeys"] = json!([
                    {"frame":cell,"blankOverride":true}]);
                assert_eq!(
                    value(&out),
                    expected,
                    "locked={locked}, empty={empty_keys}, cell={cell}"
                );
                assert_eq!(out.layers[1].locked, input.layers[1].locked);
                assert_eq!(
                    crate::object_codec::encode_storage_project(&input).unwrap(),
                    before
                );
                #[cfg(feature = "evaluation")]
                {
                    use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::playback_input;
                    use crate::evaluation::object_component_kernels::object_component_playback::{evaluate_component_playback,PlaybackOutcome};
                    let before_instance = input
                        .component_set()
                        .unwrap()
                        .instance("scene-root", "L1")
                        .unwrap();
                    let after_instance = out
                        .component_set()
                        .unwrap()
                        .instance("scene-root", "L1")
                        .unwrap();
                    for frame in 0..5 {
                        let before_play =
                            playback_input(before_instance, &input.layers[1], frame).unwrap();
                        assert!(!matches!(
                            evaluate_component_playback(&before_play, 3.0, i64::from(frame), None),
                            PlaybackOutcome::Blank
                        ));
                        let after_play =
                            playback_input(after_instance, &out.layers[1], frame).unwrap();
                        assert_eq!(
                            matches!(
                                evaluate_component_playback(
                                    &after_play,
                                    3.0,
                                    i64::from(frame),
                                    None
                                ),
                                PlaybackOutcome::Blank
                            ),
                            frame >= cell,
                            "actual component playback at {frame}"
                        );
                    }
                }
            }
        }
    }
}
