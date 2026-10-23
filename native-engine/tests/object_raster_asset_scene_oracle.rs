//! Reader for `fixtures/object-raster-asset-scene/oracle.txt` (G01 / L06-K) and
//! the pixel port double shared by the raster scene tests.
//!
//! The fixture is produced by `tools/gen_raster.js` from the REAL baseline at
//! fbe4b8a3: engine-bridge.js lines 1434-1437 and 1471 (verbatim) over motion.js
//! `transformImageRect`; the `A` rows mirror `paint_layer_items` of the baseline
//! Rust engine (not JavaScript). Rows: `CASE name pix=WxH rect=x,y,w,h,rotation
//! opacity=<n|-> trig=<0|1>`, `MAT el|motion|parent dx dy rot sx sy op ax ay pivotX
//! pivotY` (stage order), `R x y width height rotation opacity`, `A a b c d e f`,
//! `END`. Numbers are JS `String(n)`; `-0` prints `0`, so exact rows use `==`.

use super::{RasterPixelPort, RasterPixels, RasterSceneSource};
use crate::evaluation::motion_parent_chain::{ChainEntry, ImageRect, MotionMat};
use std::sync::Arc;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-raster-asset-scene/oracle.txt");

/// Rotation-coupled values may differ from V8's libm in the last places.
pub(crate) const TRIG_TOLERANCE: f64 = 1e-9;

pub(crate) const ASSET: &str = "asset-1";

pub(crate) type Handle = Arc<Vec<u8>>;

#[derive(Debug)]
pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) pixels: (u32, u32),
    pub(crate) rect: ImageRect,
    pub(crate) opacity: Option<f64>,
    pub(crate) trig: bool,
    pub(crate) element: Option<ChainEntry>,
    pub(crate) motion: Option<ChainEntry>,
    pub(crate) parents: Vec<ChainEntry>,
    /// `x y width height rotation opacity` of the baseline `imgItem`.
    pub(crate) want_rect: [f64; 6],
    /// kurbo coefficients `[a, b, c, d, e, f]`.
    pub(crate) want_place: [f64; 6],
}

impl Case {
    pub(crate) fn source(&self) -> RasterSceneSource {
        RasterSceneSource {
            asset_id: ASSET.to_owned(),
            rect: self.rect,
            // `c.opacity !== undefined ? c.opacity : 1`
            opacity: self.opacity.unwrap_or(1.0),
        }
    }

    /// Element stage, layer stage, then the parents: the baseline order.
    pub(crate) fn stages(&self) -> Vec<ChainEntry> {
        self.element
            .iter()
            .chain(self.motion.iter())
            .chain(self.parents.iter())
            .copied()
            .collect()
    }
}

fn num(token: &str) -> f64 {
    token
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("bad number {token:?}"))
}

fn nums(tokens: &[&str]) -> Vec<f64> {
    tokens.iter().map(|t| num(t)).collect()
}

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

fn field<'a>(tokens: &'a [&'a str], key: &str) -> &'a str {
    tokens
        .iter()
        .find_map(|t| t.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
        .unwrap_or_else(|| panic!("missing {key}="))
}

fn six(tokens: &[&str]) -> [f64; 6] {
    let v = nums(tokens);
    assert_eq!(v.len(), 6);
    [v[0], v[1], v[2], v[3], v[4], v[5]]
}

pub(crate) fn read() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in ORACLE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens[0] {
            "CASE" => {
                let (w, h) = field(&tokens, "pix").split_once('x').expect("WxH");
                let r = nums(&field(&tokens, "rect").split(',').collect::<Vec<_>>());
                cases.push(Case {
                    name: tokens[1].to_owned(),
                    pixels: (w.parse().expect("w"), h.parse().expect("h")),
                    rect: ImageRect {
                        x: r[0],
                        y: r[1],
                        width: r[2],
                        height: r[3],
                        rotation: r[4],
                    },
                    opacity: match field(&tokens, "opacity") {
                        "-" => None,
                        other => Some(num(other)),
                    },
                    trig: field(&tokens, "trig") == "1",
                    element: None,
                    motion: None,
                    parents: Vec::new(),
                    want_rect: [0.0; 6],
                    want_place: [0.0; 6],
                });
            }
            "MAT" => {
                let case = cases.last_mut().expect("MAT inside a case");
                let entry = stage(&tokens[2..]);
                match tokens[1] {
                    "el" => case.element = Some(entry),
                    "motion" => case.motion = Some(entry),
                    "parent" => case.parents.push(entry),
                    other => panic!("unknown stage {other:?}"),
                }
            }
            "R" => cases.last_mut().expect("R inside a case").want_rect = six(&tokens[1..]),
            "A" => cases.last_mut().expect("A inside a case").want_place = six(&tokens[1..]),
            "END" => {}
            other => panic!("unknown row {other:?}"),
        }
    }
    cases
}

/// One asset with real (tiny) RGBA bytes behind a shared handle: the port the
/// production `AssetPixels` implements in the join that owns it.
pub(crate) struct Pixels {
    pub(crate) handle: Handle,
    pub(crate) size: (u32, u32),
    pub(crate) version: String,
}

impl Pixels {
    pub(crate) fn new(size: (u32, u32)) -> Self {
        Pixels {
            handle: Arc::new(vec![0x7f, 0x20, 0x40, 0xff]),
            size,
            version: "sha256:test-version".to_owned(),
        }
    }
}

impl RasterPixelPort for Pixels {
    type Handle = Handle;
    fn pixels(&self, asset_id: &str) -> Option<RasterPixels<Handle>> {
        (asset_id == ASSET).then(|| RasterPixels {
            handle: Arc::clone(&self.handle),
            width: self.size.0,
            height: self.size.1,
            version: self.version.clone(),
        })
    }
}
