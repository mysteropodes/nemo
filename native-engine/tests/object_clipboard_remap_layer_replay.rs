//! Oracle replay of the `layer` rows of `fixtures/object-clipboard-remap/oracle.txt`
//! (F08, L02-C): the verbatim `dupOne` of `src/js/timeline.js` at fbe4b8a3 run against fabricated source
//! layers (generator `tools/gen_oracle.js`). The fresh mesh ids are the test double's `N1`, `N2` ...
//! in request order. Two rows pin DELIBERATE differences, asserted explicitly: the baseline never carries
//! `videoMeshId` onto the copy (the branch that would remap it reads the new, empty layer), and the baseline
//! silently leaves a mesh it could not duplicate shared between the two layers.

use super::tests::{flag, opt, rows, strings, text, Seq};
use super::{plan_layer_duplicate, AdmittedKinds, IdKind, LayerSource, RemapError};
use serde_json::Value;

fn layer_from(spec: &Value) -> LayerSource {
    let mesh_list = |v: &Value| -> Vec<Option<String>> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| opt(s, "mesh"))
            .collect()
    };
    let mut parents: Vec<String> = Vec::new();
    parents.extend(opt(spec, "parent"));
    parents.extend(opt(spec, "parentB"));
    LayerSource {
        frame_mesh_ids: spec["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(mesh_list)
            .collect(),
        video_mesh_id: opt(spec, "video"),
        element_motion_keys: spec.get("em").map(strings).unwrap_or_default(),
        rig_bind_mesh_ids: spec.get("rig").map(mesh_list),
        symbol_id: opt(spec, "symbol"),
        duplicator: flag(spec, "duplicator"),
        lfs_group: flag(spec, "lfs"),
        locked: flag(spec, "locked"),
        matte_source_uid: opt(spec, "matte"),
        mattes_more_uids: spec.get("mattes").map(strings).unwrap_or_default(),
        parent_uids: parents,
        unknown_references: Vec::new(),
    }
}

fn nullable(v: &Value) -> Vec<Option<String>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().map(str::to_string))
        .collect()
}

#[test]
fn layer_rows_match_the_baseline() {
    let all = AdmittedKinds {
        mesh: true,
        rig: true,
        component: true,
    };
    for row in rows("layer") {
        let who = format!("layer row {}", text(&row["n"]));
        let spec = &row["in"];
        let want = &row["o"];
        // The baseline reads videoMeshId from the NEW layer, which dupOne never copies.
        assert!(
            want.get("video").is_none(),
            "{who}: baseline never carries videoMeshId"
        );
        let src = layer_from(spec);
        let mut ids = Seq::default();
        ids.fail_for = spec.get("missing").map(strings).unwrap_or_default();
        let planned = plan_layer_duplicate(&src, all, &mut ids);
        if spec.get("missing").is_some() {
            // The baseline silently leaves the shared mesh id; native refuses the whole batch.
            assert_eq!(
                want["frames"][0][0], "im_1",
                "{who}: baseline shares the mesh"
            );
            assert_eq!(
                planned,
                Err(RemapError::IdUnavailable(IdKind::Mesh)),
                "{who}"
            );
            continue;
        }
        let plan = planned.unwrap_or_else(|e| panic!("{who} refused: {e}"));
        let frames: Vec<Vec<Option<String>>> = want["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(nullable)
            .collect();
        assert_eq!(plan.frame_mesh_ids, frames, "{who} frame mesh ids");
        assert_eq!(plan.locked, want["locked"], "{who} locked");
        let keys = want.get("em").map(strings).unwrap_or_default();
        assert_eq!(plan.element_motion_keys, keys, "{who} element motion keys");
        assert_eq!(
            plan.rig_bind_mesh_ids,
            want.get("rig").map(nullable),
            "{who} rig"
        );
        assert_eq!(plan.matte_source_uid, opt(want, "matte"), "{who} matte");
        assert_eq!(
            plan.mattes_more_uids,
            want.get("mattes").map(strings).unwrap_or_default()
        );
        assert_eq!(plan.parent_uids, src.parent_uids, "{who} parents retained");
        assert_eq!(plan.symbol_id, opt(want, "symbol"), "{who} symbol");
        let asked: Vec<String> = ids.log.iter().filter_map(|(_, r)| r.clone()).collect();
        if src.video_mesh_id.is_none() {
            assert_eq!(
                asked,
                strings(&want["dupOrder"]),
                "{who} mesh duplication order"
            );
        } else {
            // Native follows the stated acceptance: the video mesh is remapped too.
            assert_eq!(plan.video_mesh_id.as_deref(), Some("N1"), "{who}");
            assert_eq!(asked, vec!["im_5".to_string()]);
        }
    }
}
