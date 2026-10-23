//! Negative controls for the frame-clipboard transactions. Each takes the REAL history owner over every unlocked oracle
//! row of one operation with ONE mistake a faulty transaction could make (expressed as the request it would amount to),
//! through the SAME comparison the positive test uses, and requires the failing row set to equal a set derived from the
//! row data written a second time here (or, where the cause is not a row property, its pinned size plus a necessary
//! condition). `tools/model_check.js` prints the same counts from an independent JS model.
use super::build::document;
use super::rows::{compare, packet_matches, rows, Row, FORMS};
use super::runner::{apply, Mistake};
use super::{head, owner};

/// True when the mistake leaves the owner in a state the baseline comparison rejects, in either document form.
fn caught(row: &Row, mistake: Mistake) -> bool {
    FORMS.iter().any(|form| {
        let input = document(&row.input, *form);
        let mut o = owner(input.clone());
        match apply(row, &mut o, mistake) {
            Err(_) => true,
            Ok(packet) => {
                let out = head(&o).document().clone();
                let wrong_packet = match (&packet, row.op.as_str()) {
                    (Some(packet), _) => packet_matches(row, packet).is_err(),
                    (None, "cut") => true,
                    (None, _) => false,
                };
                compare(row, &input, &out).is_err() || wrong_packet
            }
        }
    })
}
fn failing(op: &str, mistake: Mistake) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| row.op == op && !row.any_locked() && caught(row, mistake))
        .map(|row| row.id)
        .collect()
}
fn ids(op: &str, predicate: impl Fn(&Row) -> bool) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| row.op == op && !row.any_locked() && predicate(row))
        .map(|row| row.id)
        .collect()
}
/// Clipboard cells of a paste row that land inside the target layers and frames.
fn landing<'a>(row: &'a Row) -> impl Iterator<Item = &'a super::rows::Entry> + 'a {
    let (layer, frame) = row.paste_base();
    let (layers, total) = (row.input.len() as u32, row.total);
    row.clip
        .iter()
        .filter(move |entry| layer as u32 + entry.rl < layers && frame + entry.rf < total)
}

#[test]
fn unmodified_transactions_pass_every_unlocked_row_in_both_forms() {
    for op in ["paste", "cut", "dupsel"] {
        assert_eq!(failing(op, Mistake::None), Vec::<usize>::new(), "{op}");
    }
}

#[test]
fn an_in_between_pasted_as_plain_is_caught_wherever_one_lands() {
    let expected = ids("paste", |row| {
        landing(row).any(|e| "IMN".contains(e.cell.flag))
    });
    assert_eq!(expected.len(), 73);
    assert_eq!(failing("paste", Mistake::InBetweenAsPlain), expected);
}

#[test]
fn plain_cells_that_are_skipped_are_caught_where_the_target_is_not_already_a_plain_hold() {
    let expected = ids("paste", |row| {
        let (layer, frame) = row.paste_base();
        landing(row).any(|e| {
            e.cell.flag == '.'
                && row.input[layer + e.rl as usize][(frame + e.rf) as usize].flag != '.'
        })
    });
    assert_eq!(expected.len(), 46);
    assert_eq!(failing("paste", Mistake::SkipPlain), expected);
}

#[test]
fn keys_pasted_without_their_strokes_are_caught_wherever_a_cell_with_strokes_lands() {
    let expected = ids("paste", |row| {
        landing(row).any(|e| !e.cell.strokes.is_empty())
    });
    assert_eq!(expected.len(), 106);
    assert_eq!(failing("paste", Mistake::KeysWithoutStrokes), expected);
}

#[test]
fn a_cut_that_forgets_the_last_selected_cell_is_caught_on_every_unlocked_cut_row() {
    let expected = ids("cut", |_| true);
    assert_eq!(expected.len(), 80);
    assert_eq!(failing("cut", Mistake::CutForgetsLast), expected);
}

#[test]
fn a_duplicate_on_the_first_selected_layer_is_caught_only_when_the_active_layer_differs() {
    let caught_rows = failing("dupsel", Mistake::DupAtMinLayer);
    let different = ids("dupsel", |row| row.dup_anchor() != row.min_selected_layer());
    assert_eq!(caught_rows.len(), 26);
    assert!(
        caught_rows.iter().all(|id| different.contains(id)),
        "a duplicate anchored on the minimum layer can only differ when the active layer is another one"
    );
}

#[test]
fn a_duplicate_pasted_inside_its_own_selection_is_caught() {
    assert_eq!(failing("dupsel", Mistake::DupOverlapping).len(), 81);
}
