//! Oracle rows for the key-retime transaction: this leaf's retime.txt, produced by running the verbatim baseline
//! `moveKeyframe` (with `captureTweenInbetweens` and `retimeTweenSpans`, fbe4b8a3) over two dense layers whose key and
//! in-between cells carry strokes, with some layers locked. Each row is rebuilt as a native document (the four forms of
//! L05-B's shared builder, locks applied), turned into ONE request payload, and compared back to the baseline,
//! including the key pairs the baseline handed `rekeyTweenPairData`. Support only: the tests are in
//! object_exposure_edit_retime_history.rs.
use super::build::{
    displayed, element_triples, expected_triples, exposure_string, render_layer, uid, untouched,
    Build, Cell, Stroke,
};
use crate::object_document::{ObjectDocument, Presence};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const RETIME: &str = include_str!("fixtures/object-exposure-edit-retime/retime.txt");

pub(crate) const OP_MOVE: &str = "command.document.object.layer.key.move";

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) layer: usize,
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) locks: Vec<bool>,
    pub(crate) chg: bool,
    pub(crate) outcome: String,
    pub(crate) total: u32,
    pub(crate) applied: Option<u32>,
    pub(crate) input: Vec<Vec<Cell>>,
    pub(crate) output: Vec<Vec<Cell>>,
    pub(crate) out_tok: Vec<String>,
    pub(crate) exposure: Vec<String>,
    pub(crate) displayed: Vec<String>,
    pub(crate) pairs: Vec<[u32; 4]>,
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
/// `fA-fB>newFA-newFB;...`, or `-` when the baseline made no `rekeyTweenPairData` call.
fn parse_pairs(text: &str) -> Vec<[u32; 4]> {
    if text == "-" {
        return Vec::new();
    }
    text.split(';')
        .map(|pair| {
            let (old, new) = pair.split_once('>').unwrap();
            let (a, b) = old.split_once('-').unwrap();
            let (c, d) = new.split_once('-').unwrap();
            [a, b, c, d].map(|frame| frame.parse().unwrap())
        })
        .collect()
}

/// Every row of retime.txt, in file order.
pub(crate) fn rows() -> Vec<Row> {
    let mut out = Vec::new();
    for line in RETIME.lines().filter(|line| line.starts_with("row ")) {
        let part: Vec<&str> = line.split(' ').collect();
        assert_eq!(part.len(), 13, "{line}");
        let args: Vec<&str> = part[2].split(':').collect();
        out.push(Row {
            id: part[1].parse().unwrap(),
            layer: args[0].parse().unwrap(),
            from: args[1].parse().unwrap(),
            to: args[2].parse().unwrap(),
            locks: part[3].chars().map(|flag| flag == '1').collect(),
            chg: part[4] == "1",
            outcome: part[5].to_owned(),
            total: part[6].parse().unwrap(),
            applied: part[7].parse().ok(),
            input: parse_layers(part[8]),
            output: parse_layers(part[9]),
            out_tok: part[9].split('|').map(str::to_owned).collect(),
            exposure: part[10].split('|').map(str::to_owned).collect(),
            displayed: part[11].split('|').map(str::to_owned).collect(),
            pairs: parse_pairs(part[12]),
        });
    }
    out
}

impl Row {
    pub(crate) fn label(&self) -> String {
        format!("retime row {}", self.id)
    }
    /// A row that needs a locked layer, which only the accepted F06 admission lets a native document carry.
    pub(crate) fn any_lock(&self) -> bool {
        self.locks.iter().any(|locked| *locked)
    }
    /// The request payload that reproduces the baseline call.
    pub(crate) fn payload(&self) -> Value {
        json!({"command":"object.layer.key.move","layerUid":uid(self.layer),"from":self.from,"to":self.to})
    }
    /// The typed refusal code the outcome class implies, `None` when the request is accepted.
    pub(crate) fn refusal(&self) -> Option<&'static str> {
        match self.outcome.as_str() {
            "changed" | "noop" | "same" => None,
            "locked" => Some("unavailable"),
            "range" | "notkey" => Some("invalid_request"),
            other => panic!("unknown outcome {other}"),
        }
    }
    /// The subject layer's cells before and after.
    pub(crate) fn subject(&self) -> (&[Cell], &[Cell]) {
        (&self.input[self.layer], &self.output[self.layer])
    }
}

/// The input document of a row in one form, with the row's locks applied.
pub(crate) fn input_document(row: &Row, form: Build) -> ObjectDocument {
    let mut input = super::build::document(&row.input, form);
    for (layer, locked) in row.locks.iter().enumerate() {
        if *locked {
            input.layers[layer].locked = Presence::Present(true);
        }
    }
    input
}

/// The shared oracle comparison; the controls run it on deliberately wrong outputs.
pub(crate) fn compare(
    row: &Row,
    build: Build,
    input: &ObjectDocument,
    out: &ObjectDocument,
) -> Result<(), String> {
    if out.total_frames != row.total {
        return Err(format!("totalFrames {} != {}", out.total_frames, row.total));
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
        let shown = displayed(out, layer);
        if shown != row.displayed[layer] {
            return Err(format!(
                "layer {layer} displayed {shown} != {}",
                row.displayed[layer]
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
        if out.layers[layer].locked != input.layers[layer].locked {
            return Err(format!("layer {layer} lock flag moved"));
        }
    }
    if untouched(input) != untouched(out) {
        return Err(
            "a record, context, policy or property holder that must not change changed".into(),
        );
    }
    ObjectSnapshot::validate_drawing(out).map_err(|error| format!("storage admission: {error}"))
}
