//! Oracle reader, document builders, mesh provider and the baseline comparator shared by the rig pose
//! evaluation tests (L03-P, I12). No test lives here.
//!
//! `fixtures/rig-pose-eval/oracle.txt` is produced by `tools/gen.cjs` (kept outside the repo; sha256 in the
//! leaf HANDOFF) from the baseline at fbe4b8a3. A row is `ev NAME RIG STROKES MESHES EXPECT`, tab separated,
//! every field compact JSON. RIG is the `exportJSON` whitelist of the rig built by the real Rig tool
//! functions (this text is the native `rig` record), STROKES / MESHES are the stored geometry and the mesh
//! store BEFORE the pose, EXPECT is what the real `relinkRigBinds` + `applyRigDeform` + `scenePayloadFor`
//! produced: `{throws, kept:[bool per bind], strokes:{id:[[px,py,hix,hiy,hox,hoy]]}, meshes:{id:{offsets,
//! payload}}}`. No existing golden suite is claimed: this oracle IS the executed baseline.
//!
//! Numbers are compared with `close` (1e-9 relative): the baseline's `cos`/`sin`/`atan2` run on V8 and the
//! kernel's on the platform libm. Mesh scene vertices are rounded to two decimals (`R2`), so they may differ
//! by one rounding step at most.
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    MeshRect, ScenePayload,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose, BindOutcome, MeshPoseSource, MeshSnapshot, PosedSegment, RigPoseEvaluation,
    RigPosePin, RigPoseRefusal,
};
use crate::object_codec::decode_project;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

pub(crate) const ORACLE: &str = include_str!("fixtures/rig-pose-eval/oracle.txt");
pub(crate) const LAYER: &str = "L1";
pub(crate) const REL_TOL: f64 = 1e-9;

/// One oracle row.
#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) name: String,
    pub(crate) rig: Value,
    /// `[{id, closed, segs}]`, the stored frame-0 geometry before the pose.
    pub(crate) strokes: Vec<Value>,
    /// `{meshId: {outline, verts, offsets, tris, rect}}` before the pose.
    pub(crate) meshes: Value,
    pub(crate) expect: Value,
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle field is JSON")
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 6, "row shape");
            assert_eq!(f[0], "ev");
            Row {
                name: f[1].to_owned(),
                rig: parse(f[2]),
                strokes: parse(f[3]).as_array().expect("strokes").clone(),
                meshes: parse(f[4]),
                expect: parse(f[5]),
            }
        })
        .collect()
}

pub(crate) fn row(name: &str) -> Row {
    rows()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("oracle has no row {name}"))
}

pub(crate) fn num(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

pub(crate) fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= REL_TOL * a.abs().max(b.abs()).max(1.0)
}

pub(crate) fn pairs(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("list of pairs")
        .iter()
        .map(|p| [num(&p[0]), num(&p[1])])
        .collect()
}

// ---- documents ------------------------------------------------------------------------------------

/// A stored stroke record of layer `layer`: closed paths use the legacy family, open ones the vector family.
pub(crate) fn stroke_record(
    layer: &str,
    stroke: &str,
    closed: bool,
    segs: &[[f64; 6]],
    frame: u32,
) -> Value {
    let segments: Vec<Value> = segs
        .iter()
        .map(|s| {
            json!({
                "point": {"x": s[0], "y": s[1]},
                "handleIn": {"x": s[2], "y": s[3]},
                "handleOut": {"x": s[4], "y": s[5]},
            })
        })
        .collect();
    let target = json!({
        "contextId": "scene-root",
        "frameScope": {"kind": "authored", "frame": frame},
        "layerUid": layer,
        "strokeId": stroke,
    });
    let geometry = json!({
        "kind": "cubic-path",
        "closed": closed,
        "coordinateSpace": "document-world",
        "handleSpace": "relative",
        "segments": segments,
    });
    if closed {
        json!({
            "schemaVersion": 1,
            "family": "closed-solid-fill-cubic-path",
            "target": target,
            "geometry": geometry,
            "fill": {"kind": "solid", "r": 0.5, "g": 0.5, "b": 0.5, "a": 1},
        })
    } else {
        json!({
            "schemaVersion": 1,
            "family": "editable-vector-path",
            "target": target,
            "geometry": geometry,
            "fill": null,
            "vector": {
                "stroke": null,
                "hasRealStroke": false,
                "strokeWidth": 1,
                "strokeCap": "round",
                "strokeJoin": "round",
                "opacity": 1,
            },
        })
    }
}

/// A v1 document over `layers` and `objects` (4 frames).
pub(crate) fn document(layers: Vec<Value>, objects: Vec<Value>) -> Value {
    json!({
        "format": "nemo.native-object-document",
        "formatVersion": 1,
        "totalFrames": 4,
        "layers": layers,
        "objects": objects,
    })
}

pub(crate) fn snapshot_of(doc: &Value) -> ObjectSnapshot {
    let bytes = serde_json::to_vec(doc).expect("document bytes");
    let decoded = decode_project(&bytes).expect("the document is admitted");
    ObjectSnapshot::new("rig-pose-instance", decoded).expect("snapshot")
}

fn seg6(v: &Value) -> [f64; 6] {
    let a = v.as_array().expect("segment");
    [
        num(&a[0]),
        num(&a[1]),
        num(&a[2]),
        num(&a[3]),
        num(&a[4]),
        num(&a[5]),
    ]
}

/// The document of an oracle row: one layer `L1` carrying the rig and one record per stored stroke.
pub(crate) fn row_document(row: &Row) -> Value {
    let objects = row
        .strokes
        .iter()
        .map(|s| {
            let segs: Vec<[f64; 6]> = s["segs"]
                .as_array()
                .expect("segs")
                .iter()
                .map(seg6)
                .collect();
            stroke_record(
                LAYER,
                s["id"].as_str().expect("id"),
                s["closed"].as_bool().expect("closed"),
                &segs,
                0,
            )
        })
        .collect();
    document(vec![json!({"layerUid": LAYER, "rig": row.rig})], objects)
}

// ---- the mesh provider ----------------------------------------------------------------------------

pub(crate) struct Provider {
    pub(crate) meshes: HashMap<String, MeshSnapshot>,
}

impl MeshPoseSource for Provider {
    fn mesh(&self, mesh_id: &str) -> Option<&MeshSnapshot> {
        self.meshes.get(mesh_id)
    }
}

pub(crate) fn mesh_of(v: &Value) -> MeshSnapshot {
    let r = &v["rect"];
    MeshSnapshot {
        outline: pairs(&v["outline"]),
        verts: pairs(&v["verts"]),
        offsets: pairs(&v["offsets"]),
        tris: v["tris"]
            .as_array()
            .expect("tris")
            .iter()
            .map(|t| t.as_u64().expect("index") as u32)
            .collect(),
        rect: Some(MeshRect {
            x: num(&r["x"]),
            y: num(&r["y"]),
            width: num(&r["width"]),
            height: num(&r["height"]),
            rotation: num(&r["rotation"]),
        }),
    }
}

pub(crate) fn row_provider(row: &Row) -> Provider {
    let meshes = row
        .meshes
        .as_object()
        .expect("meshes")
        .iter()
        .map(|(id, v)| (id.clone(), mesh_of(v)))
        .collect();
    Provider { meshes }
}

pub(crate) fn evaluate_with(
    row: &Row,
    provider: Option<&dyn MeshPoseSource>,
) -> Result<RigPoseEvaluation, RigPoseRefusal> {
    let snapshot = snapshot_of(&row_document(row));
    evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), LAYER, 0, provider)
}

pub(crate) fn evaluate_row(row: &Row) -> Result<RigPoseEvaluation, RigPoseRefusal> {
    let provider = row_provider(row);
    evaluate_with(row, Some(&provider))
}

// ---- the comparator --------------------------------------------------------------------------------

fn flat(s: &PosedSegment) -> [f64; 6] {
    [
        s.point[0],
        s.point[1],
        s.handle_in[0],
        s.handle_in[1],
        s.handle_out[0],
        s.handle_out[1],
    ]
}

fn strokes_match(got: &[PosedSegment], want: &Value) -> bool {
    let want = want.as_array().map(Vec::as_slice).unwrap_or(&[]);
    got.len() == want.len()
        && got
            .iter()
            .zip(want.iter())
            .all(|(g, w)| flat(g).iter().zip(seg6(w)).all(|(a, b)| close(*a, b)))
}

fn offsets_match(got: &[[f64; 2]], want: &Value) -> bool {
    let want = pairs(want);
    got.len() == want.len()
        && got
            .iter()
            .zip(want.iter())
            .all(|(g, w)| close(g[0], w[0]) && close(g[1], w[1]))
}

fn payload_matches(got: &ScenePayload, want: &Value) -> bool {
    let ints = |v: &Value| -> Vec<u32> {
        v.as_array()
            .expect("indices")
            .iter()
            .map(|t| t.as_u64().expect("index") as u32)
            .collect()
    };
    let verts = pairs(&want["verts"]);
    let uvs = pairs(&want["uvs"]);
    got.tris == ints(&want["tris"])
        && got.outline == ints(&want["outline"])
        && got.verts.len() == verts.len()
        && got.verts.iter().zip(verts.iter()).all(|(g, w)| {
            // one R2 rounding step of slack: the offsets differ by libm roundoff only
            (g[0] - w[0]).abs() <= 0.0100001 && (g[1] - w[1]).abs() <= 0.0100001
        })
        && got.uvs.len() == uvs.len()
        && got
            .uvs
            .iter()
            .zip(uvs.iter())
            .all(|(g, w)| close(g[0], w[0]) && close(g[1], w[1]))
}

/// `Ok` when the evaluation of a non-throwing row equals what the baseline produced; `Err` names the first
/// difference. Used on the real rows (must be `Ok`) and on perturbed ones (must be `Err`).
pub(crate) fn compare(
    row: &Row,
    provider: &Provider,
    eval: &RigPoseEvaluation,
) -> Result<(), String> {
    let binds = row.rig["binds"].as_array().expect("binds");
    let kept = row.expect["kept"].as_array().expect("kept");
    if eval.binds.len() != binds.len() {
        return Err(format!(
            "{} outcomes for {} binds",
            eval.binds.len(),
            binds.len()
        ));
    }
    let mut stroke_ids = BTreeSet::new();
    let mut mesh_ids = BTreeSet::new();
    for (i, bind) in binds.iter().enumerate() {
        let want_kept = kept[i].as_bool().expect("kept flag");
        let outcome = eval.binds[i];
        if (outcome == BindOutcome::Posed) != want_kept {
            return Err(format!(
                "bind {i}: {outcome:?} but the baseline kept={want_kept}"
            ));
        }
        if !want_kept {
            if !matches!(outcome, BindOutcome::Unlinked(_)) {
                return Err(format!(
                    "bind {i}: a dropped bind must be Unlinked, got {outcome:?}"
                ));
            }
            continue;
        }
        if let Some(id) = bind["strokeId"].as_str() {
            stroke_ids.insert(id.to_owned());
            let got = eval
                .strokes
                .iter()
                .find(|s| s.stroke_id == id)
                .ok_or_else(|| format!("stroke {id} not posed"))?;
            if !strokes_match(&got.segments, &row.expect["strokes"][id]) {
                return Err(format!("stroke {id} differs from the baseline"));
            }
        } else {
            let id = bind["meshId"].as_str().expect("mesh bind");
            mesh_ids.insert(id.to_owned());
            let got = eval
                .meshes
                .iter()
                .find(|m| m.mesh_id == id)
                .ok_or_else(|| format!("mesh {id} not posed"))?;
            let want = &row.expect["meshes"][id];
            if !offsets_match(&got.offsets, &want["offsets"]) {
                return Err(format!("mesh {id} offsets differ from the baseline"));
            }
            let source = provider.meshes.get(id).expect("mesh provided");
            let payload = got
                .scene_payload(source)
                .ok_or_else(|| format!("mesh {id} has no scene payload"))?;
            if !payload_matches(&payload, &want["payload"]) {
                return Err(format!("mesh {id} scene payload differs from the baseline"));
            }
        }
    }
    if eval.strokes.len() != stroke_ids.len() || eval.meshes.len() != mesh_ids.len() {
        return Err("the evaluation lists a stroke or mesh no surviving bind drives".into());
    }
    Ok(())
}
