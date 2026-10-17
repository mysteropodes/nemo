//! Mutation controls for the import id remap kernel (L10-G). An independent model of each rule is
//! written here; the unmutated model must agree with production on a fixed scenario set and every
//! mutation must disagree somewhere, so the scenarios and oracle rows are known to discriminate.
use super::rows::{parse, ORACLE};
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Alloc {
    Reference,
    NoRevisionBump,
    IgnoreExisting,
    SuffixFromLen,
    NoBatchMemory,
}
const ALLOC_MUTANTS: [Alloc; 4] = [
    Alloc::NoRevisionBump,
    Alloc::IgnoreExisting,
    Alloc::SuffixFromLen,
    Alloc::NoBatchMemory,
];

fn model_alloc(rev: u64, existing: &[&str], tag: &str, count: usize, m: Alloc) -> Vec<String> {
    let next = if m == Alloc::NoRevisionBump {
        rev
    } else {
        rev + 1
    };
    let mut taken: HashSet<String> = existing.iter().map(|s| (*s).to_owned()).collect();
    let mut out = Vec::new();
    for i in 0..count {
        let id = match m {
            Alloc::IgnoreExisting => format!("native-{tag}:{next}:{i}"),
            Alloc::SuffixFromLen => format!("native-{tag}:{next}:{}", taken.len()),
            _ => (0..=taken.len())
                .map(|s| format!("native-{tag}:{next}:{s}"))
                .find(|id| !taken.contains(id))
                .unwrap(),
        };
        if m != Alloc::NoBatchMemory {
            taken.insert(id.clone());
        }
        out.push(id);
    }
    out
}

type AllocScenario = (u64, Vec<&'static str>, usize);
fn alloc_scenarios() -> Vec<AllocScenario> {
    vec![
        (0, vec![], 2),
        (0, vec!["native-layer:1:0"], 1),
        (
            0,
            vec!["native-layer:1:0", "native-layer:1:1", "native-layer:1:3"],
            3,
        ),
    ]
}
fn production_alloc(s: &AllocScenario) -> Vec<String> {
    let origin = OriginIds::new(s.0).with_ids(RemapKind::Layer, s.1.iter().copied());
    allocate_ids(&origin, RemapKind::Layer, s.2).unwrap()
}

#[test]
fn the_unmutated_allocation_model_agrees_with_production() {
    for s in alloc_scenarios() {
        assert_eq!(
            model_alloc(s.0, &s.1, "layer", s.2, Alloc::Reference),
            production_alloc(&s)
        );
    }
}

#[test]
fn every_allocation_mutation_is_rejected_by_some_scenario() {
    for m in ALLOC_MUTANTS {
        let caught = alloc_scenarios()
            .iter()
            .any(|s| model_alloc(s.0, &s.1, "layer", s.2, m) != production_alloc(s));
        assert!(caught, "{m:?} survived");
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Res {
    Reference,
    OutsideAsDangling,
    DanglingAsOutside,
    SkippedAsOutside,
    BatchNotRewritten,
    OutsideTrimmed,
}
const RES_MUTANTS: [Res; 5] = [
    Res::OutsideAsDangling,
    Res::DanglingAsOutside,
    Res::SkippedAsOutside,
    Res::BatchNotRewritten,
    Res::OutsideTrimmed,
];

/// Batch: `a` -> native-layer:1:0, `s` skipped; origin: `o` and `p ` (trailing space).
fn model_resolve(r: &str, m: Res) -> Result<String, &'static str> {
    match r {
        "a" if m == Res::BatchNotRewritten => Ok(r.to_owned()),
        "a" => Ok("native-layer:1:0".to_owned()),
        "s" if m == Res::SkippedAsOutside => Ok(r.to_owned()),
        "s" => Err("skipped"),
        "o" | "p " if m == Res::OutsideAsDangling => Err("dangling"),
        "o" | "p " if m == Res::OutsideTrimmed => Ok(r.trim().to_owned()),
        "o" | "p " => Ok(r.to_owned()),
        _ if m == Res::DanglingAsOutside => Ok(r.to_owned()),
        _ => Err("dangling"),
    }
}
fn production_resolve(r: &str) -> Result<String, &'static str> {
    let origin = OriginIds::new(0).with_ids(RemapKind::Layer, ["o", "p "]);
    let batch = [
        SourceEntry::mapped(RemapKind::Layer, "a"),
        SourceEntry::skipped(RemapKind::Layer, "s", "unsupported"),
    ];
    let table = build_remap_table(&origin, &batch).unwrap();
    match table.resolve(&origin, RemapKind::Layer, r) {
        Ok(found) => Ok(found.as_str().to_owned()),
        Err(RemapError::ReferenceToSkipped { .. }) => Err("skipped"),
        Err(RemapError::DanglingReference { .. }) => Err("dangling"),
        Err(other) => panic!("{other:?}"),
    }
}
const REFS: [&str; 6] = ["a", "o", "p ", "s", "zzz", ""];

#[test]
fn the_unmutated_resolution_model_agrees_with_production() {
    for r in REFS {
        assert_eq!(
            model_resolve(r, Res::Reference),
            production_resolve(r),
            "{r:?}"
        );
    }
}

#[test]
fn every_resolution_mutation_is_rejected_by_some_reference() {
    for m in RES_MUTANTS {
        let caught = REFS
            .iter()
            .any(|r| model_resolve(r, m) != production_resolve(r));
        assert!(caught, "{m:?} survived");
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Build {
    Reference,
    SkipConsumes,
    SharedCounter,
}

/// Entries: (kind, id, skipped). Returns the native id of each mapped entry, None for skipped ones.
fn model_build(entries: &[(RemapKind, &str, bool)], m: Build) -> Vec<Option<String>> {
    let mut counters: HashMap<&str, usize> = HashMap::new();
    entries
        .iter()
        .map(|(kind, _, skipped)| {
            let key = if m == Build::SharedCounter {
                "all"
            } else {
                kind.tag()
            };
            let slot = counters.entry(key).or_insert(0);
            if *skipped && m != Build::SkipConsumes {
                return None;
            }
            let id = format!("native-{}:1:{}", kind.tag(), *slot);
            *slot += 1;
            if *skipped {
                return None;
            }
            Some(id)
        })
        .collect()
}
fn production_build(entries: &[(RemapKind, &str, bool)]) -> Vec<Option<String>> {
    let batch: Vec<SourceEntry> = entries
        .iter()
        .map(|(kind, id, skipped)| {
            if *skipped {
                SourceEntry::skipped(*kind, id, "unsupported")
            } else {
                SourceEntry::mapped(*kind, id)
            }
        })
        .collect();
    let table = build_remap_table(&OriginIds::new(0), &batch).unwrap();
    entries
        .iter()
        .map(|(kind, id, _)| table.native_id(*kind, id).map(str::to_owned))
        .collect()
}
fn build_scenario() -> Vec<(RemapKind, &'static str, bool)> {
    vec![
        (RemapKind::Layer, "a", false),
        (RemapKind::Layer, "x", true),
        (RemapKind::Stroke, "s", false),
        (RemapKind::Layer, "b", false),
    ]
}

#[test]
fn the_unmutated_build_model_agrees_and_each_mutation_is_rejected() {
    let scenario = build_scenario();
    let want = production_build(&scenario);
    assert_eq!(
        want,
        [
            Some("native-layer:1:0".to_owned()),
            None,
            Some("native-stroke:1:0".to_owned()),
            Some("native-layer:1:1".to_owned())
        ]
    );
    assert_eq!(model_build(&scenario, Build::Reference), want);
    for m in [Build::SkipConsumes, Build::SharedCounter] {
        assert_ne!(model_build(&scenario, m), want, "{m:?} survived");
    }
}

#[test]
fn id_validity_mutations_are_rejected() {
    let long = "x".repeat(256);
    let edge = "x".repeat(255);
    let probes: [(&str, bool); 5] = [
        ("", false),
        (" ", false),
        (edge.as_str(), true),
        (long.as_str(), false),
        ("a\0", false),
    ];
    let mutants: [fn(&str) -> bool; 4] = [
        |v| v.len() <= 256 && !v.trim().is_empty() && !v.contains('\0'),
        |v| v.len() <= 255 && !v.contains('\0'),
        |v| v.len() <= 255 && !v.trim().is_empty(),
        |v| !v.is_empty() && v.len() <= 255 && !v.contains('\0'),
    ];
    for (value, valid) in probes {
        assert_eq!(valid_id(value), valid);
    }
    for (i, mutant) in mutants.iter().enumerate() {
        let caught = probes.iter().any(|(value, valid)| mutant(value) != *valid);
        assert!(caught, "validity mutant {i} survived");
    }
}

#[test]
fn name_mutations_disagree_with_the_baseline_rows() {
    let o = parse(ORACLE);
    let figma: [fn(Option<&str>, usize) -> String; 3] = [
        |n, fi| match n {
            Some(v) if !v.is_empty() => v.to_owned(),
            _ => format!("Figma {fi}"),
        },
        |n, fi| match n {
            Some(v) if !v.trim().is_empty() => v.to_owned(),
            _ => format!("Figma {}", fi + 1),
        },
        |n, fi| match n {
            Some(v) if v != "0" && !v.is_empty() => v.to_owned(),
            _ => format!("Figma {}", fi + 1),
        },
    ];
    for (i, f) in figma.iter().enumerate() {
        let caught = o
            .figma
            .iter()
            .any(|(n, fi, want)| &f(n.as_deref(), *fi) != want);
        assert!(caught, "figma mutant {i} survived");
    }
    let psd: [fn(Option<&str>, usize) -> String; 2] = [
        |n, k| match n {
            Some(v) if !v.is_empty() => v.to_owned(),
            _ => format!("Calque PSD {k}"),
        },
        |n, k| match n {
            Some(v) if v != "0" && !v.is_empty() => v.to_owned(),
            _ => format!("Calque PSD {}", k + 1),
        },
    ];
    for (i, f) in psd.iter().enumerate() {
        let caught = o
            .psd
            .iter()
            .any(|(n, k, want)| &f(n.as_deref(), *k) != want);
        assert!(caught, "psd mutant {i} survived");
    }
    let next: [fn(&[&str]) -> String; 2] = [
        |names| format!("Layer {}", names.len() + 1),
        |names| {
            let mut n = 1;
            while names
                .iter()
                .any(|v| v.eq_ignore_ascii_case(&format!("Layer {n}")))
            {
                n += 1;
            }
            format!("Layer {n}")
        },
    ];
    for (i, f) in next.iter().enumerate() {
        let caught = o.layer_names.iter().any(|(names, want)| {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            &f(&refs) != want
        });
        assert!(caught, "layer name mutant {i} survived");
    }
}
