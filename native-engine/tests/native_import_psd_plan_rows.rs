//! Oracle fixture parser for the PSD import plan (L10-F). The fixture is generated from the baseline
//! `fbe4b8a3` by `tools/gen.mjs` (see HANDOFF.md); this file only decodes its rows.
use super::{PsdInput, PsdLayerInput};

pub(crate) const ORACLE: &str = include_str!("fixtures/native-import-psd-plan/oracle.txt");

/// `%XX` bytes, `$` is the empty string, `~` is null/undefined.
pub(crate) fn dec(token: &str) -> Option<String> {
    match token {
        "~" => None,
        "$" => Some(String::new()),
        _ => {
            let bytes = token.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut at = 0;
            while at < bytes.len() {
                if bytes[at] == b'%' {
                    let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap();
                    out.push(u8::from_str_radix(hex, 16).unwrap());
                    at += 3;
                } else {
                    out.push(bytes[at]);
                    at += 1;
                }
            }
            Some(String::from_utf8(out).unwrap())
        }
    }
}

/// A JavaScript number token as `String(v)` printed it; `~` is undefined/null.
pub(crate) fn num(token: &str) -> Option<f64> {
    match token {
        "~" => None,
        "NaN" => Some(f64::NAN),
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => Some(token.parse::<f64>().unwrap()),
    }
}

fn opt_u32(token: &str) -> Option<u32> {
    num(token).map(|v| v as u32)
}

/// What `importFile` did after the parse.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    Count(usize),
    Throw(String),
}

/// One Raster the baseline created, read after `onLoad`.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    pub(crate) source: usize,
    pub(crate) name: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) opacity: f64,
    pub(crate) visible: bool,
}

/// One baseline run of a PSD: the input, the canvas, every side effect.
#[derive(Clone, Debug)]
pub(crate) struct Case {
    pub(crate) id: String,
    pub(crate) psd: PsdInput,
    pub(crate) canvas: (f64, f64),
    pub(crate) resizes: Vec<(u32, u32)>,
    pub(crate) undo: u32,
    pub(crate) outcome: Outcome,
    pub(crate) placed: Vec<Placed>,
    /// `SMMediaLibrary.addEntry` calls: (source index, name, layerUid).
    pub(crate) media: Vec<(usize, String, String)>,
}

/// `T S` / `T F` rows.
#[derive(Clone, Debug)]
pub(crate) enum Toast {
    Success {
        count: usize,
        suffix: String,
        out: String,
    },
    Failure {
        prefix: String,
        message: Option<String>,
        out: String,
    },
}

fn layer(parts: &[&str]) -> PsdLayerInput {
    // L <case> <i> <name> <hidden> <canvasWxH|-> <left> <top> <opacity>
    let (width, height, has_canvas) = match parts[5] {
        "-" => (0, 0, false),
        wh => {
            let (w, h) = wh.split_once('x').unwrap();
            (w.parse().unwrap(), h.parse().unwrap(), true)
        }
    };
    PsdLayerInput {
        name: dec(parts[3]),
        hidden: parts[4] == "1",
        has_canvas,
        left: num(parts[6]),
        top: num(parts[7]),
        width,
        height,
        opacity: num(parts[8]),
    }
}

pub(crate) fn parse() -> (Vec<Case>, Vec<Toast>) {
    let mut cases: Vec<Case> = Vec::new();
    let mut toasts: Vec<Toast> = Vec::new();
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
        let p: Vec<&str> = line.split(' ').collect();
        match p[0] {
            "C" => cases.push(Case {
                id: p[1].to_string(),
                psd: PsdInput {
                    width: opt_u32(p[2]),
                    height: opt_u32(p[3]),
                    children: Vec::new(),
                },
                canvas: (num(p[4]).unwrap(), num(p[5]).unwrap()),
                resizes: Vec::new(),
                undo: 0,
                outcome: Outcome::Count(usize::MAX),
                placed: Vec::new(),
                media: Vec::new(),
            }),
            "L" | "X" | "P" | "M" => {
                let case = cases.last_mut().unwrap();
                assert_eq!(case.id, p[1], "rows of one case are contiguous");
                match (p[0], p[2]) {
                    ("L", _) => {
                        assert_eq!(case.psd.children.len(), p[2].parse::<usize>().unwrap());
                        case.psd.children.push(layer(&p));
                    }
                    ("X", "RESIZE") => case
                        .resizes
                        .push((p[3].parse().unwrap(), p[4].parse().unwrap())),
                    ("X", "UNDO") => case.undo = p[3].parse().unwrap(),
                    ("X", "COUNT") => case.outcome = Outcome::Count(p[3].parse().unwrap()),
                    ("X", "THROW") => case.outcome = Outcome::Throw(dec(p[3]).unwrap()),
                    ("P", _) => case.placed.push(Placed {
                        source: p[2].parse().unwrap(),
                        name: dec(p[3]).unwrap(),
                        x: num(p[4]).unwrap(),
                        y: num(p[5]).unwrap(),
                        w: p[6].parse().unwrap(),
                        h: p[7].parse().unwrap(),
                        opacity: num(p[8]).unwrap(),
                        visible: p[9] == "1",
                    }),
                    ("M", _) => case.media.push((
                        p[2].parse().unwrap(),
                        dec(p[3]).unwrap(),
                        p[4].to_string(),
                    )),
                    other => panic!("unknown row {other:?}"),
                }
            }
            "T" => {
                let out = dec(p[p.len() - 1]).unwrap();
                assert_eq!(p[p.len() - 2], "=>");
                toasts.push(if p[1] == "S" {
                    Toast::Success {
                        count: p[2].parse().unwrap(),
                        suffix: dec(p[3]).unwrap(),
                        out,
                    }
                } else {
                    Toast::Failure {
                        prefix: dec(p[2]).unwrap(),
                        message: dec(p[3]),
                        out,
                    }
                });
            }
            other => panic!("unknown row kind {other:?}"),
        }
    }
    (cases, toasts)
}
