//! After every changed oracle row the REAL exposure producer (`prepare_object_exposure_frame`) shows, per layer and
//! frame, exactly the strokes the dense baseline shows. The source frame of each cell is read from the baseline's own
//! `heldKeyframeAt` result (OUT_EXPOSURE) and the strokes from the baseline cells (OUT_LAYERS): neither the native
//! timing nor an array length is the oracle. Needs the `evaluation` feature (run with `--features test-history,evaluation`).
use super::build::{uid, Build};
use super::rows::{rows, Row};
use super::{apply, head, owner};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};

/// The frame each cell shows, read from one layer's OUT_EXPOSURE string. A held cell names its key with JavaScript's
/// `toString(36)`, which takes several characters from frame 36 on, so the string is not one character per frame: a
/// held cell always names the most recent key (`heldKeyframeAt`), and is read as exactly that many digits.
fn sources(exposure: &str) -> Vec<Option<usize>> {
    let chars: Vec<char> = exposure.chars().collect();
    let (mut out, mut at, mut last_key) = (Vec::new(), 0, None);
    while at < chars.len() {
        let frame = out.len();
        match chars[at] {
            '*' => {
                last_key = Some(frame);
                out.push(Some(frame));
                at += 1;
            }
            '~' | '!' => {
                out.push(Some(frame));
                at += 1;
            }
            '-' => {
                out.push(None);
                at += 1;
            }
            _ => {
                let key = last_key.expect("a held cell follows a key");
                let digits = base36(key);
                let rest: String = chars[at..].iter().take(digits.len()).collect();
                assert_eq!(rest, digits, "held cell at frame {frame} of {exposure}");
                out.push(Some(key));
                at += digits.len();
            }
        }
    }
    out
}

fn base36(mut n: usize) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit((n % 36) as u32, 36).unwrap());
        n /= 36;
        if n == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

fn expected_ids(row: &Row, layer: usize, frame: usize) -> Vec<String> {
    let source = sources(&row.exposure[layer])[frame];
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
    for row in rows().iter().filter(|row| row.chg) {
        for form in [Build::V1, Build::V1Ref] {
            let mut o = owner(row.document(form));
            apply(&mut o, &row.request())
                .unwrap_or_else(|error| panic!("{}: {error}", row.label()));
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
    assert_eq!(frames_checked, 9718);
}
