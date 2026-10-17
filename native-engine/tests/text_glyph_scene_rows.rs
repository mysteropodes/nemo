//! Row parser of `fixtures/text-glyph-scene/oracle.txt` (G38b / L06-K); the grammar
//! is documented in `text_glyph_scene_oracle.rs`, which re-exports `read`.

use super::oracle::{Case, ExpectedPath, Face, FontTable, Oracle, Placed, Strict, ORACLE};
use crate::evaluation::motion_parent_chain::{ChainEntry, MotionMat};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    TextAlign, TextCase, TextStyle,
};
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    GlyphSegment, OutlineCommand,
};

fn num(token: &str) -> f64 {
    token
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("bad number {token:?}"))
}

fn index(token: &str) -> usize {
    token
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("bad index {token:?}"))
}

fn maybe(token: &str) -> Option<f64> {
    if token == "-" {
        None
    } else {
        Some(num(token))
    }
}

fn nums(tokens: &[&str]) -> Vec<f64> {
    tokens.iter().map(|t| num(t)).collect()
}

fn segment(tokens: &[&str]) -> GlyphSegment {
    let v = nums(tokens);
    assert_eq!(v.len(), 6, "segment rows carry six numbers");
    GlyphSegment {
        point: [v[0], v[1]],
        handle_in: [v[2], v[3]],
        handle_out: [v[4], v[5]],
    }
}

fn command(tokens: &[&str]) -> OutlineCommand {
    let v = nums(&tokens[1..]);
    match tokens[0] {
        "M" => OutlineCommand::Move { x: v[0], y: v[1] },
        "L" => OutlineCommand::Line { x: v[0], y: v[1] },
        "Q" => OutlineCommand::Quad {
            x1: v[0],
            y1: v[1],
            x: v[2],
            y: v[3],
        },
        "C" => OutlineCommand::Cubic {
            x1: v[0],
            y1: v[1],
            x2: v[2],
            y2: v[3],
            x: v[4],
            y: v[5],
        },
        "Z" => OutlineCommand::Close,
        other => panic!("unknown command {other:?}"),
    }
}

/// `dx dy rot sx sy op ax ay pivotX pivotY`.
fn stage(tokens: &[&str]) -> ChainEntry {
    let v = nums(tokens);
    assert_eq!(v.len(), 10, "stage rows carry ten numbers");
    ChainEntry {
        mat: MotionMat {
            dx: v[0],
            dy: v[1],
            rot: v[2],
            sx: v[3],
            sy: v[4],
            op: v[5],
            ax: v[6],
            ay: v[7],
        },
        pivot: [v[8], v[9]],
    }
}

fn units_text(hex: &str) -> String {
    let units: Vec<u16> = hex
        .split(',')
        .map(|h| u16::from_str_radix(h, 16).expect("hex unit"))
        .collect();
    String::from_utf16(&units).expect("valid UTF-16 in the oracle")
}

fn field<'a>(tokens: &'a [&'a str], key: &str) -> &'a str {
    tokens
        .iter()
        .find_map(|t| t.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
        .unwrap_or_else(|| panic!("missing {key}="))
}

fn new_case(tokens: &[&str]) -> Case {
    let pair = |value: &str| -> [f64; 2] {
        let (a, b) = value.split_once(',').expect("x,y");
        [num(a), num(b)]
    };
    let flag = |key: &str| field(tokens, key) == "1";
    let letter_spacing = maybe(field(tokens, "ls"));
    let word_spacing = maybe(field(tokens, "ws"));
    let text_case = match field(tokens, "tc") {
        "-" => TextCase::Unchanged,
        other => TextCase::from_baseline(other),
    };
    Case {
        name: tokens[1].to_string(),
        text: units_text(field(tokens, "t")),
        font: field(tokens, "font").to_string(),
        size: num(field(tokens, "size")),
        align: TextAlign::from_baseline(field(tokens, "align")),
        fixed_width: maybe(field(tokens, "fw")),
        fixed_height: maybe(field(tokens, "fh")),
        top_left: pair(field(tokens, "tl")),
        style: TextStyle {
            bold: flag("bold"),
            italic: flag("italic"),
            underline: flag("ul"),
            strike: flag("st"),
            letter_spacing: letter_spacing.unwrap_or(0.0),
            word_spacing: word_spacing.unwrap_or(0.0),
            line_height_mult: maybe(field(tokens, "lh")),
            text_case,
        },
        item_opacity: num(field(tokens, "itemop")),
        trig: flag("trig"),
        placed: Vec::new(),
        element: Vec::new(),
        motion: None,
        parents: Vec::new(),
        paths: Vec::new(),
        strict: Strict::Ok,
        dropped: Vec::new(),
    }
}

/// Which list a following `G` row extends.
enum Target {
    None,
    Path,
    Dropped,
}

pub(crate) fn read() -> Oracle {
    let mut fonts = FontTable::default();
    let mut cases: Vec<Case> = Vec::new();
    let mut target = Target::None;
    for line in ORACLE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens[0] {
            "F" => {
                let advances = tokens[3]
                    .split(',')
                    .map(|pair| {
                        let (unit, advance) = pair.split_once(':').expect("unit:advance");
                        (unit.parse::<u16>().expect("unit"), num(advance))
                    })
                    .collect();
                fonts.faces.push((
                    tokens[1].to_string(),
                    Face {
                        upm: num(tokens[2]),
                        advances,
                    },
                ));
            }
            "CASE" => cases.push(new_case(&tokens)),
            "GL" => {
                let case = cases.last_mut().expect("GL inside a case");
                case.placed.push(Placed {
                    key: tokens[1].to_string(),
                    unit: tokens[2].parse().expect("unit"),
                    x: num(tokens[3]),
                    y: num(tokens[4]),
                    size: num(tokens[5]),
                    commands: Vec::new(),
                });
            }
            "CMD" => {
                let case = cases.last_mut().expect("CMD inside a case");
                case.placed
                    .last_mut()
                    .expect("CMD follows GL")
                    .commands
                    .push(command(&tokens[1..]));
            }
            "MAT" => {
                let case = cases.last_mut().expect("MAT inside a case");
                match tokens[1] {
                    "el" => case.element.push((index(tokens[2]), stage(&tokens[3..]))),
                    "motion" => case.motion = Some(stage(&tokens[2..])),
                    "parent" => case.parents.push(stage(&tokens[2..])),
                    other => panic!("unknown stage {other:?}"),
                }
            }
            "OUT" => target = Target::Path,
            "P" => {
                let case = cases.last_mut().expect("P inside a case");
                let signed = |token: &str| -> Option<usize> {
                    if token == "-1" {
                        None
                    } else {
                        Some(index(token))
                    }
                };
                case.paths.push(ExpectedPath {
                    kind: tokens[1].chars().next().expect("kind"),
                    line: index(tokens[2]),
                    ordinal: signed(tokens[3]),
                    path_index: signed(tokens[4]),
                    opacity: num(tokens[6]),
                    segments: Vec::new(),
                });
                target = Target::Path;
            }
            "G" => {
                let case = cases.last_mut().expect("G inside a case");
                let row = segment(&tokens[1..]);
                match target {
                    Target::Path => case
                        .paths
                        .last_mut()
                        .expect("G follows P")
                        .segments
                        .push(row),
                    Target::Dropped => case.dropped.last_mut().expect("G follows PD").1.push(row),
                    Target::None => panic!("G row outside a path"),
                }
            }
            "STRICT" => {
                let case = cases.last_mut().expect("STRICT inside a case");
                case.strict = if tokens[1] == "ok" {
                    Strict::Ok
                } else {
                    Strict::Refuse {
                        position: index(tokens[2]),
                        why: tokens[3].to_string(),
                    }
                };
            }
            "DROP" => target = Target::Dropped,
            "PD" => {
                let case = cases.last_mut().expect("PD inside a case");
                case.dropped.push((index(tokens[1]), Vec::new()));
                target = Target::Dropped;
            }
            "END" => target = Target::None,
            other => panic!("unknown row {other:?}"),
        }
    }
    Oracle { fonts, cases }
}
