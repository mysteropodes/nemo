//! Oracle tests for the duplicator evaluation join (L04-L, I08/I09).
//!
//! `fixtures/object-duplicator-evaluate/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-L leaf, which runs the verbatim baseline
//! `getEffectiveStrokesRendered` / `applyLayerDuplicator` (and everything they
//! call, plus the whole of `effector-layer.js`) in a Node `vm` over Paper.js
//! 0.12.18, with only the surroundings stubbed (the Motion lookups, the content
//! of layers, the layer in and out points). Each scenario records the inputs,
//! every port call the baseline made, the placement descriptor of every clone
//! and the strokes returned.
//!
//! Every scenario runs through the real join over a real `DuplicatorSet`,
//! real `ObjectLayer`s and a provider that answers only what the baseline was
//! asked. The comparison covers the port calls (what was asked, and in what
//! order), the placement of every clone, and every copy (source identity,
//! per-copy identity, geometry, fill, `dup3D` and the typed gaps).

use super::test_codec::{all, Out, Scenario};
use super::test_compare::{mismatch, run};

fn prefix_count(all: &[Scenario], prefix: &str) -> usize {
    all.iter().filter(|s| s.name.starts_with(prefix)).count()
}

#[test]
fn every_scenario_matches_the_baseline() {
    let scenarios = all();
    let failures: Vec<String> = scenarios
        .iter()
        .filter_map(|sc| mismatch(sc, &run(sc)))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ, first: {:?}",
        failures.len(),
        scenarios.len(),
        failures.iter().take(5).collect::<Vec<_>>()
    );
}

#[test]
fn the_oracle_covers_the_families() {
    let scenarios = all();
    assert_eq!(scenarios.len(), 189);
    let expected = [
        ("grid_", 43),
        ("radial_", 17),
        ("path_", 19),
        ("eff_", 22),
        ("tmp_", 30),
        ("direct_", 6),
        ("el_", 24),
        ("pool_", 19),
        ("guard_", 5),
        ("corpus_", 3),
        ("other_", 1),
    ];
    for (prefix, n) in expected {
        assert_eq!(prefix_count(&scenarios, prefix), n, "{prefix}");
    }
    let sum: usize = expected.iter().map(|(_, n)| n).sum();
    assert_eq!(sum, scenarios.len());
}

#[test]
fn both_entry_shapes_and_every_outcome_are_exercised() {
    let scenarios = all();
    let rendered = scenarios.iter().filter(|s| s.entry == "rendered").count();
    let direct = scenarios.iter().filter(|s| s.entry == "direct").count();
    assert_eq!((rendered, direct), (182, 7));
    let same = scenarios
        .iter()
        .filter(|s| matches!(s.out, Some(Out::Same)))
        .count();
    let throws = scenarios
        .iter()
        .filter(|s| matches!(s.out, Some(Out::Throws)))
        .count();
    let big = scenarios
        .iter()
        .filter(|s| matches!(s.out, Some(Out::Big { .. })))
        .count();
    assert_eq!((same, throws, big), (21, 1, 3));
    let lists = scenarios
        .iter()
        .filter(|s| matches!(s.out, Some(Out::List(_))))
        .count();
    assert_eq!(same + throws + big + lists, 189);
}

#[test]
fn the_scenarios_name_every_mode_and_every_random_channel() {
    let scenarios = all();
    let has = |needle: &str| {
        scenarios.iter().any(|s| {
            s.dupj
                .as_ref()
                .is_some_and(|d| d.to_string().contains(needle))
        })
    };
    for needle in [
        "\"mode\":\"grid\"",
        "\"mode\":\"radial\"",
        "\"mode\":\"path\"",
        "\"sourceMode\":\"objects\"",
        "\"falloff\":\"step\"",
        "\"falloff\":\"linear\"",
        "\"direction\":\"centerOut\"",
        "\"direction\":\"random\"",
        "\"direction\":\"backward\"",
        "\"offsetPos\"",
        "\"endAngle\":null",
        "\"pathAlignTangent\":true",
        "\"radialOrient\":true",
        "\"hue\":true",
    ] {
        assert!(has(needle), "no scenario carries {needle}");
    }
}
