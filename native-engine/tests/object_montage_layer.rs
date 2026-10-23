//! Tests for the montage layer join (L10-I, I18/I19/I20/I36/I42). Every expectation is either a row
//! of `fixtures/object-montage-layer/oracle.txt` (verbatim baseline functions, see `rows`) or a
//! literal read from it. The records under test are the real L04-G `StoryboardRecord` and
//! `ComponentSet`; the component CONTENT provider is a port (lane 4 evaluation join).
use super::rows::{
    existing_of, layer_of, locate_row, place_row, rows, total_of, world, worlds, World, ORACLE,
};
use super::{
    locate_montage_frame, plan_place_montage, resolve_montage_layer_frame, Located, MontageEmpty,
    MontageFrame, MontageLayerError, ProviderError, SymbolFramePort, UnavailableSymbolFrames,
};
use crate::evaluation::object_montage_timing::SourceFrame;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecordError;
use std::cell::RefCell;

#[path = "object_montage_layer_records.rs"]
mod records;

fn located(all: &[World], world_id: &str, montage: &str, placed: &str, frame: i64) -> Located {
    let w = world(all, world_id);
    locate_montage_frame(
        &layer_of(montage, placed),
        &w.storyboard,
        &w.components,
        frame,
    )
    .expect("admissible")
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains(
        "storyboard.js 966d09d73eba3bfe2eadaa762df338b495ee52898f7ff328e23630c97ce9bf97"
    ));
    assert_eq!(rows("W").len(), 9);
    assert_eq!(rows("T").len(), 15);
    assert_eq!(rows("F").len(), 3696);
    assert_eq!(rows("P").len(), 60);
}

#[test]
fn worlds_decode_and_validate_exactly_as_the_oracle_marks_them() {
    for w in worlds() {
        let symbols: Vec<&str> = w
            .components
            .components
            .iter()
            .map(|c| c.symbol_id())
            .collect();
        assert!(w.components.validate(&[]).is_ok(), "components of {}", w.id);
        assert_eq!(
            w.storyboard.validate(&symbols).is_ok(),
            w.valid,
            "world {}",
            w.id
        );
    }
}

#[test]
fn the_invalid_worlds_are_refused_by_their_own_record_rules_yet_still_evaluate() {
    let all = worlds();
    let ghost = world(&all, "ghostsym");
    assert!(matches!(
        ghost.storyboard.validate(&[]),
        Err(StoryboardRecordError::Unresolved(_))
    ));
    for id in ["zerodur", "inverted"] {
        let w = world(&all, id);
        assert!(matches!(
            w.storyboard.validate(&["sa"]),
            Err(StoryboardRecordError::Invalid(_))
        ));
    }
    // Decode admits what validation refuses; the join follows the baseline for all of them.
    assert_eq!(total_of(ghost, "mt"), 30);
    assert_eq!(total_of(world(&all, "zerodur"), "mt"), 20);
}

#[test]
fn montage_totals_match_the_baseline() {
    let all = worlds();
    for f in rows("T") {
        let expected: i64 = f[3].parse().expect("total");
        assert_eq!(
            total_of(world(&all, f[1]), f[2]),
            expected,
            "{} {}",
            f[1],
            f[2]
        );
    }
}

#[test]
fn every_baseline_layer_frame_row_is_reproduced() {
    let all = worlds();
    let mut bad = Vec::new();
    for f in rows("F") {
        let got = locate_row(world(&all, f[1]), &f);
        if got != f[5] {
            bad.push(format!(
                "{} {} {} {} want {} got {}",
                f[1], f[2], f[3], f[4], f[5], got
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn every_baseline_placement_row_is_reproduced() {
    let all = worlds();
    let mut bad = Vec::new();
    for f in rows("P") {
        let got = place_row(world(&all, f[1]), &f);
        if got != f[4] {
            bad.push(format!(
                "{} {} {} want {} got {}",
                f[1], f[2], f[3], f[4], got
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn absent_and_null_placement_resolve_every_frame_alike() {
    let all = worlds();
    for w in &all {
        for montage in ["mt", "mt2"] {
            for frame in -3..=70 {
                assert_eq!(
                    located(&all, w.id, montage, "~", frame),
                    located(&all, w.id, montage, "n", frame),
                    "{} {} {}",
                    w.id,
                    montage,
                    frame
                );
            }
        }
    }
}

#[test]
fn source_frames_of_the_five_member_montage_are_pinned() {
    let all = worlds();
    let at = |frame| located(&all, "plain", "mt", "~", frame);
    let src = |member_index, symbol: &str, src_frame| {
        Located::Source(SourceFrame {
            member_index,
            symbol_id: symbol.to_string(),
            src_frame,
        })
    };
    assert_eq!(at(0), src(0, "sa", 0));
    assert_eq!(at(11), src(0, "sa", 11));
    assert_eq!(at(12), src(1, "sb", 2));
    assert_eq!(at(22), src(2, "sc", 0));
    assert_eq!(at(34), src(4, "sa", 3));
    // 35 is one past the end: the layer loops back to the first member.
    assert_eq!(at(35), src(0, "sa", 0));
    assert_eq!(at(37), src(0, "sa", 2));
    // A placement shifts the montage-local frame; the layer is empty before it.
    assert_eq!(located(&all, "plain", "mt", "4", 4), src(0, "sa", 0));
    assert_eq!(located(&all, "plain", "mt", "4", 39), src(0, "sa", 0));
    // The pinned float-rounding point of L10-A (49 * 3 / 147 is exactly 1).
    assert_eq!(located(&all, "float", "mt", "~", 49), src(0, "sa", 1));
}

#[test]
fn every_empty_frame_names_its_reason() {
    let all = worlds();
    let why =
        |world_id, montage, placed, frame| match located(&all, world_id, montage, placed, frame) {
            Located::Empty(reason) => reason,
            Located::Source(s) => panic!("unexpected source {s:?}"),
        };
    assert_eq!(why("plain", "gone", "~", 3), MontageEmpty::MissingMontage);
    assert_eq!(why("plain", "i1", "~", 3), MontageEmpty::MissingMontage);
    assert_eq!(why("plain", "mt3", "~", 3), MontageEmpty::ZeroTotal);
    assert_eq!(why("dangling", "mt2", "~", 0), MontageEmpty::ZeroTotal);
    assert_eq!(why("nochain", "mt", "~", 0), MontageEmpty::ZeroTotal);
    assert_eq!(why("plain", "mt", "4", 3), MontageEmpty::BeforePlacement);
    assert_eq!(why("plain", "mt", "~", -1), MontageEmpty::BeforePlacement);
    assert_eq!(why("ghostsym", "mt", "~", 0), MontageEmpty::MissingSymbol);
}

struct Recorder {
    calls: RefCell<Vec<(String, i64)>>,
}

impl SymbolFramePort for Recorder {
    type Content = String;
    fn symbol_frame(&self, symbol_id: &str, src_frame: i64) -> Result<String, ProviderError> {
        self.calls
            .borrow_mut()
            .push((symbol_id.to_string(), src_frame));
        Ok(format!("{symbol_id}#{src_frame}"))
    }
}

#[test]
fn the_component_provider_receives_exactly_the_baseline_request() {
    let all = worlds();
    let w = world(&all, "plain");
    let port = Recorder {
        calls: RefCell::new(Vec::new()),
    };
    let got = resolve_montage_layer_frame(
        &layer_of("mt", "~"),
        &w.storyboard,
        &w.components,
        12,
        &port,
    );
    assert_eq!(
        got,
        Ok(MontageFrame::Content {
            source: SourceFrame {
                member_index: 1,
                symbol_id: "sb".to_string(),
                src_frame: 2
            },
            content: "sb#2".to_string(),
        })
    );
    assert_eq!(*port.calls.borrow(), vec![("sb".to_string(), 2)]);
    // No request is made for an empty frame or for a symbol that no longer exists.
    let before =
        resolve_montage_layer_frame(&layer_of("mt", "4"), &w.storyboard, &w.components, 3, &port);
    assert_eq!(
        before,
        Ok(MontageFrame::Empty(MontageEmpty::BeforePlacement))
    );
    let ghost = world(&all, "ghostsym");
    let missing = resolve_montage_layer_frame(
        &layer_of("mt", "~"),
        &ghost.storyboard,
        &ghost.components,
        0,
        &port,
    );
    assert_eq!(
        missing,
        Ok(MontageFrame::Empty(MontageEmpty::MissingSymbol))
    );
    assert_eq!(port.calls.borrow().len(), 1);
}

#[test]
fn without_the_component_evaluation_join_content_is_typed_unavailable() {
    let all = worlds();
    let w = world(&all, "plain");
    let unavailable = ProviderError::Unavailable(
        "component frame content: the lane 4 evaluation join is not integrated",
    );
    let got = resolve_montage_layer_frame(
        &layer_of("mt", "~"),
        &w.storyboard,
        &w.components,
        0,
        &UnavailableSymbolFrames,
    );
    assert_eq!(got, Err(MontageLayerError::Provider(unavailable)));
    let empty = resolve_montage_layer_frame(
        &layer_of("mt", "4"),
        &w.storyboard,
        &w.components,
        0,
        &UnavailableSymbolFrames,
    );
    assert_eq!(
        empty,
        Ok(MontageFrame::Empty(MontageEmpty::BeforePlacement))
    );
}

#[test]
fn resolving_never_writes_the_retime_defaults_back() {
    let all = worlds();
    let w = world(&all, "plain");
    let (storyboard, components) = (w.storyboard.clone(), w.components.clone());
    for frame in 0..40 {
        locate_montage_frame(&layer_of("mt", "~"), &w.storyboard, &w.components, frame)
            .expect("locate");
    }
    let existing = existing_of("[]");
    plan_place_montage(
        &w.storyboard,
        &w.components,
        &existing.placed,
        &existing.scene,
        "mt",
    )
    .expect("plan");
    assert_eq!(w.storyboard, storyboard);
    assert_eq!(w.components, components);
    // The module that has no retime fields still has none (the baseline filled them in).
    assert!(w.storyboard.module("i1").expect("i1").trim_in().is_none());
}
