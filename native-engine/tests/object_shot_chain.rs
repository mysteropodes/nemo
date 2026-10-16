//! Tests of the shot chain kernel (L10-B, I17/I18/I42): oracle rows from the
//! baseline `storyboard.js` plus literal pins, each pin also checked to be a
//! row of the oracle so no expectation is invented here.

use super::modules::{ModuleRef, NewInstance};
use super::rows::{
    apply_split, host_of, member_of, oracle_rows, run, run_defaults, DEFAULTS, ORACLE, PRODUCTION,
};
use super::{
    chain_split_point, chain_total, ensure_retime, reset_retime, split_chain_clip, symbol_duration,
    ChainEntry, EntryKind, InstanceModule, LeftHalf, Retime, ShotChainError, ShotHost, SymbolView,
    MAX_SAFE_INTEGER,
};

const THREE: &str = "I:P-24-24-a:0:23:24|I:P-12-12-b:2:9:16|I:M:~:~:~";
const GAPS: &str = "I:P-10-10-a:~:~:~|X|N|I:P-6-6-b:1:4:8";
const SIGNED: &str = "I:P-24-24-a:0:9:10|I:P-24-24-b:0:9:-4|I:P-24-24-c:0:9:10";
const DUP: &str = "I:P-24-24-a:0:9:10|R0";

fn oracle_has(line: &str) {
    assert!(ORACLE.lines().any(|l| l == line), "oracle lacks: {}", line);
}

fn present(total_frames: i64, first: Option<i64>) -> SymbolView {
    SymbolView::Present {
        total_frames,
        first_layer_frame_count: first,
    }
}

fn plain(trim_in: Option<i64>, trim_out: Option<i64>, duration: Option<i64>) -> InstanceModule {
    InstanceModule {
        symbol_id: "s".to_string(),
        x: 1.0,
        y: 2.0,
        trim_in,
        trim_out,
        duration,
        symbol: present(48, Some(12)),
    }
}

fn point(chain: &str, playhead: Option<i64>, id: &str, want: Option<i64>) {
    let got = chain_split_point(&host_of(chain, playhead), id).unwrap();
    assert_eq!(got, want, "{} at {:?} for {}", chain, playhead, id);
    let ph = playhead.map_or("~".to_string(), |v| v.to_string());
    let res = want.map_or("-".to_string(), |v| v.to_string());
    oracle_has(&format!("P {} {} {} {}", chain, ph, id, res));
}

#[test]
fn production_matches_every_baseline_row() {
    assert_eq!(oracle_rows('P').len(), 1321);
    assert_eq!(oracle_rows('S').len(), 241);
    assert_eq!(oracle_rows('Q').len(), 110);
    let report = run(&PRODUCTION);
    assert_eq!(report.checked, 1321 + 241);
    assert!(
        report.bad.is_empty(),
        "{:?}",
        &report.bad[..report.bad.len().min(5)]
    );
}

#[test]
fn retime_defaults_match_every_baseline_row() {
    let report = run_defaults(&DEFAULTS);
    assert_eq!(report.checked, 8 + 8 + 135);
    assert!(
        report.bad.is_empty(),
        "{:?}",
        &report.bad[..report.bad.len().min(5)]
    );
}

#[test]
fn retime_default_pins() {
    assert_eq!(symbol_duration(&SymbolView::Missing), 24);
    assert_eq!(symbol_duration(&present(48, Some(12))), 48);
    assert_eq!(symbol_duration(&present(0, Some(7))), 7);
    assert_eq!(symbol_duration(&present(0, None)), 24);
    assert_eq!(symbol_duration(&present(0, Some(0))), 24);
    for line in [
        "D M 24",
        "D P-48-12-a 48",
        "D P-0-7-a 7",
        "D P-~-~-a 24",
        "D P-0-0-a 24",
    ] {
        oracle_has(line);
    }
    let from_symbol = ensure_retime(&plain(Some(5), None, None)).unwrap();
    assert_eq!(
        from_symbol,
        Retime {
            trim_in: 5,
            trim_out: 47,
            duration: 43
        }
    );
    oracle_has("R P-48-12-a 5 ~ ~ 5,47,43");
    let kept = ensure_retime(&plain(Some(0), Some(9), Some(20))).unwrap();
    assert_eq!(
        kept,
        Retime {
            trim_in: 0,
            trim_out: 9,
            duration: 20
        }
    );
    let reset = reset_retime(&present(48, Some(12))).unwrap();
    assert_eq!(
        reset,
        Retime {
            trim_in: 0,
            trim_out: 47,
            duration: 48
        }
    );
    oracle_has("Z P-48-12-a 0,47,48");
    assert_eq!(
        reset_retime(&SymbolView::Missing).unwrap(),
        Retime {
            trim_in: 0,
            trim_out: 23,
            duration: 24
        }
    );
    oracle_has("Z M 0,23,24");
}

#[test]
fn split_point_is_strictly_inside_the_named_member() {
    // m0 [0,24), m1 [24,40), m2 [40,64)
    point(THREE, Some(12), "m0", Some(12));
    point(THREE, Some(12), "m1", None);
    point(THREE, Some(0), "m0", None);
    point(THREE, None, "m0", None);
    point(THREE, Some(23), "m0", Some(23));
    point(THREE, Some(24), "m0", None);
    point(THREE, Some(24), "m1", None);
    point(THREE, Some(25), "m1", Some(1));
    point(THREE, Some(39), "m1", Some(15));
    point(THREE, Some(40), "m1", None);
    point(THREE, Some(40), "m2", None);
    point(THREE, Some(63), "m2", Some(23));
    point(THREE, Some(64), "m2", None);
    point(THREE, Some(12), "zz", None);
}

#[test]
fn dangling_non_instance_and_signed_durations_follow_the_baseline_walk() {
    // m0 [0,10), dangling, sound, m3 [10,18)
    point(GAPS, Some(5), "m0", Some(5));
    point(GAPS, Some(12), "m3", Some(2));
    point(GAPS, Some(12), "m0", None);
    // a negative member moves the running start backwards: m2 owns frame 10
    point(SIGNED, Some(10), "m2", Some(4));
    point(SIGNED, Some(10), "m0", None);
    // a module listed twice: the second occurrence starts at 10
    point(DUP, Some(12), "m0", Some(2));
    point("_", Some(1), "zz", None);
    point("!", Some(1), "zz", None);
}

#[test]
fn split_follows_the_baseline_formula() {
    let whole = |chain: &str, id: &str, local: i64| {
        let split = split_chain_clip(&host_of(chain, None), id, local).unwrap();
        (split.left.clone(), split)
    };
    // stretched x2: speed 2, local 7 -> round(3.5) = 4 source frames on the left
    let (left, split) = whole("I:P-48-12-a:0:9:20", "m0", 7);
    assert_eq!(
        left,
        LeftHalf {
            module_id: "m0".to_string(),
            trim_in: 0,
            trim_out: 3,
            duration: 7
        }
    );
    assert_eq!(
        split.right,
        NewInstance {
            request: 0,
            symbol_id: "s0".to_string(),
            x: 0.1,
            y: 5.5,
            trim_in: Some(4),
            trim_out: Some(9),
            duration: Some(13),
        }
    );
    assert_eq!(
        split.chain,
        vec![ModuleRef::Existing("m0".to_string()), ModuleRef::New(0)]
    );
    oracle_has("S I:P-48-12-a:0:9:20 m0 7 L=0,3,7 R=s0,0.1,5.5,4,9,13 C=m0,N T=20>20");
    // exact speed kept on both halves (2 frames per source frame)
    let (left, split) = whole("I:P-48-12-a:0:9:20", "m0", 8);
    let right_src = split.right.trim_out.unwrap() - split.right.trim_in.unwrap() + 1;
    assert_eq!(left.duration as f64 / 4.0, 2.0);
    assert_eq!(split.right.duration.unwrap() as f64 / right_src as f64, 2.0);
    oracle_has("S I:P-48-12-a:0:9:20 m0 8 L=0,3,8 R=s0,0.1,5.5,4,9,12 C=m0,N T=20>20");
    // the srcLen - 1 clamp: local 19 would ask for 10 source frames of 10
    let (left, split) = whole("I:P-48-12-a:0:9:20", "m0", 19);
    assert_eq!((left.trim_out, split.right.trim_in), (8, Some(9)));
    oracle_has("S I:P-48-12-a:0:9:20 m0 19 L=0,8,19 R=s0,0.1,5.5,9,9,1 C=m0,N T=20>20");
    // the max(1, ...) floor: local 1 of speed 49 rounds to 0, one frame is kept
    let (left, split) = whole("I:P-48-3-a:0:2:147", "m0", 1);
    assert_eq!((left.trim_out, split.right.trim_in), (0, Some(1)));
    oracle_has("S I:P-48-3-a:0:2:147 m0 1 L=0,0,1 R=s0,0.1,5.5,1,2,146 C=m0,N T=147>147");
    // middle member of three: new module sits right after it, position copied
    let (left, split) = whole(THREE, "m1", 5);
    assert_eq!((left.trim_in, left.trim_out, left.duration), (2, 4, 5));
    assert_eq!((split.right.x, split.right.y), (7.35, 2.75));
    let ids: Vec<ModuleRef> = ["m0", "m1"]
        .iter()
        .map(|id| ModuleRef::Existing(id.to_string()))
        .collect();
    assert_eq!(&split.chain[..2], &ids[..]);
    assert_eq!(split.chain[2], ModuleRef::New(0));
    assert_eq!(split.chain[3], ModuleRef::Existing("m2".to_string()));
    oracle_has("S I:P-24-24-a:0:23:24|I:P-12-12-b:2:9:16|I:M:~:~:~ m1 5 L=2,4,5 R=s1,7.35,2.75,5,9,11 C=m0,m1,N,m2 T=64>64");
    // squeezed: speed 0.5, local 2 -> 4 source frames
    let (left, _) = whole("I:P-48-12-a:0:9:5", "m0", 2);
    assert_eq!((left.trim_out, left.duration), (3, 2));
    oracle_has("S I:P-48-12-a:0:9:5 m0 2 L=0,3,2 R=s0,0.1,5.5,4,9,3 C=m0,N T=5>5");
}

#[test]
fn halves_tile_the_source_and_total_changes_only_for_a_repeated_module() {
    for fields in oracle_rows('S') {
        let host = host_of(fields[0], None);
        let local: i64 = fields[2].parse().unwrap();
        let split = split_chain_clip(&host, fields[1], local).unwrap();
        let before = ensure_retime(member_of(&host, fields[1])).unwrap();
        assert_eq!(split.left.trim_in, before.trim_in, "{:?}", fields);
        assert!(split.left.trim_out >= split.left.trim_in, "{:?}", fields);
        assert_eq!(
            split.right.trim_in,
            Some(split.left.trim_out + 1),
            "{:?}",
            fields
        );
        assert_eq!(split.right.trim_out, Some(before.trim_out), "{:?}", fields);
        assert!(
            split.right.trim_in.unwrap() <= before.trim_out,
            "{:?}",
            fields
        );
        assert_eq!(split.left.duration, local, "{:?}", fields);
        assert_eq!(
            split.left.duration + split.right.duration.unwrap(),
            before.duration
        );
        let total = chain_total(&host).unwrap();
        let after = chain_total(&apply_split(&host, &split)).unwrap();
        if fields[0].contains("|R") {
            assert_ne!(after, total, "a repeated module follows the shortened one");
        } else {
            assert_eq!(after, total, "{:?}", fields);
        }
    }
    oracle_has("S I:P-24-24-a:0:9:10|R0 m0 4 L=0,3,4 R=s0,0.1,5.5,4,9,6 C=m0,N,m0 T=20>14");
    let split = split_chain_clip(&host_of(DUP, None), "m0", 4).unwrap();
    let want = ["m0", "N", "m0"];
    for (item, name) in split.chain.iter().zip(want) {
        match item {
            ModuleRef::Existing(id) => assert_eq!(id, name),
            ModuleRef::New(_) => assert_eq!(name, "N"),
        }
    }
}

#[test]
fn refused_inputs_are_typed_errors_and_leave_the_host_untouched() {
    let mut seen = [0usize; 3];
    for fields in oracle_rows('Q') {
        let host = host_of(fields[0], None);
        let snapshot = host.clone();
        let local: i64 = fields[2].parse().unwrap();
        let r = ensure_retime(member_of(&host, fields[1])).unwrap();
        let want = match fields[3] {
            "duration" => ShotChainError::InvalidDuration {
                duration: r.duration,
            },
            "local" => ShotChainError::LocalOutOfRange {
                local,
                duration: r.duration,
            },
            "source" => ShotChainError::SourceTooShort {
                src_len: r.trim_out - r.trim_in + 1,
            },
            other => panic!("unknown reason {}", other),
        };
        seen[["duration", "local", "source"]
            .iter()
            .position(|n| *n == fields[3])
            .unwrap()] += 1;
        assert_eq!(
            split_chain_clip(&host, fields[1], local),
            Err(want),
            "{:?}",
            fields
        );
        assert_eq!(host, snapshot);
    }
    assert_eq!(seen, [12, 92, 6]);
    let one = host_of("I:P-48-1-a:0:0:5", None);
    assert_eq!(
        split_chain_clip(&one, "m0", 2),
        Err(ShotChainError::SourceTooShort { src_len: 1 })
    );
    // the baseline accepted it and left a half with a source length of zero
    oracle_has("Q I:P-48-1-a:0:0:5 m0 2 source L=0,-1,2 R=s0,0.1,5.5,0,0,3 C=m0,N T=5>5");
    let zero = host_of("I:P-24-24-a:0:9:0", None);
    assert_eq!(
        split_chain_clip(&zero, "m0", 1),
        Err(ShotChainError::InvalidDuration { duration: 0 })
    );
    let negative = host_of("I:P-24-24-a:0:9:-4|I:P-24-24-b:0:9:10", None);
    assert_eq!(
        split_chain_clip(&negative, "m0", 1),
        Err(ShotChainError::InvalidDuration { duration: -4 })
    );
    let full = host_of("I:P-24-24-a:0:23:24", None);
    for local in [0, 24, -1, 25] {
        assert_eq!(
            split_chain_clip(&full, "m0", local),
            Err(ShotChainError::LocalOutOfRange {
                local,
                duration: 24
            })
        );
    }
}

#[test]
fn unknown_dangling_and_non_instance_targets_are_typed_errors() {
    let host = host_of(GAPS, Some(12));
    let snapshot = host.clone();
    assert_eq!(
        split_chain_clip(&host, "zz", 1),
        Err(ShotChainError::MemberNotFound)
    );
    assert_eq!(
        split_chain_clip(&host, "m1", 1),
        Err(ShotChainError::MemberNotInstance)
    );
    assert_eq!(
        split_chain_clip(&host, "m2", 1),
        Err(ShotChainError::MemberNotInstance)
    );
    assert_eq!(host, snapshot);
}

#[test]
fn integers_outside_the_javascript_exact_range_are_overflow() {
    let big = MAX_SAFE_INTEGER;
    assert_eq!(
        ensure_retime(&plain(None, None, Some(big + 1))),
        Err(ShotChainError::Overflow)
    );
    assert_eq!(
        ensure_retime(&plain(Some(i64::MIN), None, None)),
        Err(ShotChainError::Overflow)
    );
    let mut heavy = plain(None, None, None);
    heavy.symbol = present(i64::MAX, None);
    assert_eq!(ensure_retime(&heavy), Err(ShotChainError::Overflow));
    assert_eq!(
        reset_retime(&present(i64::MAX, None)),
        Err(ShotChainError::Overflow)
    );
    let entry = |id: &str, m: InstanceModule| ChainEntry {
        module_id: id.to_string(),
        kind: EntryKind::Instance(m),
    };
    let host = ShotHost {
        playhead: Some(big + 1),
        chain: vec![entry("a", plain(Some(0), Some(9), Some(10)))],
    };
    assert_eq!(chain_split_point(&host, "a"), Err(ShotChainError::Overflow));
    let sum = ShotHost {
        playhead: None,
        chain: vec![
            entry("a", plain(Some(0), Some(big - 1), Some(big))),
            entry("b", plain(Some(0), Some(big - 1), Some(big))),
        ],
    };
    assert_eq!(chain_total(&sum), Err(ShotChainError::Overflow));
    let unsafe_trim = ShotHost {
        playhead: None,
        chain: vec![entry("a", plain(Some(0), Some(big + 1), Some(5)))],
    };
    let snapshot = unsafe_trim.clone();
    assert_eq!(
        split_chain_clip(&unsafe_trim, "a", 1),
        Err(ShotChainError::Overflow)
    );
    assert_eq!(unsafe_trim, snapshot);
}

#[test]
fn a_new_module_is_a_request_never_a_baseline_counter_id() {
    for fields in oracle_rows('S') {
        let host = host_of(fields[0], None);
        let local: i64 = fields[2].parse().unwrap();
        let split = split_chain_clip(&host, fields[1], local).unwrap();
        assert_eq!(split.right.request, 0);
        let news: Vec<&ModuleRef> = split
            .chain
            .iter()
            .filter(|r| matches!(r, ModuleRef::New(_)))
            .collect();
        assert_eq!(news, vec![&ModuleRef::New(0)]);
        let kept: Vec<String> = split
            .chain
            .iter()
            .filter_map(|r| match r {
                ModuleRef::Existing(id) => Some(id.clone()),
                ModuleRef::New(_) => None,
            })
            .collect();
        let original: Vec<String> = host.chain.iter().map(|e| e.module_id.clone()).collect();
        assert_eq!(kept, original);
    }
    // characterisation only: the baseline mints 'sbm' + a persisted counter
    for line in [
        "I 0 sbm0 1",
        "I 1 sbm1 2",
        "I 7 sbm7 8",
        "I 41 sbm41 42",
        "I ~ sbmNaN NaN",
    ] {
        oracle_has(line);
    }
}

#[test]
fn the_oracle_contains_the_named_chain_classes() {
    let has = |prefix: &str| ORACLE.lines().any(|l| l.starts_with(prefix));
    assert!(has("P _ "), "empty chain");
    assert!(has("P ! "), "no chain field");
    assert!(has("Q I:P-48-1-a:3:3:1 "), "one-frame member");
    assert!(has("Q I:P-48-1-a:4:3:3 "), "zero-length source");
    assert!(has("Q I:P-24-24-a:0:9:0 "), "zero duration");
    assert!(has("Q I:P-24-24-a:0:9:-4|"), "negative duration");
    assert!(
        has("S I:P-10-10-a:~:~:~|X|N|I:P-6-6-b:1:4:8 m3 "),
        "dangling and sound slots"
    );
    assert!(has("S I:P-24-24-a:0:9:10|R0 m0 "), "repeated module");
    assert!(has("S I:P-100-100-a:0:99:1000 "), "heavy stretch");
    assert!(has("S I:P-100-100-a:0:99:3 "), "heavy squeeze");
}
