//! Hand-computed acceptance cases for the document-level frame edits: what the F6 content copy keeps, what
//! the clear and remove edits drop, how a v2 order scope splits and merges, what is left alone, and the typed
//! refusals. Every expected value is typed by hand from the dense editor's rules, not derived from the kernel.
use super::build::{
    decode, displayed, document, document_json, element_triples, exposure_string, object_order,
    render_layer, scope_shape, Build,
};
use super::tests::parse_layers;
use super::{
    clear_cells, clear_key, demote_span, insert_frame, promote_cells, remove_frames, DocumentEdit,
    FrameEditError,
};
use crate::object_document::{LayerTiming, ObjectDocument, Presence};
use serde_json::{json, Value};

fn doc(text: &str, build: Build) -> ObjectDocument {
    document(&parse_layers(text), build)
}
fn json_of(text: &str, build: Build) -> Value {
    document_json(&parse_layers(text), build)
}
fn changed(edit: Result<DocumentEdit, FrameEditError>) -> ObjectDocument {
    match edit.unwrap() {
        DocumentEdit::Changed(next) => next,
        DocumentEdit::NoChange => panic!("expected a change"),
    }
}
fn refused(edit: Result<DocumentEdit, FrameEditError>) -> FrameEditError {
    edit.expect_err("expected a refusal")
}

#[test]
fn held_promote_copies_the_source_cell_exactly_and_appends_it() {
    for build in [Build::V1, Build::V2] {
        let input = doc("K=a.1.10,b.2.20;.;.", build);
        let out = changed(promote_cells(&input, "L0", &[2]));
        assert_eq!(object_order(&out), ["L0:a@0", "L0:b@0", "L0:a@2", "L0:b@2"]);
        for (source, copy) in [(0usize, 2usize), (1, 3)] {
            let mut want = out.objects[source].clone();
            want.target.frame_scope.frame = 2;
            assert_eq!(
                out.objects[copy], want,
                "{build:?}: id, geometry and fill survive"
            );
        }
        assert_eq!(render_layer(&out, 0), "K=a.1.10,b.2.20;.;K=a.1.10,b.2.20");
        assert_eq!(exposure_string(&out, 0), "*0*");
        if build.v2() {
            assert_eq!(scope_shape(&out), ["0[L0:a,L0:b]", "2[L0:a,L0:b]"]);
            let (a, b) = (("a".to_owned(), 0, 1), ("b".to_owned(), 1, 2));
            assert_eq!(
                element_triples(&out, "L0", 2),
                vec![a, b],
                "natural position and order stay"
            );
        }
    }
}

#[test]
fn in_between_promote_keeps_its_own_records_and_its_manual_flag_goes() {
    for build in [Build::V1, Build::V2] {
        let input = doc("K=a.1.10;M=s.2.20;.", build);
        let out = changed(promote_cells(&input, "L0", &[1]));
        assert_eq!(
            out.objects, input.objects,
            "{build:?}: nothing is copied or moved"
        );
        assert_eq!(render_layer(&out, 0), "K=a.1.10;K=s.2.20;.");
        assert_eq!(exposure_string(&out, 0), "**1");
    }
}

#[test]
fn a_blank_source_and_a_cell_before_every_key_copy_nothing() {
    let blank = doc("K;.;K=a.1.10", Build::V2);
    let out = changed(promote_cells(&blank, "L0", &[1]));
    assert_eq!(out.objects, blank.objects);
    assert_eq!(render_layer(&out, 0), "K;K;K=a.1.10");
    let early = doc(".;K=a.1.10;.", Build::V2);
    let out = changed(promote_cells(&early, "L0", &[0]));
    assert_eq!(out.objects, early.objects);
    assert_eq!(render_layer(&out, 0), "K;K=a.1.10;.");
    assert_eq!(exposure_string(&out, 0), "**1");
    // a cell that already is a key changes nothing at all
    assert_eq!(
        promote_cells(&early, "L0", &[1]),
        Ok(DocumentEdit::NoChange)
    );
}

#[test]
fn promote_replaces_a_stale_record_stored_at_a_held_cell() {
    for build in [Build::V1, Build::V2] {
        let mut value = json_of("K=a.1.10;.", build);
        let stale = super::build::record_json("scene-root", "authored", "L0", "a", 1, 9, 99);
        value["objects"].as_array_mut().unwrap().push(stale);
        if build.v2() {
            let element =
                json!({"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{"static":[9]}});
            let scopes = value["compositionOrder"]["contexts"][0]["scopes"]
                .as_array_mut()
                .unwrap();
            scopes.push(json!({"frameScope":{"kind":"authored","frame":1},"elements":[element]}));
        }
        let input = decode(&value);
        assert_eq!(
            render_layer(&input, 0),
            "K=a.1.10;.=a.9.99",
            "the stale record is stored at a hold"
        );
        let out = changed(promote_cells(&input, "L0", &[1]));
        assert_eq!(
            render_layer(&out, 0),
            "K=a.1.10;K=a.1.10",
            "{build:?}: one record, the copy"
        );
        assert_eq!(out.objects.len(), 2);
        if build.v2() {
            assert_eq!(element_triples(&out, "L0", 1), vec![("a".to_owned(), 0, 1)]);
        }
    }
}

#[test]
fn selection_order_decides_what_a_hold_shows() {
    let input = doc("K=a.1.10;I=s.2.20;.", Build::V1);
    let forward = changed(promote_cells(&input, "L0", &[1, 2]));
    assert_eq!(render_layer(&forward, 0), "K=a.1.10;K=s.2.20;K=s.2.20");
    let backward = changed(promote_cells(&input, "L0", &[2, 1]));
    assert_eq!(render_layer(&backward, 0), "K=a.1.10;K=s.2.20;K=a.1.10");
    assert_eq!(displayed(&backward, 0), "a;s;a");
}

#[test]
fn clear_key_drops_records_and_elements_and_keeps_the_emptied_scope() {
    let input = doc("K=a.1.10;K=b.2.20|K=c.3.30;K=d.4.40", Build::V2);
    let out = changed(clear_key(&input, "L0", 1));
    assert_eq!(render_layer(&out, 0), "K=a.1.10;.");
    assert_eq!(
        render_layer(&out, 1),
        "K=c.3.30;K=d.4.40",
        "the other layer is untouched"
    );
    assert_eq!(scope_shape(&out), ["0[L0:a,L1:c]", "1[L1:d]"]);
    let single = doc("K=a.1.10;K=b.2.20", Build::V2);
    let out = changed(clear_key(&single, "L0", 1));
    assert_eq!(
        scope_shape(&out),
        ["0[L0:a]", "1[]"],
        "the blank-key shape: the scope stays, empty"
    );
    let first = changed(clear_key(&single, "L0", 0));
    assert_eq!(render_layer(&first, 0), ".;K=b.2.20");
    assert_eq!(exposure_string(&first, 0), "-*");
    let held = doc("K=a.1.10;.", Build::V2);
    assert_eq!(clear_key(&held, "L0", 1), Ok(DocumentEdit::NoChange));
}

#[test]
fn clear_cells_and_remove_tween_drop_in_between_records_and_keep_the_endpoints() {
    let input = doc("K=a.1.10;I=s.2.20;M=t.3.30;K=b.4.40", Build::V2);
    let tween = changed(demote_span(&input, "L0", 0, 3));
    assert_eq!(render_layer(&tween, 0), "K=a.1.10;.;.;K=b.4.40");
    assert_eq!(exposure_string(&tween, 0), "*00*");
    assert_eq!(
        scope_shape(&tween),
        ["0[L0:a]", "1[]", "2[]", "3[L0:b]"],
        "emptied scopes stay"
    );
    let cleared = changed(clear_cells(&input, "L0", &[0, 2]));
    assert_eq!(render_layer(&cleared, 0), ".;I=s.2.20;.;K=b.4.40");
    assert_eq!(exposure_string(&cleared, 0), "-~-*");
    let held = doc("K=a.1.10;.", Build::V2);
    assert_eq!(clear_cells(&held, "L0", &[1]), Ok(DocumentEdit::NoChange));
    assert_eq!(demote_span(&held, "L0", 0, 1), Ok(DocumentEdit::NoChange));
}

#[test]
fn insert_frame_splits_a_shared_scope_and_merges_into_an_existing_one() {
    let input = doc("K=a.1.10;.;K=b.2.20|K=c.3.30;K=d.4.40;.", Build::V2);
    assert_eq!(scope_shape(&input), ["0[L0:a,L1:c]", "1[L1:d]", "2[L0:b]"]);
    let first = changed(insert_frame(&input, 0, &["L0"]));
    assert_eq!(render_layer(&first, 0), "K=a.1.10;.;.;K=b.2.20");
    assert_eq!(
        render_layer(&first, 1),
        "K=c.3.30;K=d.4.40;.;.",
        "an untargeted layer is padded at the end"
    );
    assert_eq!(scope_shape(&first), ["0[L0:a,L1:c]", "1[L1:d]", "3[L0:b]"]);
    assert_eq!(first.total_frames, 4);
    let second = changed(insert_frame(&input, 0, &["L1"]));
    assert_eq!(render_layer(&second, 1), "K=c.3.30;.;K=d.4.40;.");
    assert_eq!(render_layer(&second, 0), "K=a.1.10;.;K=b.2.20;.");
    assert_eq!(
        scope_shape(&second),
        ["0[L0:a,L1:c]", "2[L1:d,L0:b]"],
        "d lands on the scope b already has"
    );
    let both = changed(insert_frame(&input, 1, &["L0", "L1"]));
    assert_eq!(scope_shape(&both), ["0[L0:a,L1:c]", "1[L1:d]", "3[L0:b]"]);
}

#[test]
fn an_element_less_scope_moves_only_with_the_document_wide_map() {
    let mut value = json_of("K=a.1.10;.;.;.|K;.;.;.", Build::V2);
    let scopes = value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap();
    scopes.push(json!({"frameScope":{"kind":"authored","frame":2},"elements":[]}));
    let input = decode(&value);
    assert_eq!(scope_shape(&input), ["0[L0:a]", "2[]"]);
    let all = changed(insert_frame(&input, 0, &["L0", "L1"]));
    assert_eq!(scope_shape(&all), ["0[L0:a]", "3[]"]);
    let partial = changed(insert_frame(&input, 0, &["L0"]));
    assert_eq!(scope_shape(&partial), ["0[L0:a]", "2[]"]);
    assert_eq!(
        scope_shape(&changed(remove_frames(&input, 1, 1))),
        ["0[L0:a]", "1[]"]
    );
    assert_eq!(
        scope_shape(&changed(remove_frames(&input, 2, 2))),
        ["0[L0:a]"]
    );
}

#[test]
fn remove_frames_drops_the_span_shifts_the_rest_and_never_empties_the_timeline() {
    let input = doc("K=a.1.10;.;K=b.2.20;.", Build::V2);
    let head = changed(remove_frames(&input, 0, 1));
    assert_eq!(render_layer(&head, 0), "K=b.2.20;.");
    assert_eq!(scope_shape(&head), ["0[L0:b]"]);
    assert_eq!(head.total_frames, 2);
    let middle = changed(remove_frames(&input, 1, 2));
    assert_eq!(render_layer(&middle, 0), "K=a.1.10;.");
    assert_eq!(scope_shape(&middle), ["0[L0:a]"]);
    assert_eq!(remove_frames(&input, 0, 3), Ok(DocumentEdit::NoChange));
}

#[test]
fn reference_records_other_contexts_and_policy_never_move_and_a_stranded_one_refuses() {
    let input = doc("K=a.1.10;.;K=b.2.20|K;.;.", Build::V2Extras);
    for out in [
        changed(insert_frame(&input, 0, &["L0", "L1"])),
        changed(remove_frames(&input, 0, 1)),
    ] {
        let frames: Vec<(String, u32)> = out
            .objects
            .iter()
            .filter(|r| r.target.stroke_id.starts_with('r') || r.target.stroke_id == "x0")
            .map(|r| (r.target.stroke_id.clone(), r.target.frame_scope.frame))
            .collect();
        assert_eq!(
            frames,
            [
                ("r0".to_owned(), 0),
                ("r1".to_owned(), 0),
                ("x0".to_owned(), 0)
            ]
        );
    }
    let mut value = json_of("K=a.1.10;.;.;.", Build::V1Ref);
    let stranded = super::build::record_json("scene-root", "reference", "L0", "far", 3, 1, 5);
    value["objects"].as_array_mut().unwrap().push(stranded);
    let reference = decode(&value);
    assert_eq!(
        refused(remove_frames(&reference, 0, 0)),
        FrameEditError::PreservedOutOfRange
    );
    let mut value = json_of("K=a.1.10;.;.;.", Build::V2Extras);
    value["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .for_each(|r| {
            if r["target"]["strokeId"] == "x0" {
                r["target"]["frameScope"]["frame"] = json!(3);
            }
        });
    value["compositionOrder"]["contexts"][1]["scopes"][0]["frameScope"]["frame"] = json!(3);
    let other = decode(&value);
    assert_eq!(
        refused(remove_frames(&other, 0, 0)),
        FrameEditError::PreservedOutOfRange
    );
    assert!(insert_frame(&other, 0, &["L0"]).is_ok());
}

#[test]
fn property_holders_stay_byte_identical_and_a_vanishing_holder_target_refuses() {
    let mut value = json_of("K=a.1.10;.;K=b.2.20", Build::V2);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
        "properties":[{"property":"position","static":[3,4]}]}]);
    let input = decode(&value);
    let bytes = serde_json::to_string(&input.property_holders).unwrap();
    for out in [
        changed(insert_frame(&input, 0, &["L0"])),
        changed(remove_frames(&input, 1, 1)),
        changed(promote_cells(&input, "L0", &[1])),
        changed(clear_key(&input, "L0", 2)),
    ] {
        assert_eq!(serde_json::to_string(&out.property_holders).unwrap(), bytes);
    }
    // stroke a lives only at frame 0: clearing it would leave the holder pointing at nothing
    assert_eq!(refused(clear_key(&input, "L0", 0)), FrameEditError::Invalid);
    let copied = changed(promote_cells(&input, "L0", &[1]));
    assert!(
        clear_key(&copied, "L0", 0).is_ok(),
        "the copy at frame 1 still carries stroke a"
    );
}

#[test]
fn refusals_are_typed_and_nothing_is_published() {
    let input = doc("K=a.1.10;.;K=b.2.20", Build::V2);
    let before = input.clone();
    assert_eq!(
        refused(promote_cells(&input, "nope", &[1])),
        FrameEditError::UnknownLayer
    );
    assert_eq!(
        refused(insert_frame(&input, 0, &["nope"])),
        FrameEditError::UnknownLayer
    );
    assert_eq!(
        refused(insert_frame(&input, 3, &["L0"])),
        FrameEditError::Invalid
    );
    assert_eq!(
        refused(insert_frame(&input, 0, &[])),
        FrameEditError::Invalid
    );
    assert_eq!(
        refused(promote_cells(&input, "L0", &[1, 3])),
        FrameEditError::Invalid
    );
    assert_eq!(
        refused(remove_frames(&input, 2, 1)),
        FrameEditError::Invalid
    );
    assert_eq!(
        refused(remove_frames(&input, 0, 3)),
        FrameEditError::Invalid
    );
    assert_eq!(refused(clear_key(&input, "L0", 3)), FrameEditError::Invalid);
    assert_eq!(
        refused(demote_span(&input, "L0", 1, 3)),
        FrameEditError::Invalid
    );
    assert_eq!(input, before);
    // a layer with no timing record has no defined exposure
    let mut value = json_of("K=a.1.10;.;K=b.2.20", Build::V1);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L1"}));
    let bare = decode(&value);
    assert_eq!(
        refused(remove_frames(&bare, 0, 0)),
        FrameEditError::MissingTiming
    );
    assert_eq!(
        refused(insert_frame(&bare, 0, &["L1"])),
        FrameEditError::MissingTiming
    );
    assert_eq!(
        refused(promote_cells(&bare, "L1", &[0])),
        FrameEditError::MissingTiming
    );
    let padded = changed(insert_frame(&bare, 0, &["L0"]));
    assert!(
        matches!(padded.layers[1].timing, Presence::Missing),
        "an untimed, untargeted layer stays as it is"
    );
    // another timing family is Unsupported, never edited
    let mut foreign = input.clone();
    let family = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"component",
        "contextId":"scene-root","keys":[],"samples":[]});
    foreign.layers[0].timing =
        Presence::Present(serde_json::from_value::<LayerTiming>(family).unwrap());
    assert_eq!(
        refused(insert_frame(&foreign, 0, &["L0"])),
        FrameEditError::Unsupported
    );
    assert_eq!(
        refused(remove_frames(&foreign, 0, 0)),
        FrameEditError::Unsupported
    );
}

#[test]
fn the_entry_cap_refuses_a_promote_without_truncating() {
    let keys: Vec<Value> = (0..4096).map(|frame| json!({"frame":frame})).collect();
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys,"samples":[]});
    let value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4097,
        "layers":[{"layerUid":"L0","timing":timing}],"objects":[]});
    let input = decode(&value);
    let before = input.clone();
    assert_eq!(
        refused(promote_cells(&input, "L0", &[4096])),
        FrameEditError::Invalid
    );
    assert_eq!(input, before);
}

#[test]
fn survivors_keep_their_order_and_ids_that_sort_backwards_stay_in_document_order() {
    let input = doc("K=z9.1.10,a1.2.20;.|K=m.3.30;.", Build::V1);
    let copied = changed(promote_cells(&input, "L0", &[1]));
    assert_eq!(
        object_order(&copied),
        ["L0:z9@0", "L0:a1@0", "L1:m@0", "L0:z9@1", "L0:a1@1"]
    );
    let moved = changed(insert_frame(&input, 0, &["L0"]));
    assert_eq!(object_order(&moved), ["L0:z9@0", "L0:a1@0", "L1:m@0"]);
    let late = doc("K=z9.1.10;K=a1.2.20|K=m.3.30;.", Build::V1);
    let moved = changed(insert_frame(&late, 0, &["L0"]));
    assert_eq!(object_order(&moved), ["L0:z9@0", "L0:a1@2", "L1:m@0"]);
}
