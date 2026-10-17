//! Oracle rows of the component evaluation join (L04-K): the fixture text parsed into the sets the
//! tests run. Row format and provenance: see `support`.

use crate::evaluation::property_camera_eval::CameraKey;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-evaluate/oracle.txt");

pub(crate) struct Expect {
    pub(crate) uid: String,
    pub(crate) frame: u32,
    pub(crate) want: Value,
}

pub(crate) struct InsideExpect {
    pub(crate) symbol: String,
    pub(crate) frame: u32,
    /// `(layerUid, authoredFrame, strokeId)` in baseline order.
    pub(crate) want: Vec<(String, u32, String)>,
}

#[derive(Default)]
pub(crate) struct Set {
    pub(crate) name: String,
    pub(crate) document: Value,
    pub(crate) canvas: Option<(f64, f64)>,
    pub(crate) cameras: BTreeMap<String, Vec<CameraKey>>,
    pub(crate) values: HashMap<(String, String), Value>,
    pub(crate) remap: HashMap<(String, u32), Option<f64>>,
    pub(crate) expects: Vec<Expect>,
    pub(crate) inside: Vec<InsideExpect>,
}

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle json")
}

fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().expect("number field")
}

fn pair(v: &Value) -> [f64; 2] {
    [
        v[0].as_f64().expect("pair 0"),
        v[1].as_f64().expect("pair 1"),
    ]
}

fn camera_key(v: &Value) -> CameraKey {
    CameraKey {
        frame: num(v, "frame"),
        x: num(v, "x"),
        y: num(v, "y"),
        w: num(v, "w"),
        rot: Some(num(v, "rot")),
        h_out: Some(pair(&v["hOut"])),
        h_in: Some(pair(&v["hIn"])),
        ease: v["ease"].as_array().map(|e| {
            let n: Vec<f64> = e.iter().map(|x| x.as_f64().expect("ease")).collect();
            [n[0], n[1], n[2], n[3]]
        }),
    }
}

/// The rows of one named set; the sets are contiguous in the fixture.
pub(crate) fn set(name: &str) -> Set {
    let mut found: Option<Set> = None;
    for line in ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "S" {
            if cols[1] == name {
                found = Some(Set {
                    name: name.to_owned(),
                    ..Set::default()
                });
            } else if found.is_some() {
                break;
            }
            continue;
        }
        let Some(current) = found.as_mut() else {
            continue;
        };
        match cols[0] {
            "G" => current.document = json_of(cols[1]),
            "P" => current.canvas = Some((cols[1].parse().unwrap(), cols[2].parse().unwrap())),
            "K" => {
                let keys = json_of(cols[2]);
                let keys = keys.as_array().expect("camera keys");
                current
                    .cameras
                    .insert(cols[1].to_owned(), keys.iter().map(camera_key).collect());
            }
            "V" => {
                current
                    .values
                    .insert((cols[1].to_owned(), cols[2].to_owned()), json_of(cols[3]));
            }
            "R" => {
                let value = json_of(cols[3]);
                current.remap.insert(
                    (cols[1].to_owned(), cols[2].parse().unwrap()),
                    value.as_f64(),
                );
            }
            "X" => current.expects.push(Expect {
                uid: cols[1].to_owned(),
                frame: cols[2].parse().expect("frame"),
                want: json_of(cols[3]),
            }),
            "I" => {
                let rows = json_of(cols[3]);
                let want = rows
                    .as_array()
                    .expect("inside rows")
                    .iter()
                    .map(|r| {
                        (
                            r[0].as_str().unwrap().to_owned(),
                            u32::try_from(r[1].as_u64().unwrap()).unwrap(),
                            r[2].as_str().unwrap().to_owned(),
                        )
                    })
                    .collect();
                current.inside.push(InsideExpect {
                    symbol: cols[1].to_owned(),
                    frame: cols[2].parse().expect("frame"),
                    want,
                });
            }
            other => panic!("unknown oracle row {other}"),
        }
    }
    found.unwrap_or_else(|| panic!("no oracle set {name}"))
}
