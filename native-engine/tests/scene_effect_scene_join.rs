//! L08-K pure join tests: every row of the baseline oracle (`fixtures/scene-effect-scene-join/oracle.txt`) is a
//! scenario whose expected wire slots were produced by the UNMODIFIED baseline statements of `buildSceneJson` and
//! `sceneEffectsOf` (see `tools/gen_scene_effect_scene_join_oracle.js`), plus hand-derived literals for the plan
//! shapes and the effect-stack landing. The document-driven half is `docs`, the negative controls are `controls`.
use super::{plan_layers, slot_role, JoinLayerInput, JoinOptions, LayerPlans, SlotRole};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    CompositionPlan, LayerStep, PaintSource, PaintStep,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    EffectPass, EffectSpec, EffectStackPlan, Landing, PassSource, PassStep, PassTarget,
};
use serde_json::{json, Map, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/scene-effect-scene-join/oracle.txt");

pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) native: bool,
    pub(crate) layers: Vec<Value>,
    pub(crate) wire: Vec<Value>,
}

pub(crate) fn lines_of(tag: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .filter_map(|line| line.strip_prefix(tag))
        .map(|rest| serde_json::from_str(rest.trim()).expect("oracle line is JSON"))
        .collect()
}

pub(crate) fn rows() -> Vec<Row> {
    lines_of("S ")
        .into_iter()
        .map(|row| Row {
            id: row["id"].as_str().unwrap().to_owned(),
            native: row["native"].as_bool().unwrap(),
            layers: row["layers"].as_array().unwrap().clone(),
            wire: row["wire"].as_array().unwrap().clone(),
        })
        .collect()
}

fn text(layer: &Value, key: &str) -> Option<String> {
    layer.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn flag(layer: &Value, key: &str) -> bool {
    layer.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn effect_spec(entry: &Value) -> EffectSpec {
    EffectSpec {
        effect_type: entry["type"].as_str().unwrap().to_owned(),
        enabled: entry["enabled"].as_bool().unwrap(),
        params: [1, 2, 3, 4, 5, 6, 7, 8].map(|k: usize| {
            entry
                .get(format!("p{k}"))
                .and_then(Value::as_f64)
                .map(|v| v as f32)
        }),
    }
}

/// Scenario layers -> join inputs; the position is the wire index (the oracle never reorders).
pub(crate) fn inputs_of(layers: &[Value]) -> Vec<JoinLayerInput> {
    layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let mut input = JoinLayerInput::plain(layer["layerUid"].as_str().unwrap(), index);
            input.visible = layer
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            input.is_widget = flag(layer, "isWidgetLayer");
            input.is_effect = flag(layer, "isEffectLayer");
            input.is_folder = flag(layer, "isFolderLayer");
            input.blend_mode = text(layer, "blendMode");
            input.matte_mode = text(layer, "matteMode");
            input.matte_source_uid = text(layer, "matteSourceLayerUid");
            input.effects_from = text(layer, "effectsFrom");
            input.parent_uid = text(layer, "parentLayerUid");
            input.mattes_more = layer
                .get("mattesMore")
                .and_then(Value::as_array)
                .map(|extras| {
                    extras
                        .iter()
                        .map(|x| {
                            (
                                x["uid"].as_str().unwrap().to_owned(),
                                x["mode"].as_str().unwrap().to_owned(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            input.effects = layer
                .get("effects")
                .and_then(Value::as_array)
                .map(|entries| entries.iter().map(effect_spec).collect())
                .unwrap_or_default();
            input
        })
        .collect()
}

/// The oracle's wire projection of the join result (members the baseline leaves undefined are omitted).
pub(crate) fn wire_of(plans: &LayerPlans) -> Vec<Value> {
    plans
        .composition
        .iter()
        .zip(plans.layers.iter())
        .map(|(c, layer)| {
            let mut slot = Map::new();
            if let Some(name) = &c.blend_mode {
                slot.insert("blendMode".into(), json!(name));
            }
            if let Some(name) = &c.matte_mode {
                slot.insert("matteMode".into(), json!(name));
            }
            if let Some(index) = c.matte_source_index {
                slot.insert("matteSourceIndex".into(), json!(index));
            }
            if c.is_effect_layer == Some(true) {
                slot.insert("isEffectLayer".into(), json!(true));
            }
            if c.is_folder_layer == Some(true) {
                slot.insert("isFolderLayer".into(), json!(true));
            }
            if let Some(children) = &c.folder_child_indices {
                slot.insert("folderChildIndices".into(), json!(children));
            }
            if let Some(extras) = &c.mattes_more {
                let wire: Vec<Value> = extras
                    .iter()
                    .map(|x| {
                        let mut entry = Map::new();
                        entry.insert("mode".into(), json!(x.mode.clone().unwrap()));
                        entry.insert("sourceIndex".into(), json!(x.source_index.unwrap()));
                        Value::Object(entry)
                    })
                    .collect();
                slot.insert("mattesMore".into(), Value::Array(wire));
            }
            if !layer.effects.is_empty() {
                let wire: Vec<Value> = layer
                    .effects
                    .iter()
                    .map(|e| {
                        let mut entry = Map::new();
                        entry.insert("effectType".into(), json!(e.effect_type));
                        entry.insert("enabled".into(), json!(e.enabled));
                        for (k, p) in e.params.iter().enumerate() {
                            if let Some(v) = p {
                                entry.insert(format!("p{}", k + 1), json!(f64::from(*v)));
                            }
                        }
                        Value::Object(entry)
                    })
                    .collect();
                slot.insert("effects".into(), Value::Array(wire));
            }
            Value::Object(slot)
        })
        .collect()
}

/// Integers and floats compare equal after this (the oracle writes `12`, the join emits `12.0`).
pub(crate) fn norm(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.into_iter().map(norm).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        other => other,
    }
}

pub(crate) fn plan_of(inputs: &[JoinLayerInput], zoom: f64) -> LayerPlans {
    let none = |_: &str| false;
    plan_layers(inputs, &JoinOptions::new(zoom, &none))
}

#[test]
fn oracle_header_and_counts_are_pinned() {
    let header = &lines_of("H ")[0];
    assert_eq!(
        header["baseline"],
        "fbe4b8a33e6d5a229f014154df2ba21988a9fabc"
    );
    for (key, sha) in [
        (
            "engineBridge",
            "729e94ad18b93574936c1c8501ba513289ff0d763e85b5471f1341dba36a65af",
        ),
        (
            "sceneEffectsOf",
            "bf6977beb22096e386c117f23e2dd7853cd6d7357da0549bb4c8ba524b620915",
        ),
        (
            "layerLoopHead",
            "d3b4afcdbf2d781f8f52f23e01317b7c6ec107ee1d46c5a9f8d7ad7c0bf5e9a5",
        ),
        (
            "blendMatteReads",
            "23422aacb432528d11b1ad0d4c5cd9dfd94f9c0a5c6cc4f5039b6cb5b831b906",
        ),
        (
            "slotPush",
            "c241c44f325a199dcdde0fce30c2e05eb2503c23970b17b93246a762ab517bbc",
        ),
        (
            "finalPasses",
            "a461dd28411ab55424eae79ec52fd1cf2ffd13d3fe1e02e7417aa6fe7d73e2d3",
        ),
    ] {
        assert_eq!(header[key], sha, "{key}");
    }
    assert_eq!(header["scenarios"], 26);
    assert_eq!(rows().len(), 26);
    assert_eq!(rows().iter().filter(|r| r.native).count(), 19);
    assert_eq!(lines_of("M ").len(), 10);
}

#[test]
fn every_oracle_row_matches_the_pure_join() {
    for row in rows() {
        let plans = plan_of(&inputs_of(&row.layers), 1.0);
        assert_eq!(
            norm(Value::Array(wire_of(&plans))),
            norm(Value::Array(row.wire.clone())),
            "{}",
            row.id
        );
        assert_eq!(plans.layers.len(), row.layers.len(), "{}", row.id);
    }
}

#[test]
fn slot_roles_follow_the_baseline_branch_order() {
    let mut layer = JoinLayerInput::plain("A", 0);
    assert_eq!(slot_role(&layer), SlotRole::Plain);
    layer.is_effect = true;
    assert_eq!(slot_role(&layer), SlotRole::Effect);
    layer.is_widget = true;
    assert_eq!(slot_role(&layer), SlotRole::Empty);
    layer.is_folder = true;
    assert_eq!(slot_role(&layer), SlotRole::Folder);
    layer.visible = false;
    assert_eq!(slot_role(&layer), SlotRole::Empty);
}

#[test]
fn an_all_default_stack_is_the_flat_authored_order() {
    let inputs: Vec<_> = ["A", "B", "C"]
        .iter()
        .enumerate()
        .map(|(i, uid)| JoinLayerInput::plain(uid, i))
        .collect();
    let plans = plan_of(&inputs, 1.0);
    assert_eq!(
        plans.plan,
        CompositionPlan::Flat {
            order: vec![0, 1, 2]
        }
    );
    assert!(plans.composition.iter().all(|c| *c == Default::default()));
    assert!(plans
        .layers
        .iter()
        .all(|l| l.stack.is_none() && l.effects.is_empty()));
}

#[test]
fn an_unknown_blend_name_is_kept_as_authored_but_plans_as_normal() {
    let mut inputs = vec![JoinLayerInput::plain("A", 0), JoinLayerInput::plain("B", 1)];
    inputs[1].blend_mode = Some("bogus".into());
    let plans = plan_of(&inputs, 1.0);
    assert_eq!(plans.composition[1].blend_mode.as_deref(), Some("bogus"));
    assert_eq!(plans.plan, CompositionPlan::Flat { order: vec![0, 1] });
    inputs[1].blend_mode = Some("normal".into());
    assert_eq!(plan_of(&inputs, 1.0).composition[1].blend_mode, None);
}

#[test]
fn a_multiply_layer_makes_a_layered_plan_with_the_blend_index_on_its_step() {
    let mut inputs: Vec<_> = ["A", "B", "C"]
        .iter()
        .enumerate()
        .map(|(i, uid)| JoinLayerInput::plain(uid, i))
        .collect();
    inputs[1].blend_mode = Some("multiply".into());
    let plans = plan_of(&inputs, 1.0);
    let CompositionPlan::Layered { background, steps } = &plans.plan else {
        panic!("a blend needs the layered path");
    };
    assert_eq!(*background, None);
    let blends: Vec<u32> = steps
        .iter()
        .map(|step| match step {
            LayerStep::Paint(PaintStep { blend, .. }) => *blend,
            other => panic!("unexpected step {other:?}"),
        })
        .collect();
    assert_eq!(blends, vec![0, 1, 0]);
}

#[test]
fn mask_count_and_item_effects_flow_from_their_providers_into_the_plan() {
    let mut inputs = vec![JoinLayerInput::plain("A", 0)];
    inputs[0].mask_count = 2;
    inputs[0].item_effect_enabled = true;
    let plans = plan_of(&inputs, 1.0);
    assert_eq!(plans.composition[0].mask_count, 2);
    assert!(plans.composition[0].item_effect_enabled);
    let CompositionPlan::Layered { steps, .. } = &plans.plan else {
        panic!("masks and item effects need the layered path");
    };
    let LayerStep::Paint(step) = &steps[0] else {
        panic!("a plain layer paints");
    };
    assert_eq!(step.mask_count, 2);
    assert_eq!(step.source, PaintSource::ElementEffects);
}

fn blur(enabled: bool, radius: f32) -> EffectSpec {
    let mut spec = EffectSpec::new("blur", enabled);
    spec.params[0] = Some(radius);
    spec
}

#[test]
fn an_effect_layer_lands_in_the_final_target_and_every_other_layer_ping_pongs() {
    let mut plain = JoinLayerInput::plain("A", 0);
    plain.effects = vec![blur(true, 12.0)];
    let mut grade = JoinLayerInput::plain("B", 1);
    grade.is_effect = true;
    grade.effects = vec![blur(true, 12.0)];
    let plans = plan_of(&[plain, grade], 2.0);
    let step = |target| PassStep {
        effect_index: 0,
        source: PassSource::Initial,
        target,
        pass: EffectPass::Blur { radius_px: 24.0 },
    };
    assert_eq!(
        plans.layers[0].stack,
        Some(EffectStackPlan {
            steps: vec![step(PassTarget::StackA)],
            landing: Landing::StackA
        })
    );
    assert_eq!(
        plans.layers[1].stack,
        Some(EffectStackPlan {
            steps: vec![step(PassTarget::Final)],
            landing: Landing::Final
        })
    );
    assert_eq!(plans.composition[1].is_effect_layer, Some(true));
    assert!(plans.composition[1].layer_effect_enabled);
}

#[test]
fn a_disabled_or_hidden_stack_runs_no_pass() {
    let mut off = JoinLayerInput::plain("A", 0);
    off.effects = vec![blur(false, 3.0)];
    let mut hidden = JoinLayerInput::plain("B", 1);
    hidden.visible = false;
    hidden.effects = vec![blur(true, 3.0)];
    let plans = plan_of(&[off, hidden], 1.0);
    assert!(plans.layers[0].stack.is_none());
    assert_eq!(plans.layers[0].effects.len(), 1);
    assert!(!plans.composition[0].layer_effect_enabled);
    assert!(plans.layers[1].stack.is_none());
    assert!(plans.layers[1].effects.is_empty());
    assert_eq!(plans.plan, CompositionPlan::Flat { order: vec![0, 1] });
}

#[test]
fn custom_effect_registration_comes_from_the_caller() {
    let mut layer = JoinLayerInput::plain("A", 0);
    layer.effects = vec![EffectSpec::new("custom:twirl", true)];
    for registered in [true, false] {
        let lookup = move |name: &str| registered && name == "custom:twirl";
        let plans = plan_layers(&[layer.clone()], &JoinOptions::new(1.0, &lookup));
        let stack = plans.layers[0].stack.as_ref().unwrap();
        let EffectPass::Custom {
            name,
            registered: flagged,
            ..
        } = &stack.steps[0].pass
        else {
            panic!("a custom: type plans a custom pass");
        };
        assert_eq!(name, "custom:twirl");
        assert_eq!(*flagged, registered);
    }
}
