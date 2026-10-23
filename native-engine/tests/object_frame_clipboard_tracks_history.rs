//! The cross-context clipboard through the real history owner: a scene-root pair prepares exactly what the plain clipboard
//! prepares, and one history entry and one Undo follow the unchanged L05-I commit; every other pair is refused typed before
//! the operation is read, with no candidate, no history and no change. The Motion-track join with Repeat Selection (L05-J1) is
//! in object_frame_clipboard_tracks_join.rs.
use super::*;
use crate::commands::{packet_paste_cells, CellRef, PasteRequest};
use crate::object_codec::encode_storage_project;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
pub(crate) mod build;
#[path = "object_frame_clipboard_tracks_join.rs"]
mod join;

use build::{Build, Cell, Stroke};

pub(crate) fn cell(flag: char, id: &str) -> Cell {
    Cell {
        flag,
        strokes: if id.is_empty() {
            Vec::new()
        } else {
            vec![Stroke {
                id: id.to_owned(),
                z: 1,
                g: 10,
            }]
        },
    }
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    let head = o.acquire_snapshot(o.content_revision()).unwrap();
    encode_storage_project(head.document()).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (o.content_revision(), o.history_depths(), bytes(o))
}
/// L0 holds a key with stroke `a` at frame 0; L1 holds a blank key at frame 0. Four frames.
fn owner(form: Build) -> NativeObjectHistory {
    let layers = vec![
        vec![cell('K', "a"), cell('.', ""), cell('.', ""), cell('.', "")],
        vec![cell('K', ""), cell('.', ""), cell('.', ""), cell('.', "")],
    ];
    NativeObjectHistory::new("tracks-context", build::document(&layers, form)).unwrap()
}
/// Paste the key of L0 frame 0 onto L1 frame 2.
fn paste(o: &NativeObjectHistory) -> FrameClipboardOperation {
    let packet = o
        .copy_frame_clipboard(&[CellRef {
            layer_uid: "L0".to_owned(),
            frame: 0,
        }])
        .unwrap();
    FrameClipboardOperation::Paste(PasteRequest {
        instance_id: packet.instance_id().to_owned(),
        document_id: packet.document_id().to_owned(),
        anchor_layer: "L1".to_owned(),
        anchor_frame: 2,
        cells: packet_paste_cells(&packet),
    })
}
fn pair(source: &str, target: &str) -> ClipboardContexts {
    ClipboardContexts {
        source: source.to_owned(),
        target: target.to_owned(),
    }
}

#[test]
fn a_scene_root_pair_prepares_exactly_what_the_plain_clipboard_prepares() {
    let mut o = owner(Build::V1);
    let before = bytes(&o);
    let operation = paste(&o);
    let plain = o.prepare_frame_clipboard(&operation).unwrap();
    o.commit_frame_clipboard(plain).unwrap();
    let after = bytes(&o);
    assert_ne!(after, before, "the paste lands a key on L1");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
    let contextual = o
        .prepare_contextual_frame_clipboard(&ClipboardContexts::scene_root(), &operation)
        .unwrap();
    assert_eq!(contextual.command(), "object.frames.clipboard.paste");
    o.commit_frame_clipboard(contextual).unwrap();
    assert_eq!(bytes(&o), after, "the same bytes as the plain paste");
    assert_eq!(o.history_depths(), (1, 0), "one entry, redo cleared");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before, "one Undo restores the complete bytes");
}

#[test]
fn cut_and_duplicate_cross_the_same_gate() {
    let o = owner(Build::V1);
    let selection = vec![CellRef {
        layer_uid: "L0".to_owned(),
        frame: 0,
    }];
    let cut = FrameClipboardOperation::Cut {
        selection: selection.clone(),
    };
    let duplicate = FrameClipboardOperation::Duplicate {
        selection,
        anchor_layer: "L1".to_owned(),
    };
    let root = ClipboardContexts::scene_root();
    let prepared = o.prepare_contextual_frame_clipboard(&root, &cut).unwrap();
    assert_eq!(prepared.command(), "object.frames.clipboard.cut");
    assert!(prepared.packet().is_some(), "a cut returns its packet");
    let prepared = o
        .prepare_contextual_frame_clipboard(&root, &duplicate)
        .unwrap();
    assert_eq!(prepared.command(), "object.frames.clipboard.duplicate");
    let before = state(&o);
    for operation in [&cut, &duplicate] {
        assert_eq!(
            o.prepare_contextual_frame_clipboard(&pair("scene-root", "ctx-x"), operation)
                .err(),
            Some(DispatchErrorCode::NotFound),
            "version 1 does not declare ctx-x"
        );
    }
    assert_eq!(state(&o), before);
}

#[test]
fn a_refused_pair_publishes_nothing_adds_no_history_and_is_decided_before_the_operation_is_read() {
    let o = owner(Build::V2Extras);
    let before = state(&o);
    // an ordered document cannot be copied from, so both operations are written by hand: one that would be admitted
    // by the planners' identity checks, and one that would itself be refused (unknown anchor layer)
    let operation = FrameClipboardOperation::Paste(PasteRequest {
        instance_id: o.instance_id().to_owned(),
        document_id: o.document_id().to_owned(),
        anchor_layer: "L1".to_owned(),
        anchor_frame: 2,
        cells: Vec::new(),
    });
    let broken = FrameClipboardOperation::Paste(PasteRequest {
        instance_id: o.instance_id().to_owned(),
        document_id: o.document_id().to_owned(),
        anchor_layer: "nope".to_owned(),
        anchor_frame: 0,
        cells: Vec::new(),
    });
    let cases = [
        ("scene-root", "ctx-x", DispatchErrorCode::Unavailable),
        ("ctx-x", "scene-root", DispatchErrorCode::Unavailable),
        ("ctx-x", "ctx-x", DispatchErrorCode::Unavailable),
        ("scene-root", "nope", DispatchErrorCode::NotFound),
        ("", "scene-root", DispatchErrorCode::InvalidRequest),
    ];
    for (source, target, code) in cases {
        for candidate in [&operation, &broken] {
            assert_eq!(
                o.prepare_contextual_frame_clipboard(&pair(source, target), candidate)
                    .err(),
                Some(code),
                "{source:?} -> {target:?}"
            );
        }
    }
    assert_eq!(state(&o), before, "no candidate, no revision, no entry");
    // with an admitted pair the operation's own refusal is the one that answers
    assert_eq!(
        o.prepare_contextual_frame_clipboard(&ClipboardContexts::scene_root(), &broken)
            .err(),
        Some(DispatchErrorCode::NotFound)
    );
}

#[test]
fn a_paste_never_moves_property_tracks_the_clipboard_carries_cells_only() {
    // the baseline copyFrames / pasteFrames carry `frames[f]` and nothing of `ld.motion`: the holder table is untouched
    let layers = vec![
        vec![cell('K', "a"), cell('.', ""), cell('.', ""), cell('.', "")],
        vec![cell('K', ""), cell('.', ""), cell('.', ""), cell('.', "")],
    ];
    let mut value = build::document_json(&layers, Build::V1);
    value["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},
         "properties":[{"property":"rotation","track":{"keys":[{"frame":0,"v":[10]},{"frame":1,"v":[20]}]}}]}]);
    let mut o = NativeObjectHistory::new("tracks-paste", build::decode(&value)).unwrap();
    let wire = |o: &NativeObjectHistory| -> Value {
        serde_json::to_value(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
    };
    let holders = wire(&o)["propertyHolders"].clone();
    let operation = paste(&o);
    let candidate = o
        .prepare_contextual_frame_clipboard(&ClipboardContexts::scene_root(), &operation)
        .unwrap();
    o.commit_frame_clipboard(candidate).unwrap();
    assert_eq!(
        wire(&o)["propertyHolders"],
        holders,
        "no track moved or was cloned"
    );
    let head = o.acquire_snapshot(o.content_revision()).unwrap();
    assert_eq!(build::render_layer(head.document(), 1), "K;.;K=a.1.10;.");
    assert_eq!(o.history_depths(), (1, 0));
}
