//! Oracle rows for Repeat Selection: oracle.txt, produced by running the verbatim baseline editor (fbe4b8a3:
//! `repeatSelection`, `selBounds`, `heldKeyframeAt`, the plain-layer tail of `getEffectiveStrokes`) over dense layers
//! whose key and in-between cells carry strokes. Each row is rebuilt as a native document in four forms, turned into
//! the real command and compared back to the baseline. Support only: the tests are in the files that include this one.
use super::build::{
    displayed, element_triples, expected_triples, exposure_string, render_layer, uid, untouched,
    Build, Cell, Stroke,
};
use crate::commands::RepeatRequest;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::json;

const ORACLE: &str = include_str!("fixtures/object-frame-clipboard-repeat/oracle.txt");

pub(crate) const OP_REPEAT: &str = "command.document.object.frames.repeat";
/// The four document forms every row runs on: v1, v1 with Reference records, v2 order plan, v2 with Reference records
/// and a second context.
pub(crate) const FORMS: [Build; 4] = Build::ALL;
/// The faulty argument choices the oracle records: times-1, times+1, bounds one frame short at the end / at the start,
/// bounds one layer short at the end. Position in the row's MUT bit string.
pub(crate) const MUTANTS: [&str; 5] = ["T-", "T+", "F-", "F0", "L-"];

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) locked: Vec<bool>,
    pub(crate) sel: Vec<(usize, u32)>,
    pub(crate) times: i64,
    pub(crate) chg: bool,
    pub(crate) total: u32,
    pub(crate) wa_out: u32,
    pub(crate) input: Vec<Vec<Cell>>,
    pub(crate) output: Vec<Vec<Cell>>,
    pub(crate) out_tok: Vec<String>,
    pub(crate) exposure: Vec<String>,
    pub(crate) displayed: Vec<String>,
    pub(crate) mutants: String,
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
            assert_eq!(part.len(), 14, "{line}");
            assert_eq!(part[2], "repeat", "{line}");
            Row {
                id: part[1].parse().unwrap(),
                locked: part[3].chars().map(|letter| letter == 'L').collect(),
                sel: part[4]
                    .split(',')
                    .map(|cell| {
                        let (layer, frame) = cell.split_once(':').unwrap();
                        (layer.parse().unwrap(), frame.parse().unwrap())
                    })
                    .collect(),
                times: part[5].parse().unwrap(),
                chg: part[6] == "1",
                total: part[7].parse().unwrap(),
                wa_out: part[8].parse().unwrap(),
                input: parse_layers(part[9]),
                output: parse_layers(part[10]),
                out_tok: part[10].split('|').map(str::to_owned).collect(),
                exposure: part[11].split('|').map(str::to_owned).collect(),
                displayed: part[12].split('|').map(str::to_owned).collect(),
                mutants: part[13].to_owned(),
            }
        })
        .collect()
}

impl Row {
    pub(crate) fn label(&self) -> String {
        format!("repeat row {}", self.id)
    }
    pub(crate) fn any_locked(&self) -> bool {
        self.locked.iter().any(|locked| *locked)
    }
    pub(crate) fn total_in(&self) -> u32 {
        self.input[0].len() as u32
    }
    /// The selection bounds, as `selBounds()` reduces them: (min layer, max layer, min frame, max frame).
    pub(crate) fn bounds(&self) -> (usize, usize, u32, u32) {
        let layers = self.sel.iter().map(|cell| cell.0);
        let frames = self.sel.iter().map(|cell| cell.1);
        (
            layers.clone().min().unwrap(),
            layers.max().unwrap(),
            frames.clone().min().unwrap(),
            frames.max().unwrap(),
        )
    }
    pub(crate) fn request(&self) -> RepeatRequest {
        let (min_l, max_l, min_f, max_f) = self.bounds();
        RepeatRequest {
            from_layer: uid(min_l),
            to_layer: uid(max_l),
            from_frame: min_f,
            to_frame: max_f,
            times: self.times,
        }
    }
    /// The request a faulty transaction would amount to (the same definition the generator ran on the baseline).
    pub(crate) fn mutant_request(&self, name: &str) -> RepeatRequest {
        let (min_l, max_l, min_f, max_f) = self.bounds();
        let mut request = self.request();
        match name {
            "T-" => request.times -= 1,
            "T+" => request.times += 1,
            "F-" if max_f > min_f => request.to_frame = max_f - 1,
            "F0" if max_f > min_f => request.from_frame = min_f + 1,
            "L-" if max_l > min_l => request.to_layer = uid(max_l - 1),
            "F-" | "F0" | "L-" => {}
            other => panic!("unknown mutant {other}"),
        }
        request
    }
    /// True when the verbatim baseline run with the faulty arguments differs from the real run.
    pub(crate) fn mutant_differs(&self, name: &str) -> bool {
        let at = MUTANTS.iter().position(|known| *known == name).unwrap();
        self.mutants.as_bytes()[at] == b'1'
    }
    /// The native document of the row's input: `locked: true` is authored on the marked layers (the baseline ignores
    /// it, so the expected output is the same).
    pub(crate) fn document(&self, form: Build) -> ObjectDocument {
        let mut value = super::build::document_json(&self.input, form);
        for (index, locked) in self.locked.iter().enumerate() {
            if *locked {
                value["layers"][index]["locked"] = json!(true);
            }
        }
        decode_storage_project(value.to_string().as_bytes()).expect("locked layers are admitted")
    }
}

/// The shared oracle comparison; the controls run it on deliberately wrong outputs.
pub(crate) fn compare(
    row: &Row,
    input: &ObjectDocument,
    out: &ObjectDocument,
    form: Build,
) -> Result<(), String> {
    if out.total_frames != row.total {
        return Err(format!("totalFrames {} != {}", out.total_frames, row.total));
    }
    for layer in 0..row.output.len() {
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
        if form.v2() {
            for (frame, cell) in row.output[layer].iter().enumerate() {
                let got = element_triples(out, &uid(layer), frame as u32);
                if got != expected_triples(cell) {
                    return Err(format!(
                        "layer {layer} frame {frame} order elements {got:?}"
                    ));
                }
            }
        }
    }
    if untouched(input) != untouched(out) {
        return Err(
            "a record, context, policy or property holder that must not change changed".into(),
        );
    }
    ObjectSnapshot::validate_drawing(out).map_err(|error| format!("storage admission: {error}"))
}
