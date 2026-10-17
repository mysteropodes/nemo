//! The document-level frame edits against the verbatim dense baseline editor (fbe4b8a3).
//!
//! Every oracle row is one run of the baseline insertFrame, removeFrame(Span), insertKeyframeAt,
//! convertToKeyframes, clearKeyframe, deleteSelectedFrames or removeTweenSpan (src/js/app.js, timeline.js in
//! a Node vm) over 1-3 dense layers whose key and in-between cells carry strokes. Each row is rebuilt as a
//! real native document in four forms (v1, v1 with Reference records, v2 with a composition-order plan, v2
//! with Reference and another context), edited through the real kernel, and compared back to the baseline
//! on cell flags, strokes (id, z, g), `heldKeyframeAt` exposure and the strokes the baseline shows per
//! frame. v2 element order (natural position, authored order) and everything the kernel must leave alone are
//! compared too. Mutation controls live in ..._controls.rs, hand-typed acceptance cases in ..._literals.rs.
use super::build::{
    displayed, document, element_triples, expected_triples, exposure_string, render_layer, uid,
    untouched, Build, Cell, Stroke,
};
use super::{
    clear_cells, clear_key, demote_span, insert_frame, promote_cells, remove_frames, DocumentEdit,
    FrameEditError,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-edit-baseline/documents.txt");

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) op: String,
    pub(crate) args: String,
    pub(crate) roles: String,
    pub(crate) chg: bool,
    pub(crate) in_total: u32,
    pub(crate) out_total: u32,
    pub(crate) input: Vec<Vec<Cell>>,
    pub(crate) output: Vec<Vec<Cell>>,
    pub(crate) out_tok: Vec<String>,
    pub(crate) exposure: Vec<String>,
    pub(crate) displayed: String,
}

fn parse_cell(token: &str) -> Cell {
    let strokes = match token[1..].strip_prefix('=') {
        None => Vec::new(),
        Some(list) => list
            .split(',')
            .map(|entry| {
                let part: Vec<&str> = entry.split('.').collect();
                Stroke {
                    id: part[0].to_owned(),
                    z: part[1].parse().unwrap(),
                    g: part[2].parse().unwrap(),
                }
            })
            .collect(),
    };
    Cell {
        flag: token.chars().next().unwrap(),
        strokes,
    }
}
pub(crate) fn parse_layers(text: &str) -> Vec<Vec<Cell>> {
    text.split('|')
        .map(|layer| layer.split(';').map(parse_cell).collect())
        .collect()
}
pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("row "))
        .map(|line| {
            let part: Vec<&str> = line.split(' ').collect();
            assert_eq!(part.len(), 13, "{line}");
            Row {
                id: part[1].parse().unwrap(),
                op: part[2].to_owned(),
                args: part[4].to_owned(),
                roles: part[5].to_owned(),
                chg: part[6] == "1",
                in_total: part[7].parse().unwrap(),
                out_total: part[8].parse().unwrap(),
                input: parse_layers(part[9]),
                output: parse_layers(part[10]),
                out_tok: part[10].split('|').map(str::to_owned).collect(),
                exposure: part[11].split('|').map(str::to_owned).collect(),
                displayed: part[12].to_owned(),
            }
        })
        .collect()
}

pub(crate) fn run(row: &Row, document: &ObjectDocument) -> Result<DocumentEdit, FrameEditError> {
    let nums = |text: &str| -> Vec<u32> { text.split(',').map(|n| n.parse().unwrap()).collect() };
    let span = |text: &str| -> (u32, u32) {
        let (a, b) = text.split_once(':').unwrap();
        (a.parse().unwrap(), b.parse().unwrap())
    };
    match row.op.as_str() {
        "gap" => {
            let uids: Vec<String> = row
                .roles
                .chars()
                .enumerate()
                .filter(|(_, r)| *r == 'G')
                .map(|(i, _)| uid(i))
                .collect();
            let targets: Vec<&str> = uids.iter().map(String::as_str).collect();
            insert_frame(document, row.args.parse().unwrap(), &targets)
        }
        "rm" => {
            let (min, max) = span(&row.args);
            remove_frames(document, min, max)
        }
        "promote" => promote_cells(document, "L0", &nums(&row.args)),
        "clearkey" => clear_key(document, "L0", row.args.parse().unwrap()),
        "clear" => clear_cells(document, "L0", &nums(&row.args)),
        "demote" => {
            let (from, to) = span(&row.args);
            demote_span(document, "L0", from, to)
        }
        other => panic!("unknown op {other}"),
    }
}
/// The shared oracle comparison; the controls run it on deliberately wrong outputs.
pub(crate) fn compare(
    row: &Row,
    build: Build,
    input: &ObjectDocument,
    out: &ObjectDocument,
) -> Result<(), String> {
    if out.total_frames != row.out_total {
        return Err(format!(
            "totalFrames {} != {}",
            out.total_frames, row.out_total
        ));
    }
    for (layer, expected) in row.output.iter().enumerate() {
        let (got, want) = (render_layer(out, layer), &row.out_tok[layer]);
        if &got != want {
            return Err(format!("layer {layer} cells\n got {got}\nwant {want}"));
        }
        let exposure = exposure_string(out, layer);
        if exposure != row.exposure[layer] {
            return Err(format!(
                "layer {layer} exposure {exposure} != {}",
                row.exposure[layer]
            ));
        }
        if build.v2() {
            for (frame, cell) in expected.iter().enumerate() {
                let got = element_triples(out, &uid(layer), frame as u32);
                if got != expected_triples(cell) {
                    return Err(format!(
                        "layer {layer} frame {frame} order elements {got:?}"
                    ));
                }
            }
        }
    }
    if displayed(out, 0) != row.displayed {
        return Err(format!(
            "displayed {} != {}",
            displayed(out, 0),
            row.displayed
        ));
    }
    if untouched(input) != untouched(out) {
        return Err(
            "a record, context, policy or property holder the kernel must not touch changed".into(),
        );
    }
    ObjectSnapshot::validate_drawing(out).map_err(|e| format!("storage admission: {e}"))
}

// ---- tests

#[test]
fn every_oracle_row_matches_on_every_document_form() {
    let all = rows();
    assert_eq!(all.len(), 1099);
    for row in &all {
        for build in Build::ALL {
            let input = document(&row.input, build);
            let before = input.clone();
            let edit =
                run(row, &input).unwrap_or_else(|e| panic!("row {} {build:?}: {e:?}", row.id));
            assert_eq!(
                input, before,
                "row {} {build:?}: the input document was mutated",
                row.id
            );
            let out = match edit {
                DocumentEdit::Changed(next) => {
                    assert!(row.chg, "row {} {build:?}: published a no-op", row.id);
                    next
                }
                DocumentEdit::NoChange => {
                    assert!(!row.chg, "row {} {build:?}: swallowed a change", row.id);
                    input.clone()
                }
            };
            compare(row, build, &input, &out)
                .unwrap_or_else(|e| panic!("row {} {build:?}: {e}", row.id));
            // save and reload: the published document encodes, decodes to itself and re-encodes to the same bytes
            let bytes = encode_storage_project(&out).unwrap();
            let reloaded = decode_storage_project(&bytes).unwrap();
            assert_eq!(reloaded, out, "row {} {build:?}: reload differs", row.id);
            assert_eq!(encode_storage_project(&reloaded).unwrap(), bytes);
        }
    }
}

#[test]
fn each_operation_is_exercised_on_its_own_rows() {
    let all = rows();
    let count = |op: &str| all.iter().filter(|r| r.op == op).count();
    assert_eq!(
        [
            count("gap"),
            count("rm"),
            count("promote"),
            count("clearkey"),
            count("clear"),
            count("demote")
        ],
        [239, 236, 277, 127, 104, 116]
    );
    let changed = all.iter().filter(|r| r.chg).count();
    assert_eq!((changed, all.len() - changed), (830, 269));
}

#[test]
fn oracle_rows_are_internally_consistent() {
    for row in &rows() {
        assert_eq!(row.input.len(), row.output.len(), "row {}", row.id);
        assert!(
            row.input.iter().all(|l| l.len() as u32 == row.in_total),
            "row {}",
            row.id
        );
        assert!(
            row.output.iter().all(|l| l.len() as u32 == row.out_total),
            "row {}",
            row.id
        );
        // the baseline never leaves content on a hold, and a sample or key may carry none
        for layer in row.input.iter().chain(row.output.iter()) {
            assert!(
                layer.iter().all(|c| c.flag != '.' || c.strokes.is_empty()),
                "row {}",
                row.id
            );
        }
        assert_eq!(
            row.displayed.split(';').count() as u32,
            row.out_total,
            "row {}",
            row.id
        );
        // heldKeyframeAt, written a second time over the output flags
        for (layer, cells) in row.output.iter().enumerate() {
            let mut held = None;
            let mut text = String::new();
            for (frame, cell) in cells.iter().enumerate() {
                match cell.flag {
                    'K' => {
                        held = Some(frame as u32);
                        text.push('*');
                    }
                    'M' => text.push('!'),
                    'I' | 'N' => text.push('~'),
                    _ => text.push(held.map_or('-', |k| char::from_digit(k, 36).unwrap())),
                }
            }
            assert_eq!(text, row.exposure[layer], "row {} layer {layer}", row.id);
        }
        if !row.chg {
            assert_eq!(
                row.input, row.output,
                "row {}: an unchanged row must not differ",
                row.id
            );
        }
    }
}
