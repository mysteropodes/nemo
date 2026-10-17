//! Controls for the montage layer join: each deliberate mistake, rebuilt from the SAME real adapters
//! (`ResolvedMontage`, the L10-A kernel) and run through the SAME oracle comparison as the shipped
//! functions, must be rejected by the rows that exercise it. The thresholds are what `tools/gen.mjs`
//! measured on the JS baseline with the equivalent source edit (`# mutant` header lines).
use super::rows::{
    effects_of, existing_of, layer_of, locate_row, place_row, rows, world, worlds, World, ORACLE,
};
use super::{MontageLayerError, MontagePlacement, PlaceOutcome, ResolvedMontage};
use crate::evaluation::object_montage_timing::{montage_source_frame, montage_total};

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

/// `getEffectiveStrokes`' montage branch rebuilt step by step, each step optionally dropped.
fn mutant_locate(w: &World, f: &[&str], wrap: bool, place: bool) -> String {
    let layer = layer_of(f[2], f[3]);
    let frame: i64 = f[4].parse().expect("frame");
    let montage =
        match ResolvedMontage::resolve(&w.storyboard, &w.components, f[2]).expect("resolve") {
            Some(montage) => montage,
            None => return "-".to_string(),
        };
    let members = montage.members().expect("members");
    let total = montage_total(&members).expect("total");
    if total == 0 {
        return "-".to_string();
    }
    let placed = if place {
        layer.placed_at().expect("placed").unwrap_or(0)
    } else {
        0
    };
    let mlf = frame - placed;
    if mlf < 0 {
        return "-".to_string();
    }
    let local = if wrap { mlf % total } else { mlf };
    match montage_source_frame(&members, local).expect("source") {
        Some(s) if w.components.component(&s.symbol_id).is_some() => {
            format!("{}@{}", s.symbol_id, s.src_frame)
        }
        _ => "-".to_string(),
    }
}

fn locate_mismatches(wrap: bool, place: bool) -> usize {
    let all = worlds();
    rows("F")
        .iter()
        .filter(|f| mutant_locate(world(&all, f[1]), f, wrap, place) != f[5])
        .count()
}

/// `placeMontageAsLayer` rebuilt from the same adapters with ONE step changed.
fn mutant_place(w: &World, f: &[&str], mutation: &str) -> String {
    let existing = existing_of(f[3]);
    let montage = ResolvedMontage::resolve(&w.storyboard, &w.components, f[2])
        .expect("resolve")
        .expect("montage");
    let members = montage.members().expect("members");
    let focus = existing.scene.iter().find(|l| {
        existing
            .placed
            .layers
            .iter()
            .any(|p| p.layer_uid == l.layer_uid() && p.montage_id == f[2])
    });
    let outcome: Result<PlaceOutcome, MontageLayerError> =
        if members.is_empty() && mutation != "noEmptyGuard" {
            Err(MontageLayerError::EmptyMontage)
        } else if let (Some(layer), true) = (focus, mutation != "noFocus") {
            Ok(PlaceOutcome::Focus {
                layer_uid: layer.layer_uid().to_string(),
                layer_name: layer.name().map(str::to_string),
            })
        } else {
            let layer_name = if mutation == "nameFromId" {
                f[2].to_string()
            } else {
                montage.name().expect("name").to_string()
            };
            Ok(PlaceOutcome::Create(MontagePlacement {
                montage_id: f[2].to_string(),
                layer_name,
                total_frames: montage_total(&members).expect("total"),
            }))
        };
    effects_of(&outcome, &existing)
}

fn place_mismatches(mutation: &str) -> usize {
    let all = worlds();
    rows("P")
        .iter()
        .filter(|f| mutant_place(world(&all, f[1]), f, mutation) != f[4])
        .count()
}

#[test]
fn the_unmutated_rebuilds_agree_with_the_oracle_and_with_the_shipped_functions() {
    assert_eq!(locate_mismatches(true, true), 0);
    assert_eq!(place_mismatches("none"), 0);
    let all = worlds();
    for f in rows("F") {
        let w = world(&all, f[1]);
        assert_eq!(mutant_locate(w, &f, true, true), locate_row(w, &f));
    }
    for f in rows("P") {
        let w = world(&all, f[1]);
        assert_eq!(mutant_place(w, &f, "none"), place_row(w, &f));
    }
}

#[test]
fn the_mutants_were_measured_on_the_baseline_and_are_listed() {
    let listed: Vec<&str> = ORACLE
        .lines()
        .filter_map(|l| l.strip_prefix("# mutant "))
        .map(|l| l.split(' ').next().expect("name"))
        .collect();
    assert_eq!(
        listed,
        [
            "placeNoEmptyGuard",
            "placeNoFocus",
            "placeNameFromId",
            "noWrap",
            "noPlacement"
        ]
    );
    assert_eq!(measured("placeNoEmptyGuard"), 16);
    assert_eq!(measured("placeNoFocus"), 11);
    assert_eq!(measured("placeNameFromId"), 33);
    assert_eq!(measured("noWrap"), 1383);
    assert_eq!(measured("noPlacement"), 556);
}

#[test]
fn dropping_the_loop_wrap_is_rejected() {
    assert!(locate_mismatches(false, true) >= measured("noWrap"));
}

#[test]
fn dropping_the_placement_offset_is_rejected() {
    assert!(locate_mismatches(true, false) >= measured("noPlacement"));
}

#[test]
fn placing_an_empty_montage_without_the_guard_is_rejected() {
    assert!(place_mismatches("noEmptyGuard") >= measured("placeNoEmptyGuard"));
}

#[test]
fn stacking_a_second_layer_instead_of_focusing_is_rejected() {
    assert!(place_mismatches("noFocus") >= measured("placeNoFocus"));
}

#[test]
fn naming_the_layer_after_the_id_instead_of_the_montage_is_rejected() {
    assert!(place_mismatches("nameFromId") >= measured("placeNameFromId"));
}

#[test]
fn the_oracle_covers_the_named_classes() {
    let f = rows("F");
    assert_eq!(f.iter().filter(|r| r[5] == "-").count(), 1082);
    assert_eq!(f.iter().filter(|r| r[5] != "-").count(), 2614);
    assert_eq!(f.iter().filter(|r| r[4].starts_with('-')).count(), 408);
    for placed in ["~", "n", "0", "4", "-3"] {
        assert!(f.iter().any(|r| r[3] == placed), "placed {placed}");
    }
    let p = rows("P");
    assert_eq!(
        p.iter()
            .filter(|r| r[4].contains("toastMontageEmptySnapHint"))
            .count(),
        16
    );
    assert_eq!(
        p.iter()
            .filter(|r| r[4].contains("toastMontageAlreadyPlacedLayer"))
            .count(),
        11
    );
    assert_eq!(
        p.iter()
            .filter(|r| r[4].contains("toastMontagePlacedAsLayerSuffix"))
            .count(),
        33
    );
    // A montage with a duplicate layer and one whose layer points at a deleted montage are present.
    assert!(p.iter().any(|r| r[3].contains("Dup MT")));
    assert!(p.iter().any(|r| r[3].contains("\"montageId\":\"gone\"")));
}
