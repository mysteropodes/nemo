//! Hand-written literal cases for the layer-stack planner (L08-A). Every
//! expected value was derived by reading `geometry-wasm/src/engine.rs` at
//! baseline fbe4b8a3 (line references in the production module), not read
//! from the oracle, so these cases are independent of the Node transliteration.

use super::tests::{extra, folder, layer, matted, step};
use super::*;

#[test]
fn blend_vocabulary_matches_the_baseline_literal_table() {
    let table = [
        ("multiply", 1),
        ("screen", 2),
        ("overlay", 3),
        ("darken", 4),
        ("lighten", 5),
        ("colorDodge", 6),
        ("colorBurn", 7),
        ("hardLight", 8),
        ("softLight", 9),
        ("difference", 10),
        ("exclusion", 11),
        ("hue", 12),
        ("saturation", 13),
        ("color", 14),
        ("luminosity", 15),
    ];
    for (name, index) in table {
        assert_eq!(mix_mode_index(Some(name)), index, "{name}");
        assert_eq!(BlendMode::from_name(Some(name)).index(), index, "{name}");
    }
    for (position, mode) in BlendMode::ALL.iter().enumerate() {
        assert_eq!(mode.index() as usize, position);
    }
    assert_eq!(BlendMode::ALL.len(), 16);
}

#[test]
fn absent_normal_and_unknown_blend_names_are_normal_but_distinct_inputs() {
    for name in [
        None,
        Some("normal"),
        Some("Multiply"),
        Some("MULTIPLY"),
        Some(""),
        Some("bogus"),
    ] {
        assert_eq!(mix_mode_index(name), 0, "{name:?}");
        assert_eq!(BlendMode::from_name(name), BlendMode::Normal, "{name:?}");
    }
    let absent = layer();
    let authored = LayerComposition {
        blend_mode: Some("normal".to_owned()),
        ..layer()
    };
    assert_ne!(absent, authored);
    assert!(!SlowPathReasons::of(&[absent]).any());
    assert!(!SlowPathReasons::of(&[authored]).any());
}

#[test]
fn matte_modes_map_to_mode_and_invert_literals() {
    assert_eq!(matte_mode_of(Some("alpha")), Some((0, false)));
    assert_eq!(matte_mode_of(Some("alphaInverted")), Some((0, true)));
    assert_eq!(matte_mode_of(Some("luma")), Some((1, false)));
    assert_eq!(matte_mode_of(Some("lumaInverted")), Some((1, true)));
    for other in [
        None,
        Some("none"),
        Some(""),
        Some("Alpha"),
        Some("LUMA"),
        Some("lumainverted"),
    ] {
        assert_eq!(matte_mode_of(other), None, "{other:?}");
    }
}

#[test]
fn first_matte_source_resolution_cases() {
    let at = |i: usize, m: LayerComposition| {
        let mut v = vec![layer(); 4];
        v[i] = m;
        resolve_matte_source(&v, i)
    };
    // Explicit index valid and not self: anywhere, above or below.
    assert_eq!(at(0, matted("alpha", Some(2))), Some(2));
    assert_eq!(at(3, matted("luma", Some(0))), Some(0));
    // Explicit self or out of range: None, and NO legacy i+1 fallback.
    assert_eq!(at(1, matted("alpha", Some(1))), None);
    assert_eq!(at(0, matted("alpha", Some(4))), None);
    assert_eq!(at(2, matted("alpha", Some(usize::MAX))), None);
    // Absent index: the legacy layer directly above, none past the last.
    assert_eq!(at(0, matted("alpha", None)), Some(1));
    assert_eq!(at(2, matted("lumaInverted", None)), Some(3));
    assert_eq!(at(3, matted("alpha", None)), None);
    // No recognized matte mode: no source even with an explicit index.
    assert_eq!(
        at(
            0,
            LayerComposition {
                matte_source_index: Some(1),
                ..layer()
            }
        ),
        None
    );
    assert_eq!(at(0, matted("none", Some(1))), None);
    assert_eq!(at(0, matted("Alpha", Some(1))), None);
    // An index outside the stack asks about nothing.
    assert_eq!(resolve_matte_source(&[layer()], 5), None);
}

#[test]
fn all_matte_resolution_cases() {
    // Extras ignored unless the first matte resolved.
    let more = Some(vec![extra(Some("luma"), Some(1))]);
    let dead_first = [
        vec![
            LayerComposition {
                mattes_more: more.clone(),
                ..matted("alpha", Some(0))
            },
            layer(),
        ],
        vec![
            LayerComposition {
                mattes_more: more.clone(),
                ..layer()
            },
            layer(),
        ],
        vec![
            layer(),
            LayerComposition {
                mattes_more: Some(vec![extra(Some("luma"), Some(0))]),
                ..matted("alpha", None)
            },
        ],
    ];
    assert!(resolve_all_mattes(&dead_first[0], 0).is_empty());
    assert!(resolve_all_mattes(&dead_first[1], 0).is_empty());
    assert!(resolve_all_mattes(&dead_first[2], 1).is_empty());
    // Dropped extras: self, out of range, bad mode, absent source, none mode.
    let stack = vec![
        layer(),
        LayerComposition {
            mattes_more: Some(vec![
                extra(Some("luma"), Some(1)),
                extra(Some("luma"), Some(9)),
                extra(Some("bogus"), Some(2)),
                extra(Some("alpha"), None),
                extra(None, Some(2)),
                extra(Some("none"), Some(2)),
                extra(Some("lumaInverted"), Some(2)),
            ]),
            ..matted("alpha", Some(0))
        },
        layer(),
    ];
    assert_eq!(
        resolve_all_mattes(&stack, 1),
        vec![step(0, 0, false), step(2, 1, true)]
    );
    // The first matte may itself use the legacy fallback; extras still apply
    // in order and may repeat a source.
    let legacy = vec![
        LayerComposition {
            mattes_more: Some(vec![
                extra(Some("alphaInverted"), Some(2)),
                extra(Some("luma"), Some(1)),
            ]),
            ..matted("luma", None)
        },
        layer(),
        layer(),
    ];
    assert_eq!(
        resolve_all_mattes(&legacy, 0),
        vec![step(1, 1, false), step(2, 0, true), step(1, 1, false)]
    );
    assert!(resolve_all_mattes(&legacy, 7).is_empty());
}

#[test]
fn consumption_sets_and_canvas_background() {
    let stack = vec![
        matted("alpha", None),
        layer(),
        LayerComposition {
            mattes_more: Some(vec![extra(Some("alpha"), Some(0))]),
            ..matted("luma", Some(3))
        },
        layer(),
    ];
    assert_eq!(matte_source_set(&stack), vec![true, true, false, true]);
    // A folder-shaped child stays top level; out-of-range and non-folder
    // child lists consume nothing.
    let folders = vec![
        folder(&[1, 2, 5]),
        folder(&[2]),
        layer(),
        LayerComposition {
            is_folder_layer: Some(false),
            folder_child_indices: Some(vec![0]),
            ..layer()
        },
    ];
    assert_eq!(folder_child_set(&folders), vec![false, false, true, false]);
    // First flagged layer wins; Some(false) and absent do not count.
    let bg = |flags: [Option<bool>; 4]| {
        let layers: Vec<LayerComposition> = flags
            .iter()
            .map(|f| LayerComposition {
                is_canvas_background: *f,
                ..layer()
            })
            .collect();
        background_index(&layers)
    };
    assert_eq!(bg([None, Some(false), Some(true), Some(true)]), Some(2));
    assert_eq!(bg([Some(true), None, Some(true), None]), Some(0));
    assert_eq!(bg([None, Some(false), None, Some(false)]), None);
}

#[test]
fn slow_path_predicate_names_each_reason() {
    let only = |l: LayerComposition| SlowPathReasons::of(&[l, layer()]);
    let none = SlowPathReasons::default();
    assert_eq!(only(layer()), none);
    assert!(!only(layer()).any());
    let blend = LayerComposition {
        blend_mode: Some("multiply".to_owned()),
        ..layer()
    };
    assert_eq!(
        only(blend),
        SlowPathReasons {
            blend: true,
            ..none
        }
    );
    assert_eq!(
        only(matted("alpha", None)),
        SlowPathReasons {
            matte: true,
            ..none
        }
    );
    assert_eq!(
        only(matted("alpha", Some(0))),
        none,
        "self matte consumes nothing"
    );
    let fx = LayerComposition {
        layer_effect_enabled: true,
        ..layer()
    };
    assert_eq!(
        only(fx),
        SlowPathReasons {
            effect_stack: true,
            ..none
        }
    );
    let item_fx = LayerComposition {
        item_effect_enabled: true,
        ..layer()
    };
    assert_eq!(
        only(item_fx),
        SlowPathReasons {
            effect_stack: true,
            ..none
        }
    );
    let effect_layer = LayerComposition {
        is_effect_layer: Some(true),
        ..layer()
    };
    assert_eq!(
        only(effect_layer),
        SlowPathReasons {
            effect_layer: true,
            ..none
        }
    );
    assert_eq!(
        only(LayerComposition {
            is_effect_layer: Some(false),
            ..layer()
        }),
        none
    );
    assert_eq!(
        only(LayerComposition {
            mask_count: 1,
            ..layer()
        }),
        SlowPathReasons { mask: true, ..none }
    );
    assert_eq!(
        only(LayerComposition {
            is_folder_layer: Some(true),
            ..layer()
        }),
        SlowPathReasons {
            folder: true,
            ..none
        }
    );
    assert!(only(LayerComposition {
        is_folder_layer: Some(true),
        ..layer()
    })
    .any());
}

#[test]
fn fast_path_plan_is_the_flat_authored_order() {
    for n in 0..6 {
        let plain = vec![layer(); n];
        assert_eq!(
            plan_composition(&plain),
            CompositionPlan::Flat {
                order: (0..n).collect()
            }
        );
    }
    // Background, "normal", unknown names, self/out-of-range mattes and an
    // unflagged folder child list force nothing: authored order, bg included.
    let stack = vec![
        LayerComposition {
            is_canvas_background: Some(true),
            ..layer()
        },
        LayerComposition {
            blend_mode: Some("normal".to_owned()),
            ..layer()
        },
        LayerComposition {
            blend_mode: Some("bogus".to_owned()),
            ..layer()
        },
        matted("alpha", Some(3)),
        matted("luma", Some(40)),
        LayerComposition {
            folder_child_indices: Some(vec![0]),
            ..layer()
        },
    ];
    assert_eq!(
        plan_composition(&stack),
        CompositionPlan::Flat {
            order: vec![0, 1, 2, 3, 4, 5]
        }
    );
}

#[test]
fn layered_plan_dispatch_order_literal() {
    let stack = vec![
        LayerComposition {
            is_canvas_background: Some(true),
            ..layer()
        },
        LayerComposition {
            blend_mode: Some("multiply".to_owned()),
            mask_count: 2,
            layer_effect_enabled: true,
            ..matted("luma", Some(5))
        },
        LayerComposition {
            is_effect_layer: Some(true),
            layer_effect_enabled: true,
            ..layer()
        },
        LayerComposition {
            item_effect_enabled: true,
            ..layer()
        },
        LayerComposition {
            blend_mode: Some("screen".to_owned()),
            ..folder(&[3, 9])
        },
        layer(),
    ];
    let child = FolderChildStep {
        index: 3,
        item_effects: true,
        layer_effects: false,
        blend: 0,
        seeds_accumulator: true,
    };
    assert_eq!(
        plan_composition(&stack),
        CompositionPlan::Layered {
            background: Some(0),
            steps: vec![
                LayerStep::Skipped {
                    index: 0,
                    reason: SkipReason::CanvasBackground
                },
                LayerStep::Paint(PaintStep {
                    index: 1,
                    source: PaintSource::Plain,
                    mask_count: 2,
                    mattes: vec![step(5, 1, false)],
                    layer_effects: true,
                    blend: 1,
                }),
                LayerStep::EffectLayer {
                    index: 2,
                    applies_stack: true
                },
                LayerStep::Skipped {
                    index: 3,
                    reason: SkipReason::FolderChild
                },
                LayerStep::Paint(PaintStep {
                    index: 4,
                    source: PaintSource::Folder(vec![child]),
                    mask_count: 0,
                    mattes: vec![],
                    layer_effects: false,
                    blend: 2,
                }),
                LayerStep::Skipped {
                    index: 5,
                    reason: SkipReason::MatteSource
                },
            ],
        }
    );
    // A layer that is matte source, folder child and background at once
    // reports the matte reason (the order the dispatch loop checks them).
    let both = vec![
        folder(&[1]),
        LayerComposition {
            is_canvas_background: Some(true),
            ..layer()
        },
        matted("alpha", Some(1)),
    ];
    match plan_composition(&both) {
        CompositionPlan::Layered { steps, .. } => {
            assert_eq!(
                steps[1],
                LayerStep::Skipped {
                    index: 1,
                    reason: SkipReason::MatteSource
                }
            )
        }
        other => panic!("expected layered, got {other:?}"),
    }
}
