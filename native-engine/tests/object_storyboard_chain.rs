//! Oracle tests for the StoryBoard chain selection kernel (L04-F, I35).
//!
//! `fixtures/object-storyboard-chain/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_oracle.cjs` of the L04-F leaf, which cuts `moduleById`,
//! `symbolDuration`, `ensureRetime`, `chainMods`, `montageTotal`,
//! `montageStrokesAt` and `symbolStrokesAt` out of `src/js/storyboard.js`, and
//! `resolveSymbolFrameIdx` plus the `if(ld.montageId){...}` block of
//! `getEffectiveStrokes` out of `src/js/app.js`, at baseline `fbe4b8a3`, and
//! runs them verbatim in a Node `vm` over a fixed world (19 symbols, 51 modules,
//! 61 chains). Every stroke is tagged `<symbolId>:L<layer>F<frame>`, so a row
//! names exactly which layer frame the baseline showed. The fixture header
//! carries the baseline blob hashes and the row grammar.
//!
//! Rows are space separated; `~` is an empty result, `-` undefined. `Y` symbol,
//! `G` module and `C` chain rows describe the world; `S` (symbolStrokesAt), `D`
//! (symbolDuration), `T` (montageTotal), `M` (montageStrokesAt), `L` (montage
//! layer path) and `X` (formula divergence grid) are results.

use super::{
    symbol_view, ComponentSymbol, LayerSource, ModuleKind, MontageSelection, StoryModule,
    StoryboardChain, SymbolFrame, SymbolLayer, SymbolTable,
};
use crate::evaluation::object_montage_timing::{symbol_duration, Retime};
use std::collections::BTreeMap;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-storyboard-chain/oracle.txt");

pub(crate) struct World {
    pub(crate) symbols: SymbolTable,
    pub(crate) modules: Vec<StoryModule>,
    pub(crate) chains: BTreeMap<String, Vec<String>>,
}

pub(crate) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(' ').collect::<Vec<&'static str>>())
        .filter(|f| f[0] == tag)
        .collect()
}

pub(crate) fn opt(tok: &str) -> Option<i64> {
    if tok == "-" {
        None
    } else {
        Some(
            tok.parse::<i64>()
                .unwrap_or_else(|_| panic!("number {tok}")),
        )
    }
}

/// One frame code of a `Y` row: `K` key, `I` interpolated, `B` both, `.` neither, `n` null.
fn frame(c: char) -> Option<SymbolFrame> {
    let (is_keyframe, is_interpolated) = match c {
        'K' => (true, false),
        'I' => (false, true),
        'B' => (true, true),
        '.' => (false, false),
        'n' => return None,
        other => panic!("frame code {other}"),
    };
    Some(SymbolFrame {
        is_keyframe,
        is_interpolated,
    })
}

fn layer(spec: &str) -> SymbolLayer {
    if spec == "_" {
        SymbolLayer::default()
    } else {
        SymbolLayer {
            frames: spec.chars().map(frame).collect(),
        }
    }
}

pub(crate) fn world() -> World {
    let mut symbols = SymbolTable::new();
    for f in rows("Y") {
        let layers = if f[3] == "=" {
            Vec::new()
        } else {
            f[3].split('|').map(layer).collect()
        };
        symbols.insert(
            f[1].to_string(),
            ComponentSymbol {
                name: None,
                total_frames: opt(f[2]),
                layers,
            },
        );
    }
    let modules = rows("G")
        .iter()
        .map(|f| StoryModule {
            id: f[1].to_string(),
            kind: ModuleKind::from_label(f[2]),
            symbol_id: if f[3] == "-" {
                String::new()
            } else {
                f[3].to_string()
            },
            trim_in: opt(f[4]),
            trim_out: opt(f[5]),
            duration: opt(f[6]),
        })
        .collect();
    let mut chains = BTreeMap::new();
    for f in rows("C") {
        let ids = if f[2] == "-" || f[2] == "!" {
            Vec::new()
        } else {
            f[2].split(',').map(String::from).collect()
        };
        chains.insert(f[1].to_string(), ids);
    }
    World {
        symbols,
        modules,
        chains,
    }
}

pub(crate) fn chain_of(w: &World) -> StoryboardChain<'_> {
    StoryboardChain::baseline(&w.modules, &w.symbols)
}

pub(crate) fn tag_list(symbol_id: &str, layers: &[LayerSource]) -> String {
    if layers.is_empty() {
        return "~".to_string();
    }
    layers
        .iter()
        .map(|l| format!("{symbol_id}:L{}F{}", l.layer_index, l.frame_index))
        .collect::<Vec<String>>()
        .join(",")
}

pub(crate) fn tags(selection: &Option<MontageSelection>) -> String {
    match selection {
        None => "~".to_string(),
        Some(s) => tag_list(&s.symbol_id, &s.layers),
    }
}

/// Evaluate the argument part of an `S`/`T`/`M`/`L` row (everything before the result).
pub(crate) fn eval(w: &World, prefix: &str) -> String {
    let sc = chain_of(w);
    let f: Vec<&str> = prefix.split(' ').collect();
    match f[0] {
        "S" => tag_list(f[1], &sc.symbol_selection(f[1], f[2].parse().unwrap())),
        "T" => sc.total(&w.chains[f[1]]).unwrap().to_string(),
        "M" => tags(
            &sc.selection_at(&w.chains[f[1]], f[2].parse().unwrap())
                .unwrap(),
        ),
        "L" => tags(
            &sc.layer_selection_at(&w.chains[f[1]], f[2].parse().unwrap(), opt(f[3]))
                .unwrap(),
        ),
        other => panic!("row kind {other}"),
    }
}

/// Row counts of the fixed fixture: a truncated or regenerated-differently oracle fails here.
#[test]
fn fixture_world_and_row_counts_are_pinned() {
    let counts = [
        ("Y", 19),
        ("G", 51),
        ("C", 61),
        ("S", 280),
        ("D", 20),
        ("T", 61),
        ("M", 1835),
        ("L", 2022),
        ("X", 18),
    ];
    for (tag, n) in counts {
        assert_eq!(rows(tag).len(), n, "row count of {tag}");
    }
    let w = world();
    assert_eq!(w.symbols.len(), 19);
    assert_eq!(w.modules.len(), 51);
    assert_eq!(w.chains.len(), 61);
}

#[test]
fn symbol_selection_rows_match_baseline() {
    let w = world();
    let sc = chain_of(&w);
    for f in rows("S") {
        let fi: i64 = f[2].parse().unwrap();
        assert_eq!(
            tag_list(f[1], &sc.symbol_selection(f[1], fi)),
            f[3],
            "S {} {}",
            f[1],
            f[2]
        );
    }
}

#[test]
fn symbol_duration_rows_match_baseline() {
    let w = world();
    for f in rows("D") {
        let got = symbol_duration(&symbol_view(w.symbols.get(f[1])));
        assert_eq!(got.to_string(), f[2], "D {}", f[1]);
    }
}

#[test]
fn montage_total_rows_match_baseline() {
    let w = world();
    let sc = chain_of(&w);
    for f in rows("T") {
        let got = sc.total(&w.chains[f[1]]).unwrap();
        assert_eq!(got.to_string(), f[2], "T {}", f[1]);
    }
}

#[test]
fn montage_selection_rows_match_baseline() {
    let w = world();
    let sc = chain_of(&w);
    for f in rows("M") {
        let frame: i64 = f[2].parse().unwrap();
        let got = sc.selection_at(&w.chains[f[1]], frame).unwrap();
        assert_eq!(tags(&got), f[3], "M {} {}", f[1], f[2]);
    }
}

#[test]
fn montage_layer_rows_match_baseline() {
    let w = world();
    let sc = chain_of(&w);
    for f in rows("L") {
        let frame: i64 = f[2].parse().unwrap();
        let got = sc
            .layer_selection_at(&w.chains[f[1]], frame, opt(f[3]))
            .unwrap();
        assert_eq!(tags(&got), f[4], "L {} {} {}", f[1], f[2], f[3]);
    }
}

/// Cases whose literal results come from the baseline functions (the same oracle
/// rows). Each is the case a named mutant in the controls must get wrong.
pub(crate) static PINNED: [(&str, &str); 28] = [
    // symbolStrokesAt: plain frames show the nearest preceding KEYFRAME; `B` counts as one.
    ("S s_a 9", "s_a:L0F6,s_a:L1F7"),
    // an interpolated frame shows itself; the second layer clamps to its last frame.
    ("S s_a 18", "s_a:L0F18,s_a:L1F7"),
    // an interpolated frame is not a backfill source (12, not 18).
    ("S s_a 19", "s_a:L0F12,s_a:L1F7"),
    ("S s_a 7", "s_a:L0F6,s_a:L1F7"),
    ("S s_b 7", "s_b:L0F0"),
    ("S s_g 5", "s_g:L0F4,s_g:L1F4,s_g:L2F4"),
    ("S s_b 13", "s_b:L0F9"),
    ("S s_a -1", "~"),
    ("S s_d 3", "~"),
    ("S s_e 1", "s_e:L1F0"),
    ("S s_f 2", "s_f:L1F2"),
    ("S s_f 3", "s_f:L0F3,s_f:L1F2"),
    ("S ghost 0", "~"),
    // montage totals: dangling ids and non-instance modules skipped, first duplicate id wins.
    ("T c_walk", "24"),
    ("T c_dangling", "24"),
    ("T c_non_inst", "36"),
    ("T c_ghost_mix", "60"),
    ("T c_zero_out", "13"),
    ("T c_zero_dur", "24"),
    ("T c_dup", "28"),
    // montageStrokesAt: stretch, squeeze, trim, explicit zero trimOut, member boundary.
    ("M c_slow 1", "s_a:L0F0,s_a:L1F0"),
    ("M c_slow 3", "s_a:L0F0,s_a:L1F1"),
    ("M c_fast 7", "s_a:L0F12,s_a:L1F7"),
    ("M c_trim 0", "s_a:L0F6,s_a:L1F4"),
    ("M c_zero_out 1", "s_a:L0F6,s_a:L1F4"),
    ("M c_non_inst 24", "s_a:L0F6,s_a:L1F4"),
    ("L c_walk 28 3", "s_a:L0F0,s_a:L1F1"),
    ("L c_walk 2 3", "~"),
];

#[test]
fn pinned_literals_match_baseline_and_the_oracle() {
    let w = world();
    for (prefix, expected) in PINNED {
        assert_eq!(eval(&w, prefix), expected, "{prefix}");
        let line = format!("{prefix} {expected}");
        assert!(
            ORACLE.lines().any(|l| l == line),
            "oracle row missing: {line}"
        );
    }
}

#[test]
fn dangling_ids_other_kinds_and_duplicate_ids_follow_the_baseline_lookup() {
    let w = world();
    let sc = chain_of(&w);
    // c_dangling = [gone1, i_walk, gone2]: only the real instance counts.
    assert_eq!(sc.members(&w.chains["c_dangling"]).unwrap().len(), 1);
    // c_non_inst = [snd1, i_walk, mont1, weird1, i_trim]: sound, montage and unknown kinds skipped.
    let kept = sc.members(&w.chains["c_non_inst"]).unwrap();
    assert_eq!(
        kept.iter()
            .map(|m| m.module_id.as_str())
            .collect::<Vec<&str>>(),
        ["i_walk", "i_trim"]
    );
    // two modules share the id "dup": the FIRST one (s_a, 0..3, 4 frames) answers.
    let dup = sc.members(&w.chains["c_dup"]).unwrap();
    assert_eq!(dup[0].symbol_id, "s_a");
    assert_eq!(
        dup[0].retime,
        Retime {
            trim_in: 0,
            trim_out: 3,
            duration: 4
        }
    );
}

#[test]
fn explicit_zero_retime_fields_are_kept_and_absent_ones_default() {
    let w = world();
    let sc = chain_of(&w);
    // trimOut 0 is a real one-frame source; its duration defaults to 1.
    let zero_out = sc.members(&w.chains["c_zero_out"]).unwrap();
    assert_eq!(
        zero_out[0].retime,
        Retime {
            trim_in: 0,
            trim_out: 0,
            duration: 1
        }
    );
    // duration 0 is a real zero-length member: present, but it occupies no frame.
    let zero_dur = sc.members(&w.chains["c_zero_dur"]).unwrap();
    assert_eq!(zero_dur.len(), 2);
    assert_eq!(zero_dur[0].retime.duration, 0);
    assert_eq!(sc.total(&w.chains["c_zero_dur"]).unwrap(), 24);
    // nothing given: trimIn 0, trimOut symbolDuration - 1, duration the span (s_a lasts 24).
    let walk = sc.members(&w.chains["c_walk"]).unwrap();
    assert_eq!(
        walk[0].retime,
        Retime {
            trim_in: 0,
            trim_out: 23,
            duration: 24
        }
    );
}
