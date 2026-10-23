//! Admission and edge tests of the StoryBoard chain kernel (L04-F, I35): the
//! Components-only refusal, strict-versus-tolerant neutrality, the typed negative
//! frame and the JavaScript array semantics of the backfill. Hand-built inputs;
//! the oracle world comes from `object_storyboard_chain.rs`.

use super::tests::{chain_of, rows, tags, world};
use super::{
    layer_source_frame, ComponentSymbol, ModuleKind, MontageSelection, StoryModule,
    StoryboardChain, StoryboardChainError, SymbolFrame, SymbolLayer, SymbolTable,
};

fn module(id: &str, kind: ModuleKind, symbol_id: &str) -> StoryModule {
    StoryModule {
        id: id.to_string(),
        kind,
        symbol_id: symbol_id.to_string(),
        trim_in: None,
        trim_out: None,
        duration: None,
    }
}

fn one_component() -> SymbolTable {
    let mut table = SymbolTable::new();
    table.insert(
        "comp".to_string(),
        ComponentSymbol {
            name: None,
            total_frames: Some(10),
            layers: vec![SymbolLayer {
                frames: vec![
                    Some(SymbolFrame {
                        is_keyframe: true,
                        is_interpolated: false
                    });
                    10
                ],
            }],
        },
    );
    table
}

#[test]
fn components_only_refuses_a_flat_layer_id_and_creates_nothing() {
    let symbols = one_component();
    let modules = vec![
        module("a", ModuleKind::Instance, "comp"),
        module("b", ModuleKind::Instance, "layer-7"),
    ];
    let before = (symbols.clone(), modules.clone());
    let refusal = StoryboardChain::components_only(&modules, &symbols).unwrap_err();
    assert_eq!(
        refusal,
        StoryboardChainError::NotAComponent {
            module_id: "b".to_string(),
            symbol_id: "layer-7".to_string()
        }
    );
    // no Component was made for the flat layer, no module was rewritten.
    assert_eq!((symbols.clone(), modules.clone()), before);
    // the tolerant baseline keeps its 24-frame, empty-looking fallback for the same input.
    let chain = StoryboardChain::baseline(&modules, &symbols);
    let ids = vec!["b".to_string()];
    assert_eq!(chain.total(&ids).unwrap(), 24);
    assert_eq!(
        chain.selection_at(&ids, 0).unwrap(),
        Some(MontageSelection {
            symbol_id: "layer-7".to_string(),
            src_frame: 0,
            layers: Vec::new()
        })
    );
}

#[test]
fn components_only_checks_instances_only_and_reports_the_first_in_module_order() {
    let symbols = one_component();
    let modules = vec![
        module("snd", ModuleKind::Sound, "not-a-component"),
        module("i1", ModuleKind::Instance, "comp"),
        module("i2", ModuleKind::Instance, "x1"),
        module("i3", ModuleKind::Instance, "x2"),
    ];
    match StoryboardChain::components_only(&modules, &symbols) {
        Err(StoryboardChainError::NotAComponent {
            module_id,
            symbol_id,
        }) => assert_eq!((module_id.as_str(), symbol_id.as_str()), ("i2", "x1")),
        other => panic!("expected the first unresolved instance, got {other:?}"),
    }
    let others = vec![
        module("snd", ModuleKind::Sound, "not-a-component"),
        module("mt", ModuleKind::Montage, "-"),
    ];
    assert!(StoryboardChain::components_only(&others, &symbols).is_ok());
}

#[test]
fn components_only_selects_what_the_oracle_shows_for_an_admitted_world() {
    let w = world();
    // the fixed world holds instances of the missing symbol "ghost": strict admission refuses the first.
    match StoryboardChain::components_only(&w.modules, &w.symbols) {
        Err(StoryboardChainError::NotAComponent {
            module_id,
            symbol_id,
        }) => assert_eq!(
            (module_id.as_str(), symbol_id.as_str()),
            ("i_ghost", "ghost")
        ),
        other => panic!("expected i_ghost to be refused, got {other:?}"),
    }
    let clean: Vec<StoryModule> = w
        .modules
        .iter()
        .filter(|m| m.kind != ModuleKind::Instance || w.symbols.contains_key(&m.symbol_id))
        .cloned()
        .collect();
    let strict = StoryboardChain::components_only(&clean, &w.symbols).unwrap();
    // chains that name a refused module would differ by construction; every other chain must
    // select exactly what the baseline oracle's `M` rows say, through the strict constructor.
    let removed: Vec<&str> = w
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::Instance && !w.symbols.contains_key(&m.symbol_id))
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(removed, ["i_ghost", "r14"]);
    let mut compared = 0;
    for f in rows("M") {
        let ids = &w.chains[f[1]];
        if ids.iter().any(|id| removed.contains(&id.as_str())) {
            continue;
        }
        let frame: i64 = f[2].parse().unwrap();
        let got = strict.selection_at(ids, frame).unwrap();
        assert_eq!(tags(&got), f[3], "M {} {}", f[1], f[2]);
        compared += 1;
    }
    assert_eq!(compared, 1629);
}

#[test]
fn a_negative_montage_frame_is_the_typed_error_of_the_timing_kernel() {
    use crate::evaluation::object_montage_timing::MontageTimingError;
    let w = world();
    let sc = chain_of(&w);
    assert_eq!(
        sc.selection_at(&w.chains["c_walk"], -1),
        Err(MontageTimingError::NegativeFrame)
    );
}

#[test]
fn layer_source_frame_reads_javascript_array_semantics() {
    let k = Some(SymbolFrame {
        is_keyframe: true,
        is_interpolated: false,
    });
    let plain = Some(SymbolFrame {
        is_keyframe: false,
        is_interpolated: false,
    });
    let frames = [k, plain, None, plain];
    assert_eq!(layer_source_frame(&frames, 3), Some(0));
    assert_eq!(layer_source_frame(&frames, 99), Some(0));
    assert_eq!(layer_source_frame(&frames, 0), Some(0));
    assert_eq!(layer_source_frame(&frames, -1), None);
    assert_eq!(layer_source_frame(&[], 0), None);
    assert_eq!(layer_source_frame(&[None, plain], 1), None);
}
