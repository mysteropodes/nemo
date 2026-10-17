//! Hand-computed cases for the Repeat Selection document kernel (`repeat_cells`) and its arithmetic. Every expected
//! document is written out as dense cell text (`K` key, `I`/`M`/`N` in-between, `.` hold, `=id.z.g` strokes) from the
//! baseline rules, not derived from the code under test. The oracle rows that compare the whole transaction against the
//! verbatim baseline are in object_frame_clipboard_repeat_history.rs.
use super::*;
use serde_json::json;

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_repeat_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{
    document, document_json, element_triples, exposure_string, render_layer, scope_shape, Build,
};
use rows::parse_layers;

fn request(from: usize, to: usize, first: u32, last: u32, times: i64) -> RepeatRequest {
    RepeatRequest {
        from_layer: build::uid(from),
        to_layer: build::uid(to),
        from_frame: first,
        to_frame: last,
        times,
    }
}
fn doc(text: &str, form: Build) -> ObjectDocument {
    document(&parse_layers(text), form)
}
/// `Some(document)` when the cycle changed it, `None` for a no-op.
fn cycle(
    document: &ObjectDocument,
    request: &RepeatRequest,
) -> Result<Option<ObjectDocument>, DispatchErrorCode> {
    repeat_cells(document, request).map(|edit| match edit {
        DocumentEdit::Changed(next) => Some(next),
        DocumentEdit::NoChange => None,
    })
}
fn changed(document: &ObjectDocument, request: &RepeatRequest) -> ObjectDocument {
    cycle(document, request)
        .unwrap()
        .expect("the cycle changes the document")
}

#[test]
fn a_cycle_copies_keys_in_betweens_and_holds_grows_the_timeline_and_pads_the_other_layers() {
    // Frames 0..=1 of L0 twice: needed = 1 + 1 + 2 * 2 = 6 > 3. The manual in-between copies as an in-between WITHOUT
    // its flag; the hold at frame 2 is replaced by the first copy. L1 is not selected, so it only gains holds.
    let input = doc("K=a.1.10;M=b.2.20;.|K=z.1.5;.;.", Build::V1);
    let out = changed(&input, &request(0, 0, 0, 1, 2));
    assert_eq!(out.total_frames, 6);
    assert_eq!(
        render_layer(&out, 0),
        "K=a.1.10;M=b.2.20;K=a.1.10;I=b.2.20;K=a.1.10;I=b.2.20"
    );
    assert_eq!(exposure_string(&out, 0), "*!*~*~");
    assert_eq!(render_layer(&out, 1), "K=z.1.5;.;.;.;.;.");
    assert_eq!(exposure_string(&out, 1), "*00000");
}

#[test]
fn destinations_are_replaced_whole_keys_in_betweens_and_their_strokes_included() {
    // 0..=1 once on a 5-frame layer: frame 2 (key b) becomes a copy of the key, frame 3 (in-between c) becomes the hold.
    let input = doc("K=a.1.10;.;K=b.2.20;I=c.3.30;.", Build::V1);
    let out = changed(&input, &request(0, 0, 0, 1, 1));
    assert_eq!(out.total_frames, 5, "needed 1 + 1 + 2 = 4 fits");
    assert_eq!(render_layer(&out, 0), "K=a.1.10;.;K=a.1.10;.;.");
    assert_eq!(exposure_string(&out, 0), "*0*22");
    assert!(
        out.objects
            .iter()
            .all(|record| record.target.stroke_id == "a"),
        "the replaced strokes b and c are gone"
    );
}

#[test]
fn a_blank_key_and_a_hold_cycle_as_a_blank_key_and_a_hold() {
    let out = changed(&doc("K;.", Build::V1), &request(0, 0, 0, 1, 1));
    assert_eq!(render_layer(&out, 0), "K;.;K;.");
    assert_eq!(exposure_string(&out, 0), "*0*2");
    assert!(out.objects.is_empty(), "a blank key copies no record");
}

#[test]
fn times_follows_the_baseline_clamp() {
    let input = doc("K=a.1.10;.;.", Build::V1);
    // 0 and negative counts run once; 50 and above run fifty times.
    for times in [0, -5, i64::MIN, 1] {
        let out = changed(&input, &request(0, 0, 0, 0, times));
        assert_eq!(
            render_layer(&out, 0),
            "K=a.1.10;K=a.1.10;.",
            "times {times}"
        );
        assert_eq!(exposure_string(&out, 0), "**1");
    }
    let fifty = changed(&input, &request(0, 0, 0, 0, 50));
    assert_eq!(fifty.total_frames, 51);
    assert_eq!(
        render_layer(&fifty, 0),
        vec!["K=a.1.10"; 51].join(";"),
        "frame 0 and fifty copies"
    );
    for times in [51, 99, i64::MAX] {
        assert_eq!(
            changed(&input, &request(0, 0, 0, 0, times)),
            fifty,
            "times {times}"
        );
    }
}

#[test]
fn the_lock_flag_is_never_consulted_and_layers_between_the_bounds_are_all_cycled() {
    let mut value = document_json(&parse_layers("K=a.1.10;.|K=b.2.20;.|K=c.3.30;."), Build::V1);
    value["layers"][0]["locked"] = json!(true);
    let input = crate::object_codec::decode_storage_project(value.to_string().as_bytes()).unwrap();
    assert!(input.layers[0].locked(), "L0 is locked");
    // Bounds L0..=L1 over frames 0..=1, once: L2 is outside the bounds and only gains holds.
    let out = changed(&input, &request(0, 1, 0, 1, 1));
    assert_eq!(render_layer(&out, 0), "K=a.1.10;.;K=a.1.10;.");
    assert_eq!(render_layer(&out, 1), "K=b.2.20;.;K=b.2.20;.");
    assert_eq!(render_layer(&out, 2), "K=c.3.30;.;.;.");
    assert!(out.layers[0].locked(), "the layer field survives");
}

#[test]
fn an_ordered_document_copies_the_elements_with_their_natural_positions_and_order() {
    let input = doc("K=a.1.10,b.2.20;.;.", Build::V2);
    let out = changed(&input, &request(0, 0, 0, 1, 1));
    assert_eq!(out.total_frames, 4);
    assert_eq!(scope_shape(&out), vec!["0[L0:a,L0:b]", "2[L0:a,L0:b]"]);
    let triples = vec![("a".to_owned(), 0, 1), ("b".to_owned(), 1, 2)];
    assert_eq!(element_triples(&out, "L0", 2), triples);
    assert_eq!(
        element_triples(&out, "L0", 0),
        triples,
        "the source is untouched"
    );
    assert_eq!(render_layer(&out, 0), "K=a.1.10,b.2.20;.;K=a.1.10,b.2.20;.");
}

#[test]
fn reference_records_and_other_contexts_survive_a_cycle_that_replaces_the_cells_around_them() {
    let input = doc("K=a.1.10;.", Build::V2Extras);
    let out = changed(&input, &request(0, 0, 0, 1, 1));
    assert_eq!(
        build::untouched(&input),
        build::untouched(&out),
        "Reference records, the ctx-x context, the policy and the holders are unchanged"
    );
    assert_eq!(render_layer(&out, 0), "K=a.1.10;.;K=a.1.10;.");
}

#[test]
fn a_cycle_that_writes_what_is_already_there_publishes_nothing() {
    // a hold copied over a hold, inside the existing timeline
    let holds = doc("K=a.1.10;.;.;.", Build::V1);
    assert_eq!(cycle(&holds, &request(0, 0, 1, 1, 1)), Ok(None));
    // a key copied over an identical key
    let same = doc("K=a.1.10;K=a.1.10;.", Build::V1);
    assert_eq!(cycle(&same, &request(0, 0, 0, 0, 1)), Ok(None));
    // the same with another layer's records stored after the destination, in v1 and v2: nothing is rewritten, nothing moves
    for form in [Build::V1, Build::V2] {
        let two = doc("K=a.1.10;K=a.1.10;.|K=z.1.5;.;.", form);
        assert_eq!(cycle(&two, &request(0, 0, 0, 0, 1)), Ok(None), "{form:?}");
    }
    // identical content but a longer timeline is still a change
    let grown = changed(&holds, &request(0, 0, 1, 1, 50));
    assert_eq!(grown.total_frames, 52);
    assert_eq!(
        render_layer(&grown, 0),
        format!("K=a.1.10{}", ";.".repeat(51))
    );
}

#[test]
fn malformed_requests_and_documents_refuse_typed_and_change_nothing() {
    let input = doc("K=a.1.10;.;K=b.2.20|K;.;.", Build::V1);
    let refusal = |from, to, first, last| cycle(&input, &request(from, to, first, last, 1));
    assert_eq!(refusal(0, 1, 0, 2).map(|out| out.is_some()), Ok(true));
    assert_eq!(
        refusal(1, 0, 0, 1),
        Err(DispatchErrorCode::InvalidRequest),
        "layers reversed"
    );
    assert_eq!(
        refusal(0, 0, 2, 1),
        Err(DispatchErrorCode::InvalidRequest),
        "frames reversed"
    );
    assert_eq!(
        refusal(0, 0, 0, 3),
        Err(DispatchErrorCode::InvalidRequest),
        "past the end"
    );
    assert_eq!(
        refusal(0, 5, 0, 1),
        Err(DispatchErrorCode::NotFound),
        "unknown last layer"
    );
    assert_eq!(
        refusal(7, 7, 0, 1),
        Err(DispatchErrorCode::NotFound),
        "unknown first layer"
    );
    // a layer in the bounds that has no timing record is never given a made-up one
    let mut value = document_json(&parse_layers("K=a.1.10;.|K;."), Build::V1);
    value["layers"][1].as_object_mut().unwrap().remove("timing");
    let bare = crate::object_codec::decode_storage_project(value.to_string().as_bytes()).unwrap();
    assert_eq!(
        cycle(&bare, &request(0, 1, 0, 0, 1)),
        Err(DispatchErrorCode::Unavailable)
    );
    let grown = changed(&bare, &request(0, 0, 0, 1, 1));
    assert_eq!(
        grown.total_frames, 4,
        "outside the bounds a layer without timing only gains length"
    );
    assert!(grown.layers[1].timing.value().is_none());
    // a frame counter that cannot hold the result
    let mut huge = doc("K", Build::V1);
    huge.total_frames = u32::MAX;
    assert_eq!(
        cycle(&huge, &request(0, 0, u32::MAX - 1, u32::MAX - 1, 1)),
        Err(DispatchErrorCode::InvalidRequest)
    );
}

#[test]
fn the_new_frame_count_and_the_count_clamp_are_the_baseline_arithmetic() {
    // needed = last + 1 + span * times, never below the current count
    assert_eq!(grown_total(5, 2, 3, 2), Some(8));
    assert_eq!(grown_total(10, 0, 0, 1), Some(10));
    assert_eq!(grown_total(1, 0, 0, 50), Some(51));
    assert_eq!(grown_total(5, 4, 4, 50), Some(55));
    assert_eq!(grown_total(5, 3, 2, 1), None, "an empty range");
    assert_eq!(
        grown_total(5, 0, u32::MAX - 1, 1),
        None,
        "does not fit the counter"
    );
    assert_eq!(
        grown_total(5, u32::MAX - 52, u32::MAX - 52, 50),
        Some(u32::MAX - 1)
    );
    assert_eq!(
        grown_total(5, u32::MAX - 51, u32::MAX - 51, 50),
        Some(u32::MAX)
    );
    assert_eq!(grown_total(5, u32::MAX - 50, u32::MAX - 50, 50), None);
    let clamps = [
        (i64::MIN, 1),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 2),
        (49, 49),
        (50, 50),
        (51, 50),
        (i64::MAX, 50),
    ];
    for (raw, expected) in clamps {
        assert_eq!(clamp_times(raw), expected, "times {raw}");
    }
}
