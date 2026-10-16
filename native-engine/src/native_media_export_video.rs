//! Video half of the media export plan (G44): the resolved `exportMP4ToPath`
//! options, the ffmpeg argument vector and the JS number text of the frame rate.
//!
//! Mirrors `src/js/export.js` at fbe4b8a3: `exportMP4ToPath` and the
//! `MP4_QUALITY_QV` table. The baseline `||` fallbacks are kept (a missing,
//! `0` or `NaN` frame rate uses the project rate, a missing or empty quality
//! uses `high`, an unknown quality uses `65`). Deliberate typed refusals instead
//! of ffmpeg garbage: a non-positive or non-finite resolved rate and an empty
//! directory or output path. A quality that is a JS prototype member
//! (`constructor`) is simply unknown here; the baseline put a function in argv.

use crate::export_job::native_media_export::native_media_export_plan::PlanError;

const DEFAULT_QUALITY_KEY: &str = "high";
const UNKNOWN_QUALITY_QV: &str = "65";
const MP4_QUALITY_QV: [(&str, &str); 4] = [
    ("draft", "35"),
    ("medium", "50"),
    ("high", "65"),
    ("best", "80"),
];

/// `exportMP4ToPath` options, resolved: the frame rate text ffmpeg receives
/// (JS `String(fps)`) and the `-q:v` value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VideoSpec {
    fps: f64,
    fps_text: String,
    quality_qv: &'static str,
}

impl VideoSpec {
    /// `fps = opts.fps || state.fps`, `qv = MP4_QUALITY_QV[opts.quality || 'high'] || '65'`.
    pub(crate) fn resolve(
        requested_fps: Option<f64>,
        project_fps: f64,
        quality: Option<&str>,
    ) -> Result<Self, PlanError> {
        let fps = match requested_fps {
            Some(value) if value != 0.0 && !value.is_nan() => value,
            _ => project_fps,
        };
        if !(fps.is_finite() && fps > 0.0) {
            return Err(PlanError::InvalidFps);
        }
        let fps_text = js_number_text(fps).ok_or(PlanError::InvalidFps)?;
        let key = match quality {
            Some(text) if !text.is_empty() => text,
            _ => DEFAULT_QUALITY_KEY,
        };
        let quality_qv = MP4_QUALITY_QV
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, qv)| *qv)
            .unwrap_or(UNKNOWN_QUALITY_QV);
        Ok(Self {
            fps,
            fps_text,
            quality_qv,
        })
    }

    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }
    pub(crate) fn fps_text(&self) -> &str {
        &self.fps_text
    }
    pub(crate) fn quality_qv(&self) -> &'static str {
        self.quality_qv
    }
}

/// The `exportMP4ToPath` ffmpeg argument vector, in the baseline order.
/// `staged_dir` and `out_path` are joined verbatim (no normalisation).
pub(crate) fn mp4_ffmpeg_args(
    spec: &VideoSpec,
    staged_dir: &str,
    out_path: &str,
) -> Result<Vec<String>, PlanError> {
    if staged_dir.is_empty() {
        return Err(PlanError::EmptyPath("staged directory"));
    }
    if out_path.is_empty() {
        return Err(PlanError::EmptyPath("output path"));
    }
    let pattern = format!("{staged_dir}/frame_%04d.png");
    let parts: [&str; 13] = [
        "-y",
        "-framerate",
        spec.fps_text(),
        "-i",
        pattern.as_str(),
        "-c:v",
        "h264_videotoolbox",
        "-pix_fmt",
        "yuv420p",
        "-q:v",
        spec.quality_qv(),
        "-profile:v",
        "high",
    ];
    let mut args: Vec<String> = parts.iter().map(|part| (*part).to_owned()).collect();
    args.push(out_path.to_owned());
    Ok(args)
}

/// ECMAScript `Number::toString(x)` for finite `x` (the text JS `String(fps)`
/// produces): positional below 1e21 and from 1e-6, exponent form outside.
pub(crate) fn js_number_text(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some("0".to_owned());
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let power = n - 1;
        let power_sign = if power < 0 { '-' } else { '+' };
        if k == 1 {
            format!("{digits}e{power_sign}{}", power.abs())
        } else {
            format!(
                "{}.{}e{power_sign}{}",
                &digits[..1],
                &digits[1..],
                power.abs()
            )
        }
    };
    Some(format!("{sign}{body}"))
}
