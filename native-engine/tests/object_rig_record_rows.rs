//! Oracle row parser and document builders for the rig-record fixture (L03-N, I11).
//!
//! `fixtures/object-rig-record/oracle.txt` is produced by `tools/gen.cjs` from the baseline JS at
//! fbe4b8a3 (verbatim extracts executed in a Node `vm` with the shipped Paper.js). One tab-separated
//! row per case: `ens` ensureLayerRig, `fre` rigFreshId, `rig` a rig built by the real Rig-tool
//! functions (export whitelist, cloneRigForSymbol, in-memory JSON), `grd` guard return values, `ikset`
//! rigSetIK results, `rel` relinkRigBinds verdicts, `hand` older save shapes through the real whitelist.

pub(crate) const ORACLE: &str = include_str!("fixtures/object-rig-record/oracle.txt");

fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .filter(|fields| fields[0] == tag)
        .collect()
}

fn flag(text: &str) -> bool {
    match text {
        "1" => true,
        "0" => false,
        other => panic!("oracle flag must be 0 or 1, got {other}"),
    }
}

/// A rig the baseline Rig tool built: the three JSON texts it left behind.
pub(crate) struct RigRow {
    pub(crate) name: &'static str,
    /// The timeline.js `rig:` whitelist applied to the rig (what a save writes).
    pub(crate) export: &'static str,
    /// `cloneRigForSymbol(rig)` (what a Component conversion keeps).
    pub(crate) symbol: &'static str,
    /// The whole in-memory rig minus the live Paper/Raster references.
    pub(crate) memory: &'static str,
}

pub(crate) fn rig_rows() -> Vec<RigRow> {
    rows("rig")
        .into_iter()
        .map(|f| RigRow {
            name: f[1],
            export: f[2],
            symbol: f[3],
            memory: f[4],
        })
        .collect()
}

pub(crate) fn rig_row(name: &str) -> RigRow {
    rig_rows()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no rig row {name}"))
}

/// `ensureLayerRig(ld)`: the layer before, the rig after, and whether the same object came back.
pub(crate) struct EnsureRow {
    pub(crate) name: &'static str,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
    pub(crate) same_ref: bool,
}

pub(crate) fn ensure_rows() -> Vec<EnsureRow> {
    rows("ens")
        .into_iter()
        .map(|f| EnsureRow {
            name: f[1],
            before: f[2],
            after: f[3],
            same_ref: flag(f[4]),
        })
        .collect()
}

/// `rigFreshId(rig, prefix)` called `ids.len()` times starting at `nextId = start`.
pub(crate) struct FreshRow {
    pub(crate) name: &'static str,
    pub(crate) prefix: &'static str,
    pub(crate) start: u64,
    pub(crate) ids: Vec<String>,
    pub(crate) end: u64,
}

pub(crate) fn fresh_rows() -> Vec<FreshRow> {
    rows("fre")
        .into_iter()
        .map(|f| FreshRow {
            name: f[1],
            prefix: f[2],
            start: f[3].parse().unwrap(),
            ids: serde_json::from_str(f[4]).unwrap(),
            end: f[5].parse().unwrap(),
        })
        .collect()
}

/// One `relinkRigBinds` case: the input spec and, per input bind, whether the baseline kept it.
pub(crate) struct RelRow {
    pub(crate) name: &'static str,
    pub(crate) spec: serde_json::Value,
    pub(crate) kept: Vec<bool>,
    pub(crate) live: Vec<String>,
}

pub(crate) fn rel_rows() -> Vec<RelRow> {
    rows("rel")
        .into_iter()
        .map(|f| RelRow {
            name: f[1],
            spec: serde_json::from_str(f[2]).unwrap(),
            kept: serde_json::from_str(f[3]).unwrap(),
            live: serde_json::from_str(f[4]).unwrap(),
        })
        .collect()
}

/// The raw text of the single-value `grd` row `name`.
pub(crate) fn guard(name: &str) -> &'static str {
    rows("grd")
        .into_iter()
        .find(|f| f[1] == name)
        .unwrap_or_else(|| panic!("oracle has no grd row {name}"))[2]
}

/// `rigSetIK` on one fixture: the key triple + flip it was called with, its return value, the table left.
pub(crate) struct IkSetRow {
    pub(crate) name: &'static str,
    pub(crate) returned: bool,
    pub(crate) chains: &'static str,
}

pub(crate) fn ikset_rows() -> Vec<IkSetRow> {
    rows("ikset")
        .into_iter()
        .map(|f| IkSetRow {
            name: f[1],
            returned: serde_json::from_str(f[3]).unwrap(),
            chains: f[4],
        })
        .collect()
}

/// An older/hand-built rig (in-memory JSON) and what the real export whitelist makes of it.
pub(crate) struct HandRow {
    pub(crate) name: &'static str,
    pub(crate) input: &'static str,
    pub(crate) export: &'static str,
}

pub(crate) fn hand_row(name: &str) -> HandRow {
    rows("hand")
        .into_iter()
        .map(|f| HandRow {
            name: f[1],
            input: f[2],
            export: f[3],
        })
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no hand row {name}"))
}

// ---- document builders shared by the test files -------------------------------------------

/// A layer carrying `rig` (raw JSON text).
pub(crate) fn layer(uid: &str, rig: &str) -> String {
    format!("{{\"layerUid\":\"{uid}\",\"rig\":{rig}}}")
}

/// Cubic geometry with `n` segments on a diagonal (point only, zero handles).
fn geometry(n: usize, closed: bool) -> String {
    let segments: Vec<String> = (0..n)
        .map(|i| {
            format!(
                "{{\"point\":{{\"x\":{i},\"y\":{}}},\"handleIn\":{{\"x\":0,\"y\":0}},\"handleOut\":{{\"x\":0,\"y\":0}}}}",
                i * 2
            )
        })
        .collect();
    format!(
        "{{\"kind\":\"cubic-path\",\"closed\":{closed},\"coordinateSpace\":\"document-world\",\"handleSpace\":\"relative\",\"segments\":[{}]}}",
        segments.join(",")
    )
}

fn target(uid: &str, stroke: &str, frame: u32, kind: &str) -> String {
    format!("{{\"contextId\":\"scene-root\",\"frameScope\":{{\"kind\":\"{kind}\",\"frame\":{frame}}},\"layerUid\":\"{uid}\",\"strokeId\":\"{stroke}\"}}")
}

/// A closed-solid-fill object record with `n` segments, authored at `frame`.
pub(crate) fn legacy_object(uid: &str, stroke: &str, frame: u32, n: usize) -> String {
    format!(
        "{{\"schemaVersion\":1,\"family\":\"closed-solid-fill-cubic-path\",\"target\":{},\"geometry\":{},\"fill\":{{\"kind\":\"solid\",\"r\":0.5,\"g\":0.5,\"b\":0.5,\"a\":1}}}}",
        target(uid, stroke, frame, "authored"),
        geometry(n, true)
    )
}

/// An open editable-vector-path object record with `n` segments, authored at `frame`.
pub(crate) fn vector_object(uid: &str, stroke: &str, frame: u32, n: usize) -> String {
    format!(
        "{{\"schemaVersion\":1,\"family\":\"editable-vector-path\",\"target\":{},\"geometry\":{},\"fill\":null,\"vector\":{{\"stroke\":null,\"hasRealStroke\":false,\"strokeWidth\":1,\"strokeCap\":\"round\",\"strokeJoin\":\"round\",\"opacity\":1}}}}",
        target(uid, stroke, frame, "authored"),
        geometry(n, false)
    )
}

/// The same record as [`legacy_object`] but in the reference scope of the frame.
pub(crate) fn reference_object(uid: &str, stroke: &str, frame: u32, n: usize) -> String {
    legacy_object(uid, stroke, frame, n).replace(
        &target(uid, stroke, frame, "authored"),
        &target(uid, stroke, frame, "reference"),
    )
}

/// An object record whose family the rig does not bind. Decodes (the record type does not check the
/// family); the codec admission would refuse it, which is why the rig guard is tested on this value.
pub(crate) fn foreign_object(uid: &str, stroke: &str) -> String {
    legacy_object(uid, stroke, 0, 3).replace("closed-solid-fill-cubic-path", "compound-path")
}

/// A bind on `stroke` with `n` rest points on the diagonal and an empty weight list per point.
pub(crate) fn stroke_bind(stroke: &str, n: usize) -> String {
    bind_with("strokeId", stroke, n)
}

/// A bind on mesh `mesh` with `n` rest points.
pub(crate) fn mesh_bind(mesh: &str, n: usize) -> String {
    bind_with("meshId", mesh, n)
}

fn bind_with(member: &str, id: &str, n: usize) -> String {
    let rest: Vec<String> = (0..n).map(|i| format!("[{i},{}]", i * 2)).collect();
    let weights: Vec<&str> = (0..n).map(|_| "[]").collect();
    format!(
        "{{\"{member}\":\"{id}\",\"rest\":[{}],\"weights\":[{}],\"rotate\":false}}",
        rest.join(","),
        weights.join(",")
    )
}

/// A rig with the two-vertex bone `bone1` and the given binds (raw JSON texts).
pub(crate) fn rig_with_binds(binds: &[String]) -> String {
    format!(
        "{{\"bones\":{{\"bone1\":{{\"segments\":[{{\"point\":[0,0]}},{{\"point\":[10,0]}}],\"restSegments\":[{{\"point\":[0,0]}},{{\"point\":[10,0]}}],\"closed\":false}}}},\"ikChains\":{{}},\"nextId\":2,\"binds\":[{}]}}",
        binds.join(",")
    )
}

/// Canonical v1 document text (member order = native serialization order).
pub(crate) fn v1(layers: &[String], objects: &[String]) -> String {
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":1,\"totalFrames\":3,\"layers\":[{}],\"objects\":[{}]}}",
        layers.join(","),
        objects.join(",")
    )
}

/// Explicit order plan listing `uids` in one context with no scoped elements.
pub(crate) fn plan(uids: &[&str]) -> String {
    let layers: Vec<String> = uids
        .iter()
        .enumerate()
        .map(|(position, uid)| {
            format!("{{\"layerUid\":\"{uid}\",\"naturalPosition\":{position},\"order\":{{}}}}")
        })
        .collect();
    format!(
        "{{\"format\":\"nemo.native-object-order-plan\",\"formatVersion\":1,\"documentFormatVersion\":2,\"contexts\":[{{\"contextId\":\"scene-root\",\"layers\":[{}],\"scopes\":[]}}],\"migration\":\"explicit-plan-required\",\"referencePolicy\":\"unresolved\"}}",
        layers.join(",")
    )
}

/// v2 document text over the same layers; `uids` must list every layer once.
pub(crate) fn v2(layers: &[String], uids: &[&str], objects: &[String]) -> String {
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":2,\"totalFrames\":3,\"layers\":[{}],\"objects\":[{}],\"compositionOrder\":{}}}",
        layers.join(","),
        objects.join(","),
        plan(uids)
    )
}
