//! Oracle tests of the component convert / place / expose plans (L04-M). Every
//! expected value is a baseline row the generator executed from the verbatim
//! JavaScript, never a value read back from the code under test.
use super::fixture::{
    convert_oracle, decode, document, duplicator_table, layer, s1_document, s2_document, uids,
    wire, CONVERT_ORACLE,
};
use super::verify::verify;
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn facts(f: impl FnOnce(&mut SourceFacts)) -> SourceFacts {
    let mut facts = SourceFacts::default();
    f(&mut facts);
    facts
}

const FIRST_SYMBOL: &str = "native-component:1:0";

fn holder(layer: &str, stroke: Option<&str>) -> Value {
    let mut target = json!({"contextId":"scene-root","layerUid":layer});
    if let Some(stroke) = stroke {
        target["strokeId"] = json!(stroke);
    }
    json!({"propertyHolders":[{"target":target,"properties":[{"property":"opacity","static":[50]}]}]})
}

fn positions(after: &Value) -> BTreeMap<String, u64> {
    after["compositionOrder"]["contexts"][0]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["layerUid"].as_str().unwrap().to_string(),
                l["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_oracle_is_pinned_to_the_baseline_commit_and_its_row_count() {
    assert!(CONVERT_ORACLE.contains("fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert_eq!(convert_oracle().rows(), 208);
}

#[test]
fn each_refused_kind_returns_the_baseline_reason_text() {
    let o = convert_oracle();
    let cases = [
        (
            "R.symbolId",
            facts(|f| f.component = true),
            SourceRefusal::Component,
        ),
        (
            "R.lfsGroup",
            facts(|f| f.lfs_group = true),
            SourceRefusal::LfsGroup,
        ),
        (
            "R.montageId",
            facts(|f| f.montage = true),
            SourceRefusal::Montage,
        ),
        (
            "R.nativeVideo",
            facts(|f| f.native_video = true),
            SourceRefusal::NativeVideo,
        ),
        (
            "R.isNullLayer",
            facts(|f| f.null_layer = true),
            SourceRefusal::Null,
        ),
        (
            "R.isEffectLayer",
            facts(|f| f.effect_layer = true),
            SourceRefusal::Effect,
        ),
        (
            "R.isGuideLayer",
            facts(|f| f.guide_layer = true),
            SourceRefusal::Guide,
        ),
        (
            "R.isWidgetLayer",
            facts(|f| f.widget_layer = true),
            SourceRefusal::Widget,
        ),
    ];
    for (row, facts, refusal) in cases {
        assert_eq!(bad_component_source_reason(&facts), Some(refusal), "{row}");
        assert_eq!(o.text(row), refusal.reason(), "{row}");
        let toast = format!("Impossible de convertir : {}", o.text(row));
        assert_eq!(refusal.message(), toast.as_str(), "{row}");
    }
}

#[test]
fn the_baseline_test_order_decides_between_several_kinds() {
    let o = convert_oracle();
    let cases = [
        (
            "R.precedence.componentFirst",
            facts(|f| {
                f.component = true;
                f.lfs_group = true;
                f.widget_layer = true;
            }),
        ),
        (
            "R.precedence.nullBeforeGuide",
            facts(|f| {
                f.null_layer = true;
                f.guide_layer = true;
            }),
        ),
        (
            "R.precedence.guideBeforeWidget",
            facts(|f| {
                f.guide_layer = true;
                f.widget_layer = true;
            }),
        ),
    ];
    for (row, kinds) in cases {
        let got = bad_component_source_reason(&kinds).expect("a kind is set");
        assert_eq!(o.text(row), got.reason(), "{row}");
    }
}

#[test]
fn folders_effector_layers_duplicator_hosts_and_plain_layers_are_not_refused() {
    let o = convert_oracle();
    for row in [
        "R.plain",
        "R.isFolderLayer",
        "R.isEffectorLayer",
        "R.duplicator",
    ] {
        assert_eq!(o.get(row), &Value::Null, "{row}");
    }
    assert_eq!(bad_component_source_reason(&SourceFacts::default()), None);
    let hosts = document(
        &[
            layer("L1", "One", &[(0, "a")]),
            layer("L2", "Two", &[(0, "b")]),
        ],
        json!({"componentTables":{"duplicators":duplicator_table(&[("L1", json!({"mode":"grid"}))])}}),
    );
    let plan = plan_convert(&decode(&hosts), 1, &uids(&["L1"])).unwrap();
    assert!(plan.changed, "a duplicator host converts like any layer");
}

#[test]
fn one_layer_converts_in_place_exactly_like_the_baseline_rows() {
    let o = convert_oracle();
    let doc = decode(&s1_document(holder("ly_hand", None)));
    let plan = plan_convert(&doc, 1, &uids(&["ly_hand"])).unwrap();
    assert!(plan.changed);
    assert_eq!(plan.symbol_id.as_deref(), Some(FIRST_SYMBOL));
    assert_eq!(plan.layer_uid.as_deref(), Some("ly_hand"));
    assert_eq!(
        plan.created, None,
        "the source layer itself becomes the instance"
    );
    let after = wire(&plan.document);
    verify(&after, FIRST_SYMBOL, "S1", &o).unwrap();
    // The instance keeps its own motion (baseline `ld.motion` stays on `ld`).
    assert_eq!(o.get("S1.inst.motionPresent"), &json!(true));
    assert_eq!(after["propertyHolders"][0]["target"]["layerUid"], "ly_hand");
    // Inner identity: a fresh inner layer, the group table re-scoped to the component.
    let inner = &after["componentTables"]["components"]["components"][0]["layers"][0];
    assert_eq!(inner["layerUid"], "native-component:1:0:layer-1");
    assert_eq!(inner["groups"]["contextId"], FIRST_SYMBOL);
    let host = after["layers"][0].as_object().unwrap();
    assert!(
        !host.contains_key("groups"),
        "the dead registry copy is not carried"
    );
    assert_eq!(decode(&after), plan.document, "storage round trip");
}

#[test]
fn moved_strokes_keep_their_ids_frames_and_leave_the_scene_and_its_order_plan() {
    let doc = decode(&s1_document(json!({})));
    let plan = plan_convert(&doc, 1, &uids(&["ly_hand"])).unwrap();
    let after = wire(&plan.document);
    let moved: Vec<(String, u64)> = after["componentTables"]["components"]["components"][0]
        ["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            assert_eq!(r["target"]["contextId"], FIRST_SYMBOL);
            (
                r["target"]["strokeId"].as_str().unwrap().to_string(),
                r["target"]["frameScope"]["frame"].as_u64().unwrap(),
            )
        })
        .collect();
    let expected = [("sa", 0), ("sb", 0), ("sc", 0), ("sa", 12), ("sb", 12)];
    assert_eq!(moved, expected.map(|(s, f)| (s.to_string(), f)));
    let scene: Vec<&str> = after["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["target"]["layerUid"].as_str().unwrap())
        .collect();
    assert_eq!(scene, ["ly_other"]);
    let elements: Vec<&str> = after["compositionOrder"]["contexts"][0]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["elements"].as_array().unwrap().iter())
        .map(|e| e["layerUid"].as_str().unwrap())
        .collect();
    assert_eq!(elements, ["ly_other"]);
}

#[test]
fn several_layers_merge_into_one_instance_at_the_first_source_slot() {
    let o = convert_oracle();
    let doc = decode(&s2_document());
    let plan = plan_convert(&doc, 5, &uids(&["ly_c", "ly_a", "ly_b", "ly_a"])).unwrap();
    assert_eq!(plan.symbol_id.as_deref(), Some("native-component:5:0"));
    assert_eq!(plan.created.as_deref(), Some("native-layer:5:0"));
    let after = wire(&plan.document);
    verify(&after, "native-component:5:0", "S2", &o).unwrap();
    let order: Vec<&str> = after["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["ly_x", "native-layer:5:0", "ly_y"]);
    let slots = positions(&after);
    assert_eq!(slots["ly_x"], 0);
    assert_eq!(slots["native-layer:5:0"], 1);
    assert_eq!(slots["ly_y"], 2);
    assert_eq!(slots.len(), 3);
    // The first source's duplicator moved to the instance (`firstSrc.duplicator`).
    let hosts = after["componentTables"]["duplicators"]["duplicators"]
        .as_array()
        .unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0]["layerUid"], "native-layer:5:0");
    assert_eq!(
        hosts[0]["duplicator"],
        json!({"mode":"grid","rows":2,"cols":3})
    );
    assert_eq!(decode(&after), plan.document, "storage round trip");
}

#[test]
fn merged_groups_stay_per_layer_with_their_original_ids() {
    let doc = decode(&s2_document());
    let plan = plan_convert(&doc, 5, &uids(&["ly_a", "ly_b", "ly_c"])).unwrap();
    let after = wire(&plan.document);
    let layers = &after["componentTables"]["components"]["components"][7]["layers"];
    assert_eq!(layers[0]["groups"]["contextId"], "native-component:5:0");
    assert_eq!(layers[0]["groups"]["entries"][0]["gid"], "g1");
    assert_eq!(layers[1]["groups"]["entries"][0]["gid"], "g1");
    assert_eq!(layers[2].get("groups"), None);
}
