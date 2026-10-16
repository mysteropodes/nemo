//! Oracle-driven tests for the layer-stack composition planner (L08-A). The
//! hand-written literal cases live in `scene_effect_order_cases.rs`.
//!
//! `fixtures/scene-effect-order/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_scene_effect_order_oracle.js` against baseline commit
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc. Its `#` header pins the SHA-256
//! of every extracted `geometry-wasm/src/engine.rs` region (mix_mode_index,
//! matte_mode_of, resolve_all_mattes, resolve_matte_source, the composite_scene
//! precompute / dispatch / epilogue and the folder child loop) and of the
//! verbatim `src/js/engine-bridge.js` final uid->wire-index passes. The Rust
//! regions were executed through a hand-written Node transliteration (no
//! compiler was available when the oracle was cut); the three bridge passes
//! ran unmodified in a Node `vm`.
//!
//! One JSON row per line. `layers` holds the wire fields the planner reads
//! (`blendMode matteMode matteSourceIndex mattesMore isEffectLayer
//! isCanvasBackground isFolderLayer folderChildIndices`) plus the reductions
//! `fx` (any enabled layer effect), `ifx` (any enabled item effect) and `masks`
//! (count). `expect` holds `matteSource` (per layer), `mattes`
//! (`[source, mode, invert]` per layer), `matteSet` / `folderSet` (0/1 strings),
//! `bg`, `slow` (blend matte fx fxlayer mask folder), `path` and `plan`
//! (rendered by `describe`). `bridge` rows add `bridge.intended`: the
//! matteSourceIndex the verbatim JS emitted, `null` where it left it unset.
//! Every literal below that is not read from the oracle was written by hand
//! from engine.rs, independent of the transliteration.

use super::*;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/scene-effect-order/oracle.txt");

pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) layers: Vec<LayerComposition>,
    pub(crate) expect: Value,
    pub(crate) intended: Option<Vec<Option<usize>>>,
}

fn opt_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn opt_usize(v: &Value, k: &str) -> Option<usize> {
    v.get(k).and_then(Value::as_u64).map(|n| n as usize)
}

fn opt_bool(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_i64).unwrap_or(0) != 0
}

fn layer_of(v: &Value) -> LayerComposition {
    LayerComposition {
        blend_mode: opt_str(v, "blendMode"),
        matte_mode: opt_str(v, "matteMode"),
        matte_source_index: opt_usize(v, "matteSourceIndex"),
        mattes_more: v.get("mattesMore").and_then(Value::as_array).map(|a| {
            a.iter()
                .map(|m| MatteExtra {
                    mode: opt_str(m, "mode"),
                    source_index: opt_usize(m, "sourceIndex"),
                })
                .collect()
        }),
        is_effect_layer: opt_bool(v, "isEffectLayer"),
        is_canvas_background: opt_bool(v, "isCanvasBackground"),
        is_folder_layer: opt_bool(v, "isFolderLayer"),
        folder_child_indices: v
            .get("folderChildIndices")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_u64)
                    .map(|n| n as usize)
                    .collect()
            }),
        layer_effect_enabled: flag(v, "fx"),
        item_effect_enabled: flag(v, "ifx"),
        mask_count: opt_usize(v, "masks").unwrap_or(0),
    }
}

pub(crate) fn rows_of(text: &str) -> Vec<Row> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle row is json");
            Row {
                id: v["id"].as_str().expect("row id").to_owned(),
                kind: v["kind"].as_str().expect("row kind").to_owned(),
                layers: v["layers"]
                    .as_array()
                    .expect("layers")
                    .iter()
                    .map(layer_of)
                    .collect(),
                expect: v["expect"].clone(),
                intended: v.get("bridge").map(|b| {
                    b["intended"]
                        .as_array()
                        .expect("intended")
                        .iter()
                        .map(|x| x.as_u64().map(|n| n as usize))
                        .collect()
                }),
            }
        })
        .collect()
}

pub(crate) fn rows() -> Vec<Row> {
    rows_of(ORACLE)
}

pub(crate) fn row(id: &str) -> Row {
    rows()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("oracle row {id}"))
}

pub(crate) fn bits(set: &[bool]) -> String {
    set.iter().map(|&b| if b { '1' } else { '0' }).collect()
}

fn join<T: ToString>(items: &[T]) -> String {
    items.iter().map(T::to_string).collect::<Vec<_>>().join(",")
}

fn step_text(s: &LayerStep) -> String {
    match s {
        LayerStep::Skipped { index, reason } => {
            let why = match reason {
                SkipReason::MatteSource => "matte",
                SkipReason::FolderChild => "folderchild",
                SkipReason::CanvasBackground => "bg",
            };
            format!("skip:{index}:{why}")
        }
        LayerStep::EffectLayer {
            index,
            applies_stack,
        } => {
            format!(
                "fx:{index}:{}",
                if *applies_stack { "stack" } else { "pass" }
            )
        }
        LayerStep::Paint(p) => {
            let source = match &p.source {
                PaintSource::Plain => "plain".to_owned(),
                PaintSource::ElementEffects => "elem".to_owned(),
                PaintSource::Folder(kids) => {
                    let kids: Vec<String> = kids
                        .iter()
                        .map(|c| {
                            format!(
                                "{}.{}.{}.{}.{}",
                                c.index,
                                c.item_effects as u8,
                                c.layer_effects as u8,
                                c.blend,
                                c.seeds_accumulator as u8
                            )
                        })
                        .collect();
                    format!("folder({})", kids.join(","))
                }
            };
            let mattes: Vec<String> = p
                .mattes
                .iter()
                .map(|m| format!("{}.{}.{}", m.source, m.mode, m.invert as u8))
                .collect();
            format!(
                "paint:{}:{source}:k{}:m[{}]:l{}:b{}",
                p.index,
                p.mask_count,
                mattes.join(","),
                p.layer_effects as u8,
                p.blend
            )
        }
    }
}

pub(crate) fn describe(plan: &CompositionPlan) -> Vec<String> {
    match plan {
        CompositionPlan::Flat { order } => vec![format!("flat {}", join(order))],
        CompositionPlan::Layered { background, steps } => {
            let bg = background.map_or("-".to_owned(), |b| b.to_string());
            let mut out = vec![format!("layered bg={bg}")];
            out.extend(steps.iter().map(step_text));
            out
        }
    }
}

pub(crate) fn mattes_json(mattes: &[MatteStep]) -> Value {
    Value::Array(
        mattes
            .iter()
            .map(|m| serde_json::json!([m.source, m.mode, m.invert as u8]))
            .collect(),
    )
}

pub(crate) fn slow_json(s: &SlowPathReasons) -> Value {
    serde_json::json!({
        "blend": s.blend as u8, "matte": s.matte as u8, "fx": s.effect_stack as u8,
        "fxlayer": s.effect_layer as u8, "mask": s.mask as u8, "folder": s.folder as u8,
    })
}

pub(crate) fn layer() -> LayerComposition {
    LayerComposition::default()
}

pub(crate) fn matted(mode: &str, source: Option<usize>) -> LayerComposition {
    LayerComposition {
        matte_mode: Some(mode.to_owned()),
        matte_source_index: source,
        ..LayerComposition::default()
    }
}

pub(crate) fn extra(mode: Option<&str>, source: Option<usize>) -> MatteExtra {
    MatteExtra {
        mode: mode.map(str::to_owned),
        source_index: source,
    }
}

pub(crate) fn step(source: usize, mode: u32, invert: bool) -> MatteStep {
    MatteStep {
        source,
        mode,
        invert,
    }
}

pub(crate) fn folder(kids: &[usize]) -> LayerComposition {
    LayerComposition {
        is_folder_layer: Some(true),
        folder_child_indices: Some(kids.to_vec()),
        ..LayerComposition::default()
    }
}

#[test]
fn oracle_rows_are_well_formed() {
    let header = |prefix: &str| {
        ORACLE
            .lines()
            .find(|l| l.starts_with(prefix))
            .unwrap_or_default()
    };
    assert_eq!(
        header("# baseline commit: "),
        "# baseline commit: fbe4b8a33e6d5a229f014154df2ba21988a9fabc"
    );
    assert_eq!(
        header("# blend table"),
        "# blend table (parsed from extracted mix_mode_index): {\"multiply\":1,\"screen\":2,\"overlay\":3,\"darken\":4,\"lighten\":5,\"colorDodge\":6,\"colorBurn\":7,\"hardLight\":8,\"softLight\":9,\"difference\":10,\"exclusion\":11,\"hue\":12,\"saturation\":13,\"color\":14,\"luminosity\":15}"
    );
    let all = rows();
    assert_eq!(all.len(), 60);
    assert_eq!(all.iter().filter(|r| r.kind == "hand").count(), 49);
    assert_eq!(all.iter().filter(|r| r.kind == "bridge").count(), 11);
    let mut ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 60, "row ids are unique");
    assert!(all
        .iter()
        .filter(|r| r.kind == "bridge")
        .all(|r| r.intended.is_some()));
    assert!(all
        .iter()
        .filter(|r| r.kind == "hand")
        .all(|r| r.intended.is_none()));
}

#[test]
fn oracle_matte_resolution_rows() {
    for r in rows() {
        let sources: Vec<Value> = (0..r.layers.len())
            .map(|i| resolve_matte_source(&r.layers, i).map_or(Value::Null, |s| s.into()))
            .collect();
        assert_eq!(
            Value::Array(sources),
            r.expect["matteSource"],
            "{} matteSource",
            r.id
        );
        let all: Vec<Value> = (0..r.layers.len())
            .map(|i| mattes_json(&resolve_all_mattes(&r.layers, i)))
            .collect();
        assert_eq!(Value::Array(all), r.expect["mattes"], "{} mattes", r.id);
    }
}

#[test]
fn oracle_consumption_and_predicate_rows() {
    for r in rows() {
        assert_eq!(
            bits(&matte_source_set(&r.layers)),
            r.expect["matteSet"].as_str().unwrap(),
            "{} matteSet",
            r.id
        );
        assert_eq!(
            bits(&folder_child_set(&r.layers)),
            r.expect["folderSet"].as_str().unwrap(),
            "{} folderSet",
            r.id
        );
        let bg = background_index(&r.layers).map_or(Value::Null, |b| b.into());
        assert_eq!(bg, r.expect["bg"], "{} bg", r.id);
        assert_eq!(
            slow_json(&SlowPathReasons::of(&r.layers)),
            r.expect["slow"],
            "{} slow",
            r.id
        );
    }
}

#[test]
fn oracle_plan_rows() {
    let mut flat = 0;
    for r in rows() {
        let plan = plan_composition(&r.layers);
        let want: Vec<String> = r.expect["plan"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(describe(&plan), want, "{} plan", r.id);
        let is_flat = matches!(plan, CompositionPlan::Flat { .. });
        assert_eq!(is_flat, r.expect["path"] == "flat", "{} path", r.id);
        assert_eq!(
            is_flat,
            !SlowPathReasons::of(&r.layers).any(),
            "{} predicate agrees",
            r.id
        );
        flat += is_flat as usize;
    }
    assert_eq!(flat, 16, "flat rows pin the unchanged fast path");
}

#[test]
fn oracle_bridge_rows_reproduce_the_verbatim_js_passes() {
    for r in rows().into_iter().filter(|r| r.kind == "bridge") {
        let intended = r.intended.as_ref().unwrap();
        assert_eq!(intended.len(), r.layers.len(), "{}", r.id);
        for (i, l) in r.layers.iter().enumerate() {
            // What the JS emitted is exactly what the passes resolved.
            assert_eq!(l.matte_source_index, intended[i], "{} wire[{i}]", r.id);
            if matte_mode_of(l.matte_mode.as_deref()).is_some() {
                let got = resolve_matte_source(&r.layers, i);
                match intended[i] {
                    // Resolved uid: the kernel keeps the JS index.
                    Some(s) => assert_eq!(got, Some(s), "{} layer {i}", r.id),
                    // Unresolved uid: only the legacy i+1 remains.
                    None => assert_eq!(
                        got,
                        if i + 1 < r.layers.len() {
                            Some(i + 1)
                        } else {
                            None
                        },
                        "{} layer {i}",
                        r.id
                    ),
                }
            }
        }
    }
}

#[test]
fn bridge_scenarios_with_dangling_self_and_duplicate_uids_literal() {
    let r = row("bridge-valid-uid-reordered-by-order-sort");
    assert_eq!(r.layers[2].matte_source_index, Some(1));
    assert_eq!(resolve_matte_source(&r.layers, 2), Some(1));
    let r = row("bridge-dangling-uid-falls-back-to-legacy-above");
    assert_eq!(r.layers[1].matte_source_index, None);
    assert_eq!(resolve_matte_source(&r.layers, 1), Some(2));
    let r = row("bridge-self-uid-unresolved-then-legacy");
    assert_eq!(resolve_matte_source(&r.layers, 2), Some(3));
    let r = row("bridge-duplicate-uid-first-other-layer-wins");
    assert_eq!(r.layers[1].matte_source_index, Some(2));
    assert_eq!(r.layers[3].matte_source_index, Some(2));
    assert_eq!(resolve_matte_source(&r.layers, 1), Some(2));
    assert_eq!(resolve_matte_source(&r.layers, 3), Some(2));
    let r = row("bridge-source-without-entry-unresolved");
    assert_eq!(resolve_matte_source(&r.layers, 1), Some(2));
    let r = row("bridge-extras-valid-dangling-self-none-and-bad-mode");
    assert_eq!(
        resolve_all_mattes(&r.layers, 2),
        vec![step(3, 0, false), step(4, 1, false), step(1, 1, true)]
    );
    let r = row("bridge-extras-ride-along-but-first-unresolved-at-last-layer");
    assert!(r.layers[2].mattes_more.is_some());
    assert!(resolve_all_mattes(&r.layers, 2).is_empty());
    assert_eq!(
        plan_composition(&r.layers),
        CompositionPlan::Flat {
            order: vec![0, 1, 2]
        }
    );
    let r = row("bridge-extras-without-first-matte-mode-not-emitted");
    assert!(r.layers.iter().all(|l| l.mattes_more.is_none()));
    let r = row("bridge-folder-children-resolved-through-bg-shift-and-order");
    assert_eq!(r.layers[3].folder_child_indices, Some(vec![4, 2]));
    match plan_composition(&r.layers) {
        CompositionPlan::Layered { background, steps } => {
            assert_eq!(background, Some(0));
            match &steps[3] {
                LayerStep::Paint(p) => match &p.source {
                    PaintSource::Folder(kids) => {
                        assert_eq!(
                            kids.iter()
                                .map(|k| (k.index, k.seeds_accumulator))
                                .collect::<Vec<_>>(),
                            vec![(4, true), (2, false)]
                        )
                    }
                    other => panic!("folder source expected, got {other:?}"),
                },
                other => panic!("paint expected, got {other:?}"),
            }
        }
        other => panic!("expected layered, got {other:?}"),
    }
    let r = row("bridge-folder-with-no-resolvable-child-keeps-empty-list");
    assert_eq!(r.layers[1].folder_child_indices, Some(vec![]));
}
