//! Oracle tests for the element-splitting plan (L04-H, D33 + F14).
//!
//! `fixtures/object-context-split/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_split_oracle.cjs` of the L04-H leaf, which cuts
//! `splitLayerIntoElements` and `splitLayerIntoElementsCore` out of `src/js/app.js`
//! and `enterComponentLayer`, `layerElements` and `elementLabel` out of
//! `src/js/motion.js` at baseline `fbe4b8a3` and runs them verbatim in a Node
//! `vm`. The header carries the blob hashes and the row grammar. `Y` rows describe
//! a layer, `E O P M` rows the baseline result for it, `H G G2` an entry.

use super::{plan_manual_split, plan_split, SplitPlan, SplitRefusal};
use crate::evaluation::object_component_kernels::object_context_split_facts::{
    element_label, layer_elements, unstamped_positions, CarryMode, ElementMotion, ElementWords,
    FieldState, FrameFacts, LayerFacts, StrokeFacts,
};
use std::collections::BTreeMap;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-context-split/oracle.txt");

pub(crate) struct Spec {
    pub(crate) id: String,
    pub(crate) play: i64,
    pub(crate) words: ElementWords,
    /// The layer without reference strokes; [`with_reference`] adds them.
    pub(crate) layer: LayerFacts,
}

pub(crate) struct LayerCase {
    pub(crate) spec: Spec,
    pub(crate) expected: Vec<String>,
}

pub(crate) struct EntryCase {
    pub(crate) id: String,
    pub(crate) total: i64,
    pub(crate) resolved: i64,
    pub(crate) specs: Vec<Spec>,
    pub(crate) g: String,
    pub(crate) g2: String,
}

pub(crate) fn enc(s: &str) -> String {
    s.replace(' ', "_")
}

pub(crate) fn list(items: Vec<String>) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(",")
    }
}

fn opt_id(tok: &str) -> Option<String> {
    if tok == "-" {
        None
    } else {
        Some(tok.to_string())
    }
}

fn stroke(tok: &str) -> StrokeFacts {
    let (id, flags) = tok.split_once('|').expect("stroke token");
    let mut s = StrokeFacts {
        stroke_id: opt_id(id),
        ..StrokeFacts::default()
    };
    for flag in flags.split('+').filter(|f| *f != "-") {
        match flag {
            "r" => s.is_raster = true,
            "f" => s.has_fill = true,
            "t" => s.is_brush_texture_copy = true,
            "l" => s.is_linked_fill_companion = true,
            other if other.starts_with("g:") => s.group_id = Some(other[2..].to_string()),
            other if other.starts_with("k:") => s.linked_fill_id = Some(other[2..].to_string()),
            other => panic!("flag {other}"),
        }
    }
    s
}

fn frame(tok: &str) -> FrameFacts {
    let code = tok.chars().next().unwrap();
    let inner = &tok[2..tok.len() - 1];
    FrameFacts {
        is_keyframe: code == 'K' || code == 'B',
        is_interpolated: code == 'I' || code == 'B',
        strokes: inner
            .split(',')
            .filter(|s| !s.is_empty())
            .map(stroke)
            .collect(),
    }
}

fn state(c: &str) -> FieldState {
    match c {
        "n" => FieldState::Null,
        "f" => FieldState::Falsy,
        "e" => FieldState::Empty,
        "v" => FieldState::Truthy,
        other => panic!("state {other}"),
    }
}

fn field<'a>(tokens: &[&'a str], key: &str) -> &'a str {
    let prefix = format!("{key}=");
    tokens
        .iter()
        .find_map(|t| t.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("no {key}"))
}

fn parse_spec(line: &str) -> Spec {
    let tokens: Vec<&str> = line.split(' ').collect();
    let pairs = |k: &str| -> Vec<(String, String)> {
        match field(&tokens, k) {
            "-" => Vec::new(),
            v => v
                .split(',')
                .map(|p| p.split_once(':').expect("pair"))
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    };
    let (shape, stroke_word) = field(&tokens, "words").split_once('/').unwrap();
    let name = match field(&tokens, "name") {
        "-" => None,
        "~" => Some(String::new()),
        n => Some(n.replace('_', " ")),
    };
    let groups = match field(&tokens, "groups") {
        "-" => None,
        "~" => Some(Vec::new()),
        g => Some(g.split(',').map(|s| s.to_string()).collect()),
    };
    let mut layer = LayerFacts {
        name,
        has_symbol: field(&tokens, "sym") == "1",
        // The generator joins zero frames to the empty string (`noFrames`), not to a token.
        frames: field(&tokens, "frames")
            .split(';')
            .filter(|t| !t.is_empty())
            .map(frame)
            .collect(),
        groups,
        ..LayerFacts::default()
    };
    for (id, n) in pairs("names") {
        layer.shape_names.insert(id, n);
    }
    for (id, m) in pairs("motion") {
        layer.element_motion.insert(
            id,
            ElementMotion {
                has_motion: m.contains('m'),
                has_motion_static: m.contains('s'),
            },
        );
    }
    for (f, s) in pairs("fields") {
        layer.fields.insert(f, state(&s));
    }
    Spec {
        id: tokens[1].to_string(),
        play: field(&tokens, "play").parse().unwrap(),
        words: ElementWords {
            shape: shape.to_string(),
            stroke: stroke_word.to_string(),
        },
        layer,
    }
}

pub(crate) fn world() -> (Vec<LayerCase>, Vec<EntryCase>) {
    let (mut layers, mut entries): (Vec<LayerCase>, Vec<EntryCase>) = (Vec::new(), Vec::new());
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
        let tag = line.split(' ').next().unwrap();
        match tag {
            "Y" if line.split(' ').nth(1).unwrap().contains('#') => {
                entries.last_mut().unwrap().specs.push(parse_spec(line))
            }
            "Y" => layers.push(LayerCase {
                spec: parse_spec(line),
                expected: Vec::new(),
            }),
            "E" | "O" | "P" | "M" => layers.last_mut().unwrap().expected.push(line.to_string()),
            "H" => {
                let t: Vec<&str> = line.split(' ').collect();
                let n = |k: &str| -> i64 {
                    t.iter()
                        .find_map(|x| x.strip_prefix(k))
                        .unwrap()
                        .parse()
                        .unwrap()
                };
                entries.push(EntryCase {
                    id: t[1].to_string(),
                    total: n("total="),
                    resolved: n("resolved="),
                    specs: Vec::new(),
                    g: String::new(),
                    g2: String::new(),
                });
            }
            "G" => entries.last_mut().unwrap().g = line.to_string(),
            "G2" => entries.last_mut().unwrap().g2 = line.to_string(),
            other => panic!("row tag {other}"),
        }
    }
    (layers, entries)
}

/// The generator's stand-in for `getEffectiveStrokes` on a plain layer: a keyframe
/// or interpolated frame shows its own strokes, any other frame the nearest
/// PRECEDING keyframe, the index clamped to the last frame.
pub(crate) fn effective_strokes(frames: &[FrameFacts], frame: i64) -> Vec<StrokeFacts> {
    let i = frame.min(frames.len() as i64 - 1);
    if i < 0 {
        return Vec::new();
    }
    let i = i as usize;
    if frames[i].is_keyframe || frames[i].is_interpolated {
        return frames[i].strokes.clone();
    }
    (0..i)
        .rev()
        .find(|&k| frames[k].is_keyframe)
        .map_or(Vec::new(), |k| frames[k].strokes.clone())
}

pub(crate) fn with_reference(layer: &LayerFacts, frame: i64) -> LayerFacts {
    LayerFacts {
        reference_strokes: effective_strokes(&layer.frames, frame),
        ..layer.clone()
    }
}

pub(crate) type Planned = Result<SplitPlan, SplitRefusal>;

/// A deviation seam: `prepare` edits the layer before planning, `post` the result.
pub(crate) struct Tweak {
    pub(crate) prepare: Box<dyn Fn(&LayerFacts) -> LayerFacts>,
    pub(crate) post: Box<dyn Fn(Planned, &LayerFacts) -> Planned>,
}

pub(crate) fn real() -> Tweak {
    Tweak {
        prepare: Box::new(|l| l.clone()),
        post: Box::new(|r, _| r),
    }
}

fn e_row(id: &str, layer: &LayerFacts, words: &ElementWords) -> String {
    if layer.has_symbol {
        return format!("E {id} sym");
    }
    let u: Vec<String> = unstamped_positions(layer)
        .iter()
        .map(|p| p.to_string())
        .collect();
    let els: Vec<String> = layer_elements(layer)
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let label = enc(&element_label(e, i, layer, words));
            format!("{}:{}", e.stroke_id.as_deref().unwrap_or("-"), label)
        })
        .collect();
    format!("E {id} u={} els={}", list(u), list(els))
}

fn p_row(id: &str, e: usize, plan: &SplitPlan, layer: &LayerFacts) -> String {
    let piece = &plan.pieces[e];
    let frames: Vec<String> = piece
        .frames
        .iter()
        .map(|f| {
            let code = match (f.is_keyframe, f.is_interpolated) {
                (true, true) => 'B',
                (true, false) => 'K',
                (false, true) => 'I',
                (false, false) => '.',
            };
            let idx: Vec<String> = f.stroke_indices.iter().map(|i| i.to_string()).collect();
            format!("{code}[{}]", idx.join(","))
        })
        .collect();
    let carry: Vec<String> = piece
        .carry
        .iter()
        .map(|c| {
            let object = matches!(
                layer.fields.get(c.field),
                Some(FieldState::Empty) | Some(FieldState::Truthy)
            );
            let tag = if c.mode == CarryMode::DeepCopy && object {
                'd'
            } else {
                'v'
            };
            format!("{}:{tag}", c.field)
        })
        .collect();
    let m = piece.matte;
    format!(
        "P {id} {e} name={} frames={} motion={}/{} carry={} matte={}/{}/{} uid={} groups={}",
        enc(&piece.name),
        frames.join(";"),
        piece.promote_motion as u8,
        piece.promote_motion_static as u8,
        list(carry),
        m.mode as u8,
        m.source_uid as u8,
        m.more as u8,
        piece.keeps_layer_uid as u8,
        list(piece.kept_groups.clone()),
    )
}

/// The `E O P M` rows of one layer through `tweak`.
pub(crate) fn scenario_rows(spec: &Spec, tweak: &Tweak) -> Vec<String> {
    let id = spec.id.as_str();
    let layer = (tweak.prepare)(&with_reference(&spec.layer, spec.play));
    let mut rows = vec![e_row(id, &layer, &spec.words)];
    let result = (tweak.post)(plan_split(0, &layer, &spec.words), &layer);
    let out = match &result {
        Ok(_) => "ok",
        Err(SplitRefusal::HasSymbol) => "nosym",
        Err(_) => "few",
    };
    rows.push(format!("O {id} {out}"));
    if let Ok(plan) = &result {
        for e in 0..plan.pieces.len() {
            rows.push(p_row(id, e, plan, &layer));
        }
    }
    let manual = plan_manual_split(Some("x"), 0, &layer, &spec.words);
    let open = manual == Err(SplitRefusal::ComponentOpen);
    rows.push(format!("M {id} {}", if open { "open" } else { "NOT-OPEN" }));
    rows
}

/// `(rows compared, rows that differ)` over every layer scenario.
pub(crate) fn compare_layers(tweak: &Tweak) -> (usize, usize) {
    let (cases, _) = world();
    let (mut total, mut wrong) = (0, 0);
    for case in &cases {
        let got = scenario_rows(&case.spec, tweak);
        total += case.expected.len();
        wrong += case.expected.iter().filter(|r| !got.contains(r)).count();
        wrong += got.len().abs_diff(case.expected.len());
    }
    (total, wrong)
}

#[test]
fn fixture_world_and_row_counts_are_pinned() {
    let (cases, entries) = world();
    assert_eq!((cases.len(), entries.len()), (98, 43));
    let count = |p: &str| {
        cases
            .iter()
            .flat_map(|c| c.expected.iter())
            .filter(|r| r.starts_with(p))
            .count()
    };
    assert_eq!(
        (count("E "), count("O "), count("P "), count("M ")),
        (98, 98, 149, 98)
    );
    let outs = |s: &str| {
        cases
            .iter()
            .flat_map(|c| c.expected.iter())
            .filter(|r| r.starts_with("O ") && r.ends_with(s))
            .count()
    };
    assert_eq!((outs(" ok"), outs(" few"), outs(" nosym")), (54, 38, 6));
    let header = ORACLE.lines().find(|l| l.starts_with("# layers ")).unwrap();
    assert_eq!(
        header,
        "# layers 98 entries 43 E 98 O 98 P 149 M 98 G 43 G2 43"
    );
}

#[test]
fn every_baseline_layer_row_is_reproduced() {
    let (total, wrong) = compare_layers(&real());
    assert_eq!(total, 98 + 98 + 149 + 98);
    assert_eq!(wrong, 0, "{wrong} of {total} rows differ from the baseline");
}

#[test]
fn the_manual_command_is_the_same_split_when_no_component_is_open() {
    let (cases, _) = world();
    for case in &cases {
        let layer = with_reference(&case.spec.layer, case.spec.play);
        let w = &case.spec.words;
        assert_eq!(
            plan_manual_split(None, 4, &layer, w),
            plan_split(4, &layer, w),
            "{}",
            case.spec.id
        );
        assert_eq!(
            plan_manual_split(Some("c"), 4, &layer, w),
            Err(SplitRefusal::ComponentOpen)
        );
    }
}

#[test]
fn plans_depend_only_on_the_layer_facts() {
    // Planning twice, or planning a clone, yields the same plan and edits nothing.
    let (cases, _) = world();
    let by_id: BTreeMap<&str, &LayerCase> = cases.iter().map(|c| (c.spec.id.as_str(), c)).collect();
    for id in ["two", "overflow", "groups", "fieldsAll"] {
        let layer = with_reference(&by_id[id].spec.layer, by_id[id].spec.play);
        let before = layer.clone();
        let w = &by_id[id].spec.words;
        assert_eq!(plan_split(0, &layer, w), plan_split(0, &before, w));
        assert_eq!(layer, before);
    }
}
