//! Oracle rows for the frame-structure transactions: L05-B's documents.txt (insert frame `gap`, remove `rm`)
//! and this leaf's structure.txt (`extend`, `dup`), both produced by running the verbatim baseline editor
//! (fbe4b8a3) over dense layers whose key and in-between cells carry strokes. Each row is rebuilt as a native
//! document (the four forms of the shared builder), turned into ONE request payload, and compared back to
//! the baseline. Support only: the tests are in object_frame_edit_structure_history.rs.
use super::build::{
    displayed, element_triples, expected_triples, exposure_string, render_layer, uid, untouched,
    Build, Cell, Stroke,
};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const DOCUMENTS: &str = include_str!("fixtures/object-frame-edit-baseline/documents.txt");
const STRUCTURE: &str = include_str!("fixtures/object-frame-edit-baseline/structure.txt");

pub(crate) const OP_INSERT: &str = "command.document.object.frames.insert";
pub(crate) const OP_EXTEND: &str = "command.document.object.frames.extend";
pub(crate) const OP_DUPLICATE: &str = "command.document.object.frames.duplicate";
pub(crate) const OP_REMOVE: &str = "command.document.object.frames.remove";

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) op: String,
    pub(crate) args: String,
    pub(crate) roles: String,
    pub(crate) chg: bool,
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
fn numbers(text: &str) -> (u32, u32) {
    let (a, b) = text.split_once(':').unwrap();
    (a.parse().unwrap(), b.parse().unwrap())
}

/// Insert frame and remove rows from L05-B's file, extend and duplicate rows from this leaf's.
pub(crate) fn rows() -> Vec<Row> {
    let mut out = Vec::new();
    for (text, ops) in [
        (DOCUMENTS, &["gap", "rm"][..]),
        (STRUCTURE, &["extend", "dup"][..]),
    ] {
        for line in text.lines().filter(|line| line.starts_with("row ")) {
            let part: Vec<&str> = line.split(' ').collect();
            assert_eq!(part.len(), 13, "{line}");
            if !ops.contains(&part[2]) {
                continue;
            }
            out.push(Row {
                id: part[1].parse().unwrap(),
                op: part[2].to_owned(),
                args: part[4].to_owned(),
                roles: part[5].to_owned(),
                chg: part[6] == "1",
                out_total: part[8].parse().unwrap(),
                input: parse_layers(part[9]),
                output: parse_layers(part[10]),
                out_tok: part[10].split('|').map(str::to_owned).collect(),
                exposure: part[11].split('|').map(str::to_owned).collect(),
                displayed: part[12].to_owned(),
            });
        }
    }
    out
}

impl Row {
    pub(crate) fn label(&self) -> String {
        format!("{} row {}", self.op, self.id)
    }
    pub(crate) fn operation(&self) -> &'static str {
        match self.op.as_str() {
            "gap" => OP_INSERT,
            "rm" => OP_REMOVE,
            "extend" => OP_EXTEND,
            "dup" => OP_DUPLICATE,
            other => panic!("unknown op {other}"),
        }
    }
    /// The request payload that reproduces the baseline run. Insert frame names the layers the baseline
    /// actually spliced (ROLES G) unless that is every layer, which is the baseline's no-selection default.
    pub(crate) fn payload(&self) -> Value {
        match self.op.as_str() {
            "gap" => {
                let targeted: Vec<String> = self
                    .roles
                    .chars()
                    .enumerate()
                    .filter(|(_, role)| *role == 'G')
                    .map(|(index, _)| uid(index))
                    .collect();
                let mut payload = json!({"command":"object.frames.insert",
                    "cell":self.args.parse::<u32>().unwrap()});
                if targeted.len() != self.roles.len() {
                    payload["layerUids"] = json!(targeted);
                }
                payload
            }
            "rm" => {
                let (from, to) = numbers(&self.args);
                json!({"command":"object.frames.remove","from":from,"to":to})
            }
            "extend" => {
                let (cell, count) = numbers(&self.args);
                json!({"command":"object.frames.extend","cell":cell,"count":count})
            }
            "dup" => json!({"command":"object.frames.duplicate","layerUid":"L0",
                "cell":self.args.parse::<u32>().unwrap()}),
            other => panic!("unknown op {other}"),
        }
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
            "a record, context, policy or property holder that must not change changed".into(),
        );
    }
    ObjectSnapshot::validate_drawing(out).map_err(|error| format!("storage admission: {error}"))
}
