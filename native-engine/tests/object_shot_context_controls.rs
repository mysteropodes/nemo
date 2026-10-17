//! Controls for the montage view context. Four consumer mistakes are replayed through the SAME oracle
//! comparison as the shipped kernel (`rows::Replay` with a `Deviation`) and must be rejected by the
//! rows that exercise them; the kernel's own steps (snapshot restore, swap, guards) are protected by
//! the baseline mutants `tools/gen.mjs` measured (`# mutant` header lines, one JS source edit each):
//! the oracle must discriminate every one of them, and the counts are pinned here.
use super::rows::{mismatches, rows, Deviation, ORACLE};

fn measured(name: &str) -> usize {
    let prefix = format!("# mutant {name} ");
    ORACLE
        .lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("header line for mutant {name}"))
        .trim()
        .parse()
        .expect("count")
}

#[test]
fn the_unchanged_replay_is_clean() {
    assert!(mismatches(Deviation::default()).is_empty());
}

#[test]
fn the_baseline_mutants_are_all_discriminated_and_pinned() {
    let listed: Vec<&str> = ORACLE
        .lines()
        .filter_map(|l| l.strip_prefix("# mutant "))
        .map(|l| l.split(' ').next().expect("name"))
        .collect();
    assert_eq!(
        listed,
        [
            "noCameraSwap",
            "noMarkersSwap",
            "noWriteBack",
            "noCameraWriteBack",
            "noRestoreFps",
            "noRestoreFrame",
            "noRestoreLayer",
            "noComponentGuard",
            "noSameViewNoop",
            "noClearSelExit"
        ]
    );
    let counts = [101, 120, 4, 3, 1, 42, 36, 24, 11, 48];
    for (name, count) in listed.iter().zip(counts) {
        assert_eq!(measured(name), count, "{name}");
        assert!(count > 0, "{name} must be discriminated");
    }
}

#[test]
fn a_consumer_that_drops_the_write_back_is_rejected() {
    let bad = mismatches(Deviation {
        ignore_write_back: true,
        ..Deviation::default()
    });
    assert!(bad.len() >= measured("noWriteBack"), "{}", bad.len());
}

#[test]
fn a_consumer_that_drops_the_camera_write_back_is_rejected() {
    let bad = mismatches(Deviation {
        drop_camera_write_back: true,
        ..Deviation::default()
    });
    assert!(bad.len() >= measured("noCameraWriteBack"), "{}", bad.len());
}

#[test]
fn a_consumer_that_keeps_the_selection_on_exit_is_rejected() {
    let bad = mismatches(Deviation {
        skip_exit_clear: true,
        ..Deviation::default()
    });
    assert!(bad.len() >= measured("noClearSelExit"), "{}", bad.len());
}

#[test]
fn a_consumer_that_forgets_the_open_component_is_rejected() {
    let bad = mismatches(Deviation {
        forget_component_flag: true,
        ..Deviation::default()
    });
    assert!(bad.len() >= measured("noComponentGuard"), "{}", bad.len());
}

#[test]
fn the_oracle_covers_the_named_classes() {
    let x = rows("X");
    let result = |prefix: &str| x.iter().filter(|r| r[5].starts_with(prefix)).count();
    assert_eq!(result("entered:"), 52);
    assert_eq!(result("exited"), 33);
    assert_eq!(result("toast:toastCloseComponentFirst"), 18);
    assert_eq!(result("toast:toastCloseMontageFirst"), 10);
    assert_eq!(result("toast:hsMontageEmpty"), 15);
    assert_eq!(result("noop"), 89);
    // Every montage-owned field and every restored scalar is moved inside a view at least once.
    for op in [
        "\"edit\",\"mk\"",
        "\"edit\",\"ma\"",
        "\"edit\",\"to\"",
        "\"edit\",\"te\"",
        "\"edit\",\"ck\"",
        "\"replace\",\"mk\"",
        "\"replace\",\"ck\"",
        "\"set\",\"cur\"",
        "\"set\",\"fps\"",
        "\"set\",\"tot\"",
        "\"set\",\"lay\"",
        "\"set\",\"wa\"",
        "[\"del\"]",
    ] {
        assert!(x.iter().any(|r| r[4].contains(op)), "no row with {op}");
    }
    // Stored fields: undefined, falsy and truthy variants all enter the oracle.
    let inits: Vec<&&str> = x.iter().filter(|r| r[3] == "0").map(|r| &r[7]).collect();
    assert!(inits.iter().any(|s| s.contains("mk=~")));
    assert!(inits
        .iter()
        .any(|s| s.contains("ck=null") && s.contains("ma=false")));
    assert!(inits.iter().any(|s| s.contains("mk=[\"s1\"]")));
}
