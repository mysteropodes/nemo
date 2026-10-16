//! Negative and mutation controls for the layer-stack planner (L08-A).
//!
//! A local reference copy of the matte / consumption / predicate logic takes
//! one deliberate `Mutation`. Each mutation must disagree with the oracle on a
//! named row and family (so the oracle can actually fail), while the unmutated
//! copy agrees with both the oracle and the production kernel.

use super::tests::{bits, mattes_json, row, rows, rows_of, slow_json, Row, ORACLE};
use super::*;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mutation {
    None,
    /// The legacy fallback lands two above instead of one.
    FallbackSkipsOne,
    /// The legacy fallback points at the layer itself.
    FallbackIsSelf,
    /// The explicit-index self check is dropped.
    DropSelfCheck,
    /// An explicit self index falls back to the legacy layer above.
    ExplicitSelfFallsBack,
    /// An out-of-range explicit index falls back to the legacy layer above.
    ExplicitOutOfRangeFallsBack,
    /// The invert flag of every matte mode is swapped.
    SwapInvert,
    /// An extra matte without a source gets the legacy fallback.
    ExtrasLegacyFallback,
    /// An extra whose source is the layer itself is kept.
    ExtrasKeepSelf,
    /// Extras are applied even when the first matte did not resolve.
    ExtrasWithoutFirst,
    /// A folder-shaped child is consumed like any other child.
    FolderChildConsumedWhenFolder,
    /// The slow-path predicate forgets folders.
    SlowIgnoresFolder,
    /// The slow-path predicate forgets per-item effects.
    SlowIgnoresItemEffects,
    /// The slow-path predicate forgets the effect-layer flag.
    SlowIgnoresEffectLayerFlag,
    /// An authored "normal" blend counts as a blend.
    SlowNormalCountsAsBlend,
}

fn mode_of(m: Mutation, s: Option<&str>) -> Option<(u32, bool)> {
    let r = matte_mode_of(s);
    if m == Mutation::SwapInvert {
        r.map(|(mode, invert)| (mode, !invert))
    } else {
        r
    }
}

fn legacy(m: Mutation, i: usize, n: usize) -> Option<usize> {
    match m {
        Mutation::FallbackSkipsOne => (i + 2 < n).then_some(i + 2),
        Mutation::FallbackIsSelf => (i < n).then_some(i),
        _ => (i + 1 < n).then_some(i + 1),
    }
}

fn source_of(m: Mutation, layers: &[LayerComposition], i: usize) -> Option<usize> {
    let layer = layers.get(i)?;
    mode_of(m, layer.matte_mode.as_deref())?;
    let n = layers.len();
    match layer.matte_source_index {
        Some(s) => {
            let valid = if m == Mutation::DropSelfCheck {
                s < n
            } else {
                s < n && s != i
            };
            if valid {
                Some(s)
            } else if (m == Mutation::ExplicitSelfFallsBack && s == i)
                || (m == Mutation::ExplicitOutOfRangeFallsBack && s >= n)
            {
                legacy(m, i, n)
            } else {
                None
            }
        }
        None => legacy(m, i, n),
    }
}

fn all_of(m: Mutation, layers: &[LayerComposition], i: usize) -> Vec<MatteStep> {
    let mut out = Vec::new();
    let layer = match layers.get(i) {
        Some(layer) => layer,
        None => return out,
    };
    if let (Some((mode, invert)), Some(source)) = (
        mode_of(m, layer.matte_mode.as_deref()),
        source_of(m, layers, i),
    ) {
        out.push(MatteStep {
            source,
            mode,
            invert,
        });
    }
    if out.is_empty() && m != Mutation::ExtrasWithoutFirst {
        return out;
    }
    let n = layers.len();
    for e in layer.mattes_more.iter().flatten() {
        let source = match e.source_index {
            Some(s) if s < n && (s != i || m == Mutation::ExtrasKeepSelf) => s,
            None if m == Mutation::ExtrasLegacyFallback && i + 1 < n => i + 1,
            _ => continue,
        };
        if let Some((mode, invert)) = mode_of(m, e.mode.as_deref()) {
            out.push(MatteStep {
                source,
                mode,
                invert,
            });
        }
    }
    out
}

fn matte_set_of(m: Mutation, layers: &[LayerComposition]) -> Vec<bool> {
    let mut set = vec![false; layers.len()];
    for i in 0..layers.len() {
        for step in all_of(m, layers, i) {
            set[step.source] = true;
        }
    }
    set
}

fn folder_set_of(m: Mutation, layers: &[LayerComposition]) -> Vec<bool> {
    let n = layers.len();
    let folder = |l: &LayerComposition| l.is_folder_layer.unwrap_or(false);
    let mut set = vec![false; n];
    for layer in layers.iter().filter(|l| folder(*l)) {
        for &ci in layer.folder_child_indices.iter().flatten() {
            if ci < n && (m == Mutation::FolderChildConsumedWhenFolder || !folder(&layers[ci])) {
                set[ci] = true;
            }
        }
    }
    set
}

fn slow_of(m: Mutation, layers: &[LayerComposition]) -> SlowPathReasons {
    let mut s = SlowPathReasons::of(layers);
    match m {
        Mutation::SlowIgnoresFolder => s.folder = false,
        Mutation::SlowIgnoresItemEffects => {
            s.effect_stack = layers.iter().any(|l| l.layer_effect_enabled)
        }
        Mutation::SlowIgnoresEffectLayerFlag => s.effect_layer = false,
        Mutation::SlowNormalCountsAsBlend => {
            s.blend = layers.iter().any(|l| l.blend_mode.is_some())
        }
        _ => {}
    }
    s
}

/// Expectation families on which the (possibly mutated) copy disagrees.
fn failing(r: &Row, m: Mutation) -> Vec<&'static str> {
    let l = &r.layers;
    let e = &r.expect;
    let mut bad = Vec::new();
    let sources: Vec<Value> = (0..l.len())
        .map(|i| source_of(m, l, i).map_or(Value::Null, |s| s.into()))
        .collect();
    if Value::Array(sources) != e["matteSource"] {
        bad.push("matteSource");
    }
    let all: Vec<Value> = (0..l.len())
        .map(|i| mattes_json(&all_of(m, l, i)))
        .collect();
    if Value::Array(all) != e["mattes"] {
        bad.push("mattes");
    }
    if bits(&matte_set_of(m, l)) != e["matteSet"].as_str().unwrap() {
        bad.push("matteSet");
    }
    if bits(&folder_set_of(m, l)) != e["folderSet"].as_str().unwrap() {
        bad.push("folderSet");
    }
    if slow_json(&slow_of(m, l)) != e["slow"] {
        bad.push("slow");
    }
    bad
}

#[test]
fn unmutated_reference_copy_matches_oracle_and_production() {
    for r in rows() {
        assert_eq!(failing(&r, Mutation::None), Vec::<&str>::new(), "{}", r.id);
        for i in 0..r.layers.len() {
            assert_eq!(
                all_of(Mutation::None, &r.layers, i),
                resolve_all_mattes(&r.layers, i),
                "{} layer {i}",
                r.id
            );
        }
        assert_eq!(
            matte_set_of(Mutation::None, &r.layers),
            matte_source_set(&r.layers)
        );
        assert_eq!(
            folder_set_of(Mutation::None, &r.layers),
            folder_child_set(&r.layers)
        );
        assert_eq!(
            slow_of(Mutation::None, &r.layers),
            SlowPathReasons::of(&r.layers)
        );
    }
}

#[test]
fn every_mutation_flips_a_named_oracle_literal() {
    let cases: [(Mutation, &str, &str); 14] = [
        (
            Mutation::FallbackSkipsOne,
            "matteSource",
            "matte-legacy-above",
        ),
        (
            Mutation::FallbackIsSelf,
            "matteSource",
            "matte-legacy-above",
        ),
        (
            Mutation::DropSelfCheck,
            "matteSource",
            "matte-explicit-self-no-fallback-flat",
        ),
        (
            Mutation::ExplicitSelfFallsBack,
            "matteSource",
            "matte-explicit-self-no-fallback-flat",
        ),
        (
            Mutation::ExplicitOutOfRangeFallsBack,
            "matteSource",
            "matte-explicit-out-of-range-no-fallback-flat",
        ),
        (
            Mutation::SwapInvert,
            "mattes",
            "matte-four-modes-shared-source",
        ),
        (
            Mutation::ExtrasLegacyFallback,
            "mattes",
            "extras-never-get-legacy-fallback",
        ),
        (
            Mutation::ExtrasKeepSelf,
            "mattes",
            "extras-drop-self-oob-badmode-and-absent-source",
        ),
        (
            Mutation::ExtrasWithoutFirst,
            "mattes",
            "extras-ignored-when-first-self-flat",
        ),
        (
            Mutation::FolderChildConsumedWhenFolder,
            "folderSet",
            "folder-nested-folder-child-stays-top-level",
        ),
        (Mutation::SlowIgnoresFolder, "slow", "folder-basic"),
        (Mutation::SlowIgnoresItemEffects, "slow", "item-effect-only"),
        (
            Mutation::SlowIgnoresEffectLayerFlag,
            "slow",
            "effect-layer-with-nothing-enabled-passthrough",
        ),
        (
            Mutation::SlowNormalCountsAsBlend,
            "slow",
            "blend-normal-string-and-unknown-stay-flat",
        ),
    ];
    for (m, family, id) in cases {
        let r = row(id);
        assert!(failing(&r, Mutation::None).is_empty(), "{id} baseline");
        let bad = failing(&r, m);
        assert!(
            bad.contains(&family),
            "{m:?} must flip {family} on {id}, got {bad:?}"
        );
        let total = rows().iter().filter(|r| !failing(r, m).is_empty()).count();
        assert!(total >= 1, "{m:?} caught by {total} rows");
    }
}

#[test]
fn a_corrupted_oracle_literal_is_detected() {
    let wanted = "\"matteSource\":[1,null,null]";
    let text = ORACLE.replace(wanted, "\"matteSource\":[2,null,null]");
    assert_ne!(text, ORACLE, "row literal must exist to be corrupted");
    let bad: Vec<String> = rows_of(&text)
        .into_iter()
        .filter(|r| !failing(r, Mutation::None).is_empty())
        .map(|r| r.id)
        .collect();
    assert!(bad.contains(&"matte-legacy-above".to_owned()), "{bad:?}");
}

#[test]
fn oracle_coverage_guard_spans_vocabulary_and_plan_shapes() {
    let mut blends = [false; 16];
    let mut matte_modes = std::collections::BTreeSet::new();
    let (mut skips, mut effect_layers, mut sources) = ([false; 3], [false; 2], [false; 3]);
    let mut slow_seen = [false; 6];
    for r in rows() {
        let plan = plan_composition(&r.layers);
        if let CompositionPlan::Layered { steps, .. } = &plan {
            for s in steps {
                match s {
                    LayerStep::Skipped { reason, .. } => skips[*reason as usize] = true,
                    LayerStep::EffectLayer { applies_stack, .. } => {
                        effect_layers[*applies_stack as usize] = true
                    }
                    LayerStep::Paint(p) => {
                        blends[p.blend as usize] = true;
                        for m in &p.mattes {
                            matte_modes.insert((m.mode, m.invert));
                        }
                        match p.source {
                            PaintSource::Plain => sources[0] = true,
                            PaintSource::ElementEffects => sources[1] = true,
                            PaintSource::Folder(_) => sources[2] = true,
                        }
                    }
                }
            }
        }
        let s = SlowPathReasons::of(&r.layers);
        for (k, flag) in [
            s.blend,
            s.matte,
            s.effect_stack,
            s.effect_layer,
            s.mask,
            s.folder,
        ]
        .into_iter()
        .enumerate()
        {
            slow_seen[k] |= flag;
        }
    }
    assert_eq!(blends, [true; 16], "all sixteen blend indices are painted");
    assert_eq!(matte_modes.len(), 4, "all four matte (mode, invert) pairs");
    assert_eq!(
        (skips, effect_layers, sources),
        ([true; 3], [true; 2], [true; 3])
    );
    assert_eq!(slow_seen, [true; 6], "every slow-path reason is exercised");
}

#[test]
fn exhaustive_small_stacks_hold_planner_invariants() {
    let palette: [fn() -> LayerComposition; 7] = [
        LayerComposition::default,
        || LayerComposition {
            matte_mode: Some("alpha".into()),
            ..Default::default()
        },
        || LayerComposition {
            matte_mode: Some("luma".into()),
            matte_source_index: Some(0),
            mattes_more: Some(vec![MatteExtra {
                mode: Some("alphaInverted".into()),
                source_index: Some(2),
            }]),
            ..Default::default()
        },
        || LayerComposition {
            is_folder_layer: Some(true),
            folder_child_indices: Some(vec![0, 1, 5]),
            ..Default::default()
        },
        || LayerComposition {
            is_canvas_background: Some(true),
            ..Default::default()
        },
        || LayerComposition {
            is_effect_layer: Some(true),
            layer_effect_enabled: true,
            ..Default::default()
        },
        || LayerComposition {
            blend_mode: Some("hue".into()),
            mask_count: 1,
            ..Default::default()
        },
    ];
    for n in 0..=3usize {
        for code in 0..palette.len().pow(n as u32) {
            let layers: Vec<LayerComposition> = (0..n)
                .map(|k| palette[(code / palette.len().pow(k as u32)) % palette.len()]())
                .collect();
            let plan = plan_composition(&layers);
            let slow = SlowPathReasons::of(&layers);
            match plan {
                CompositionPlan::Flat { order } => {
                    assert!(!slow.any(), "{code}/{n}");
                    assert_eq!(order, (0..n).collect::<Vec<_>>());
                }
                CompositionPlan::Layered { background, steps } => {
                    assert!(slow.any(), "{code}/{n}");
                    assert_eq!(background, background_index(&layers));
                    assert_eq!(steps.len(), n, "one step per top-level layer");
                    for (i, s) in steps.iter().enumerate() {
                        match s {
                            LayerStep::Skipped { index, .. }
                            | LayerStep::EffectLayer { index, .. } => assert_eq!(*index, i),
                            LayerStep::Paint(p) => {
                                assert_eq!(p.index, i);
                                assert!(p.mattes.iter().all(|m| m.source < n));
                                if let PaintSource::Folder(kids) = &p.source {
                                    assert!(kids.iter().all(|k| k.index < n));
                                    assert_eq!(
                                        kids.iter().filter(|k| k.seeds_accumulator).count(),
                                        kids.len().min(1)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
