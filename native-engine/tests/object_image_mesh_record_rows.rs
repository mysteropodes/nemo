//! Oracle row parsing and comparison harness for the image-mesh store record (L03-J, F17).
//!
//! `fixtures/object-image-mesh-record/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs`
//! (kept outside the repo; sha256 recorded in the leaf HANDOFF) from `src/js/image-mesh.js`
//! at fbe4b8a3 run in a Node `vm` with the vendored Delaunator, a fresh vm per row because
//! the baseline id counter `_seq` is module state. Meshes come from the real `createMesh`,
//! `setOutline`, `setOffset`, `resetOffsets`, `duplicate` and from the corpus mesh cut
//! verbatim out of `tests/fixtures/lib/corpus.cjs`.
//!
//! Tab separated rows:
//! `ld name input baselineSerialize issuedIds bytes expect` and `iss name idsJson issuedIds`.
//! `baselineSerialize` is `JSON.stringify(SMImageMesh.serialize())` straight after
//! `load(input)` (defaults filled in), `issuedIds` the next three `attach()` ids,
//! `expect` the input in native field order with only the authored keys.

use super::{ImageMeshStore, MeshIdSequence};
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-record/oracle.txt");

pub(crate) struct LoadRow {
    pub(crate) name: String,
    pub(crate) input: String,
    pub(crate) baseline: String,
    pub(crate) issued: Vec<String>,
    pub(crate) bytes: bool,
    pub(crate) expect: String,
}

pub(crate) struct IssueRow {
    pub(crate) name: String,
    pub(crate) ids: Vec<String>,
    pub(crate) issued: Vec<String>,
}

fn ids(text: &str) -> Vec<String> {
    serde_json::from_str(text).expect("id list")
}

pub(crate) fn load_rows() -> Vec<LoadRow> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("ld\t"))
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 7, "ld row has seven fields: {}", f[1]);
            LoadRow {
                name: f[1].into(),
                input: f[2].into(),
                baseline: f[3].into(),
                issued: ids(f[4]),
                bytes: f[5] == "1",
                expect: f[6].into(),
            }
        })
        .collect()
}

pub(crate) fn issue_rows() -> Vec<IssueRow> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("iss\t"))
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 4, "iss row has four fields: {}", f[1]);
            IssueRow {
                name: f[1].into(),
                ids: ids(f[2]),
                issued: ids(f[3]),
            }
        })
        .collect()
}

pub(crate) fn json_points(value: &Value) -> Vec<[f64; 2]> {
    value
        .as_array()
        .expect("point array")
        .iter()
        .map(|p| [p[0].as_f64().expect("x"), p[1].as_f64().expect("y")])
        .collect()
}

pub(crate) fn same_points(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p[0].to_bits() == q[0].to_bits() && p[1].to_bits() == q[1].to_bits())
}

/// The state `SMImageMesh.load` leaves behind, as the native record must evaluate it.
pub(crate) fn loaded_state_mismatches(row: &LoadRow, store: &ImageMeshStore) -> Vec<String> {
    let mut out = Vec::new();
    let baseline: Value = serde_json::from_str(&row.baseline).expect("baseline json");
    let table = baseline.as_object().expect("baseline object");
    if table.len() != store.len() {
        out.push(format!(
            "{}: entry count {} vs {}",
            row.name,
            store.len(),
            table.len()
        ));
    }
    for (id, mesh) in store.entries() {
        let Some(want) = table.get(id) else {
            out.push(format!("{}: id {id:?} missing from the baseline", row.name));
            continue;
        };
        let checks = [
            (
                "outline",
                same_points(&mesh.outline_points(), &json_points(&want["outline"])),
            ),
            (
                "verts",
                same_points(&mesh.rest_points(), &json_points(&want["verts"])),
            ),
            (
                "offsets",
                same_points(&mesh.effective_offsets(), &json_points(&want["offsets"])),
            ),
            (
                "tris",
                want["tris"].as_array().is_some_and(|tris| {
                    tris.iter().map(|t| t.as_u64()).collect::<Vec<_>>()
                        == mesh
                            .triangles()
                            .iter()
                            .map(|t| Some(u64::from(*t)))
                            .collect::<Vec<_>>()
                }),
            ),
            (
                "cols",
                want["cols"].as_u64() == Some(u64::from(mesh.effective_cols())),
            ),
            (
                "rows",
                want["rows"].as_u64() == Some(u64::from(mesh.effective_rows())),
            ),
        ];
        for (field, ok) in checks {
            if !ok {
                out.push(format!(
                    "{}: {id} {field} differs from the baseline",
                    row.name
                ));
            }
        }
    }
    out
}

/// Numeric value the baseline left in `_seq` after `load`: the first issued id minus one.
pub(crate) fn expected_last(row: &LoadRow) -> u64 {
    let first = &row.issued[0];
    first
        .strip_prefix("im_")
        .and_then(|n| n.parse::<u64>().ok())
        .expect("im_<n>")
        - 1
}

/// Sequence rows through an arbitrary per-id rule (production or a mutant): the rule maps an id
/// to the counter value `load()` would take from it (0 when it does not advance).
pub(crate) fn sequence_mismatches(rule: &dyn Fn(&str) -> u64) -> Vec<String> {
    let mut out = Vec::new();
    for row in load_rows() {
        let store: ImageMeshStore = serde_json::from_str(&row.input).expect("store");
        let got = store
            .entries()
            .iter()
            .map(|(id, _)| rule(id.as_str()))
            .max()
            .unwrap_or(0);
        if got != expected_last(&row) {
            out.push(format!(
                "{}: counter {} vs {}",
                row.name,
                got,
                expected_last(&row)
            ));
        }
    }
    out
}

pub(crate) fn issue_three(sequence: &mut MeshIdSequence, store: &ImageMeshStore) -> Vec<String> {
    (0..3)
        .map(|_| sequence.issue(store).expect("issuable"))
        .collect()
}
