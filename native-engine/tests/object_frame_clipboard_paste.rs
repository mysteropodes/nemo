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
fn ordered_documents_and_a_layer_without_timing_are_typed_unavailable() {
    let ordered = document(&parse_layers(SMALL), Build::V2);
    let one = [cell(0, 0, true, &[("n", 1, 1)])];
    assert_eq!(
        paste(&ordered, 0, 1, &one),
        Err(DispatchErrorCode::Unavailable)
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
