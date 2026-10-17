//! Negative controls for the frame-structure oracle. Each control runs the REAL dispatch path over every oracle
//! row with ONE deliberate mistake (an altered request, or a published document corrupted afterwards), through the
//! SAME comparison the positive test uses, and requires it to fail on exactly the rows where the mistake is
//! observable. The failing set is derived from the row data in Rust and its size is pinned as a literal that
//! tools/reach_counts.js (the same predicates written a second time) prints.
use super::build::{document, Build};
use super::rows::{compare, rows, Row};
use super::{dispatch, head, owner, request};
use crate::object_document::{FrameScopeKind, ObjectDocument};
use serde_json::{json, Value};

type Mutate<'a> = &'a dyn Fn(&Row, Build, &mut ObjectDocument);

fn replay(row: &Row, build: Build, payload: Value, mutate: Mutate<'_>) -> Result<(), String> {
    let input = document(&row.input, build);
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
fn failing(builds: &[Build], alter: &dyn Fn(&Row) -> Value, mutate: Mutate<'_>) -> Vec<String> {
    let mut failed = Vec::new();
    for row in &rows() {
        let payload = alter(row);
        if builds
            .iter()
            .any(|build| replay(row, *build, payload.clone(), mutate).is_err())
        {
            failed.push(row.label());
        }
    }
    failed
}
fn labels(select: &dyn Fn(&Row) -> bool) -> Vec<String> {
    rows()
        .iter()
        .filter(|row| select(row))
        .map(|row| row.label())
        .collect()
}
fn untouched_payload(row: &Row) -> Value {
    row.payload()
}
fn span(row: &Row) -> (u32, u32) {
    let (from, to) = row.args.split_once(':').unwrap();
    (from.parse().unwrap(), to.parse().unwrap())
}
fn no_mutation(_: &Row, _: Build, _: &mut ObjectDocument) {}

#[test]
fn unmodified_transactions_pass_every_row_in_every_form() {
    assert_eq!(
        failing(&Build::ALL, &untouched_payload, &no_mutation),
        Vec::<String>::new()
    );
}

#[test]
fn an_extend_count_one_too_large_is_caught_on_every_extend_row() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "extend" {
            payload["count"] = json!(payload["count"].as_u64().unwrap() + 1);
        }
        payload
    };
    let failed = failing(&Build::ALL, &alter, &no_mutation);
    assert_eq!(failed, labels(&|row| row.op == "extend"));
    assert_eq!(failed.len(), 231);
}

#[test]
fn a_remove_span_one_frame_short_is_caught_on_every_multi_frame_span() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "rm" {
            let (from, to) = span(row);
            if from < to {
                payload["to"] = json!(to - 1);
            }
        }
        payload
    };
    let failed = failing(&Build::ALL, &alter, &no_mutation);
    assert_eq!(
        failed,
        labels(&|row| row.op == "rm" && span(row).0 < span(row).1)
    );
    assert_eq!(failed.len(), 51);
}

#[test]
fn an_insert_that_ignores_the_selection_is_caught_where_a_padded_layer_has_cells_to_move() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if row.op == "gap" {
            payload.as_object_mut().unwrap().remove("layerUids");
        }
        payload
    };
    let observable = |row: &Row| {
        let cell: usize = row.args.parse().unwrap_or(0);
        row.op == "gap"
            && row.roles.chars().enumerate().any(|(layer, role)| {
                role == 'P'
                    && row.input[layer]
                        .iter()
                        .skip(cell + 1)
                        .any(|c| c.flag != '.')
            })
    };
    let failed = failing(&Build::ALL, &alter, &no_mutation);
    assert_eq!(failed, labels(&observable));
    assert_eq!(failed.len(), 111);
}

#[test]
fn a_duplicate_copied_from_the_key_instead_of_the_shown_sample_is_caught_on_sample_sources() {
    fn copy_from_key(row: &Row, _: Build, out: &mut ObjectDocument) {
        let sample = |row: &Row| {
            let cell: usize = row.args.parse().unwrap();
            row.op == "dup" && row.chg && matches!(row.input[0][cell].flag, 'I' | 'M' | 'N')
        };
        if !sample(row) {
            return;
        }
        let cell: usize = row.args.parse().unwrap();
        let at = cell as u32 + 1;
        let key = row.input[0][..=cell].iter().rposition(|c| c.flag == 'K');
        let here = |r: &crate::object_document::ObjectRecord, frame: u32| {
            r.target.layer_uid == "L0"
                && r.target.context_id == "scene-root"
                && r.target.frame_scope.kind == FrameScopeKind::Authored
                && r.target.frame_scope.frame == frame
        };
        out.objects.retain(|r| !here(r, at));
        if let Some(key) = key {
            let copies: Vec<_> = out
                .objects
                .iter()
                .filter(|r| here(r, key as u32))
                .cloned()
                .map(|mut r| {
                    r.target.frame_scope.frame = at;
                    r
                })
                .collect();
            out.objects.extend(copies);
        }
    }
    let failed = failing(
        &[Build::V1, Build::V1Ref],
        &untouched_payload,
        &copy_from_key,
    );
    let sample_sources = labels(&|row| {
        let cell: usize = row.args.parse().unwrap_or(0);
        row.op == "dup" && row.chg && matches!(row.input[0][cell].flag, 'I' | 'M' | 'N')
    });
    assert_eq!(failed, sample_sources);
    assert_eq!(failed.len(), 48);
}

#[test]
fn a_published_document_that_forgot_its_new_timing_is_caught_wherever_a_flag_moves() {
    fn stale(row: &Row, build: Build, out: &mut ObjectDocument) {
        if row.op == "rm" {
            return;
        }
        let input = document(&row.input, build);
        for (layer, source) in out.layers.iter_mut().zip(input.layers.iter()) {
            layer.timing = source.timing.clone();
        }
    }
    let moved = |row: &Row| {
        row.op != "rm"
            && row
                .input
                .iter()
                .zip(row.output.iter())
                .any(|(before, after)| {
                    let mut padded: Vec<char> = before.iter().map(|c| c.flag).collect();
                    padded.resize(after.len(), '.');
                    padded != after.iter().map(|c| c.flag).collect::<Vec<char>>()
                })
    };
    let failed = failing(&Build::ALL, &untouched_payload, &stale);
    assert_eq!(failed, labels(&moved));
    assert_eq!(failed.len(), 428);
}
