//! After every changed unlocked oracle row the REAL exposure producer (`prepare_object_exposure_frame`) shows, per layer and
//! frame, exactly the strokes the dense baseline shows. The source frame of each cell is read from the baseline's own
//! `heldKeyframeAt` result (OUT_EXPOSURE) and the strokes from the baseline cells (OUT_LAYERS): neither the native timing nor
//! an array length is the oracle. Needs the `evaluation` feature (run with `--features test-history,evaluation`).
use super::build::{uid, Build};
use super::rows::{input_document, rows, Row};
use super::{dispatch, head, owner, request};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};

fn expected_ids(row: &Row, layer: usize, frame: usize) -> Vec<String> {
    let source = match row.exposure[layer].chars().nth(frame).unwrap() {
        '*' | '~' | '!' => Some(frame),
        '-' => None,
        digit => Some(digit.to_digit(36).unwrap() as usize),
    };
    let mut ids: Vec<String> = match source {
        Some(source) => row.output[layer][source]
            .strokes
            .iter()
            .map(|stroke| stroke.id.clone())
            .collect(),
        None => Vec::new(),
    };
    ids.sort();
    ids
}

#[test]
fn the_exposure_producer_shows_the_baseline_strokes_after_every_changed_row() {
    let mut frames_checked = 0;
    for row in rows().iter().filter(|row| row.chg && !row.any_lock()) {
        for form in [Build::V1, Build::V1Ref, Build::V2] {
            let mut o = owner(input_document(row, form));
            let submitted = request(&o, "exposure", row.operation(), row.payload());
            assert_eq!(dispatch(&mut o, &submitted)["ok"], true, "{}", row.label());
            let snapshot = head(&o);
            for frame in 0..row.total {
                let selector = ObjectFrameSelector {
                    instance_id: snapshot.instance_id(),
                    document_id: snapshot.document_id(),
                    content_revision: snapshot.content_revision(),
                    document_snapshot_id: snapshot.snapshot_id(),
                    context_id: "scene-root",
                    scope_kind: FrameScopeKind::Authored,
                    frame,
                };
                let packet =
                    prepare_object_exposure_frame(&snapshot, &selector).unwrap_or_else(|error| {
                        panic!("{} {form:?} frame {frame}: {error:?}", row.label())
                    });
                for layer in 0..row.output.len() {
                    let mut shown: Vec<String> = packet
                        .records()
                        .filter(|shown| shown.record().target().layer_uid() == uid(layer))
                        .map(|shown| shown.record().target().stroke_id().to_owned())
                        .collect();
                    shown.sort();
                    assert_eq!(
                        shown,
                        expected_ids(row, layer, frame as usize),
                        "{} {form:?} layer {layer} frame {frame}",
                        row.label()
                    );
                }
                frames_checked += 1;
            }
        }
    }
    assert_eq!(frames_checked, 6972);
}
