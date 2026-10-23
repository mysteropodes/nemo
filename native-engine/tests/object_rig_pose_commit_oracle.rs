//! Oracle row parser for the pose transaction tests (L03-Q, I13). No test lives here.
//!
//! `fixtures/object-rig-pose-commit/oracle.txt` is produced by `tools/gen.cjs` from the baseline JS at
//! fbe4b8a3: the rig functions of `app.js` cut verbatim (including `rigResetPose` and `rigCommitFrame`)
//! and executed in a Node `vm` with the shipped Paper.js. One tab-separated row per scene:
//! `pose  name  rig  strokes  meshes  expect`.
//! - `rig`: the `exportJSON` whitelist of the rig around the pose (the text `null`: the layer never had one);
//! - `strokes`: `[{id, closed, segs:[[px,py,hix,hiy,hox,hoy]]}]`, the stored geometry BEFORE any pose;
//! - `meshes`: `{meshId:{outline, verts, offsets, tris, rect}}`, the mesh store BEFORE any pose;
//! - `expect`: `{kept:[bool per bind], reset:{throws, rigAfter, live, meshOffsets, restAfterEdit},
//!   commit:{ret, toasts, undo, saved, savedMeshOffsets, rigAfter}}`. `kept` says which binds the real
//!   `relinkRigBinds` keeps; `reset.rigAfter` carries the BEFORE `binds` because the native record never
//!   drops an unlinked bind; `commit.saved` is what `saveActiveLayerFrame` would store.
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-rig-pose-commit/oracle.txt");
/// Relative tolerance of a posed number: V8 and the platform libm differ in the last bits of cos/sin/atan2.
pub(crate) const REL_TOL: f64 = 1e-9;

/// One baseline scene.
pub(crate) struct Row {
    pub(crate) name: &'static str,
    /// The rig export text exactly as the baseline wrote it (`null` for a layer without a rig).
    pub(crate) rig_raw: &'static str,
    pub(crate) rig: Value,
    pub(crate) strokes: Vec<Value>,
    pub(crate) meshes: Value,
    pub(crate) expect: Value,
}

impl Row {
    /// What `rigCommitFrame` returned: `true`, `false` or the text `throws`.
    pub(crate) fn commit_ret(&self) -> &Value {
        &self.expect["commit"]["ret"]
    }

    /// The rig export after `rigResetPose` (bones from the baseline, binds as they were).
    pub(crate) fn reset_after(&self) -> &str {
        self.expect["reset"]["rigAfter"].as_str().expect("rigAfter")
    }

    pub(crate) fn has_meshes(&self) -> bool {
        !self.meshes.as_object().expect("meshes").is_empty()
    }

    /// Some bind survives the relink.
    pub(crate) fn any_kept(&self) -> bool {
        self.expect["kept"]
            .as_array()
            .expect("kept")
            .iter()
            .any(|kept| kept == &Value::Bool(true))
    }

    /// The commit the native transaction is expected to perform: the baseline succeeded, no mesh bind is
    /// involved (no provider is wired) and at least one path bind poses a stroke.
    pub(crate) fn commits_natively(&self) -> bool {
        self.commit_ret() == &Value::Bool(true) && !self.has_meshes() && self.any_kept()
    }
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .map(|f| {
            assert_eq!(f.len(), 6, "oracle row shape: {}", f[1]);
            assert_eq!(f[0], "pose");
            Row {
                name: f[1],
                rig_raw: f[2],
                rig: serde_json::from_str(f[2]).expect("rig json"),
                strokes: serde_json::from_str::<Value>(f[3])
                    .expect("strokes json")
                    .as_array()
                    .expect("strokes")
                    .clone(),
                meshes: serde_json::from_str(f[4]).expect("meshes json"),
                expect: serde_json::from_str(f[5]).expect("expect json"),
            }
        })
        .collect()
}

pub(crate) fn row(name: &str) -> Row {
    rows()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no row {name}"))
}

pub(crate) fn num(value: &Value) -> f64 {
    value.as_f64().expect("number")
}

pub(crate) fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= REL_TOL * a.abs().max(b.abs()).max(1.0)
}

/// `[px,py,hix,hiy,hox,hoy]` rows of one stroke.
pub(crate) fn segs(value: &Value) -> Vec<[f64; 6]> {
    value
        .as_array()
        .expect("segment list")
        .iter()
        .map(|s| {
            let a = s.as_array().expect("segment");
            [
                num(&a[0]),
                num(&a[1]),
                num(&a[2]),
                num(&a[3]),
                num(&a[4]),
                num(&a[5]),
            ]
        })
        .collect()
}
