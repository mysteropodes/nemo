//! Pinned literals and semantic checks for the duplicator count and mode-offset
//! kernel (L04-B, I07). Every pinned case is looked up in the oracle fixture of
//! `duplicator_placement_modes.rs` (its row must hold the literal bit for bit)
//! and is then evaluated by the kernel (within the same trig tolerance the oracle
//! test uses), so a literal cannot drift from the baseline unnoticed.

use super::tests::{
    bits, candidates, close, hex, infos, js, label, resolve_owned, rows, spec_of, tolerance,
    TRIG_ULPS,
};
use super::{
    clone_indices, dup_mode, duplicator_count, grid_cols, mode_offset, places_no_clones, DupMode,
    DuplicatorSpec, JsNumber, ModeOffset, ModeOffsetError, PathResolveError,
};

/// A numeric word becomes the hex bits the fixture holds; any other word stays.
fn hexify(word: &str) -> String {
    match word.parse::<f64>() {
        Ok(v) => format!("{:016x}", bits(v)),
        Err(_) => word.to_string(),
    }
}

/// The tokens after `=>` of the one row of `kind` that starts with `prefix`.
fn after_arrow(kind: &str, prefix: &[String]) -> Vec<&'static str> {
    let found: Vec<_> = rows(kind)
        .into_iter()
        .filter(|t| {
            t.len() > prefix.len() && t[..prefix.len()] == prefix[..] && t[prefix.len()] == "=>"
        })
        .collect();
    assert_eq!(found.len(), 1, "{kind} row {prefix:?}");
    found[0][prefix.len() + 1..].to_vec()
}

fn assert_oracle_triple(tokens: &[&str], want: [f64; 3], what: &str) {
    for (i, (token, w)) in tokens.iter().zip(want).enumerate() {
        assert!(
            close(hex(token), w, 0),
            "{what}: oracle field {i} is {} not {w}",
            hex(token)
        );
    }
}

fn assert_kernel_triple(
    out: Result<ModeOffset, ModeOffsetError>,
    want: [f64; 3],
    tol: [u64; 3],
    what: &str,
) {
    let got = out.expect(what);
    for (i, v) in [got.base_dx, got.base_dy, got.base_rot]
        .into_iter()
        .enumerate()
    {
        assert!(
            close(v, want[i], tol[i]),
            "{what}: field {i} is {v} not {}",
            want[i]
        );
    }
}

#[test]
fn pinned_counts_match_baseline_and_the_oracle() {
    // (mode, rows, cols, count) -> (count, cols)
    let pins: [([&str; 4], f64, f64); 12] = [
        (["grid", "30", "31", "u"], 900.0, 31.0),
        (["grid", "29", "31", "u"], 899.0, 31.0),
        (["grid", "2.5", "3", "u"], 7.5, 3.0),
        (["grid", "0", "2", "u"], 2.0, 2.0),
        (["grid", "-3", "3", "u"], 3.0, 3.0),
        (["grid", "NaN", "Inf", "u"], 900.0, f64::INFINITY),
        (["-", "3", "4", "u"], 12.0, 4.0),
        (["radial", "100", "100", "501"], 500.0, 100.0),
        (["path", "100", "100", "2.5"], 2.5, 100.0),
        (["bogus", "100", "100", "u"], 1.0, 100.0),
        (["Grid", "100", "100", "900"], 500.0, 100.0),
        (["radial", "100", "100", "NaN"], 1.0, 100.0),
    ];
    for (words, count, cols) in pins {
        let toks: Vec<String> = vec![
            words[0].to_string(),
            hexify(words[1]),
            hexify(words[2]),
            hexify(words[3]),
        ];
        let want = after_arrow("cnt", &toks);
        assert!(
            close(hex(want[0]), count, 0) && close(hex(want[1]), cols, 0),
            "oracle {words:?}"
        );
        let spec = DuplicatorSpec {
            mode: label(words[0]),
            rows: js(&toks[1]),
            cols: js(&toks[2]),
            count: js(&toks[3]),
            ..DuplicatorSpec::default()
        };
        assert!(close(duplicator_count(&spec), count, 0), "count {words:?}");
        assert!(close(grid_cols(&spec), cols, 0), "cols {words:?}");
    }
}

#[test]
fn pinned_offsets_match_baseline_and_the_oracle() {
    let nan = f64::NAN;
    // mode rows cols count spx spy start end radius orient align, then k -> (dx, dy, rot)
    let pins: [([&str; 11], &str, [f64; 3]); 9] = [
        (
            ["grid", "3", "2", "u", "10", "20", "u", "u", "u", "0", "0"],
            "5",
            [10.0, 40.0, 0.0],
        ),
        (
            ["grid", "3", "0", "u", "10", "20", "u", "u", "u", "0", "0"],
            "2",
            [0.0, 40.0, 0.0],
        ),
        (
            ["grid", "3", "0", "u", "NaN", "3", "u", "u", "u", "0", "0"],
            "2",
            [0.0, 6.0, 0.0],
        ),
        (
            [
                "grid", "3", "2.5", "u", "-5", "0.5", "u", "u", "u", "0", "0",
            ],
            "3",
            [-2.5, 0.5, 0.0],
        ),
        (
            [
                "radial", "u", "u", "6", "u", "u", "n", "180", "100", "1", "0",
            ],
            "1",
            [86.60254037844388, 49.99999999999999, 30.0],
        ),
        (
            [
                "radial", "u", "u", "6", "u", "u", "u", "180", "100", "1", "0",
            ],
            "1",
            [nan, nan, nan],
        ),
        (
            [
                "radial", "u", "u", "6", "u", "u", "90", "u", "100", "1", "0",
            ],
            "5",
            [86.6025403784439, 49.99999999999993, 390.0],
        ),
        (
            ["radial", "u", "u", "4", "u", "u", "0", "u", "50", "0", "0"],
            "1",
            [3.061616997868383e-15, 50.0, 0.0],
        ),
        (
            [
                "radial", "u", "u", "6", "u", "u", "90", "180", "100", "1", "0",
            ],
            "1",
            [-25.881904510252085, 96.59258262890683, 105.0],
        ),
    ];
    for (words, k, want) in pins {
        let mut toks: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        for t in toks.iter_mut().take(9).skip(1) {
            *t = hexify(t);
        }
        let mut prefix = toks.clone();
        prefix.push(k.to_string());
        let what = format!("{words:?} k={k}");
        assert_oracle_triple(&after_arrow("off", &prefix), want, &what);
        let refs: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
        let spec = spec_of(&refs);
        let out = mode_offset(
            &spec,
            k.parse().expect("k"),
            duplicator_count(&spec),
            grid_cols(&spec),
            None,
        );
        assert_kernel_triple(out, want, tolerance(dup_mode(spec.mode)), &what);
    }
}

#[test]
fn pinned_path_rows_match_baseline_and_the_oracle() {
    let infos = infos();
    // (candidate, count, align, k) -> (dx, dy, rot)
    let pins: [(&str, &str, &str, &str, [f64; 3]); 6] = [
        ("line", "5", "1", "3", [74.99999999999999, 0.0, 0.0]),
        (
            "diag",
            "5",
            "1",
            "1",
            [25.000000000000004, 25.000000000000004, 45.0],
        ),
        ("diag", "3", "1", "1", [50.0, 50.0, 45.0]),
        ("poly4c", "5", "1", "2", [100.0, 60.00000000000001, 90.0]),
        ("poly4c", "5", "1", "4", [0.0, 80.0, -90.0]),
        ("poly4c", "5", "0", "2", [100.0, 60.00000000000001, 0.0]),
    ];
    for (name, count, align, k, want) in pins {
        let prefix: Vec<String> = vec![
            name.into(),
            "path".into(),
            hexify(count),
            align.into(),
            k.into(),
        ];
        let what = format!("{name} count={count} align={align} k={k}");
        assert_oracle_triple(&after_arrow("pth", &prefix), want, &what);
        let spec = DuplicatorSpec {
            mode: Some("path"),
            count: js(&hexify(count)),
            path_align_tangent: align == "1",
            ..DuplicatorSpec::default()
        };
        let out = mode_offset(
            &spec,
            k.parse().expect("k"),
            duplicator_count(&spec),
            grid_cols(&spec),
            infos.get(name),
        );
        assert_kernel_triple(out, want, [0, 0, TRIG_ULPS], &what);
    }
}

#[test]
fn pinned_resolution_rules_match_baseline_and_the_oracle() {
    let cands = candidates();
    let resolve = |name: &str| resolve_owned(&cands[name]);
    for null in [
        "none",
        "dup",
        "zero",
        "zero_then_line",
        "raster_only",
        "empty",
    ] {
        assert_eq!(resolve(null), Ok(None), "{null}");
        assert_eq!(
            after_arrow("rsv", &[null.to_string()]),
            ["-"],
            "oracle {null}"
        );
    }
    // (candidate, length, start x, start y)
    for (name, length, x, y) in [
        ("raster_first", 100.0, 0.0, 0.0),
        ("single_seg_first", 100.0, 0.0, 0.0),
        ("offsetstart", 200.0, 50.0, -30.0),
    ] {
        let info = resolve(name).expect(name).expect(name);
        assert_eq!(
            (info.length, info.start, info.closed),
            (length, [x, y], false),
            "{name}"
        );
        let want = after_arrow("rsv", &[name.to_string()]);
        assert_eq!(
            (hex(want[0]), hex(want[1]), hex(want[2])),
            (length, x, y),
            "oracle {name}"
        );
    }
    assert!(resolve("poly4c").expect("closed").expect("closed").closed);
    for name in ["missing_handle", "missing_out"] {
        assert_eq!(
            resolve(name),
            Err(PathResolveError::MissingHandle),
            "{name}"
        );
        assert_eq!(
            after_arrow("rsv", &[name.to_string()]),
            ["throw"],
            "oracle {name}"
        );
    }
}

#[test]
fn numbers_read_like_the_javascript_or_operator() {
    for off in [
        JsNumber::Undefined,
        JsNumber::Null,
        JsNumber::Value(0.0),
        JsNumber::Value(-0.0),
        JsNumber::Value(f64::NAN),
    ] {
        assert_eq!(off.or_default(7.0), 7.0);
    }
    assert_eq!(JsNumber::Value(-3.0).or_default(7.0), -3.0);
    assert_eq!(
        JsNumber::Value(f64::NEG_INFINITY).or_default(7.0),
        f64::NEG_INFINITY
    );
    assert!(!JsNumber::Undefined.is_present() && !JsNumber::Null.is_present());
    assert!(JsNumber::Value(f64::NAN).is_present() && JsNumber::Value(0.0).is_present());
    assert!(JsNumber::Undefined.operand().is_nan());
    assert_eq!(JsNumber::Null.operand(), 0.0);
    assert_eq!(JsNumber::Value(5.5).operand(), 5.5);
}

#[test]
fn mode_labels_and_fractional_counts_follow_the_baseline_loop() {
    for grid in [None, Some(""), Some("grid")] {
        assert_eq!(dup_mode(grid), DupMode::Grid);
    }
    assert_eq!(dup_mode(Some("radial")), DupMode::Radial);
    assert_eq!(dup_mode(Some("path")), DupMode::Path);
    for other in ["Grid", "RADIAL", "bogus", " grid"] {
        assert_eq!(dup_mode(Some(other)), DupMode::Other, "{other}");
    }
    // `for (k = 0; k < count; k++)`: 7.5 runs k = 0..=7; `count <= 1` places nothing.
    assert_eq!(clone_indices(7.5), 8);
    assert_eq!(clone_indices(900.0), 900);
    assert_eq!(clone_indices(1.0), 1);
    assert_eq!(clone_indices(f64::NAN), 0);
    assert!(places_no_clones(1.0) && !places_no_clones(1.5));
}

#[test]
fn oracle_covers_the_families_and_edges() {
    let off = rows("off");
    let count = |m: &str| off.iter().filter(|t| t[0] == m).count();
    assert_eq!((count("grid"), count("radial"), count("-")), (615, 750, 6));
    assert_eq!(
        count("bogus") + count("path") + count("Radial") + count("Path"),
        6
    );
    assert_eq!(off.iter().filter(|t| t[13] == "throw").count(), 6);
    assert!(
        off.iter().any(|t| t[13] == "7ff8000000000000"),
        "NaN offsets present"
    );
    let mut names: Vec<&str> = rows("cand").iter().map(|t| t[0]).collect();
    names.sort_unstable();
    assert_eq!(names.len(), 33);
    for needed in [
        "none",
        "dup",
        "raster_first",
        "nosegs_first",
        "single_seg_first",
        "zero_then_line",
        "missing_handle",
        "circle",
        "poly4c",
        "fz7",
    ] {
        assert!(names.contains(&needed), "{needed}");
    }
    let path = rows("pth");
    assert!(path.iter().any(|t| t[1] == "path" && t[3] == "1") && path.iter().any(|t| t[3] == "0"));
    assert!(infos().len() >= 25, "candidates that resolve");
}
