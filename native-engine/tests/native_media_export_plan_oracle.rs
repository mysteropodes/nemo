//! Baseline oracle rows: the shipped `exportFrameRange`, `exportRenderPNGsToDir`,
//! `exportSvgSequenceJob` frame names, `String(fps)` and `exportMP4ToPath`
//! (fbe4b8a3 `src/js/export.js`), evaluated in a Node vm by `tools/gen.js`.
//! Each `*_rows_hold` takes the function under test, so the negative controls
//! can run the same oracle against a deliberately wrong one.
use super::tests::{output, pins};
use super::*;
use crate::export_job::native_media_export::native_media_export_video::js_number_text;

const ORACLE: &str = include_str!("fixtures/native-media-export-plan/oracle.txt");
/// The work area the V rows were generated with (`state.waIn` / `state.waOut`).
pub(super) const V_AREA: WorkArea = WorkArea { start: 0, end: 47 };

pub(super) fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => panic!("dangling escape in oracle row"),
        }
    }
    out
}

pub(super) fn rows(op: &str) -> Vec<Vec<String>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == op)
        .map(|cols| cols[1..].iter().map(|c| unescape(c)).collect())
        .collect()
}

pub(super) fn bits(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).unwrap())
}

fn optional(token: &str) -> Option<u32> {
    (token != "-").then(|| token.parse().unwrap())
}

/// R rows: `f(request, work_area) -> (start, end)` must equal `exportFrameRange`.
pub(super) fn range_rows_hold(f: impl Fn(RangeRequest, WorkArea) -> (u32, u32)) -> bool {
    rows("R").iter().all(|row| {
        let request = if row[0] == "n" {
            RangeRequest::default()
        } else {
            RangeRequest {
                start: optional(&row[1]),
                end: optional(&row[2]),
            }
        };
        let area = WorkArea {
            start: row[3].parse().unwrap(),
            end: row[4].parse().unwrap(),
        };
        f(request, area) == (row[5].parse().unwrap(), row[6].parse().unwrap())
    })
}

/// P rows: `f(start, end) -> names` must equal the files `exportRenderPNGsToDir` wrote
/// (none for an empty range), and the progress text must be `i/count`.
pub(super) fn sequence_rows_hold(f: impl Fn(u32, u32) -> Vec<String>) -> bool {
    rows("P").iter().all(|row| {
        let (start, end): (u32, u32) = (row[0].parse().unwrap(), row[1].parse().unwrap());
        let count: usize = row[2].parse().unwrap();
        let names = f(start, end);
        let progress: Vec<String> = (1..=count).map(|i| format!("{i}/{count}")).collect();
        names.len() == count && names.join(",") == row[3] && progress.join(",") == row[4]
    })
}

/// F rows: `f(x)` must equal JS `String(x)`.
pub(super) fn number_rows_hold(f: impl Fn(f64) -> Option<String>) -> bool {
    rows("F")
        .iter()
        .all(|row| f(bits(&row[0])).as_deref() == Some(row[1].as_str()))
}

/// The ffmpeg argv a V row's inputs produce through the real plan, or `None`
/// when the plan refuses them.
pub(super) fn real_argv(row: &[String]) -> Option<Vec<String>> {
    let requested = match row[0].as_str() {
        "u" => None,
        "n" => Some(f64::NAN),
        token => Some(bits(token.strip_prefix("h:").unwrap())),
    };
    let quality = match row[1].as_str() {
        "u" => None,
        token => Some(token.strip_prefix("s:").unwrap().to_owned()),
    };
    let spec = VideoSpec::resolve(requested, bits(&row[2]), quality.as_deref()).ok()?;
    let request = RangeRequest {
        start: optional(&row[3]),
        end: optional(&row[4]),
    };
    let range = FrameRange::resolve(request, V_AREA).ok()?;
    assert_eq!(
        (range.start().to_string(), range.end().to_string()),
        (row[9].clone(), row[10].clone()),
        "the plan range is the range the baseline rendered"
    );
    let plan = MediaExportPlan::new(
        OutputKind::Video(spec),
        FrameFormat::Png,
        pins(),
        output(320, 180),
        range,
        DEFAULT_BATCH_FRAMES,
    )
    .ok()?;
    plan.mp4_ffmpeg_args(&row[6], &row[5]).ok()
}

/// V rows: `f(row)` must reproduce the baseline argv (class S), the baseline
/// argv with `65` in the `-q:v` slot (class X: the baseline put a function
/// there) or be a refusal (class R: the baseline fed ffmpeg a non-positive or
/// non-finite rate text).
pub(super) fn argv_rows_hold(f: impl Fn(&[String]) -> Option<Vec<String>>) -> bool {
    rows("V").iter().all(|row| {
        let expected: Vec<String> = row[8].split('|').map(str::to_owned).collect();
        match (row[7].as_str(), f(row)) {
            ("R", got) => got.is_none(),
            ("S", Some(got)) => got == expected,
            ("X", Some(got)) => {
                let fixed: Vec<String> = expected
                    .iter()
                    .map(|a| {
                        if a == "<non-string>" {
                            "65".to_owned()
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                got == fixed
            }
            _ => false,
        }
    })
}

pub(super) fn real_names(start: u32, end: u32) -> Vec<String> {
    match FrameRange::new(start, end) {
        Err(_) => Vec::new(),
        Ok(range) => {
            let plan = MediaExportPlan::new(
                OutputKind::Sequence,
                FrameFormat::Png,
                pins(),
                output(320, 180),
                range,
                DEFAULT_BATCH_FRAMES,
            )
            .unwrap();
            (1..=plan.frame_count())
                .map(|i| plan.entry(i).unwrap().name)
                .collect()
        }
    }
}

#[test]
fn range_rows_match_export_frame_range() {
    assert_eq!(rows("R").len(), 104);
    assert!(range_rows_hold(frame_range_raw));
}

#[test]
fn sequence_rows_match_the_files_export_render_pngs_to_dir_wrote() {
    assert_eq!(rows("P").len(), 7);
    assert!(sequence_rows_hold(real_names));
    // The 7..6 row wrote nothing; the plan refuses it as a typed empty range.
    assert_eq!(
        FrameRange::new(7, 6),
        Err(PlanError::EmptyRange { start: 7, end: 6 })
    );
    let rolled = rows("P").into_iter().find(|r| r[0] == "9998").unwrap();
    assert_eq!(
        rolled[3],
        "frame_0001.png,frame_0002.png,frame_0003.png,frame_0004.png"
    );
}

#[test]
fn svg_sequence_name_rows_match_the_baseline_frame_name_callback() {
    let rows = rows("S");
    assert_eq!(rows.len(), 11);
    for row in &rows {
        let ordinal: u64 = row[0].parse().unwrap();
        assert_eq!(frame_file_name(ordinal, FrameFormat::Svg), row[1]);
    }
    assert!(rows.iter().any(|r| r[1] == "frame_10000.svg"));
}

#[test]
fn number_rows_pin_js_string_including_exponent_forms() {
    assert_eq!(rows("F").len(), 34);
    assert!(number_rows_hold(js_number_text));
}

#[test]
fn mp4_rows_match_export_mp4_to_path() {
    let all = rows("V");
    assert_eq!(all.len(), 176);
    let count = |class: &str| all.iter().filter(|r| r[7] == class).count();
    assert_eq!((count("S"), count("X"), count("R")), (100, 38, 38));
    assert!(argv_rows_hold(real_argv));
}
