//! Oracle row parser for the rig edit transaction (L03-O, I12).
//!
//! `fixtures/object-rig-edit/oracle.txt` is produced by `tools/gen.cjs` from the baseline JS at fbe4b8a3:
//! the rig functions of `app.js` cut verbatim and executed in a Node `vm` with the shipped Paper.js, and
//! the statements of `rig-bridge.js` (bone write, pose write, influence-circle write, weight popover
//! handlers) cut verbatim from their text. One tab-separated row per edit:
//! `act  name  strokes  action  ret  before  after`.
//! - `strokes`: the Paper paths of the layer (`{id, closed, pts, hs}`; a `null` handle is the zero handle);
//! - `action`: the native `action` payload that performs the same edit;
//! - `ret`: what the baseline returned (`null` when it returned nothing, the new id for a drawn bone);
//! - `before` / `after`: the `exportJSON` whitelist of the rig around the edit (`null`: no rig).
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-rig-edit/oracle.txt");

/// One baseline edit.
pub(crate) struct ActRow {
    pub(crate) name: &'static str,
    pub(crate) strokes: Value,
    pub(crate) action: Value,
    pub(crate) ret: Value,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
}

impl ActRow {
    pub(crate) fn kind(&self) -> &str {
        self.action["kind"].as_str().expect("action kind")
    }

    /// The baseline changed the rig.
    pub(crate) fn changes(&self) -> bool {
        self.before != self.after
    }
}

pub(crate) fn act_rows() -> Vec<ActRow> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .map(|f| {
            assert_eq!(f.len(), 7, "oracle row shape: {}", f[1]);
            assert_eq!(f[0], "act");
            ActRow {
                name: f[1],
                strokes: serde_json::from_str(f[2]).expect("strokes json"),
                action: serde_json::from_str(f[3]).expect("action json"),
                ret: serde_json::from_str(f[4]).expect("ret json"),
                before: f[5],
                after: f[6],
            }
        })
        .collect()
}

pub(crate) fn act(name: &str) -> ActRow {
    act_rows()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no row {name}"))
}

/// The rows of one action kind, in oracle order.
pub(crate) fn of_kind(kind: &str) -> Vec<ActRow> {
    act_rows()
        .into_iter()
        .filter(|row| row.kind() == kind)
        .collect()
}
