//! Tests of the montage timing kernel against the baseline oracle rows and
//! literal pins (see `object_montage_timing_rows.rs` for the row format and
//! provenance of `fixtures/object-montage-timing/oracle.txt`).

use super::rows::{chain_of, once_for, resolve, run, sym_view, ORACLE, PRODUCTION, WALK_TOKEN};
use super::{
    ensure_retime, montage_layer_frame, montage_layer_source, montage_source_frame, montage_total,
    montage_view_segments, once_symbol_frame, resolve_chain, storyboard_source_frame,
    symbol_duration, ChainMember, LayerFrame, LayerNone, MontageMember, MontageTimingError,
    OnceLayer, Retime, SymbolView,
};

#[test]
fn production_matches_every_baseline_row() {
    let report = run(&PRODUCTION);
    assert!(
        report.bad.is_empty(),
        "{:#?}",
        &report.bad[..report.bad.len().min(5)]
    );
    let kinds: Vec<(char, usize)> = report.kinds.into_iter().collect();
    assert_eq!(
        kinds,
        vec![
            ('C', 720),
            ('D', 11),
            ('F', 640),
            ('L', 1130),
            ('O', 384),
            ('R', 135),
            ('T', 26),
            ('V', 26)
        ]
    );
}

#[test]
fn resolve_chain_equals_the_injectable_harness_resolution() {
    let mut chains = 0;
    for line in ORACLE.lines().filter(|l| l.starts_with("T ")) {
        let token = line.split(' ').nth(1).expect("chain");
        let chain = chain_of(token);
        let direct = resolve_chain(&chain.ids, &chain).expect("resolve");
        assert_eq!(direct, resolve(&chain, &PRODUCTION), "{token}");
        chains += 1;
    }
    assert_eq!(chains, 26);
}

fn source(members: &[ChainMember], frame: i64) -> Option<(usize, String, i64)> {
    montage_source_frame(members, frame)
        .expect("source")
        .map(|s| (s.member_index, s.symbol_id, s.src_frame))
}

#[test]
fn symbol_duration_falls_back_totalframes_layer_length_24() {
    let present = |total, first| SymbolView::Present {
        name: None,
        total_frames: total,
        first_layer_frame_count: first,
    };
    assert_eq!(symbol_duration(&SymbolView::Missing), 24);
    assert_eq!(symbol_duration(&present(48, Some(12))), 48);
    assert_eq!(symbol_duration(&present(0, Some(7))), 7);
    assert_eq!(symbol_duration(&present(0, Some(0))), 24);
    assert_eq!(symbol_duration(&present(0, None)), 24);
    assert_eq!(symbol_duration(&present(-3, Some(7))), -3);
}

#[test]
fn ensure_retime_defaults_are_pinned() {
    let member = |tin, tout, dur| MontageMember {
        symbol_id: "s".to_string(),
        trim_in: tin,
        trim_out: tout,
        duration: dur,
        symbol: sym_view("P-48-12-a"),
    };
    let r = |tin, tout, dur| ensure_retime(&member(tin, tout, dur)).expect("retime");
    assert_eq!(
        r(None, None, None),
        Retime {
            trim_in: 0,
            trim_out: 47,
            duration: 48
        }
    );
    assert_eq!(
        r(Some(5), None, None),
        Retime {
            trim_in: 5,
            trim_out: 47,
            duration: 43
        }
    );
    assert_eq!(
        r(None, Some(9), None),
        Retime {
            trim_in: 0,
            trim_out: 9,
            duration: 10
        }
    );
    assert_eq!(
        r(Some(2), Some(9), Some(20)),
        Retime {
            trim_in: 2,
            trim_out: 9,
            duration: 20
        }
    );
    assert_eq!(
        r(Some(2), Some(9), Some(0)),
        Retime {
            trim_in: 2,
            trim_out: 9,
            duration: 0
        }
    );
}

#[test]
fn dangling_and_non_instance_modules_are_skipped_not_errors() {
    let chain = chain_of("I:P-10-10-a:~:~:~|X|N|I:P-6-6-b:1:4:8");
    assert_eq!(chain.ids, ["m0", "m1", "m2", "m3"]);
    let members = resolve_chain(&chain.ids, &chain).expect("resolve");
    let ids: Vec<&str> = members.iter().map(|m| m.module_id.as_str()).collect();
    assert_eq!(ids, ["m0", "m3"]);
    assert_eq!(montage_total(&members), Ok(18));
    assert_eq!(source(&members, 9), Some((0, "s0".to_string(), 9)));
    assert_eq!(source(&members, 10), Some((1, "s3".to_string(), 1)));
    assert_eq!(source(&members, 17), Some((1, "s3".to_string(), 4)));
    assert_eq!(source(&members, 18), None);
    let absent = chain_of("!");
    assert!(absent.ids.is_empty());
    assert_eq!(resolve_chain(&absent.ids, &absent), Ok(Vec::new()));
}

#[test]
fn source_frames_of_a_three_member_chain() {
    let chain = chain_of(WALK_TOKEN);
    let members = resolve_chain(&chain.ids, &chain).expect("resolve");
    assert_eq!(montage_total(&members), Ok(64));
    let s = |i: usize, id: &str, f| Some((i, id.to_string(), f));
    assert_eq!(source(&members, 0), s(0, "s0", 0));
    assert_eq!(source(&members, 23), s(0, "s0", 23));
    assert_eq!(source(&members, 24), s(1, "s1", 2));
    assert_eq!(source(&members, 25), s(1, "s1", 2));
    assert_eq!(source(&members, 26), s(1, "s1", 3));
    assert_eq!(source(&members, 39), s(1, "s1", 9));
    assert_eq!(source(&members, 40), s(2, "s2", 0));
    assert_eq!(source(&members, 63), s(2, "s2", 23));
    assert_eq!(source(&members, 64), None);
    assert_eq!(
        montage_source_frame(&members, -1),
        Err(MontageTimingError::NegativeFrame)
    );
}

#[test]
fn one_frame_zero_length_and_stretched_members() {
    let one = chain_of("I:P-48-1-a:3:3:1");
    let one = resolve_chain(&one.ids, &one).expect("resolve");
    assert_eq!(source(&one, 0), Some((0, "s0".to_string(), 3)));
    assert_eq!(source(&one, 1), None);
    // srcLen 0: the min(srcLen - 1) clamp pulls every frame to trimIn - 1.
    let zero = chain_of("I:P-48-1-a:4:3:3");
    let zero = resolve_chain(&zero.ids, &zero).expect("resolve");
    for f in 0..3 {
        assert_eq!(source(&zero, f), Some((0, "s0".to_string(), 3)), "{f}");
    }
    let slow = chain_of("I:P-48-12-a:0:9:20");
    let slow = resolve_chain(&slow.ids, &slow).expect("resolve");
    let got: Vec<i64> = [0, 1, 2, 3, 10, 19]
        .iter()
        .map(|f| source(&slow, *f).expect("inside").2)
        .collect();
    assert_eq!(got, [0, 0, 1, 1, 5, 9]);
    let nothing = chain_of("I:P-48-1-a:0:9:0");
    let nothing = resolve_chain(&nothing.ids, &nothing).expect("resolve");
    assert_eq!(montage_total(&nothing), Ok(0));
    assert_eq!(source(&nothing, 0), None);
}

#[test]
fn montage_layer_frame_loops_and_honours_placement() {
    let chain = chain_of(WALK_TOKEN);
    let members = resolve_chain(&chain.ids, &chain).expect("resolve");
    let at = |frame, placed| {
        montage_layer_source(&members, frame, placed)
            .expect("layer")
            .map(|s| (s.symbol_id, s.src_frame))
    };
    let s = |id: &str, f| Some((id.to_string(), f));
    assert_eq!(at(63, None), s("s2", 23));
    assert_eq!(at(64, None), s("s0", 0));
    assert_eq!(at(133, Some(0)), s("s0", 5));
    assert_eq!(at(4, Some(5)), None);
    assert_eq!(at(5, Some(5)), s("s0", 0));
    assert_eq!(at(68, Some(5)), s("s2", 23));
    assert_eq!(at(69, Some(5)), s("s0", 0));
    assert_eq!(at(-3, Some(-3)), s("s0", 0));
    assert_eq!(at(-4, Some(-3)), None);
    assert_eq!(
        montage_layer_frame(10, Some(0), 0),
        Ok(LayerFrame::None(LayerNone::ZeroTotal))
    );
    assert_eq!(
        montage_layer_frame(1, Some(2), 0),
        Ok(LayerFrame::None(LayerNone::ZeroTotal))
    );
    assert_eq!(
        montage_layer_frame(1, Some(2), 9),
        Ok(LayerFrame::None(LayerNone::BeforePlacement))
    );
    assert_eq!(
        montage_layer_frame(100, None, 64),
        Ok(LayerFrame::Local(36))
    );
}

#[test]
fn montage_view_segment_specs() {
    let chain = chain_of(WALK_TOKEN);
    let members = resolve_chain(&chain.ids, &chain).expect("resolve");
    let spec = montage_view_segments(&members).expect("spec");
    assert_eq!(spec.total_frames, 64);
    let names: Vec<&str> = spec.segments.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "a \u{2014} montage",
            "b \u{2014} montage",
            "Composant \u{2014} montage"
        ]
    );
    let speeds: Vec<f64> = spec.segments.iter().map(|s| s.sym_speed).collect();
    assert_eq!(speeds, [1.0, 0.5, 1.0]);
    let spans: Vec<(i64, i64, i64, i64, i64)> = spec
        .segments
        .iter()
        .map(|s| {
            (
                s.sym_placed_at,
                s.in_point,
                s.out_point,
                s.sym_trim_in,
                s.sym_trim_out,
            )
        })
        .collect();
    assert_eq!(
        spans,
        [(0, 0, 23, 0, 23), (24, 24, 39, 2, 9), (40, 40, 63, 0, 23)]
    );
    assert!(spec
        .segments
        .iter()
        .all(|s| s.locked && s.visible && s.sym_play_mode == "once" && s.sym_single_frame == 0));
    let nameless = chain_of("I:P-48-12-~:3:~:~");
    let nameless = resolve_chain(&nameless.ids, &nameless).expect("resolve");
    let spec = montage_view_segments(&nameless).expect("spec");
    assert_eq!(spec.segments[0].name, "undefined \u{2014} montage");
    let empty = chain_of("N|X");
    let empty = resolve_chain(&empty.ids, &empty).expect("resolve");
    assert_eq!(
        montage_view_segments(&empty).err(),
        Some(MontageTimingError::EmptyChain)
    );
    let flat = chain_of("I:P-48-1-a:0:9:0");
    let flat = resolve_chain(&flat.ids, &flat).expect("resolve");
    let spec = montage_view_segments(&flat).expect("spec");
    assert!(spec.segments[0].sym_speed.is_infinite());
    assert_eq!(
        (spec.segments[0].in_point, spec.segments[0].out_point),
        (0, -1)
    );
}

#[test]
fn arithmetic_overflow_is_a_typed_error() {
    let member = MontageMember {
        symbol_id: "s".to_string(),
        trim_in: Some(i64::MIN),
        trim_out: Some(i64::MAX),
        duration: None,
        symbol: SymbolView::Missing,
    };
    assert_eq!(ensure_retime(&member), Err(MontageTimingError::Overflow));
    let big = |duration| ChainMember {
        module_id: "m".to_string(),
        symbol_id: "s".to_string(),
        retime: Retime {
            trim_in: 0,
            trim_out: 0,
            duration,
        },
        symbol: SymbolView::Missing,
    };
    assert_eq!(
        montage_total(&[big(i64::MAX), big(1)]),
        Err(MontageTimingError::Overflow)
    );
}

const SRC_LENS: [i64; 15] = [1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 24, 25, 30, 48, 50];

fn durations() -> Vec<i64> {
    let mut all: Vec<i64> = (1..=30).collect();
    all.extend([
        48, 49, 50, 96, 97, 98, 99, 100, 101, 120, 147, 196, 206, 214, 240, 294, 300,
    ]);
    all
}

/// (srcLen, duration, local, storyboard frame, `once` frame), trimIn 0.
const PINNED_DISAGREEMENTS: [(i64, i64, i64, i64, i64); 54] = [
    (2, 98, 49, 1, 0),
    (2, 196, 98, 1, 0),
    (2, 206, 103, 1, 0),
    (2, 214, 107, 1, 0),
    (3, 147, 49, 1, 0),
    (3, 147, 98, 2, 1),
    (3, 294, 98, 1, 0),
    (3, 294, 196, 2, 1),
    (4, 98, 49, 2, 1),
    (4, 196, 49, 1, 0),
    (4, 196, 98, 2, 1),
    (4, 196, 147, 3, 2),
    (4, 206, 103, 2, 1),
    (4, 214, 107, 2, 1),
    (6, 147, 49, 2, 1),
    (6, 147, 98, 4, 3),
    (6, 294, 49, 1, 0),
    (6, 294, 98, 2, 1),
    (6, 294, 147, 3, 2),
    (6, 294, 196, 4, 3),
    (8, 98, 49, 4, 3),
    (8, 196, 49, 2, 1),
    (8, 196, 98, 4, 3),
    (8, 196, 147, 6, 5),
    (8, 206, 103, 4, 3),
    (8, 214, 107, 4, 3),
    (12, 147, 49, 4, 3),
    (12, 147, 98, 8, 7),
    (12, 294, 49, 2, 1),
    (12, 294, 98, 4, 3),
    (12, 294, 147, 6, 5),
    (12, 294, 196, 8, 7),
    (16, 98, 49, 8, 7),
    (16, 196, 49, 4, 3),
    (16, 196, 98, 8, 7),
    (16, 196, 147, 12, 11),
    (16, 206, 103, 8, 7),
    (16, 214, 107, 8, 7),
    (24, 147, 49, 8, 7),
    (24, 147, 98, 16, 15),
    (24, 294, 49, 4, 3),
    (24, 294, 98, 8, 7),
    (24, 294, 147, 12, 11),
    (24, 294, 196, 16, 15),
    (30, 22, 11, 15, 14),
    (30, 26, 13, 15, 14),
    (30, 206, 103, 15, 14),
    (30, 214, 107, 15, 14),
    (48, 147, 49, 16, 15),
    (48, 147, 98, 32, 31),
    (48, 294, 49, 8, 7),
    (48, 294, 98, 16, 15),
    (48, 294, 147, 24, 23),
    (48, 294, 196, 32, 31),
];

/// The two baseline formulas disagree exactly at these grid points, always by
/// one frame (the `once` replay lands one below), and nowhere else on it.
#[test]
fn storyboard_and_once_formulas_disagree_at_pinned_points() {
    let mut found = Vec::new();
    for src_len in SRC_LENS {
        for duration in durations() {
            for local in 0..duration {
                let sb = storyboard_source_frame(0, src_len, local, duration).expect("frame");
                let on = once_for(0, src_len, duration, local);
                if sb != on {
                    found.push((src_len, duration, local, sb, on));
                }
            }
        }
    }
    assert_eq!(found, PINNED_DISAGREEMENTS.to_vec());
    assert!(found.iter().all(|p| p.3 - p.4 == 1));
    // Neither formula is the other: pin one point of each side explicitly.
    assert_eq!(storyboard_source_frame(0, 2, 49, 98), Ok(1));
    assert_eq!(2.0 / 98.0 * 49.0, 0.9999999999999999);
    assert_eq!(once_for(0, 2, 98, 49), 0);
}

#[test]
fn once_branch_pins() {
    let layer = |tin, tout, placed, speed| OnceLayer {
        sym_trim_in: tin,
        sym_trim_out: tout,
        sym_placed_at: placed,
        sym_speed: speed,
    };
    // symSpeed 0 is falsy and reads as 1; holds at the default trimOut total - 1.
    assert_eq!(
        once_symbol_frame(24, &layer(None, None, None, Some(0.0)), 9),
        9.0
    );
    assert_eq!(
        once_symbol_frame(24, &layer(None, None, None, Some(0.0)), 40),
        23.0
    );
    // total_frames 0 still plays one frame; main before placement clamps to 0.
    assert_eq!(once_symbol_frame(0, &layer(None, None, None, None), 5), 0.0);
    assert_eq!(
        once_symbol_frame(24, &layer(Some(5), Some(10), Some(4), Some(2.0)), -2),
        5.0
    );
    assert_eq!(
        once_symbol_frame(24, &layer(Some(5), Some(10), Some(4), Some(2.0)), 6),
        9.0
    );
    assert!(once_symbol_frame(24, &layer(None, None, Some(0), Some(f64::INFINITY)), 0).is_nan());
}
