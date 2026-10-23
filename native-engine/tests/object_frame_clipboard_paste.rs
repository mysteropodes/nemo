//! Document-level cell-write tests for pasteFrames (fbe4b8a3 src/js/timeline.js): hand-computed results on small dense
//! documents, the silent drops, the semantic no-ops and the typed refusals. The oracle rows (verbatim baseline editor)
//! run through the history owner in object_frame_clipboard_paste_history.rs.
use super::*;
use crate::object_document::{ObjectDocument, Presence};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_paste_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document, exposure_string, record_json, render_layer, untouched, Build, Stroke};
use rows::{parse_layers, record_of};

const SMALL: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn cell(rel_layer: u32, rel_frame: u32, keyframe: bool, strokes: &[(&str, i64, i64)]) -> PasteCell {
    PasteCell {
        order_elements: crate::object_document::Presence::Missing,
        rel_layer,
        rel_frame,
        keyframe,
        records: strokes
            .iter()
            .map(|(id, z, g)| {
                record_of(&Stroke {
                    id: (*id).to_owned(),
                    z: *z,
                    g: *g,
                })
            })
            .collect(),
    }
}
/// The published document, or `None` for a semantic no-op.
fn paste(
    document: &ObjectDocument,
    layer: usize,
    frame: u32,
    cells: &[PasteCell],
) -> Result<Option<ObjectDocument>, DispatchErrorCode> {
    Ok(match paste_cells(document, layer, frame, cells)? {
        DocumentEdit::Changed(next) => Some(next),
        DocumentEdit::NoChange => None,
    })
}
fn text(document: &ObjectDocument) -> Vec<String> {
    (0..document.layers.len())
        .map(|layer| render_layer(document, layer))
        .collect()
}
fn pasted(input: &str, layer: usize, frame: u32, cells: &[PasteCell]) -> ObjectDocument {
    paste(&doc(input), layer, frame, cells)
        .unwrap()
        .expect("the paste changes the document")
}

#[test]
fn a_key_cell_replaces_the_target_and_keeps_the_original_stroke_id_on_another_layer() {
    let out = pasted(SMALL, 1, 1, &[cell(0, 0, true, &[("a", 1, 10)])]);
    assert_eq!(text(&out), ["K=a.1.10;.;K=b.2.20;.", "K;K=a.1.10;.;K"]);
    assert_eq!(exposure_string(&out, 1), "**1*");
    // the same id now exists on two layers: a target is keyed by layer too, so storage admission accepts it
    assert!(ObjectSnapshot::validate_drawing(&out).is_ok());
    // a key replaces what the target held, strokes included
    let again = pasted(
        SMALL,
        0,
        0,
        &[cell(0, 0, true, &[("n", 4, 44), ("m", 5, 55)])],
    );
    assert_eq!(text(&again)[0], "K=n.4.44,m.5.55;.;K=b.2.20;.");
}

#[test]
fn a_copied_in_between_lands_as_a_full_key_and_a_blank_key_keeps_no_strokes() {
    let input = "K=a.1.10;I=m.2.20;.;K=c.3.30";
    let out = pasted(
        input,
        0,
        1,
        &[cell(0, 0, true, &[("n", 3, 30)]), cell(0, 2, true, &[])],
    );
    assert_eq!(text(&out), ["K=a.1.10;K=n.3.30;.;K"]);
    assert_eq!(exposure_string(&out, 0), "**1*");
}

#[test]
fn a_plain_cell_clears_a_key_and_an_in_between_to_holds_of_the_previous_key() {
    let input = "K=a.1.10;K=b.2.20;I=m.3.30;K=c.4.40";
    let cells = [
        cell(0, 0, false, &[]),
        cell(0, 1, false, &[]),
        cell(0, 2, false, &[]),
    ];
    let out = pasted(input, 0, 1, &cells);
    assert_eq!(text(&out), ["K=a.1.10;.;.;."]);
    assert_eq!(exposure_string(&out, 0), "*000");
}

#[test]
fn cells_outside_the_layers_or_frames_are_dropped_silently() {
    let cells = [
        cell(0, 0, true, &[("x", 1, 1)]),
        cell(1, 0, true, &[("z", 9, 9)]),
        cell(0, 2, true, &[("w", 8, 8)]),
        cell(0, 1, true, &[("y", 2, 2)]),
    ];
    let out = pasted(SMALL, 1, 2, &cells);
    assert_eq!(text(&out), ["K=a.1.10;.;K=b.2.20;.", "K;.;K=x.1.1;K=y.2.2"]);
}

#[test]
fn identical_empty_and_fully_outside_pastes_are_semantic_no_ops() {
    let document = doc(SMALL);
    let same = [cell(0, 0, true, &[("a", 1, 10)])];
    let outside = [
        cell(5, 0, true, &[("z", 1, 1)]),
        cell(0, 1, true, &[("z", 1, 1)]),
    ];
    let plain_on_hold = [cell(0, 0, false, &[])];
    assert_eq!(
        paste(&document, 0, 0, &same),
        Ok(None),
        "exactly what the cell holds"
    );
    assert_eq!(
        paste(&document, 1, 3, &outside),
        Ok(None),
        "every cell dropped"
    );
    assert_eq!(paste(&document, 0, 0, &[]), Ok(None), "an empty clipboard");
    assert_eq!(
        paste(&document, 0, 1, &plain_on_hold),
        Ok(None),
        "a hold onto a hold"
    );
}

#[test]
fn malformed_cell_sets_refuse_before_any_write() {
    let document = doc(SMALL);
    let foreign = |context: &str, scope: &str| {
        serde_json::from_value(record_json(context, scope, "L9", "q", 0, 1, 1)).unwrap()
    };
    let with_record = |record| PasteCell {
        records: vec![record],
        ..cell(0, 0, true, &[])
    };
    let too_many: Vec<PasteCell> = (0..=MAX_PACKET_CELLS as u32)
        .map(|frame| cell(0, frame, false, &[]))
        .collect();
    let cases: Vec<(&str, Vec<PasteCell>)> = vec![
        (
            "duplicate offsets",
            vec![cell(0, 0, true, &[]), cell(0, 0, false, &[])],
        ),
        (
            "other context",
            vec![with_record(foreign("ctx-x", "authored"))],
        ),
        (
            "reference scope",
            vec![with_record(foreign("scene-root", "reference"))],
        ),
        (
            "plain cell with strokes",
            vec![cell(0, 0, false, &[("a", 1, 1)])],
        ),
        ("over the cell bound", too_many),
    ];
    for (name, cells) in cases {
        assert_eq!(
            paste(&document, 0, 0, &cells),
            Err(DispatchErrorCode::InvalidRequest),
            "{name}"
        );
    }
}

#[test]
fn ordered_cells_require_explicit_order_and_a_layer_without_timing_refuses() {
    let ordered = document(&parse_layers(SMALL), Build::V2);
    let one = [cell(0, 0, true, &[("n", 1, 1)])];
    assert_eq!(
        paste(&ordered, 0, 1, &one),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut untimed = doc(SMALL);
    untimed.layers[1].timing = Presence::Missing;
    assert_eq!(
        paste(&untimed, 1, 1, &one),
        Err(DispatchErrorCode::Unavailable)
    );
    assert!(
        paste(&untimed, 0, 1, &one).unwrap().is_some(),
        "the timed layer is unaffected"
    );
}

#[test]
fn reference_records_and_every_other_cell_survive_a_paste() {
    let input = document(&parse_layers(SMALL), Build::V1Ref);
    let out = paste(&input, 1, 1, &[cell(0, 0, true, &[("a", 1, 10)])])
        .unwrap()
        .unwrap();
    assert_eq!(
        untouched(&input),
        untouched(&out),
        "Reference records are not cell content"
    );
    assert_eq!(text(&out)[0], "K=a.1.10;.;K=b.2.20;.");
    assert_eq!(
        out.total_frames, input.total_frames,
        "a paste never grows the timeline"
    );
}

#[test]
fn ordered_metadata_is_required_exact_and_can_change_without_record_changes() {
    let input = rows::ordered_document(Build::V2Extras);
    let o = crate::commands::ObjectFillOwner::new("ordered", input.clone()).unwrap();
    let packet = o
        .copy_frame_clipboard(&[crate::commands::CellRef {
            layer_uid: "L0".into(),
            frame: 0,
        }])
        .unwrap();
    let cells = crate::commands::packet_paste_cells(&packet);
    assert_eq!(paste(&input, 0, 0, &cells), Ok(None));
    let mut wrong = cells.clone();
    wrong[0].order_elements = Presence::Missing;
    assert_eq!(
        paste(&input, 0, 1, &wrong),
        Err(DispatchErrorCode::InvalidRequest)
    );
    for mutate in [0, 1, 2, 3] {
        let mut wrong = cells.clone();
        let Presence::Present(elements) = &mut wrong[0].order_elements else {
            panic!()
        };
        match mutate {
            0 => {
                elements.pop();
            }
            1 => elements[0].natural_position = 2,
            2 => elements[0].stroke_id = "not-a-record".into(),
            _ => elements[0].natural_position = elements[1].natural_position,
        }
        assert_eq!(
            paste(&input, 0, 1, &wrong),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    let mut changed_order = cells.clone();
    let Presence::Present(elements) = &mut changed_order[0].order_elements else {
        panic!()
    };
    elements[0].order.static_value = Presence::Present(vec![serde_json::Number::from(99)]);
    let out = paste(&input, 0, 0, &changed_order).unwrap().unwrap();
    assert_eq!(
        render_layer(&out, 0),
        render_layer(&input, 0),
        "record content is identical while authored order changes"
    );
    let mut expected = rows::ordered_elements("L0");
    expected[0]["order"]["static"] = serde_json::json!([99]);
    assert_eq!(rows::scoped_elements(&out, "L0", 0), expected);
    assert_eq!(untouched(&out), untouched(&input));
    assert_eq!(paste(&input, 9, 0, &cells), Ok(None));
    assert_eq!(paste(&input, 0, input.total_frames, &cells), Ok(None));
    let legacy = doc(SMALL);
    assert_eq!(
        paste(&legacy, 0, 1, &cells),
        Err(DispatchErrorCode::InvalidRequest)
    );
}

#[test]
fn ordered_key_manual_sample_held_and_blank_normalize_through_real_copy_and_paste() {
    use crate::commands::{packet_paste_cells, CellRef, ObjectFillOwner};
    use serde_json::json;
    for form in [Build::V2, Build::V2Extras] {
        let mut value =
            build::document_json(&parse_layers("K=a.1.10;M=b.2.20;.;K|K=z.3.30;.;.;."), form);
        let a = json!({"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{"static":[7]}});
        let b = json!({"layerUid":"L0","strokeId":"b","naturalPosition":0,"order":{"static":[-4]}});
        for scope in value["compositionOrder"]["contexts"][0]["scopes"]
            .as_array_mut()
            .unwrap()
        {
            for element in scope["elements"].as_array_mut().unwrap() {
                if element["layerUid"] == "L0" {
                    *element = if element["strokeId"] == "a" {
                        a.clone()
                    } else {
                        b.clone()
                    };
                }
            }
        }
        let input =
            crate::object_codec::decode_storage_project(value.to_string().as_bytes()).unwrap();
        let frozen = serde_json::to_vec(&input).unwrap();
        let owner = ObjectFillOwner::new("ordered-normalize", input.clone()).unwrap();
        let selected: Vec<CellRef> = (0..4)
            .map(|frame| CellRef {
                layer_uid: "L0".into(),
                frame,
            })
            .collect();
        let packet = owner.copy_frame_clipboard(&selected).unwrap();
        let source = packet.cells();
        assert!(source[1].manual_edit());
        assert_eq!(
            source
                .iter()
                .map(|cell| cell.source_frame())
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(0), Some(3)]
        );
        assert_eq!(
            source[2].records().len(),
            1,
            "held source facts retain the shown key"
        );
        assert_eq!(
            serde_json::to_value(source[2].order_elements().value().unwrap()).unwrap(),
            json!([a])
        );
        let cells = packet_paste_cells(&packet);
        assert_eq!(
            cells
                .iter()
                .map(|cell| (cell.keyframe, cell.records.len()))
                .collect::<Vec<_>>(),
            [(true, 1), (true, 1), (false, 0), (true, 0)]
        );
        assert_eq!(
            serde_json::to_value(cells[1].order_elements.value().unwrap()).unwrap(),
            json!([b])
        );
        for index in [2, 3] {
            assert_eq!(cells[index].order_elements.value().unwrap().len(), 0);
        }
        let out = paste(&input, 1, 0, &cells).unwrap().unwrap();
        assert_eq!(render_layer(&out, 1), "K=a.1.10;K=b.2.20;.;K");
        assert_eq!(exposure_string(&out, 1), "**1*");
        let records: Vec<serde_json::Value> = out
            .objects
            .iter()
            .filter(|record| scene_authored(record) && record.target.layer_uid == "L1")
            .map(|record| serde_json::to_value(record).unwrap())
            .collect();
        assert_eq!(
            records,
            vec![
                record_json("scene-root", "authored", "L1", "a", 0, 1, 10),
                record_json("scene-root", "authored", "L1", "b", 1, 2, 20)
            ]
        );
        assert_eq!(
            out.layers[1].timing.value().unwrap().exposure(1),
            ("key", Some(1), false)
        );
        assert_eq!(
            rows::scoped_elements(&out, "L1", 0),
            json!([
            {"layerUid":"L1","strokeId":"a","naturalPosition":0,"order":{"static":[7]}}])
        );
        assert_eq!(
            rows::scoped_elements(&out, "L1", 1),
            json!([
            {"layerUid":"L1","strokeId":"b","naturalPosition":0,"order":{"static":[-4]}}])
        );
        assert_eq!(rows::scoped_elements(&out, "L1", 2), json!([]));
        assert_eq!(rows::scoped_elements(&out, "L1", 3), json!([]));
        assert_eq!(render_layer(&out, 0), "K=a.1.10;M=b.2.20;.;K");
        assert_eq!(untouched(&out), untouched(&input));
        assert_eq!(serde_json::to_vec(&input).unwrap(), frozen);
    }
}
