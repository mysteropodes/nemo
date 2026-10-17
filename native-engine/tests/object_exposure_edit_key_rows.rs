//! Oracle rows for the key-exposure transactions: this leaf's keys.txt, produced by running the verbatim baseline editor
//! (insertKeyframe, insertKeyframeAt, convertToKeyframes, clearKeyframe, deleteSelectedFrames, removeTweenSpan at fbe4b8a3)
//! over several dense layers whose key and in-between cells carry strokes, with some layers locked. Each row is rebuilt as a
//! native document (the four forms of L05-B's shared builder, locks applied), turned into ONE request payload, and compared
//! back to the baseline, including the typed outcome the baseline's own toast implies. Support only: the tests are in
//! object_exposure_edit_key_history.rs.
use super::build::{
    displayed, element_triples, expected_triples, exposure_string, render_layer, uid, untouched,
    Build, Cell, Stroke,
};
use crate::object_document::{ObjectDocument, Presence};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const KEYS: &str = include_str!("fixtures/object-exposure-edit-key/keys.txt");

pub(crate) const OP_PROMOTE: &str = "command.document.object.layer.key.promote";
pub(crate) const OP_CLEAR: &str = "command.document.object.layer.key.clear";
pub(crate) const OP_CLEAR_CELLS: &str = "command.document.object.layer.key.clear_cells";
pub(crate) const OP_TWEEN: &str = "command.document.object.layer.tween.remove";

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) op: String,
    pub(crate) args: String,
    pub(crate) locks: Vec<bool>,
    pub(crate) sel: String,
    pub(crate) chg: bool,
    pub(crate) outcome: String,
    pub(crate) total: u32,
    pub(crate) input: Vec<Vec<Cell>>,
    pub(crate) output: Vec<Vec<Cell>>,
    pub(crate) out_tok: Vec<String>,
    pub(crate) exposure: Vec<String>,
    pub(crate) displayed: Vec<String>,
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
fn pair(text: &str) -> (usize, u32) {
    let (layer, frame) = text.split_once(':').unwrap();
    (layer.parse().unwrap(), frame.parse().unwrap())
}

/// Every row of keys.txt, in file order.
pub(crate) fn rows() -> Vec<Row> {
    let mut out = Vec::new();
    for line in KEYS.lines().filter(|line| line.starts_with("row ")) {
        let part: Vec<&str> = line.split(' ').collect();
        assert_eq!(part.len(), 13, "{line}");
        out.push(Row {
            id: part[1].parse().unwrap(),
            op: part[2].to_owned(),
            args: part[3].to_owned(),
            locks: part[4].chars().map(|flag| flag == '1').collect(),
            sel: part[5].to_owned(),
            chg: part[6] == "1",
            outcome: part[7].to_owned(),
            total: part[8].parse().unwrap(),
            input: parse_layers(part[9]),
            output: parse_layers(part[10]),
            out_tok: part[10].split('|').map(str::to_owned).collect(),
            exposure: part[11].split('|').map(str::to_owned).collect(),
            displayed: part[12].split('|').map(str::to_owned).collect(),
        });
    }
    out
}

impl Row {
    pub(crate) fn label(&self) -> String {
        format!("{} row {}", self.op, self.id)
    }
    /// A row that needs a locked layer, which only the accepted F06 admission lets a native document carry.
    pub(crate) fn any_lock(&self) -> bool {
        self.locks.iter().any(|locked| *locked)
    }
    pub(crate) fn operation(&self) -> &'static str {
        match self.op.as_str() {
            "f6" | "at" | "conv" => OP_PROMOTE,
            "clearkey" => OP_CLEAR,
            "dsel" => OP_CLEAR_CELLS,
            "demote" => OP_TWEEN,
            other => panic!("unknown op {other}"),
        }
    }
    /// The conv selection restricted to the active layer, in selection order (the baseline ignores other layers' cells).
    pub(crate) fn conv_cells(&self) -> Vec<u32> {
        let (active, list) = self.args.split_once('/').unwrap();
        let active: usize = active.parse().unwrap();
        list.split(',')
            .map(pair)
            .filter(|(layer, _)| *layer == active)
            .map(|(_, frame)| frame)
            .collect()
    }
    /// The request payload that reproduces the baseline run.
    pub(crate) fn payload(&self) -> Value {
        match self.op.as_str() {
            "f6" => {
                let cell = self.args.parse::<u32>().unwrap();
                let mut payload = json!({"command":"object.layer.key.promote","cells":[cell]});
                if self.sel != "-" {
                    let named: Vec<String> = self
                        .sel
                        .split(',')
                        .map(|layer| uid(layer.parse().unwrap()))
                        .collect();
                    payload["layerUids"] = json!(named);
                }
                payload
            }
            "at" => {
                let (layer, frame) = pair(&self.args);
                json!({"command":"object.layer.key.promote","cells":[frame],"layerUids":[uid(layer)]})
            }
            "conv" => {
                let active: usize = self.args.split_once('/').unwrap().0.parse().unwrap();
                json!({"command":"object.layer.key.promote","cells":self.conv_cells(),
                    "layerUids":[uid(active)]})
            }
            "clearkey" => {
                let (layer, frame) = pair(&self.args);
                json!({"command":"object.layer.key.clear","layerUid":uid(layer),"cell":frame})
            }
            "dsel" => {
                let targets: Vec<Value> = self
                    .args
                    .split(',')
                    .map(pair)
                    .map(|(layer, frame)| json!({"layerUid":uid(layer),"frame":frame}))
                    .collect();
                json!({"command":"object.layer.key.clear_cells","targets":targets})
            }
            "demote" => {
                let part: Vec<&str> = self.args.split(':').collect();
                json!({"command":"object.layer.tween.remove","layerUid":uid(part[0].parse().unwrap()),
                    "from":part[1].parse::<u32>().unwrap(),"to":part[2].parse::<u32>().unwrap()})
            }
            other => panic!("unknown op {other}"),
        }
    }
    /// How many layers differ between the baseline input and output.
    pub(crate) fn changed_layers(&self) -> usize {
        (0..self.input.len())
            .filter(|layer| self.input[*layer] != self.output[*layer])
            .count()
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
