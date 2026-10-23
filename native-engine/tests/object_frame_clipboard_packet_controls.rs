//! Mutation and negative controls for the clipboard packet oracle.
//!
//! The REAL kernel produces every packet. A control either bends the way the packet is rendered
//! back to the oracle tokens (one switch per baseline rule) or bends the dense-to-sparse document
//! builder (one switch per timing fact); it must then disagree with the baseline on exactly the
//! number of rows the Node generator counted from the baseline outputs. With every switch off the
//! disagreement is zero, so a green main test cannot be a vacuous comparison.
use super::rendering::{expected, render, Mutant, Rendered};
use super::tests::{copy_row, document, rows, snapshot, Build, Row};
use super::FrameClipboardPacket;
use crate::object_document::FrameScopeKind;

fn packets(build: &Build) -> Vec<(Row, FrameClipboardPacket)> {
    rows()
        .into_iter()
        .map(|row| {
            let packet = copy_row(&snapshot(&document(&row, build)), &row)
                .unwrap_or_else(|e| panic!("row {}: {e:?}", row.id));
            (row, packet)
        })
        .collect()
}

fn disagreeing(all: &[(Row, FrameClipboardPacket)], mutant: Mutant) -> usize {
    all.iter()
        .filter(|(row, packet)| render(packet, mutant) != expected(row))
        .count()
}

#[test]
fn faithful_rendering_agrees_with_every_row() {
    let all = packets(&Build::PLAIN);
    assert_eq!(all.len(), 425);
    assert_eq!(disagreeing(&all, Mutant::None), 0);
}

#[test]
fn each_rendering_mutant_disagrees_on_exactly_the_rows_the_baseline_makes_decisive() {
    let all = packets(&Build::PLAIN);
    let table = [
        // copied in-betweens must paste as full keys
        (Mutant::TweenKept, 223),
        // isManualEdit must not survive the paste
        (Mutant::ManualKept, 128),
        // a held cell is not an unexposed cell
        (Mutant::HeldDropped, 270),
        // a blank key is a key, not a plain cell
        (Mutant::BlankAsHold, 121),
        // offsets are relative to the selection bounds
        (Mutant::AbsoluteOffsets, 289),
        // the clipboard keeps the selection order
        (Mutant::SortedOrder, 184),
        // the baseline copy of a plain cell has no strokes, so it pastes as a plain cell
        (Mutant::HeldPastesSource, 270),
        // a cell before any key is not held from anything
        (Mutant::UnexposedAsHeld, 65),
    ];
    for (mutant, count) in table {
        assert_eq!(disagreeing(&all, mutant), count, "{mutant:?}");
    }
}

#[test]
fn comparing_against_a_tampered_expectation_fails_on_every_row() {
    let all = packets(&Build::PLAIN);
    let tampered = |row: &Row| -> Rendered {
        let mut want = expected(row);
        want.pasted[0] = format!("{}!", want.pasted[0]);
        want
    };
    let bad = all
        .iter()
        .filter(|(row, packet)| render(packet, Mutant::None) != tampered(row))
        .count();
    assert_eq!(bad, 425);
}

#[test]
fn timing_mutants_of_the_document_break_exactly_the_rows_that_select_the_changed_cell() {
    // Dropping every sample turns each selected in-between into a plain cell.
    let no_samples = packets(&Build {
        drop_samples: true,
        ..Build::PLAIN
    });
    assert_eq!(disagreeing(&no_samples, Mutant::None), 223);
    // Clearing the manual flag turns each selected manual in-between into a plain one.
    let no_manual = packets(&Build {
        drop_manual: true,
        ..Build::PLAIN
    });
    assert_eq!(disagreeing(&no_manual, Mutant::None), 128);
    // Marking every sample manual changes each selected plain in-between.
    let all_manual = packets(&Build {
        all_manual: true,
        ..Build::PLAIN
    });
    assert_eq!(disagreeing(&all_manual, Mutant::None), 135);
}

#[test]
fn reference_control_is_not_vacuous_and_nothing_from_reference_scope_is_copied() {
    let all = packets(&Build {
        with_reference: true,
        ..Build::PLAIN
    });
    let mut exposed_rows = 0usize;
    for (row, packet) in &all {
        let snapshot = snapshot(&document(
            row,
            &Build {
                with_reference: true,
                ..Build::PLAIN
            },
        ));
        let stored = snapshot.document().objects();
        // a Reference-scope record sits at the frame some selected cell shows
        let shadowed = packet.cells().iter().any(|cell| {
            cell.source_frame().is_some_and(|source| {
                stored.iter().any(|record| {
                    record.target().scope_kind() == &FrameScopeKind::Reference
                        && record.target().layer_uid() == cell.layer_uid()
                        && record.target().frame() == source
                })
            })
        });
        exposed_rows += usize::from(shadowed);
        for cell in packet.cells() {
            assert!(cell
                .records()
                .iter()
                .all(|record| record.target().scope_kind() == &FrameScopeKind::Authored));
        }
    }
    assert_eq!(exposed_rows, 418);
    assert_eq!(disagreeing(&all, Mutant::None), 0);
}

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_paste_rows.rs"]
#[allow(dead_code)]
mod ordered_rows;

#[test]
fn ordered_copy_keeps_authored_order_independent_of_record_sequence() {
    let input = ordered_rows::ordered_document(build::Build::V2Extras);
    let o = crate::commands::ObjectFillOwner::new("ordered-copy", input).unwrap();
    let packet = o
        .copy_frame_clipboard(&[crate::commands::CellRef {
            layer_uid: "L0".into(),
            frame: 0,
        }])
        .unwrap();
    let cell = &packet.cells()[0];
    assert_eq!(
        cell.records()
            .iter()
            .map(|record| record.target().stroke_id())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        serde_json::to_value(cell.order_elements().value().unwrap()).unwrap(),
        ordered_rows::ordered_elements("L0")
    );
}
