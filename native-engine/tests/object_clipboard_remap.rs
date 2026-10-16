//! Oracle replay for the clipboard / duplicate remap kernel (F15 + F08, L02-C).
//!
//! `fixtures/object-clipboard-remap/oracle.txt` (`ORACLE`, 195 JSON lines) was
//! produced by running the verbatim `_materializeClones`, `_remapCombineGroups`,
//! `ensureStrokeId`, `duplicateSelection` and `pasteSelection` of
//! `src/js/tools.js` and `dupOne` of `src/js/timeline.js` at fbe4b8a3 in a Node
//! `vm` context (generator `tools/gen_oracle.js`; `Date.now` pinned and
//! `Math.random` returning the call sequence number, so every minted id is
//! canonicalised to `N<position of its request>`). The test double [`Seq`]
//! issues `N1`, `N2` ... in request order, so id strings and the order and kind
//! of every request are pinned without renaming.
//!
//! Rows: `clone` (`in` = items/groups/dst/offset, `o` = request kinds, clone
//! identities, dabs, recreated groups in insertion order), `paste`
//! (`in` = frames/current/mode/clipboard origin, `o` = offset and directive),
//! `dupsel`, `layer` (`in` = source layer references, `o` = the duplicate; replayed by
//! `object_clipboard_remap_layer_replay.rs`).

use super::{
    paste_frame_directive, paste_offset, plan_clone, ClipSpot, ClonePlan, CloneRequest, ClonedItem,
    DabSource, FrameState, IdKind, IdSource, ItemSource, PasteFrameDirective, RemapError,
    DUPLICATE_OFFSET_PX,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupEntry,
};
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-clipboard-remap/oracle.txt");

/// Deterministic fresh-id port: `N1`, `N2` ... in request order, with a request log.
#[derive(Default)]
pub(crate) struct Seq {
    pub(crate) next: usize,
    /// Requests allowed before the provider reports exhaustion.
    pub(crate) limit: Option<usize>,
    /// Source ids the provider cannot replace (a mesh whose record is gone).
    pub(crate) fail_for: Vec<String>,
    pub(crate) log: Vec<(IdKind, Option<String>)>,
}

impl IdSource for Seq {
    fn fresh(&mut self, kind: IdKind, replaces: Option<&str>) -> Option<String> {
        if let Some(source) = replaces {
            if self.fail_for.iter().any(|gone| gone.as_str() == source) {
                return None;
            }
        }
        if let Some(limit) = self.limit {
            if self.log.len() >= limit {
                return None;
            }
        }
        self.next += 1;
        self.log.push((kind, replaces.map(str::to_string)));
        Some(format!("N{}", self.next))
    }
}

pub(crate) fn text(v: &Value) -> String {
    v.as_str().expect("string field").to_string()
}

pub(crate) fn opt(v: &Value, key: &str) -> Option<String> {
    v.get(key).map(text)
}

pub(crate) fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn strings(v: &Value) -> Vec<String> {
    v.as_array().expect("array").iter().map(text).collect()
}

fn kind_of(name: &str) -> IdKind {
    match name {
        "stroke" => IdKind::Stroke,
        "linkedFill" => IdKind::LinkedFill,
        "brushGroup" => IdKind::BrushGroup,
        "combineGroup" => IdKind::CombineGroup,
        "mesh" => IdKind::Mesh,
        other => panic!("unknown request kind {other}"),
    }
}

fn pair(v: &Value) -> (f64, f64) {
    (v["x"].as_f64().expect("x"), v["y"].as_f64().expect("y"))
}

fn item_from(it: &Value) -> ItemSource {
    ItemSource {
        stroke_id: opt(it, "sid"),
        group_id: opt(it, "gid"),
        brush_group_id: opt(it, "bg"),
        track_role_id: opt(it, "role"),
        attached_to_role_id: opt(it, "att"),
        attach_offset: it.get("attOff").map(pair),
        linked_fill_id: opt(it, "lf"),
        vector_brush_with_centerline: flag(it, "vb"),
        linked_fill_snapshot: flag(it, "vbFill"),
        dabs: it
            .get("dabs")
            .map(|dabs| {
                dabs.as_array()
                    .expect("dabs")
                    .iter()
                    .map(|d| DabSource {
                        stroke_id: None,
                        group_id: opt(d, "gid"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn groups_from(v: &Value) -> Option<Vec<GroupEntry>> {
    if v.is_null() {
        return None;
    }
    Some(
        v.as_array()
            .expect("groups")
            .iter()
            .map(|g| GroupEntry {
                gid: text(&g["gid"]),
                name: opt(g, "name"),
                combine_mode: g
                    .get("mode")
                    .map(|m| CombineMode::parse(m.as_str().unwrap()).expect("mode")),
                parent: opt(g, "parent"),
                order: strings(&g["order"]),
            })
            .collect(),
    )
}

struct Scenario {
    items: Vec<ItemSource>,
    groups: Option<Vec<GroupEntry>>,
    dst: Vec<String>,
    offset: f64,
}

fn scenario(spec: &Value) -> Scenario {
    Scenario {
        items: spec["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(item_from)
            .collect(),
        groups: groups_from(&spec["groups"]),
        dst: strings(&spec["dst"]),
        offset: spec["offset"].as_f64().expect("offset"),
    }
}

fn run(sc: &Scenario, ids: &mut Seq) -> Result<ClonePlan, RemapError> {
    let req = CloneRequest {
        items: &sc.items,
        source_groups: sc.groups.as_deref(),
        destination_group_ids: &sc.dst,
        offset: sc.offset,
    };
    plan_clone(&req, ids)
}

fn check_item(got: &ClonedItem, want: &Value, who: &str) {
    assert_eq!(got.stroke_id, text(&want["s"]), "{who} stroke id");
    assert_eq!(got.group_id, opt(want, "g"), "{who} group tag");
    assert_eq!(got.brush_group_id, opt(want, "bg"), "{who} brush group");
    assert_eq!(got.track_role_id, opt(want, "role"), "{who} track role");
    assert_eq!(got.attached_to_role_id, opt(want, "att"), "{who} attached");
    assert_eq!(
        got.attach_offset,
        want.get("attOff").map(pair),
        "{who} attach offset"
    );
    assert_eq!(got.linked_fill_id, opt(want, "lf"), "{who} linked fill id");
    match want.get("companion") {
        Some(c) => {
            assert!(got.companion, "{who} companion");
            assert_eq!(Some(text(&c["lf"])), got.linked_fill_id, "{who} pair id");
            assert_eq!(c["flag"], true, "{who} companion flag");
        }
        None => assert!(!got.companion, "{who} no companion"),
    }
}

fn check_clone_row(row: &Value) {
    let who = format!("clone row {}", text(&row["n"]));
    let sc = scenario(&row["in"]);
    let mut ids = Seq::default();
    let plan = run(&sc, &mut ids).unwrap_or_else(|e| panic!("{who} refused: {e}"));
    let want = &row["o"];
    let want_req: Vec<IdKind> = want["req"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| kind_of(k.as_str().unwrap()))
        .collect();
    let got_req: Vec<IdKind> = ids.log.iter().map(|(k, _)| *k).collect();
    assert_eq!(got_req, want_req, "{who} request kinds in order");
    let items = want["items"].as_array().unwrap();
    assert_eq!(plan.items.len(), items.len(), "{who} item count");
    for (i, (got, w)) in plan.items.iter().zip(items).enumerate() {
        assert_eq!(got.source, i);
        check_item(got, w, &format!("{who} item {i}"));
    }
    let dabs = want["dabs"].as_array().unwrap();
    assert_eq!(plan.dabs.len(), dabs.len(), "{who} dab count");
    for (got, w) in plan.dabs.iter().zip(dabs) {
        assert_eq!(got.stroke_id, None, "{who} dab keeps no source id");
        assert_eq!(
            Some(got.brush_group_id.clone()),
            opt(w, "bg"),
            "{who} dab bg"
        );
        assert_eq!(got.group_id, opt(w, "g"), "{who} dab tag");
    }
    let groups = want["groups"].as_array().unwrap();
    assert_eq!(plan.groups.len(), groups.len(), "{who} recreated groups");
    for (got, w) in plan.groups.iter().zip(groups) {
        assert_eq!(got.source, text(&w["from"]), "{who} group source");
        assert_eq!(got.fresh, text(&w["gid"]), "{who} group fresh id");
        assert_eq!(got.entry.gid, got.fresh);
        assert_eq!(got.kept, w["kept"].as_bool().unwrap(), "{who} kept");
        assert_eq!(got.entry.order, strings(&w["order"]), "{who} order");
        assert_eq!(got.entry.name, opt(w, "name"), "{who} name");
        let mode = w
            .get("mode")
            .map(|m| CombineMode::parse(m.as_str().unwrap()).unwrap());
        assert_eq!(got.entry.combine_mode, mode, "{who} mode");
        // The baseline copies the SOURCE parent gid verbatim ...
        let source_entry = sc
            .groups
            .as_ref()
            .unwrap()
            .iter()
            .find(|e| e.gid == got.source);
        assert_eq!(
            source_entry.unwrap().parent,
            opt(w, "parent"),
            "{who} baseline parent is the verbatim source value"
        );
        // ... native re-points it at the fresh id of the recreated parent.
        let repointed = opt(w, "parent").and_then(|p| {
            plan.groups
                .iter()
                .find(|g| g.source == p)
                .map(|g| g.fresh.clone())
        });
        assert_eq!(got.entry.parent, repointed, "{who} re-pointed parent");
    }
    assert_eq!(plan.offset, sc.offset);
    let wanted_map: Vec<(String, String)> = row["in"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .zip(items)
        .filter_map(|(spec, made)| opt(spec, "sid").map(|old| (old, text(&made["s"]))))
        .collect();
    assert_eq!(plan.stroke_map, wanted_map, "{who} old to new stroke ids");
    let offsets = want["offsets"].as_array().unwrap();
    let moved = if sc.offset == 0.0 {
        0
    } else {
        plan.items.len() + plan.dabs.len()
    };
    assert_eq!(offsets.len(), moved, "{who} translated clones");
    assert!(offsets.iter().all(|o| o.as_f64() == Some(sc.offset)));
}

pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle line is JSON"))
        .filter(|row| row["k"] == kind)
        .collect()
}

#[test]
fn oracle_corpus_shape_is_pinned() {
    assert_eq!(ORACLE.len(), 64404, "oracle bytes");
    assert_eq!(rows("clone").len(), 88);
    assert_eq!(rows("paste").len(), 91);
    assert_eq!(rows("dupsel").len(), 1);
    assert_eq!(rows("layer").len(), 15);
    let clones = rows("clone");
    let has_kind = |name: &str| {
        clones
            .iter()
            .any(|r| r["o"]["req"].as_array().unwrap().iter().any(|k| k == name))
    };
    for kind in ["stroke", "linkedFill", "brushGroup", "combineGroup"] {
        assert!(has_kind(kind), "corpus exercises {kind} requests");
    }
    let groups: Vec<&Value> = clones
        .iter()
        .flat_map(|r| r["o"]["groups"].as_array().unwrap().iter())
        .collect();
    assert!(groups.iter().any(|g| g["kept"] == true));
    assert!(groups.iter().any(|g| g["kept"] == false));
    assert!(groups.iter().any(|g| g.get("parent").is_some()));
    assert!(clones
        .iter()
        .any(|r| !r["o"]["dabs"].as_array().unwrap().is_empty()));
}

#[test]
fn clone_rows_match_the_baseline() {
    for row in rows("clone") {
        check_clone_row(&row);
    }
}

fn frame_of(name: &str) -> FrameState {
    match name {
        "none" => FrameState::Missing,
        "key" => FrameState::Keyframe,
        "interp" => FrameState::Interpolated,
        "held" => FrameState::Held,
        other => panic!("unknown frame kind {other}"),
    }
}

fn directive_of(name: &str) -> PasteFrameDirective {
    match name {
        "current" => PasteFrameDirective::WriteCurrent,
        "promote" => PasteFrameDirective::NeedsKeyframePromotion,
        "held-no-keyframe" => PasteFrameDirective::HeldWithoutKeyframe,
        other => PasteFrameDirective::WriteIntoHeldKeyframe(
            other
                .strip_prefix("hold:")
                .expect("hold:N")
                .parse()
                .unwrap(),
        ),
    }
}

#[test]
fn paste_rows_match_the_baseline() {
    for row in rows("paste") {
        let spec = &row["in"];
        let frames: Vec<FrameState> = spec["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| frame_of(f.as_str().unwrap()))
            .collect();
        let cur = spec["cur"].as_u64().unwrap() as usize;
        let who = format!("paste {spec}");
        let got = paste_frame_directive(&frames, cur, flag(spec, "motion"));
        assert_eq!(
            got,
            directive_of(row["o"]["directive"].as_str().unwrap()),
            "{who}"
        );
        let origin = ClipSpot {
            layer: spec["srcLayer"].as_u64().unwrap() as usize,
            frame: spec["srcFrame"].as_u64().unwrap() as usize,
        };
        let here = ClipSpot {
            layer: 0,
            frame: cur,
        };
        assert_eq!(
            Some(paste_offset(origin, here)),
            row["o"]["offset"].as_f64(),
            "{who} offset"
        );
    }
}

#[test]
fn duplicate_selection_offset_matches_the_baseline() {
    let all = rows("dupsel");
    let row = &all[0];
    assert_eq!(row["o"]["offset"].as_f64(), Some(DUPLICATE_OFFSET_PX));
    // duplicateSelection hands _materializeClones the LIVE layer table (not a snapshot).
    assert_eq!(row["o"]["live"], true);
}
