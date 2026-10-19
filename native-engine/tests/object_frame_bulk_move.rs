//! Original moveFrames/shiftLayerFrames controls: hand-computed source cells,
//! checkpoint distinctions and sealed existing-owner publication. No history/UI model.
// Same-lexical fixture/support extraction preserves original test names and bodies.
include!("object_frame_bulk_move_fixture.rs");
#[test]
fn overlapping_move_captures_all_sources_before_clear_and_replaces_destination_cells() {
    for form in build::Build::ALL {
        let input = document("K=a.1.10;K=b.2.20;K=c.3.30;.;.", form);
        let mut o = owner(input.clone());
        let out = commit(&mut o, moving(&[("L0", 0), ("L0", 1)], "L0", 1));
        assert_rows(&input, &out, ".;K=a.1.10;K=b.2.20;.;.", form);
        assert_eq!(o.content_revision(), 1);
    }
}

#[test]
fn absolute_bbox_anchor_preserves_unselected_cells_without_rectangle_ripple() {
    let input = document(
        "K=a.1.10;.;K=b.2.20;.;K=c.3.30;.|K=x.4.40;.;K=y.5.50;.;K=z.6.60;.",
        build::Build::V2Extras,
    );
    let mut o = owner(input.clone());
    let out = commit(&mut o, moving(&[("L0", 2), ("L1", 2)], "L0", 3));
    assert_rows(
        &input,
        &out,
        "K=a.1.10;.;.;K=b.2.20;K=c.3.30;.|K=x.4.40;.;.;K=y.5.50;K=z.6.60;.",
        build::Build::V2Extras,
    );
}

#[test]
fn source_and_destination_locks_filter_before_checkpoint_but_clipped_sources_are_removed() {
    let mut input = document(
        "K=a.1.10;.;.;.|K=b.2.20;.;.;.|K=c.3.30;.;.;.",
        build::Build::V2,
    );
    input.layers[1].locked = Presence::Present(true);
    let mut o = owner(input.clone());
    let rejected = prepare(&o, moving(&[("L0", 0), ("L1", 0)], "L1", 1)).unwrap();
    assert!(!rejected.checkpoint_required());
    assert!(!rejected.document_changed());
    assert_eq!(o.commit_bulk_frame_edit(rejected).unwrap(), 0);
    assert_eq!(value(&head(&o)), value(&input));
    let candidate = prepare(&o, moving(&[("L0", 0), ("L1", 0), ("L2", 0)], "L1", 1)).unwrap();
    assert!(candidate.checkpoint_required());
    assert!(candidate.document_changed());
    assert_eq!(
        selection(&candidate),
        Some(Vec::new()),
        "only unlocked source is clipped off layer range"
    );
    o.commit_bulk_frame_edit(candidate).unwrap();
    assert_rows(
        &input,
        &head(&o),
        "K=a.1.10;.;.;.|K=b.2.20;.;.;.|.;.;.;.",
        build::Build::V2,
    );
}

#[test]
fn multi_cell_frame_crop_drops_out_of_range_contents_after_clearing_sources() {
    let input = document("K=z.1.10;K=a.2.20;K=b.3.30;.", build::Build::V1Ref);
    let mut o = owner(input.clone());
    let candidate = prepare(&o, moving(&[("L0", 1), ("L0", 2)], "L0", 3)).unwrap();
    assert_eq!(selection(&candidate), Some(vec![("L0".into(), 3)]));
    o.commit_bulk_frame_edit(candidate).unwrap();
    assert_rows(
        &input,
        &head(&o),
        "K=z.1.10;.;.;K=a.2.20",
        build::Build::V1Ref,
    );
}

#[test]
fn single_key_move_ripples_later_keys_and_reports_actual_clamped_pairs() {
    let input = document("K=a.1.10;.;K=b.2.20;.;K=c.3.30;.", build::Build::V2);
    let mut o = owner(input.clone());
    let candidate = prepare(&o, moving(&[("L0", 2)], "L0", 4)).unwrap();
    assert_eq!(
        selection(&candidate),
        Some(vec![("L0".into(), 3), ("L0".into(), 5)])
    );
    assert_eq!(
        pairs(&candidate),
        vec![("L0".into(), 0, 2, 0, 3), ("L0".into(), 2, 4, 3, 5)]
    );
    o.commit_bulk_frame_edit(candidate).unwrap();
    assert_rows(
        &input,
        &head(&o),
        "K=a.1.10;.;.;K=b.2.20;.;K=c.3.30",
        build::Build::V2,
    );
}

#[test]
fn zero_ripple_clamp_and_byte_noop_still_checkpoint_but_original_same_anchor_does_not() {
    let input = document("K=a.1.10;.;K=b.2.20;.;K=c.3.30", build::Build::V1);
    let mut o = owner(input.clone());
    let before = bytes(&o);
    for revision in 1..=2 {
        let candidate = prepare(&o, moving(&[("L0", 2)], "L0", 3)).unwrap();
        assert!(candidate.checkpoint_required());
        assert!(!candidate.document_changed());
        assert_eq!(o.commit_bulk_frame_edit(candidate).unwrap(), revision);
        assert_eq!(bytes(&o), before);
    }
    let same = prepare(&o, moving(&[("L0", 2)], "L0", 2)).unwrap();
    assert!(!same.checkpoint_required());
    assert!(!same.document_changed());
    assert_eq!(o.commit_bulk_frame_edit(same).unwrap(), 2);
    let plain = prepare(&o, moving(&[("L0", 1)], "L0", 3)).unwrap();
    assert!(plain.checkpoint_required());
    assert!(!plain.document_changed());
    assert_eq!(o.commit_bulk_frame_edit(plain).unwrap(), 3);
    let empty = prepare(&o, moving(&[], "L0", 0)).unwrap();
    assert!(!empty.checkpoint_required());
    assert_eq!(o.commit_bulk_frame_edit(empty).unwrap(), 3);
}

#[test]
fn shift_moves_only_drawn_cells_retains_whole_sample_kind_and_discards_blank_timing_keys() {
    for form in build::Build::ALL {
        let input = document("K=a.1.10;K;M=b.2.20;.;N=c.3.30;I=d.4.40;K", form);
        let mut o = owner(input.clone());
        let candidate = prepare(&o, shifting("L0", 1)).unwrap();
        assert_eq!(candidate.command(), "object.layer.frames.shift");
        assert!(!candidate.checkpoint_required());
        assert!(candidate.document_changed());
        assert!(candidate.shifted());
        assert_eq!(selection(&candidate), None);
        assert_eq!(
            pairs(&candidate),
            vec![("L0".into(), 0, 1, 1, 2), ("L0".into(), 1, 6, 2, 7)]
        );
        o.commit_bulk_frame_edit(candidate).unwrap();
        assert_rows(
            &input,
            &head(&o),
            ".;K=a.1.10;.;M=b.2.20;.;N=c.3.30;I=d.4.40",
            form,
        );
    }
}

#[test]
fn negative_shift_keeps_signed_offedge_arc_pairs_and_does_not_clamp_metadata() {
    let input = document(
        "K=a.1.10;K;M=b.2.20;.;N=c.3.30;I=d.4.40;K",
        build::Build::V2Extras,
    );
    let mut o = owner(input.clone());
    let candidate = prepare(&o, shifting("L0", -2)).unwrap();
    assert_eq!(
        pairs(&candidate),
        vec![("L0".into(), 0, 1, -2, -1), ("L0".into(), 1, 6, -1, 4)]
    );
    assert!(candidate.shifted());
    o.commit_bulk_frame_edit(candidate).unwrap();
    assert_rows(
        &input,
        &head(&o),
        "M=b.2.20;.;N=c.3.30;I=d.4.40;.;.;.",
        build::Build::V2Extras,
    );
}

#[test]
fn shift_zero_locked_and_empty_drawing_have_distinct_return_and_publication_semantics() {
    let input = document(".;.;.;.", build::Build::V1);
    let mut o = owner(input.clone());
    let zero = prepare(&o, shifting("L0", 0)).unwrap();
    assert!(!zero.shifted());
    assert!(!zero.document_changed());
    assert!(!zero.checkpoint_required());
    assert_eq!(o.commit_bulk_frame_edit(zero).unwrap(), 0);
    let empty = prepare(&o, shifting("L0", 2)).unwrap();
    assert!(empty.shifted());
    assert!(!empty.document_changed());
    assert!(!empty.checkpoint_required());
    assert_eq!(o.commit_bulk_frame_edit(empty).unwrap(), 0);
    let mut locked = input;
    locked.layers[0].locked = Presence::Present(true);
    let mut o = owner(locked.clone());
    let candidate = prepare(&o, shifting("L0", 1)).unwrap();
    assert!(!candidate.shifted());
    assert_eq!(o.commit_bulk_frame_edit(candidate).unwrap(), 0);
    assert_eq!(value(&head(&o)), value(&locked));
}

#[test]
fn moved_key_retimes_dense_and_sparse_spans_with_original_manual_presence_rules() {
    for (before, from, to, expected) in [
        (
            "K=a.1.10;M=b.2.20;K=c.3.30;.;K=d.4.40;.",
            2,
            3,
            "K=a.1.10;I=b.2.20;M=b.2.20;K=c.3.30;.;K=d.4.40",
        ),
        (
            "K=a.1.10;M=b.2.20;.;.;K=c.3.30;.;K=d.4.40;.",
            4,
            5,
            "K=a.1.10;M=b.2.20;.;.;.;K=c.3.30;.;K=d.4.40",
        ),
    ] {
        for form in build::Build::ALL {
            let input = document(before, form);
            let mut o = owner(input.clone());
            let out = commit(&mut o, moving(&[("L0", from)], "L0", to));
            assert_rows(&input, &out, expected, form);
        }
    }
}

#[test]
fn malformed_duplicate_unknown_missing_and_unsafe_inputs_never_change_the_owner() {
    let o = owner(document("K=a.1.10;.;.;K=b.2.20", build::Build::V2Extras));
    let before = bytes(&o);
    for payload in fixture::malformed_payloads() {
        assert_eq!(
            prepare(&o, payload).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        assert_eq!(bytes(&o), before);
        assert_eq!(o.content_revision(), 0);
    }
    let raw = moving(&[("L0", 0)], "L0", 1).to_string();
    for malformed in [
        raw.replace("\"frame\":1", "\"frame\":1,\"frame\":1"),
        raw.replace("\"selection\":", "\"selec\\u0074ion\":[],\"selection\":"),
        raw.replace("\"frame\":0", "\"frame\":0,\"frame\":0"),
    ] {
        assert_ne!(malformed, raw);
        assert_eq!(
            o.prepare_bulk_frame_edit_json(malformed.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        assert_eq!(bytes(&o), before);
    }
    for payload in [
        moving(&[("L0", 0)], "absent", 1),
        moving(&[("absent", 0)], "L0", 1),
        shifting("absent", 1),
    ] {
        assert_eq!(
            prepare(&o, payload).unwrap_err(),
            DispatchErrorCode::NotFound
        );
        assert_eq!(bytes(&o), before);
    }
    assert_eq!(
        prepare(&o, moving(&[("L0", 4)], "L0", 1)).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(bytes(&o), before);
}

#[test]
fn private_json_payload_bound_is_inclusive_without_claiming_a_larger_public_envelope() {
    let o = owner(document("K=a.1.10;.;.;.", build::Build::V1));
    let before = bytes(&o);
    let mut payload = shifting("missing", 1);
    let padding = 4096 - payload.to_string().len();
    payload["layerUid"] = json!(format!("missing{}", "x".repeat(padding)));
    assert_eq!(payload.to_string().len(), 4096);
    assert_eq!(
        prepare(&o, payload.clone()).unwrap_err(),
        DispatchErrorCode::NotFound,
        "bounded valid payload reaches actual UID lookup"
    );
    payload["layerUid"] = json!(payload["layerUid"].as_str().unwrap().to_owned() + "x");
    assert_eq!(payload.to_string().len(), 4097);
    assert_eq!(
        prepare(&o, payload).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(bytes(&o), before);
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn sealed_candidates_reject_other_document_instance_and_stale_snapshot_without_publication() {
    let input = document("K=a.1.10;.;.;K=b.2.20", build::Build::V1Ref);
    let mut a = owner(input.clone());
    let mut b = owner(input.clone());
    let candidate = prepare(&a, shifting("L0", 1)).unwrap();
    let before = bytes(&b);
    assert_eq!(
        b.commit_bulk_frame_edit(candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(bytes(&b), before);
    let candidate = prepare(&a, shifting("L0", 1)).unwrap();
    let mut foreign = ObjectFillOwner::new("foreign-bulk", input.clone()).unwrap();
    assert_eq!(
        foreign.commit_bulk_frame_edit(candidate).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(value(&head(&foreign)), value(&input));
    let first = prepare(&a, shifting("L0", 1)).unwrap();
    let stale = prepare(&a, shifting("L0", -1)).unwrap();
    let retained = a.acquire_snapshot(0).unwrap();
    a.commit_bulk_frame_edit(first).unwrap();
    let changed = bytes(&a);
    assert_eq!(
        a.commit_bulk_frame_edit(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(bytes(&a), changed);
    assert_eq!(value(retained.document()), value(&input));
}

#[test]
fn absent_timing_real_component_and_orphaned_holder_refuse_without_erasing_preserved_data() {
    let mut untimed = document("K=a.1.10;.;.;.", build::Build::V1);
    untimed.layers[0].timing = Presence::Missing;
    let o = owner(untimed);
    let before = bytes(&o);
    assert_eq!(
        prepare(&o, shifting("L0", 1)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(bytes(&o), before);
    let o = owner(f7::component(false));
    let before = bytes(&o);
    for payload in [shifting("L1", 1), moving(&[("L1", 3)], "L1", 2)] {
        assert_eq!(
            prepare(&o, payload).unwrap_err(),
            DispatchErrorCode::Unavailable
        );
        assert_eq!(bytes(&o), before);
    }
    let o = owner(fixture::with_holder(
        document("K=a.1.10;.;.;.", build::Build::V1),
        true,
    ));
    let before = bytes(&o);
    assert_eq!(
        prepare(&o, shifting("L0", -1)).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(bytes(&o), before);
    assert_eq!(o.content_revision(), 0);
}

#[test]
fn content_shift_preserves_motion_holder_schedule_for_the_later_atomic_caller_join() {
    let input = fixture::with_holder(
        document("K=a.1.10;.;K=b.2.20;.", build::Build::V2Extras),
        false,
    );
    let mut o = owner(input.clone());
    let out = commit(&mut o, shifting("L0", 1));
    assert_rows(
        &input,
        &out,
        ".;K=a.1.10;.;K=b.2.20",
        build::Build::V2Extras,
    );
    assert_eq!(
        value(&out)["propertyHolders"],
        value(&input)["propertyHolders"]
    );
}

#[test]
fn cross_layer_move_preserves_authored_order_instead_of_rebuilding_it_from_record_order() {
    let mut input = value(&document(
        "K=a.1.10,b.2.20;.;.;.|K=z.3.30;.;.;.",
        build::Build::V2,
    ));
    let scope = &mut input["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    scope.as_array_mut().unwrap().swap(0, 1);
    let input = admit(input);
    let source = value(&input)["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|element| element["layerUid"] == "L0")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        source
            .iter()
            .map(|v| v["strokeId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["b", "a"]
    );
    let mut o = owner(input);
    let out = commit(&mut o, moving(&[("L0", 0)], "L1", 1));
    let v = value(&out);
    let scope = v["compositionOrder"]["contexts"][0]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scope| scope["frameScope"]["frame"] == 1)
        .unwrap();
    let mut expected = source;
    for element in &mut expected {
        element["layerUid"] = json!("L1");
    }
    assert_eq!(scope["elements"], json!(expected));
}

#[cfg(feature = "project_media_codec")]
#[test]
fn raster_text_targets_and_exact_order_move_together_without_touching_unrelated_items_or_contexts()
{
    let input = f7::mixed_items();
    let mut o = owner(input.clone());
    let out = commit(&mut o, moving(&[("L0", 0)], "L1", 1));
    let expected = fixture::expected_mixed_move(&input);
    assert_eq!(
        fixture::canonical(value(&out)),
        fixture::canonical(expected)
    );
}

#[test]
fn sparse_shift_at_u32_timeline_bound_moves_one_cell_without_dense_timeline_materialization() {
    let mut raw = value(&document("K=a.1.10;.;.", build::Build::V2Extras));
    raw["totalFrames"] = json!(u32::MAX);
    let input = admit(raw);
    let mut o = owner(input.clone());
    let retained = o.acquire_snapshot(0).unwrap();
    let candidate = prepare(&o, shifting("L0", 1)).unwrap();
    assert!(candidate.shifted());
    assert!(candidate.document_changed());
    o.commit_bulk_frame_edit(candidate).unwrap();
    let out = value(&head(&o));
    let mut expected = value(&input);
    expected["layers"][0]["timing"]["keys"] = json!([{"frame":1}]);
    let target = expected["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| {
            record["target"]["contextId"] == "scene-root"
                && record["target"]["frameScope"]["kind"] == "authored"
        })
        .unwrap();
    target["target"]["frameScope"]["frame"] = json!(1);
    expected["compositionOrder"]["contexts"][0]["scopes"][0]["frameScope"]["frame"] = json!(1);
    assert_eq!(fixture::canonical(out), fixture::canonical(expected));
    assert_eq!(value(retained.document()), value(&input));
}

#[cfg(feature = "project_media_codec")]
#[test]
fn empty_raster_or_text_tables_are_rejected_by_real_storage_before_any_move_owner_exists() {
    let input = f7::mixed_items();
    let o = owner(input.clone());
    let before = bytes(&o);
    let retained = o.acquire_snapshot(0).unwrap();
    for kind in ["raster", "text"] {
        let raw = fixture::empty_item_table_wire(kind);
        let source = serde_json::to_vec(&raw).unwrap();
        let error = crate::object_codec::decode_ordered_project(&source).unwrap_err();
        assert_eq!(
            error.kind(),
            crate::object_codec::ObjectCodecErrorKind::Invalid
        );
        assert_eq!(
            error.message(),
            "an items container must hold at least one entry"
        );
        assert!(crate::object_codec::decode_storage_project(&source).is_err());
        let typed: ObjectDocument = serde_json::from_value(raw.clone()).unwrap();
        assert!(ObjectFillOwner::new("empty-items-rejected", typed).is_err());
        assert_eq!(serde_json::to_vec(&raw).unwrap(), source);
        assert_eq!(bytes(&o), before);
        assert_eq!(value(retained.document()), value(&input));
        assert_eq!(o.content_revision(), 0);
    }
}
