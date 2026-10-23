//! Owner-level frame-clipboard tests: hand-computed cut and duplicate results, the selection-end anchor on the ACTIVE
//! layer, the timeline never growing, typed refusals and the baseline lock policy. The oracle rows (verbatim baseline
//! editor at fbe4b8a3) run through the history owner in object_frame_clipboard_paste_history.rs.
//!
//! The lock case authors `locked: true` on a layer; the batch-5 head (F06 policy) admits it and `ObjectLayer::locked()`
//! reads it.
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::Presence;
use serde_json::json;

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_paste_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document, document_json, exposure_string, render_layer, Build, Stroke};
use rows::{parse_layers, record_of};

const SMALL: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";
const THREE: &str = "K=a.1.10;.;.;.|K=b.2.20;.;.;.|K;.;.;.";

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("frame-clipboard", document).unwrap()
}
fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn text(o: &ObjectFillOwner) -> Vec<String> {
    let document = head(o);
    (0..document.layers.len())
        .map(|layer| render_layer(&document, layer))
        .collect()
}
fn refs(cells: &[(&str, u32)]) -> Vec<CellRef> {
    cells
        .iter()
        .map(|(layer, frame)| CellRef {
            layer_uid: (*layer).to_owned(),
            frame: *frame,
        })
        .collect()
}
fn run(
    o: &mut ObjectFillOwner,
    operation: FrameClipboardOperation,
) -> Result<FrameClipboardOutcome, DispatchErrorCode> {
    let candidate = o.prepare_frame_clipboard(&operation)?;
    o.commit_frame_clipboard(candidate)
}
fn duplicate_op(selection: &[(&str, u32)], anchor: &str) -> FrameClipboardOperation {
    FrameClipboardOperation::Duplicate {
        selection: refs(selection),
        anchor_layer: anchor.to_owned(),
    }
}
fn cut_op(selection: &[(&str, u32)]) -> FrameClipboardOperation {
    FrameClipboardOperation::Cut {
        selection: refs(selection),
    }
}
fn paste_op(
    o: &ObjectFillOwner,
    layer: &str,
    frame: u32,
    cells: Vec<PasteCell>,
) -> FrameClipboardOperation {
    FrameClipboardOperation::Paste(PasteRequest {
        instance_id: o.instance_id().to_owned(),
        document_id: o.document_id().to_owned(),
        anchor_layer: layer.to_owned(),
        anchor_frame: frame,
        cells,
    })
}
fn paste_with(
    o: &ObjectFillOwner,
    edit: impl FnOnce(&mut PasteRequest),
) -> FrameClipboardOperation {
    let mut request = PasteRequest {
        instance_id: o.instance_id().to_owned(),
        document_id: o.document_id().to_owned(),
        anchor_layer: "L1".to_owned(),
        anchor_frame: 1,
        cells: vec![key("n")],
    };
    edit(&mut request);
    FrameClipboardOperation::Paste(request)
}
fn key(id: &str) -> PasteCell {
    PasteCell {
        order_elements: crate::object_document::Presence::Missing,
        rel_layer: 0,
        rel_frame: 0,
        keyframe: true,
        records: vec![record_of(&Stroke {
            id: id.to_owned(),
            z: 1,
            g: 1,
        })],
    }
}

#[test]
fn each_operation_names_itself_and_preparing_publishes_nothing() {
    let o = owner(doc(SMALL));
    let ops = [
        (
            paste_op(&o, "L1", 1, vec![key("n")]),
            "object.frames.clipboard.paste",
        ),
        (cut_op(&[("L0", 0)]), "object.frames.clipboard.cut"),
        (
            duplicate_op(&[("L0", 0)], "L1"),
            "object.frames.clipboard.duplicate",
        ),
    ];
    for (operation, command) in ops {
        let candidate = o.prepare_frame_clipboard(&operation).unwrap();
        assert_eq!(candidate.command(), command);
        assert_eq!(candidate.packet().is_some(), command.ends_with(".cut"));
    }
    assert_eq!(o.content_revision(), 0, "preparing publishes nothing");
}

#[test]
fn cut_blanks_the_selected_cells_and_returns_the_packet_of_the_pre_edit_document() {
    let mut o = owner(doc(SMALL));
    let selection = [("L0", 0), ("L0", 1), ("L1", 3)];
    let copied = o.copy_frame_clipboard(&refs(&selection)).unwrap();
    let outcome = run(&mut o, cut_op(&selection)).unwrap();
    assert_eq!(outcome.revision, 1);
    assert_eq!(outcome.packet.as_ref(), Some(&copied));
    // the key at L0/0 and the blank key at L1/3 become holds; the held cell at L0/1 stays a hold
    assert_eq!(text(&o), [".;.;K=b.2.20;.", "K;.;.;."]);
    assert_eq!(exposure_string(&head(&o), 0), "--*2");
    let kinds: Vec<(bool, usize)> = copied
        .cells()
        .iter()
        .map(|cell| (cell.paste_is_keyframe(), cell.paste_records().len()))
        .collect();
    assert_eq!(kinds, [(true, 1), (false, 0), (true, 0)]);
}

#[test]
fn a_cut_of_holds_returns_its_packet_publishes_nothing_and_an_empty_cut_has_no_packet() {
    let mut o = owner(doc(SMALL));
    let outcome = run(&mut o, cut_op(&[("L0", 1), ("L0", 3)])).unwrap();
    assert_eq!(outcome.revision, 0);
    assert_eq!(outcome.packet.unwrap().cells().len(), 2);
    let empty = run(&mut o, cut_op(&[])).unwrap();
    assert_eq!((empty.revision, empty.packet), (0, None));
    assert_eq!(text(&o), ["K=a.1.10;.;K=b.2.20;.", "K;.;.;K"]);
}

#[test]
fn duplicate_pastes_after_the_selection_on_the_active_layer_not_the_first_selected_layer() {
    let mut o = owner(doc(THREE));
    // the selection lives on L1; the active layer is L2, so the copy lands on L2 at frame 1 (bounds end at 0)
    run(&mut o, duplicate_op(&[("L1", 0)], "L2")).unwrap();
    assert_eq!(
        text(&o),
        ["K=a.1.10;.;.;.", "K=b.2.20;.;.;.", "K;K=b.2.20;.;."]
    );
    assert_eq!(exposure_string(&head(&o), 2), "**11");
    let mut at_first = owner(doc(THREE));
    run(&mut at_first, duplicate_op(&[("L1", 0)], "L1")).unwrap();
    assert_eq!(text(&at_first)[1], "K=b.2.20;K=b.2.20;.;.");
}

#[test]
fn duplicate_never_grows_the_timeline_and_drops_what_falls_past_the_end() {
    let mut o = owner(doc(SMALL));
    let outcome = run(&mut o, duplicate_op(&[("L0", 2), ("L0", 3)], "L0")).unwrap();
    assert_eq!(outcome.revision, 0, "every pasted cell lies past frame 3");
    assert_eq!(head(&o).total_frames, 4);
    let partial = run(
        &mut o,
        duplicate_op(&[("L0", 0), ("L0", 1), ("L0", 2)], "L1"),
    )
    .unwrap();
    assert_eq!(partial.revision, 1);
    assert_eq!(head(&o).total_frames, 4);
    // bounds end at 2, so the copy starts at frame 3: only its first cell (the key) fits
    assert_eq!(text(&o)[1], "K;.;.;K=a.1.10");
}

#[test]
fn selection_and_anchor_errors_are_typed_and_publish_nothing() {
    let mut o = owner(doc(SMALL));
    let cases = [
        (
            duplicate_op(&[("zz", 0)], "L1"),
            DispatchErrorCode::NotFound,
        ),
        (
            duplicate_op(&[("L0", 0)], "zz"),
            DispatchErrorCode::NotFound,
        ),
        (
            duplicate_op(&[("L0", 4)], "L1"),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            duplicate_op(&[("L0", 0), ("L0", 0)], "L1"),
            DispatchErrorCode::InvalidRequest,
        ),
        (cut_op(&[("zz", 0)]), DispatchErrorCode::NotFound),
        (cut_op(&[("L0", 9)]), DispatchErrorCode::InvalidRequest),
        (
            cut_op(&[("L1", 1), ("L1", 1)]),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (operation, code) in cases {
        assert_eq!(
            run(&mut o, operation.clone()).unwrap_err(),
            code,
            "{operation:?}"
        );
    }
    assert_eq!(o.content_revision(), 0);
    let mut untimed = doc(SMALL);
    untimed.layers[0].timing = Presence::Missing;
    let mut without = owner(untimed);
    assert_eq!(
        run(&mut without, cut_op(&[("L0", 0)])).unwrap_err(),
        DispatchErrorCode::Unavailable,
        "a missing timing model is not a blank key"
    );
}

#[test]
fn paste_refusals_are_typed_and_publish_nothing() {
    let mut o = owner(doc(SMALL));
    let cases = [
        (
            paste_with(&o, |r| r.instance_id = "other".into()),
            DispatchErrorCode::WrongInstance,
        ),
        (
            paste_with(&o, |r| r.document_id = "other".into()),
            DispatchErrorCode::WrongDocument,
        ),
        (
            paste_with(&o, |r| r.anchor_layer = "zz".into()),
            DispatchErrorCode::NotFound,
        ),
        (
            paste_with(&o, |r| r.anchor_frame = 4),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (operation, code) in cases {
        assert_eq!(run(&mut o, operation).unwrap_err(), code);
    }
    assert_eq!(o.content_revision(), 0);
    let ordered = owner(document(&parse_layers(SMALL), Build::V2));
    let malformed = paste_op(&ordered, "L1", 1, vec![key("n")]);
    assert_eq!(
        ordered.prepare_frame_clipboard(&malformed).unwrap_err(),
        DispatchErrorCode::InvalidRequest,
        "v1 packet metadata cannot be guessed for v2"
    );
    assert_eq!(ordered.content_revision(), 0);
}

/// The document with the listed layers locked (`locked: true` authored; F06 admits it at the batch-5 head).
fn locked_document(source: &str, locked: &[usize]) -> ObjectDocument {
    let mut value = document_json(&parse_layers(source), Build::V1);
    for &layer in locked {
        value["layers"][layer]["locked"] = json!(true);
    }
    decode_storage_project(value.to_string().as_bytes()).expect("F06 admits locked layers")
}

#[test]
fn locked_layers_are_skipped_by_paste_and_duplicate_and_kept_in_the_cut_packet() {
    let source = "K=a.1.10;.;K=b.2.20;.|K=c.3.30;.;.;.";
    let mut o = owner(locked_document(source, &[1]));
    let wide = vec![
        key("x"),
        PasteCell {
            rel_layer: 1,
            ..key("y")
        },
    ];
    let op = paste_op(&o, "L0", 1, wide);
    run(&mut o, op).unwrap();
    assert_eq!(
        text(&o),
        ["K=a.1.10;K=x.1.1;K=b.2.20;.", "K=c.3.30;.;.;."],
        "the locked layer is skipped"
    );
    let cut = run(&mut o, cut_op(&[("L0", 0), ("L1", 0)])).unwrap();
    assert_eq!(
        cut.packet.unwrap().cells().len(),
        2,
        "a locked cell stays in the packet"
    );
    assert_eq!(
        text(&o),
        [".;K=x.1.1;K=b.2.20;.", "K=c.3.30;.;.;."],
        "but is never blanked"
    );
    let before = o.content_revision();
    assert_eq!(
        run(&mut o, duplicate_op(&[("L0", 1)], "L1"))
            .unwrap()
            .revision,
        before
    );
}

#[test]
fn ordered_prepared_candidates_keep_current_pins_and_locked_targets() {
    for form in [Build::V2, Build::V2Extras] {
        let mut value = serde_json::to_value(rows::ordered_document(form)).unwrap();
        value["layers"][1]["locked"] = json!(true);
        let mut o = owner(decode_storage_project(value.to_string().as_bytes()).unwrap());
        let op = duplicate_op(&[("L0", 0)], "L1");
        let frozen = head(&o);
        assert_eq!(run(&mut o, op).unwrap().revision, 0);
        assert_eq!(head(&o), frozen);
        let op = duplicate_op(&[("L0", 0)], "L0");
        let first = o.prepare_frame_clipboard(&op).unwrap();
        let stale = o.prepare_frame_clipboard(&op).unwrap();
        o.commit_frame_clipboard(first).unwrap();
        let frozen = head(&o);
        assert_eq!(
            o.commit_frame_clipboard(stale).unwrap_err(),
            DispatchErrorCode::StaleRevision
        );
        assert_eq!(head(&o), frozen);
    }
}
