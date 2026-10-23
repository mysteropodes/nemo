// Included in the original test module: imports, fixture/support items and
// any baseline fixture-admission test keep their exact lexical namespace.
use super::*;
use crate::commands::ObjectFillOwner;
use crate::object_document::{ObjectDocument, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_blank_key_edit_build.rs"]
#[allow(dead_code)]
mod f7;
#[path = "object_frame_bulk_move_build.rs"]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
use fixture::{admit, assert_rows, bytes, document, head, moving, owner, shifting, value};

fn prepare(
    o: &ObjectFillOwner,
    payload: Value,
) -> Result<PreparedBulkFrameEdit, DispatchErrorCode> {
    o.prepare_bulk_frame_edit_json(payload.to_string().as_bytes())
}
fn selection(candidate: &PreparedBulkFrameEdit) -> Option<Vec<(String, u32)>> {
    candidate.selection().map(|cells| {
        cells
            .iter()
            .map(|cell| (cell.layer_uid.clone(), cell.frame))
            .collect()
    })
}
fn pairs(candidate: &PreparedBulkFrameEdit) -> Vec<(String, u32, u32, i64, i64)> {
    candidate
        .pairs()
        .iter()
        .map(|pair| {
            (
                pair.layer_uid.clone(),
                pair.from_a,
                pair.from_b,
                pair.to_a,
                pair.to_b,
            )
        })
        .collect()
}
fn commit(o: &mut ObjectFillOwner, payload: Value) -> ObjectDocument {
    let candidate = prepare(o, payload).unwrap();
    o.commit_bulk_frame_edit(candidate).unwrap();
    head(o)
}

#[test]
fn cross_layer_rectangle_carries_key_sample_manual_false_and_plain_cells_verbatim_in_four_forms() {
    for form in build::Build::ALL {
        let input = document(
            "K=a.1.10;M=b.2.20;.;N=c.3.30;I=d.4.40;K=e.5.50|K=x.6.60;K;K;K;K;K=z.7.70",
            form,
        );
        let mut o = owner(input.clone());
        let retained = o.acquire_snapshot(0).unwrap();
        let candidate = prepare(
            &o,
            moving(
                &[("L0", 0), ("L0", 1), ("L0", 2), ("L0", 3), ("L0", 4)],
                "L1",
                0,
            ),
        )
        .unwrap();
        assert_eq!(candidate.command(), "object.frames.move");
        assert!(candidate.checkpoint_required());
        assert!(candidate.document_changed());
        assert!(!candidate.shifted());
        assert_eq!(
            selection(&candidate),
            Some((0..5).map(|frame| ("L1".into(), frame)).collect())
        );
        assert_eq!(o.content_revision(), 0);
        assert_eq!(value(retained.document()), value(&input));
        assert_eq!(o.commit_bulk_frame_edit(candidate).unwrap(), 1);
        assert_rows(
            &input,
            &head(&o),
            ".;.;.;.;.;K=e.5.50|K=a.1.10;M=b.2.20;.;N=c.3.30;I=d.4.40;K=z.7.70",
            form,
        );
        assert_eq!(value(retained.document()), value(&input));
    }
}
