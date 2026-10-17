//! Tests of the shot module edits (L10-B, I17/I42): oracle rows from the
//! baseline `storyboard.js` plus literal pins, each pin also checked to be a
//! row of the oracle so no expectation is invented here.

use super::super::rows::{oracle_rows, ORACLE};
use super::super::ShotChainError;
use super::rows::{kinds_of, num_field, run, sound_of, split_text, PRODUCTION};
use super::{
    add_instance_auto, auto_instance_position_with, count_kind, js_round, montage_name,
    new_instance, new_montage, sound_split_menu_enabled, split_sound, AutoGrid, Field, ModuleKind,
    ModuleRef, AUTO_GRID, SOUND_MENU_MARGIN, SOUND_SPLIT_X_OFFSET,
};

fn oracle_has(line: &str) {
    assert!(ORACLE.lines().any(|l| l == line), "oracle lacks: {}", line);
}

fn sound(in_sec: &str, out_sec: &str, cursor: &str) -> super::SoundModule {
    sound_of("snd", in_sec, out_sec, cursor, 12.5, 7.0)
}

#[test]
fn production_matches_every_baseline_row() {
    assert_eq!(oracle_rows('K').len(), 525);
    assert_eq!(oracle_rows('W').len(), 118);
    assert_eq!(oracle_rows('X').len(), 457);
    assert_eq!(oracle_rows('G').len(), 51);
    assert_eq!(oracle_rows('A').len(), 52);
    assert_eq!(oracle_rows('M').len(), 18);
    let report = run(&PRODUCTION);
    assert_eq!(report.checked, 525 + 118 + 457 + 51 + 52 + 18);
    assert!(
        report.bad.is_empty(),
        "{:?}",
        &report.bad[..report.bad.len().min(5)]
    );
}

#[test]
fn split_sound_cuts_at_the_cursor_and_shares_the_source() {
    let split = split_sound(&sound("1.5", "3.25", "2")).unwrap();
    assert_eq!(split.module_id, "snd0");
    assert_eq!(split.left_out_sec, 2.0);
    assert_eq!(split.left_cursor_sec, Field::Null);
    assert_eq!(split.right.request, 0);
    assert_eq!(split.right.in_sec, Field::Value(2.0));
    assert_eq!(split.right.out_sec, Field::Value(3.25));
    assert_eq!(split.right.cursor_sec, Field::Null);
    assert_eq!(split.right.source, "B64");
    assert_eq!(split.right.name, Field::Value("snd".to_string()));
    assert_eq!(
        (split.right.x, split.right.y),
        (12.5 + SOUND_SPLIT_X_OFFSET, 7.0)
    );
    assert_eq!(SOUND_SPLIT_X_OFFSET, 250.0);
    assert_eq!(
        split_text(&split),
        "lo=2;lc=null;r=snd,2,3.25,null,262.5,7,B64,N"
    );
    oracle_has("W snd 1.5 3.25 2 12.5 7 lo=2;lc=null;r=snd,2,3.25,null,262.5,7,B64,N");
    oracle_has("W snd 1.5 3.25 3 12.5 7 lo=3;lc=null;r=snd,3,3.25,null,262.5,7,B64,N");
}

#[test]
fn split_sound_keeps_an_absent_or_null_name_and_reads_missing_range_ends_as_zero() {
    for (name, text) in [("~", "~"), ("null", "null")] {
        let module = sound_of(name, "~", "10", "1.5", 12.5, 7.0);
        let split = split_sound(&module).unwrap();
        let want = format!("lo=1.5;lc=null;r={},1.5,10,null,262.5,7,B64,N", text);
        assert_eq!(split_text(&split), want);
        oracle_has(&format!("W {} ~ 10 1.5 12.5 7 {}", name, want));
    }
    // inSec absent reads as 0: a cursor of 0.04 is inside (0, 10)
    let split = split_sound(&sound("~", "10", "0.04")).unwrap();
    assert_eq!(split.right.in_sec, Field::Value(0.04));
    oracle_has("W snd ~ 10 0.04 12.5 7 lo=0.04;lc=null;r=snd,0.04,10,null,262.5,7,B64,N");
    // outSec absent reads as 0: a negative cursor above a negative inSec is inside
    let split = split_sound(&sound("-2", "~", "-1")).unwrap();
    assert_eq!(split.left_out_sec, -1.0);
    assert_eq!(split.right.out_sec, Field::Absent);
    oracle_has("W snd -2 ~ -1 12.5 7 lo=-1;lc=null;r=snd,-1,~,null,262.5,7,B64,N");
    let split = split_sound(&sound("-2", "null", "-1")).unwrap();
    assert_eq!(split.right.out_sec, Field::Null);
    oracle_has("W snd -2 null -1 12.5 7 lo=-1;lc=null;r=snd,-1,null,null,262.5,7,B64,N");
}

#[test]
fn split_sound_refuses_what_the_baseline_would_have_split_into_a_broken_pair() {
    let mut reasons = [0usize; 2];
    for fields in oracle_rows('X') {
        let module = sound_of(
            fields[0],
            fields[1],
            fields[2],
            fields[3],
            fields[4].parse().unwrap(),
            fields[5].parse().unwrap(),
        );
        let snapshot = module.clone();
        let want = if fields[6] == "nocursor" {
            reasons[0] += 1;
            ShotChainError::CursorMissing
        } else {
            reasons[1] += 1;
            ShotChainError::CursorNotInside
        };
        assert_eq!(split_sound(&module), Err(want), "{:?}", fields);
        // A refused split leaves the module untouched. Compared through Debug because a
        // NaN cursor (oracle rows carry one) is never `==` to itself under PartialEq.
        assert_eq!(
            format!("{module:?}"),
            format!("{snapshot:?}"),
            "{:?}",
            fields
        );
    }
    assert_eq!(reasons, [50, 407]);
    // both range ends and the missing cursor are in the oracle as accepted-by-baseline rows
    oracle_has("X snd ~ ~ ~ 12.5 7 nocursor lo=~;lc=null;r=snd,~,~,null,262.5,7,B64,N");
    oracle_has("X snd ~ ~ null 12.5 7 nocursor lo=null;lc=null;r=snd,null,~,null,262.5,7,B64,N");
    oracle_has("X snd ~ ~ 0 12.5 7 outside lo=0;lc=null;r=snd,0,~,null,262.5,7,B64,N");
    assert_eq!(
        split_sound(&sound("1.5", "3.25", "1.5")),
        Err(ShotChainError::CursorNotInside)
    );
    assert_eq!(
        split_sound(&sound("1.5", "3.25", "3.25")),
        Err(ShotChainError::CursorNotInside)
    );
    assert_eq!(
        split_sound(&sound("1.5", "3.25", "NaN")),
        Err(ShotChainError::CursorNotInside)
    );
    assert_eq!(
        split_sound(&sound("1.5", "3.25", "~")),
        Err(ShotChainError::CursorMissing)
    );
    assert_eq!(
        split_sound(&sound("1.5", "3.25", "null")),
        Err(ShotChainError::CursorMissing)
    );
    let off_plane = sound_of("snd", "1.5", "3.25", "2", f64::NAN, 7.0);
    assert_eq!(split_sound(&off_plane), Err(ShotChainError::NonFinite));
}

#[test]
fn the_menu_guard_keeps_a_margin_from_both_ends() {
    assert_eq!(SOUND_MENU_MARGIN, 0.05);
    let menu = |i: &str, o: &str, c: &str| sound_split_menu_enabled(&sound(i, o, c));
    assert!(!menu("~", "10", "0.04"));
    assert!(!menu("~", "10", "0.05"));
    assert!(menu("~", "10", "0.06"));
    assert!(menu("~", "10", "9.94"));
    assert!(!menu("~", "10", "9.95"));
    assert!(!menu("~", "10", "9.99"));
    assert!(menu("1.5", "3.25", "1.6"));
    assert!(menu("1.5", "3.25", "3"));
    assert!(!menu("1.5", "3.25", "3.25"));
    assert!(!menu("1.5", "3.25", "~"));
    assert!(!menu("1.5", "3.25", "null"));
    assert!(!menu("1.5", "3.25", "NaN"));
    for line in [
        "K ~ 10 0.04 0",
        "K ~ 10 0.05 0",
        "K ~ 10 0.06 1",
        "K ~ 10 9.94 1",
        "K ~ 10 9.95 0",
        "K 1.5 3.25 1.6 1",
        "K 1.5 3.25 3 1",
        "K 1.5 3.25 3.25 0",
        "K 1.5 3.25 NaN 0",
    ] {
        oracle_has(line);
    }
    // whatever the menu offers, the split accepts; the margin is a gesture rule only
    let mut offered = 0;
    let mut split_only = 0;
    for fields in oracle_rows('K') {
        let module = sound(fields[0], fields[1], fields[2]);
        let enabled = sound_split_menu_enabled(&module);
        if enabled {
            offered += 1;
            assert!(split_sound(&module).is_ok(), "{:?}", fields);
        } else if split_sound(&module).is_ok() {
            split_only += 1;
        }
    }
    assert_eq!((offered, split_only), (80, 22));
}

#[test]
fn auto_instances_fill_a_six_column_grid_counting_only_instances() {
    let at = |kinds: &str| {
        let added = add_instance_auto("sym", &kinds_of(kinds)).unwrap();
        (added.x, added.y)
    };
    assert_eq!(at("-"), (40.0, 40.0));
    assert_eq!(at("i"), (180.0, 40.0));
    assert_eq!(at("iiiii"), (740.0, 40.0));
    assert_eq!(at("iiiiii"), (40.0, 200.0));
    assert_eq!(at("iiiiiii"), (180.0, 200.0));
    assert_eq!(at("iiiiiiiiiiii"), (40.0, 360.0));
    let long = "i".repeat(39);
    assert_eq!(at(long.as_str()), (460.0, 1000.0));
    assert_eq!(at("msoms"), (40.0, 40.0));
    assert_eq!(at("imsoiiimm"), (600.0, 40.0));
    for line in [
        "G - 40 40",
        "G i 180 40",
        "G iiiii 740 40",
        "G iiiiii 40 200",
        "G iiiiiii 180 200",
        "G iiiiiiiiiiii 40 360",
        "G msoms 40 40",
        "G imsoiiimm 600 40",
    ] {
        oracle_has(line);
    }
    assert_eq!(count_kind(&kinds_of("imsoiiimm"), ModuleKind::Instance), 4);
    assert_eq!(count_kind(&kinds_of("imsoiiimm"), ModuleKind::Montage), 3);
    let added = add_instance_auto("sym", &[]).unwrap();
    assert_eq!(added.request, 0);
    assert_eq!(
        (added.trim_in, added.trim_out, added.duration),
        (None, None, None)
    );
}

#[test]
fn a_bare_instance_sits_on_the_rounded_drop_point() {
    let at = |x: f64, y: f64| {
        let added = new_instance("sym", x, y).unwrap();
        (added.x, added.y)
    };
    assert_eq!(at(0.5, 2.5), (1.0, 3.0));
    assert_eq!(at(1.5, 2.5), (2.0, 3.0));
    assert_eq!(at(2.5, 2.5), (3.0, 3.0));
    assert_eq!(at(-0.5, -0.5), (0.0, 0.0));
    assert_eq!(at(-1.5, -2.5), (-1.0, -2.0));
    assert_eq!(at(123.456, -123.456), (123.0, -123.0));
    assert_eq!(at(1e21, 0.0), (1e21, 0.0));
    assert_eq!(at(0.49999999999999994, 2.4999999999999996), (0.0, 2.0));
    assert_eq!(new_instance("sym", 3.0, 4.0).unwrap().symbol_id, "sym");
    for line in [
        "A 0.5 2.5 1 3 sym",
        "A 1.5 2.5 2 3 sym",
        "A 2.5 2.5 3 3 sym",
        "A -0.5 -0.5 0 0 sym",
        "A -1.5 2.5 -1 3 sym",
        "A -2.5 2.5 -2 3 sym",
        "A 123.456 -123.456 123 -123 sym",
        "A 1e21 0 1e+21 0 sym",
        "A 0.49999999999999994 2.5 0 3 sym",
        "A 2.4999999999999996 2.5 2 3 sym",
    ] {
        oracle_has(line);
    }
    // negative zero never leaves the kernel: -0.4 rounds to 0, not -0
    assert_eq!(at(-0.4, 0.0).0.to_bits(), 0.0f64.to_bits());
}

#[test]
fn js_round_ties_go_toward_positive_infinity() {
    let round = |v: f64| js_round(v).to_bits();
    assert_eq!(round(0.5), 1.0f64.to_bits());
    assert_eq!(round(-0.5), 0.0f64.to_bits());
    assert_eq!(round(-1.5), (-1.0f64).to_bits());
    assert_eq!(round(-2.5), (-2.0f64).to_bits());
    assert_eq!(round(2.5), 3.0f64.to_bits());
    assert_eq!(round(-0.0), 0.0f64.to_bits());
    assert_eq!(round(0.49999999999999994), 0.0f64.to_bits());
    assert_eq!(round(1e21), 1e21f64.to_bits());
    assert!(js_round(f64::NAN).is_nan());
}

#[test]
fn a_missing_range_end_reads_as_zero_like_the_baseline_or_operator() {
    assert_eq!(Field::<f64>::Absent.or_zero(), 0.0);
    assert_eq!(Field::<f64>::Null.or_zero(), 0.0);
    assert_eq!(Field::Value(0.0f64).or_zero(), 0.0);
    assert_eq!(Field::Value(-0.0f64).or_zero().to_bits(), 0.0f64.to_bits());
    assert_eq!(Field::Value(f64::NAN).or_zero(), 0.0);
    assert_eq!(Field::Value(3.25f64).or_zero(), 3.25);
    assert_eq!(num_field("null"), Field::Null);
    assert_eq!(num_field("~"), Field::Absent);
}

#[test]
fn a_new_montage_is_named_after_the_montage_count_empty_and_active() {
    for (kinds, name) in [
        ("-", "Montage 1"),
        ("m", "Montage 2"),
        ("mm", "Montage 3"),
        ("msim", "Montage 3"),
    ] {
        let added = new_montage(&kinds_of(kinds), 0.5, -0.5).unwrap();
        assert_eq!(added.montage.name, name, "{}", kinds);
        assert_eq!((added.montage.x, added.montage.y), (1.0, 0.0));
        assert!(added.montage.chain.is_empty() && added.montage.audio.is_empty());
        assert_eq!(added.montage.playhead, 0);
        assert_eq!(added.montage.request, 0);
        assert_eq!(added.active_montage, ModuleRef::New(0));
    }
    oracle_has("M - 0.5 -0.5 Montage_1 1 0 0 0 0 N 1 last");
    oracle_has("M m 0 0 Montage_2 0 0 0 0 0 N 1 last");
    oracle_has("M mm 0 0 Montage_3 0 0 0 0 0 N 1 last");
    oracle_has("M msim 0 0 Montage_3 0 0 0 0 0 N 1 last");
    oracle_has("M iiimmmsssoo 123.456 -7.5 Montage_4 123 -7 0 0 0 N 1 last");
    assert_eq!(montage_name(0).unwrap(), "Montage 1");
}

#[test]
fn non_finite_coordinates_and_counts_are_typed_errors() {
    assert_eq!(
        new_instance("sym", f64::NAN, 0.0),
        Err(ShotChainError::NonFinite)
    );
    assert_eq!(
        new_instance("sym", 0.0, f64::INFINITY),
        Err(ShotChainError::NonFinite)
    );
    assert_eq!(
        new_montage(&[], f64::NEG_INFINITY, 0.0),
        Err(ShotChainError::NonFinite)
    );
    assert_eq!(
        new_montage(&[], 0.0, f64::NAN),
        Err(ShotChainError::NonFinite)
    );
    assert_eq!(montage_name(usize::MAX), Err(ShotChainError::Overflow));
    assert_eq!(
        auto_instance_position_with(&AUTO_GRID, usize::MAX),
        Err(ShotChainError::Overflow)
    );
    let wide = AutoGrid {
        x_step: i64::MAX,
        ..AUTO_GRID
    };
    assert_eq!(
        auto_instance_position_with(&wide, 5),
        Err(ShotChainError::Overflow)
    );
    assert_eq!(auto_instance_position_with(&AUTO_GRID, 13), Ok((180, 360)));
}
