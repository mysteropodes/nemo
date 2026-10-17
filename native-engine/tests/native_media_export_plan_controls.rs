//! Negative and mutation controls for the plan kernel. Each control runs the
//! SAME oracle checker the positive tests use against a deliberately wrong
//! implementation, or splices one mistake into the REAL plan output, and must
//! be caught.
use super::oracle::{
    argv_rows_hold, bits, number_rows_hold, range_rows_hold, real_argv, real_names, rows,
    sequence_rows_hold, V_AREA,
};
use super::tests::sequence;
use super::*;
use crate::export_job::native_media_export::native_media_export_video::js_number_text;

/// Run the real V-row path, then let the mutation edit the result.
fn edited_argv(edit: impl Fn(&mut Vec<String>)) -> impl Fn(&[String]) -> Option<Vec<String>> {
    move |row| {
        real_argv(row).map(|mut args| {
            edit(&mut args);
            args
        })
    }
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert!(range_rows_hold(frame_range_raw));
    assert!(sequence_rows_hold(real_names));
    assert!(number_rows_hold(js_number_text));
    assert!(argv_rows_hold(real_argv));
    assert!(argv_rows_hold(edited_argv(|_| {})));
}

#[test]
fn mutation_zero_start_treated_as_unset_is_caught() {
    let mutant = |request: RangeRequest, area: WorkArea| {
        let start = request.start.filter(|s| *s != 0).unwrap_or(area.start);
        (start, request.end.unwrap_or(area.end))
    };
    assert!(!range_rows_hold(mutant));
}

#[test]
fn mutation_end_defaults_to_the_work_area_start_is_caught() {
    let mutant = |request: RangeRequest, area: WorkArea| {
        (
            request.start.unwrap_or(area.start),
            request.end.unwrap_or(area.start),
        )
    };
    assert!(!range_rows_hold(mutant));
}

#[test]
fn mutation_names_zero_based_padded_short_or_truncated_are_caught() {
    let zero_based = |start: u32, end: u32| {
        real_names(start, end)
            .iter()
            .enumerate()
            .map(|(i, _)| format!("frame_{:04}.png", i))
            .collect::<Vec<_>>()
    };
    assert!(!sequence_rows_hold(zero_based));
    let pad3 = |start: u32, end: u32| {
        real_names(start, end)
            .iter()
            .enumerate()
            .map(|(i, _)| format!("frame_{:03}.png", i + 1))
            .collect::<Vec<_>>()
    };
    assert!(!sequence_rows_hold(pad3));
    let dropped_last = |start: u32, end: u32| {
        let mut names = real_names(start, end);
        names.pop();
        names
    };
    assert!(!sequence_rows_hold(dropped_last));
    let source_numbered = |start: u32, end: u32| {
        real_names(start, end)
            .iter()
            .enumerate()
            .map(|(i, _)| format!("frame_{:04}.png", start as usize + i + 1))
            .collect::<Vec<_>>()
    };
    assert!(!sequence_rows_hold(source_numbered));
}

#[test]
fn mutation_rust_display_for_the_frame_rate_text_is_caught() {
    let display = |x: f64| Some(format!("{x}"));
    assert!(!number_rows_hold(display));
    // Rust Display agrees with JS only where JS prints a positional number.
    let agree = rows("F")
        .iter()
        .filter(|r| format!("{}", bits(&r[0])) == r[1])
        .count();
    assert!(agree > 0 && agree < rows("F").len());
}

#[test]
fn mutation_fps_fallback_only_on_missing_is_caught() {
    let mutant = |row: &[String]| {
        let requested = match row[0].as_str() {
            "u" => None,
            "n" => Some(f64::NAN),
            token => Some(bits(token.strip_prefix("h:").unwrap())),
        };
        // `unwrap_or` instead of the JS `||`: 0 and NaN no longer fall back.
        let fps = requested.unwrap_or_else(|| bits(&row[2]));
        if !(fps.is_finite() && fps > 0.0) {
            return None;
        }
        let mut args = real_argv(row)?;
        args[2] = js_number_text(fps)?;
        Some(args)
    };
    assert!(!argv_rows_hold(mutant));
}

#[test]
fn mutation_quality_table_and_defaults_are_caught() {
    assert!(!argv_rows_hold(edited_argv(|a| {
        if a[10] == "65" {
            a[10] = "50".into();
        }
    })));
    assert!(!argv_rows_hold(edited_argv(|a| {
        if a[10] == "35" {
            a[10] = "30".into();
        }
    })));
    assert!(!argv_rows_hold(edited_argv(|a| {
        if a[10] == "80" {
            a[10] = "85".into();
        }
    })));
}

type ArgvEdit = Box<dyn Fn(&mut Vec<String>)>;

fn argv_edits() -> Vec<(&'static str, ArgvEdit)> {
    let mut edits: Vec<(&'static str, ArgvEdit)> = Vec::new();
    edits.push((
        "dropped -y",
        Box::new(|a: &mut Vec<String>| {
            a.remove(0);
        }),
    ));
    edits.push((
        "framerate and input swapped",
        Box::new(|a: &mut Vec<String>| a.swap(1, 3)),
    ));
    edits.push((
        "software encoder",
        Box::new(|a: &mut Vec<String>| a[6] = "libx264".into()),
    ));
    edits.push((
        "pixel format",
        Box::new(|a: &mut Vec<String>| a[8] = "yuv444p".into()),
    ));
    edits.push((
        "three digit pattern",
        Box::new(|a: &mut Vec<String>| a[4] = a[4].replace("%04d", "%03d")),
    ));
    edits.push((
        "profile dropped",
        Box::new(|a: &mut Vec<String>| {
            a.truncate(11);
            a.push("/dropped.mp4".into());
        }),
    ));
    edits.push((
        "output missing",
        Box::new(|a: &mut Vec<String>| {
            a.pop();
        }),
    ));
    edits.push((
        "start number added",
        Box::new(|a: &mut Vec<String>| {
            a.insert(1, "-start_number".into());
            a.insert(2, "1".into());
        }),
    ));
    edits
}

#[test]
fn mutation_argv_edits_are_caught() {
    for (label, edit) in argv_edits() {
        assert!(
            !argv_rows_hold(edited_argv(edit)),
            "mutation `{label}` must be caught"
        );
    }
}

#[test]
fn mutation_swapped_work_area_cannot_satisfy_the_baseline_render_range() {
    // The V rows pin the range the baseline rendered (columns 9 and 10); a plan
    // resolved against a swapped work area cannot reproduce it.
    let swapped = WorkArea {
        start: V_AREA.end,
        end: V_AREA.start,
    };
    let reproduces = |area: WorkArea| {
        rows("V").iter().all(|row| {
            let request = RangeRequest {
                start: (row[3] != "-").then(|| row[3].parse().unwrap()),
                end: (row[4] != "-").then(|| row[4].parse().unwrap()),
            };
            FrameRange::resolve(request, area)
                .map(|r| {
                    (r.start().to_string(), r.end().to_string())
                        == (row[9].clone(), row[10].clone())
                })
                .unwrap_or(false)
        })
    };
    assert!(reproduces(V_AREA));
    assert!(!reproduces(swapped));
}

#[test]
fn mutation_truncating_to_the_host_bound_is_caught() {
    let plan = sequence(0, 69);
    let covers = |batches: &[BatchPlan]| {
        let frames: Vec<u32> = batches.iter().flat_map(|b| b.frames.clone()).collect();
        frames == (0..=69).collect::<Vec<u32>>()
    };
    let all: Vec<BatchPlan> = (0..plan.batch_count())
        .map(|i| plan.batch(i).unwrap())
        .collect();
    assert!(covers(&all));
    assert!(
        !covers(&all[..1]),
        "keeping only the first host job loses frames"
    );
    let mut dropped_tail = all.clone();
    dropped_tail[2].frames.pop();
    assert!(!covers(&dropped_tail));
}
