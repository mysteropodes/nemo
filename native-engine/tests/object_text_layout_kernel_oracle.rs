//! Oracle reader and stub ports shared by the text layout kernel tests (G38a / L06-A).
//!
//! `fixtures/object-text-layout-kernel/oracle.txt` (`ORACLE`, sha256
//! ae219225a90762ccf665318b6b9397e5f70738bd27c73496752999bbcec0f247) was produced
//! by `gen.js` (sha256 b319cc207044c8a50f537cdd98ec530bd7aca44b0241bfa6e586d7b4bc5b6bf7,
//! with `harness.js` 1bfc0dc79c7c061ce314158bd8c0747544b8ee2280439c8887a80b7d88313a1f;
//! both kept outside the repo, Node v25.9.0) running the REAL `buildVectorTextGroup`,
//! `wrapVectorLines` and `applyTextCase` of `src/js/vector-text-bridge.js` (blob
//! b7ba868854db6a13b23e87f3f8d992b01777da58) at fbe4b8a3 in a Node vm with
//! deterministic Paper/opentype stubs (the stub faces are the `F` rows, so the
//! Rust side measures with the very same advances).
//!
//! Rows (space separated, numbers are JS `String(n)`, `-` is undefined/null,
//! text is comma-joined UTF-16 hex units, `~` empty): `F key upm unit:adv,..`;
//! `CASE name t font size align fw fh tl bold italic ul st ls ws lh tc`;
//! then `OUT font width height overflow kept` | `FAIL message`, `L li t w sx`
//! per kept line (`-` where no decoration made it observable) and `I` items in
//! baseline paint order (`I U|S left w h cy`, `I G unit x y size ci wi li m`),
//! closed by `END`; `K case in out` rows pin `applyTextCase` alone. Numbers are
//! compared with `==` (the JS printout drops the sign of a zero).

use std::collections::HashMap;

use super::layout_text;
use super::model::{
    FontAdvancePort, FontCatalogPort, LaidOutText, TextAlign, TextCase, TextLayoutError,
    TextLayoutRequest, TextStyle,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-text-layout-kernel/oracle.txt");

pub(crate) struct StubFace {
    pub(crate) upm: f64,
    pub(crate) advances: HashMap<u16, f64>,
}

impl FontAdvancePort for StubFace {
    fn units_per_em(&self) -> f64 {
        self.upm
    }
    fn advance_units(&self, utf16_unit: u16) -> Option<f64> {
        self.advances.get(&utf16_unit).copied()
    }
}

#[derive(Default)]
pub(crate) struct StubCatalog {
    pub(crate) faces: Vec<(String, StubFace)>,
}

impl FontCatalogPort for StubCatalog {
    fn knows_key(&self, key: &str) -> bool {
        self.faces.iter().any(|(name, _)| name == key)
    }
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort> {
        self.faces
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, face)| face as &dyn FontAdvancePort)
    }
}

pub(crate) enum Item {
    Deco {
        kind: char,
        left: f64,
        width: f64,
        thickness: f64,
        center_y: f64,
    },
    Glyph {
        unit: u16,
        x: f64,
        y: f64,
        size: f64,
        ci: usize,
        wi: usize,
        li: usize,
        m: Option<[f64; 6]>,
    },
}

pub(crate) struct OracleLine {
    pub(crate) text: Vec<u16>,
    pub(crate) width: Option<f64>,
    pub(crate) start_x: Option<f64>,
}

pub(crate) struct Expected {
    pub(crate) font: String,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) overflow: bool,
    pub(crate) kept: usize,
    pub(crate) lines: Vec<OracleLine>,
    pub(crate) items: Vec<Item>,
}

pub(crate) enum Expect {
    Fail(String),
    Layout(Expected),
}

pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) text: Vec<u16>,
    pub(crate) font: String,
    pub(crate) size: f64,
    pub(crate) align: String,
    pub(crate) fixed_width: Option<f64>,
    pub(crate) fixed_height: Option<f64>,
    pub(crate) top_left: [f64; 2],
    pub(crate) style: TextStyle,
    pub(crate) expect: Expect,
}

pub(crate) struct CaseRow {
    pub(crate) name: String,
    pub(crate) input: Vec<u16>,
    pub(crate) output: Vec<u16>,
}

pub(crate) struct Oracle {
    pub(crate) catalog: StubCatalog,
    pub(crate) cases: Vec<Case>,
    pub(crate) case_rows: Vec<CaseRow>,
}

fn opt(token: &str) -> Option<f64> {
    if token == "-" {
        None
    } else {
        Some(num(token))
    }
}

fn num(token: &str) -> f64 {
    token
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("number {token}"))
}

pub(crate) fn units(token: &str) -> Vec<u16> {
    if token == "~" {
        return Vec::new();
    }
    token
        .split(',')
        .map(|hex| u16::from_str_radix(hex, 16).expect("hex unit"))
        .collect()
}

pub(crate) fn text_of(units: &[u16]) -> String {
    String::from_utf16(units).expect("well-formed utf16")
}

fn fields(tokens: &[&str]) -> HashMap<String, String> {
    tokens
        .iter()
        .filter_map(|t| t.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn parse_case(tokens: &[&str]) -> Case {
    let f = fields(tokens);
    let flag = |k: &str| f[k] == "1";
    let text_case = if f["tc"] == "-" { "" } else { f["tc"].as_str() };
    let style = TextStyle {
        bold: flag("bold"),
        italic: flag("italic"),
        underline: flag("ul"),
        strike: flag("st"),
        letter_spacing: opt(&f["ls"]).unwrap_or(0.0),
        word_spacing: opt(&f["ws"]).unwrap_or(0.0),
        // The baseline coerces an explicit 0 with `|| 1.25`; the native request
        // refuses Some(0.0), so the oracle's `lh=0` row is fed as the default.
        line_height_mult: opt(&f["lh"]).filter(|v| *v != 0.0),
        text_case: TextCase::from_baseline(text_case),
    };
    let tl: Vec<f64> = f["tl"].split(',').map(num).collect();
    Case {
        name: tokens[1].to_string(),
        text: units(&f["t"]),
        font: f["font"].clone(),
        size: num(&f["size"]),
        align: if f["align"] == "-" {
            String::new()
        } else {
            f["align"].clone()
        },
        fixed_width: opt(&f["fw"]),
        fixed_height: opt(&f["fh"]),
        top_left: [tl[0], tl[1]],
        style,
        expect: Expect::Fail(String::new()),
    }
}

fn parse_item(t: &[&str]) -> Item {
    if t[1] == "G" {
        let m = if t[9] == "-" {
            None
        } else {
            let v: Vec<f64> = t[9].split(',').map(num).collect();
            Some([v[0], v[1], v[2], v[3], v[4], v[5]])
        };
        Item::Glyph {
            unit: t[2].parse().expect("unit"),
            x: num(t[3]),
            y: num(t[4]),
            size: num(t[5]),
            ci: t[6].parse().expect("ci"),
            wi: t[7].parse().expect("wi"),
            li: t[8].parse().expect("li"),
            m,
        }
    } else {
        Item::Deco {
            kind: t[1].chars().next().expect("kind"),
            left: num(t[2]),
            width: num(t[3]),
            thickness: num(t[4]),
            center_y: num(t[5]),
        }
    }
}

pub(crate) fn parse_oracle() -> Oracle {
    let mut oracle = Oracle {
        catalog: StubCatalog::default(),
        cases: Vec::new(),
        case_rows: Vec::new(),
    };
    let mut current: Option<(Case, Option<Expected>)> = None;
    for row in ORACLE
        .lines()
        .filter(|r| !r.is_empty() && !r.starts_with('#'))
    {
        let t: Vec<&str> = row.split(' ').collect();
        match t[0] {
            "F" => {
                let advances = t[3]
                    .split(',')
                    .map(|pair| {
                        let (unit, adv) = pair.split_once(':').expect("unit:advance");
                        (unit.parse().expect("unit"), num(adv))
                    })
                    .collect();
                oracle.catalog.faces.push((
                    t[1].to_string(),
                    StubFace {
                        upm: num(t[2]),
                        advances,
                    },
                ));
            }
            "CASE" => current = Some((parse_case(&t), None)),
            "FAIL" => {
                let (case, _) = current.as_mut().expect("case");
                case.expect = Expect::Fail(t[1].to_string());
            }
            "OUT" => {
                let f = fields(&t);
                let expected = Expected {
                    font: f["font"].clone(),
                    width: num(&f["width"]),
                    height: num(&f["height"]),
                    overflow: f["overflow"] == "1",
                    kept: f["kept"].parse().expect("kept"),
                    lines: Vec::new(),
                    items: Vec::new(),
                };
                current.as_mut().expect("case").1 = Some(expected);
            }
            "L" => {
                let f = fields(&t);
                let line = OracleLine {
                    text: units(&f["t"]),
                    width: opt(&f["w"]),
                    start_x: opt(&f["sx"]),
                };
                current
                    .as_mut()
                    .and_then(|c| c.1.as_mut())
                    .expect("out")
                    .lines
                    .push(line);
            }
            "I" => current
                .as_mut()
                .and_then(|c| c.1.as_mut())
                .expect("out")
                .items
                .push(parse_item(&t)),
            "END" => {
                let (mut case, expected) = current.take().expect("case");
                if let Some(expected) = expected {
                    case.expect = Expect::Layout(expected);
                }
                oracle.cases.push(case);
            }
            "K" => oracle.case_rows.push(CaseRow {
                name: if t[1] == "-" {
                    String::new()
                } else {
                    t[1].to_string()
                },
                input: units(t[2]),
                output: units(t[3]),
            }),
            other => panic!("unknown oracle row {other}"),
        }
    }
    oracle
}

pub(crate) fn run_case(case: &Case, catalog: &StubCatalog) -> Result<LaidOutText, TextLayoutError> {
    let text = text_of(&case.text);
    let request = TextLayoutRequest {
        holder_id: "holder-1",
        text: &text,
        font_key: &case.font,
        size: case.size,
        align: TextAlign::from_baseline(&case.align),
        fixed_width: case.fixed_width,
        fixed_height: case.fixed_height,
        top_left: case.top_left,
        style: case.style,
    };
    layout_text(&request, Some(catalog))
}
