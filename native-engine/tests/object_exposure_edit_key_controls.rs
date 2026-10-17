//! Negative controls for the key-exposure oracle. Each control runs the REAL dispatch path over every unlocked oracle row
//! (locked rows need the F06 admission and are covered by their own test) with ONE deliberate mistake (an altered request, or
//! a published document corrupted afterwards), through the SAME comparison the positive test uses, and requires it to fail on
//! exactly the rows where the mistake is observable. The failing set is derived from the row data in Rust and its size is
//! pinned as a literal that tools/reach_counts.js (the same predicates written a second time) prints.
use super::build::{document, Build};
use super::rows::{compare, input_document, rows, Row};
use super::{dispatch, head, owner, request};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

type Mutate<'a> = &'a dyn Fn(&Row, Build, &mut ObjectDocument);

fn unlocked() -> Vec<Row> {
    rows().into_iter().filter(|row| !row.any_lock()).collect()
}
fn replay(row: &Row, build: Build, payload: Value, mutate: Mutate<'_>) -> Result<(), String> {
    let input = input_document(row, build);
    let mut o = owner(input.clone());
    let submitted = request(&o, "control", row.operation(), payload);
    let reply = dispatch(&mut o, &submitted);
    if reply["ok"] != true {
        return Err(format!("refused: {reply}"));
    }
    if (reply["result"]["applied"] == true) != row.chg {
        return Err("the applied flag differs from the baseline".into());
    }
    let mut out = head(&o).document().clone();
    mutate(row, build, &mut out);
    compare(row, build, &input, &out)
}
fn failing(alter: &dyn Fn(&Row) -> Value, mutate: Mutate<'_>) -> Vec<String> {
    let mut failed = Vec::new();
    for row in &unlocked() {
        let payload = alter(row);
        if Build::ALL
            .iter()
            .any(|build| replay(row, *build, payload.clone(), mutate).is_err())
        {
            failed.push(row.label());
        }
    }
    failed
}
fn labels(select: &dyn Fn(&Row) -> bool) -> Vec<String> {
    unlocked()
        .iter()
        .filter(|row| select(row))
        .map(|row| row.label())
        .collect()
}
fn untouched_payload(row: &Row) -> Value {
    row.payload()
}
fn no_mutation(_: &Row, _: Build, _: &mut ObjectDocument) {}
fn cell_of(row: &Row, layer: &str, frame: &str) -> char {
    row.input[layer.parse::<usize>().unwrap()][frame.parse::<usize>().unwrap()].flag
}

/// F6 with a layer selection: some unselected layer would change if the selection were ignored.
fn selection_matters(row: &Row) -> bool {
    if row.op != "f6" || row.sel == "-" {
        return false;
    }
    let cell: usize = row.args.parse().unwrap();
    let named: Vec<usize> = row.sel.split(',').map(|l| l.parse().unwrap()).collect();
    (0..row.input.len()).any(|layer| !named.contains(&layer) && row.input[layer][cell].flag != 'K')
}
/// Delete Frames with at least two targets whose last target is not already a hold.
fn last_target_matters(row: &Row) -> bool {
    if row.op != "dsel" {
        return false;
    }
    let pairs: Vec<&str> = row.args.split(',').collect();
    if pairs.len() < 2 {
        return false;
    }
    let (layer, frame) = pairs[pairs.len() - 1].split_once(':').unwrap();
    cell_of(row, layer, frame) != '.'
}
/// A convert-to-keyframes selection of at least two cells whose last cell is not already a key.
fn last_cell_matters(row: &Row) -> bool {
    if row.op != "conv" || row.conv_cells().len() < 2 {
        return false;
    }
    let active = row.args.split_once('/').unwrap().0;
    let last = row.conv_cells()[row.conv_cells().len() - 1];
    cell_of(row, active, &last.to_string()) != 'K'
}
/// Remove Tween whose end cell is not already a hold and which has a frame after it to widen into.
fn tween_end_matters(row: &Row) -> bool {
    if row.op != "demote" {
        return false;
    }
    let part: Vec<&str> = row.args.split(':').collect();
    let end: u32 = part[2].parse().unwrap();
    end + 1 < row.total && cell_of(row, part[0], part[2]) != '.'
}

#[test]
fn unmodified_transactions_pass_every_row_in_every_form() {
    assert_eq!(
        failing(&untouched_payload, &no_mutation),
        Vec::<String>::new()
    );
}

#[test]
fn an_f6_that_ignores_the_layer_selection_is_caught_where_an_unselected_layer_would_change() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "f6" {
            payload.as_object_mut().unwrap().remove("layerUids");
        }
        payload
    };
    let failed = failing(&alter, &no_mutation);
    assert_eq!(failed, labels(&selection_matters));
    assert_eq!(failed.len(), 76);
}

#[test]
fn a_delete_frames_that_drops_its_last_target_is_caught_where_that_cell_was_not_a_hold() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "dsel" {
            let targets = payload["targets"].as_array_mut().unwrap();
            if targets.len() > 1 {
                targets.pop();
            }
        }
        payload
    };
    let failed = failing(&alter, &no_mutation);
    assert_eq!(failed, labels(&last_target_matters));
    assert_eq!(failed.len(), 83);
}

#[test]
fn a_convert_that_drops_its_last_cell_is_caught_where_that_cell_was_not_a_key() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "conv" {
            let cells = payload["cells"].as_array_mut().unwrap();
            if cells.len() > 1 {
                cells.pop();
            }
        }
        payload
    };
    let failed = failing(&alter, &no_mutation);
    assert_eq!(failed, labels(&last_cell_matters));
    assert_eq!(failed.len(), 56);
}

#[test]
fn a_remove_tween_that_reaches_one_frame_too_far_is_caught_where_the_end_key_was_not_a_hold() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "demote" {
            let end = payload["to"].as_u64().unwrap();
            if end + 1 < u64::from(row.total) {
                payload["to"] = json!(end + 1);
            }
        }
        payload
    };
    let failed = failing(&alter, &no_mutation);
    assert_eq!(failed, labels(&tween_end_matters));
    assert_eq!(failed.len(), 37);
}

#[test]
fn a_published_document_that_forgot_its_new_timing_is_caught_on_every_changed_row() {
    fn stale(row: &Row, build: Build, out: &mut ObjectDocument) {
        let input = document(&row.input, build);
        for (layer, source) in out.layers.iter_mut().zip(input.layers.iter()) {
            layer.timing = source.timing.clone();
        }
    }
    let failed = failing(&untouched_payload, &stale);
    assert_eq!(failed, labels(&|row| row.chg));
    assert_eq!(failed.len(), 569);
}
