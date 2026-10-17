//! Negative controls for Repeat Selection. Each takes the REAL history owner over every oracle row with ONE mistake a
//! faulty transaction could make, through the SAME comparison the positive test uses, and requires the failing row set
//! to equal a set the oracle itself carries: the generator ran the verbatim baseline with the same faulty arguments
//! (times-1, times+1, bounds one frame short at either end, bounds one layer short) and recorded per row whether the
//! dense result differs. The mistake that has no argument form, a repeat built from the paste transaction (an
//! in-between lands as a key, the timeline never grows, locked layers are skipped), is pinned by its size plus the rows
//! it must fail. `tools/model_check.js` prints the same counts from an independent JS model.
use super::build::{uid, Build};
use super::rows::{compare, rows, Row, MUTANTS};
use super::{apply, head, owner};
use crate::commands::{
    packet_paste_cells, CellRef, FrameClipboardOperation, PasteRequest, RepeatRequest,
};
use crate::history::NativeObjectHistory;

/// Two document forms are enough here: plain v1 and the ordered v2 with Reference records and a second context.
const CONTROL_FORMS: [Build; 2] = [Build::V1, Build::V2Extras];

/// True when `run` leaves the owner in a state the baseline comparison rejects (or refuses outright) in any form.
fn caught(
    row: &Row,
    forms: &[Build],
    run: &dyn Fn(&mut NativeObjectHistory) -> Result<(), String>,
) -> bool {
    forms.iter().any(|form| {
        let input = row.document(*form);
        let mut o = owner(input.clone());
        match run(&mut o) {
            Err(_) => true,
            Ok(()) => compare(row, &input, head(&o).document(), *form).is_err(),
        }
    })
}
fn failing_with(
    forms: &[Build],
    run: &dyn Fn(&Row, &mut NativeObjectHistory) -> Result<(), String>,
) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| caught(row, forms, &|o| run(row, o)))
        .map(|row| row.id)
        .collect()
}
fn failing_request(request: &dyn Fn(&Row) -> RepeatRequest) -> Vec<usize> {
    failing_with(&CONTROL_FORMS, &|row, o| {
        apply(o, &request(row)).map(|_| ())
    })
}
/// The rows the oracle says a faulty argument choice must break.
fn oracle_says(name: &str) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| row.mutant_differs(name))
        .map(|row| row.id)
        .collect()
}
fn mutant_is_caught_exactly_where_the_baseline_differs(name: &str, size: usize) {
    assert!(MUTANTS.contains(&name));
    let expected = oracle_says(name);
    assert_eq!(expected.len(), size, "{name}");
    assert_eq!(
        failing_request(&|row| row.mutant_request(name)),
        expected,
        "{name}"
    );
}

#[test]
fn the_unmodified_request_passes_every_row_in_both_control_forms() {
    assert_eq!(failing_request(&|row| row.request()), Vec::<usize>::new());
}

#[test]
fn one_repetition_too_few_is_caught_exactly_where_the_baseline_differs() {
    mutant_is_caught_exactly_where_the_baseline_differs("T-", 159);
}

#[test]
fn one_repetition_too_many_is_caught_exactly_where_the_baseline_differs() {
    mutant_is_caught_exactly_where_the_baseline_differs("T+", 215);
}

#[test]
fn bounds_one_frame_short_at_the_end_are_caught_exactly_where_the_baseline_differs() {
    mutant_is_caught_exactly_where_the_baseline_differs("F-", 176);
}

#[test]
fn bounds_one_frame_short_at_the_start_are_caught_exactly_where_the_baseline_differs() {
    mutant_is_caught_exactly_where_the_baseline_differs("F0", 176);
}

#[test]
fn bounds_one_layer_short_are_caught_exactly_where_the_baseline_differs() {
    mutant_is_caught_exactly_where_the_baseline_differs("L-", 83);
}

/// A repeat assembled from L05-I's paste: the packet of the bounds pasted once per round after the selection.
fn repeat_by_paste(row: &Row, o: &mut NativeObjectHistory) -> Result<(), String> {
    let code = |error: crate::request_receipts::DispatchErrorCode| format!("{error:?}");
    let (min_l, max_l, first, last) = row.bounds();
    let span = last - first + 1;
    let selection: Vec<CellRef> = (min_l..=max_l)
        .flat_map(|layer| {
            (first..=last).map(move |frame| CellRef {
                layer_uid: uid(layer),
                frame,
            })
        })
        .collect();
    let packet = o.copy_frame_clipboard(&selection).map_err(code)?;
    for round in 0..crate::commands::clamp_times(row.times) {
        let paste = PasteRequest {
            instance_id: o.instance_id().to_owned(),
            document_id: o.document_id().to_owned(),
            anchor_layer: uid(min_l),
            anchor_frame: last + 1 + round * span,
            cells: packet_paste_cells(&packet),
        };
        let candidate = o
            .prepare_frame_clipboard(&FrameClipboardOperation::Paste(paste))
            .map_err(code)?;
        o.commit_frame_clipboard(candidate).map_err(code)?;
    }
    Ok(())
}

#[test]
fn a_repeat_built_from_the_paste_transaction_is_caught_wherever_it_cannot_be_the_same() {
    // Paste refuses ordered documents, so the plain forms only.
    let failing = failing_with(&[Build::V1, Build::V1Ref], &repeat_by_paste);
    let all = rows();
    let grows = |row: &Row| row.total > row.total_in();
    // the timeline never grows through a paste, and a paste past the end refuses: every growing row is caught
    assert!(all
        .iter()
        .filter(|row| grows(row))
        .all(|row| failing.contains(&row.id)));
    assert_eq!(all.iter().filter(|row| grows(row)).count(), 186);
    assert_eq!(failing.len(), 235);
    // inside the timeline the only differences are an in-between landing as a key and a locked layer being skipped
    let inside: Vec<&Row> = all
        .iter()
        .filter(|row| !grows(row) && failing.contains(&row.id))
        .collect();
    assert_eq!(inside.len(), 49);
    for row in inside {
        let (min_l, max_l, first, last) = row.bounds();
        let in_between = (min_l..=max_l).any(|layer| {
            (first..=last).any(|frame| "IMN".contains(row.input[layer][frame as usize].flag))
        });
        let locked = row.locked[min_l..=max_l].contains(&true);
        assert!(in_between || locked, "{}", row.label());
    }
}
