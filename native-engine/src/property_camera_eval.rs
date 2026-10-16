//! Camera track evaluation kernel (K-CAM), pure and I/O free.
//!
//! Mirrors `src/js/camera.js` at 1f663ac3: `DEFAULT_EASE`, `bezierEase`,
//! `snap`, `cameraAtFrameFor` and `cameraMatrixAtFrame`, plus the Paper.js
//! 0.12.18 `Matrix` append semantics (`translate`, `scale(s, center)`,
//! `rotate(deg, center)`) they rely on. Arithmetic keeps the JS operand
//! order, so results are bit-identical (NaN payloads aside) wherever the
//! platform `sin`/`cos` agree with V8's; the `Math.max`/`min` NaN and `-0`
//! cases are mirrored but no oracle row reaches them.
//!
//! Camera semantics kept on purpose:
//! - keys are used as given: never sorted, duplicates allowed. The first and
//!   last array entries are the clamps, and the first matching segment wins;
//! - `frame <= first` snaps to the first key, `frame >= last` to the last
//!   (so with duplicate trailing frames the last wins, leading ones the
//!   first). A zero-length segment is never interpolated;
//! - only the departing key's `ease` is used (default ease-in-out); `h_out`
//!   comes from the departing key, `h_in` from the arriving one, `[0,0]`
//!   when absent; x/y follow that cubic, `w` and `rot` lerp on the eased `t`;
//! - a missing, `0` or `NaN` `rot` reads as `0` (`k.rot || 0`); the roll
//!   `rotate` is skipped when the pose rot is falsy (`0`, `-0`, `NaN`);
//! - `bezierEase`: `t<=0` is 0 and `t>=1` is 1; up to 8 Newton steps (exit on
//!   `|dbx|<1e-6`, return on `|err|<1e-9`), then 24 bisection steps;
//! - `w == 0` gives an infinite scale and non-finite matrix entries; they are
//!   returned, not rejected. No keys (`None` or empty) give `None`.
//!
//! Not provided: the Paper `Layer` path (`applyToExportLayer`), `ensureState`,
//! key editing and the viewport rect helpers.

/// One camera key (`state.cameraKeys[i]`); absent JS fields are `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraKey {
    pub(crate) frame: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) rot: Option<f64>,
    pub(crate) h_out: Option<[f64; 2]>,
    pub(crate) h_in: Option<[f64; 2]>,
    pub(crate) ease: Option<[f64; 4]>,
}

/// The `{x, y, w, rot}` pose `cameraAtFrameFor` returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraPose {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) rot: f64,
}

/// `DEFAULT_EASE`: easeInOut.
pub(crate) const DEFAULT_EASE: [f64; 4] = [0.42, 0.0, 0.58, 1.0];

/// JS `Math.max(a, b)` (NaN propagates, `+0` beats `-0`).
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            b
        } else {
            a
        }
    } else if a > b {
        a
    } else {
        b
    }
}

/// JS `Math.min(a, b)` (NaN propagates, `-0` beats `+0`).
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}

/// JS `x || 0` for a number.
fn or_zero(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        0.0
    } else {
        x
    }
}

/// `bezierEase(t, x1, y1, x2, y2)`: y of the CSS cubic-bezier at x = `t`.
pub(crate) fn bezier_ease(t: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let bx = |u: f64| {
        let v = 1.0 - u;
        3.0 * v * v * u * x1 + 3.0 * v * u * u * x2 + u * u * u
    };
    let by = |u: f64| {
        let v = 1.0 - u;
        3.0 * v * v * u * y1 + 3.0 * v * u * u * y2 + u * u * u
    };
    let dbx = |u: f64| {
        let v = 1.0 - u;
        3.0 * v * v * x1 + 6.0 * v * u * (x2 - x1) + 3.0 * u * u * (1.0 - x2)
    };
    let mut u = t;
    for _ in 0..8 {
        let d = dbx(u);
        if d.abs() < 1e-6 {
            break;
        }
        let err = bx(u) - t;
        if err.abs() < 1e-9 {
            return by(u);
        }
        u = js_max(0.0, js_min(1.0, u - err / d));
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        u = (lo + hi) / 2.0;
        if bx(u) < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    by(u)
}

/// `snap(k)`.
fn snap(k: &CameraKey) -> CameraPose {
    CameraPose {
        x: k.x,
        y: k.y,
        w: k.w,
        rot: or_zero(k.rot.unwrap_or(0.0)),
    }
}

/// `cameraAtFrameFor(ks, frame)`; `None` for no keys (`null`/`undefined`/`[]`).
pub(crate) fn camera_at_frame(keys: Option<&[CameraKey]>, frame: f64) -> Option<CameraPose> {
    let ks = keys.filter(|ks| !ks.is_empty())?;
    if frame <= ks[0].frame {
        return Some(snap(&ks[0]));
    }
    let last = &ks[ks.len() - 1];
    if frame >= last.frame {
        return Some(snap(last));
    }
    for pair in ks.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if frame >= a.frame && frame < b.frame {
            let t = (frame - a.frame) / (b.frame - a.frame);
            let e = a.ease.unwrap_or(DEFAULT_EASE);
            let y = bezier_ease(t, e[0], e[1], e[2], e[3]);
            let ho = a.h_out.unwrap_or([0.0, 0.0]);
            let hi = b.h_in.unwrap_or([0.0, 0.0]);
            let (p1x, p1y) = (a.x + ho[0], a.y + ho[1]);
            let (p2x, p2y) = (b.x + hi[0], b.y + hi[1]);
            let v = 1.0 - y;
            let px =
                v * v * v * a.x + 3.0 * v * v * y * p1x + 3.0 * v * y * y * p2x + y * y * y * b.x;
            let py =
                v * v * v * a.y + 3.0 * v * v * y * p1y + 3.0 * v * y * y * p2y + y * y * y * b.y;
            let (ar, br) = (or_zero(a.rot.unwrap_or(0.0)), or_zero(b.rot.unwrap_or(0.0)));
            return Some(CameraPose {
                x: px,
                y: py,
                w: a.w + (b.w - a.w) * y,
                rot: ar + (br - ar) * y,
            });
        }
    }
    Some(snap(last))
}

/// Paper.js 0.12.18 `Matrix` (`a b c d tx ty`) with append semantics.
struct Matrix([f64; 6]);

impl Matrix {
    fn translate(&mut self, x: f64, y: f64) {
        let m = &mut self.0;
        m[4] += x * m[0] + y * m[2];
        m[5] += x * m[1] + y * m[3];
    }

    fn scale(&mut self, s: f64, cx: f64, cy: f64) {
        self.translate(cx, cy);
        for v in &mut self.0[..4] {
            *v *= s;
        }
        self.translate(-cx, -cy);
    }

    fn rotate(&mut self, degrees: f64, x: f64, y: f64) {
        let angle = degrees * (std::f64::consts::PI / 180.0);
        let (cos, sin) = (angle.cos(), angle.sin());
        let tx = x - x * cos + y * sin;
        let ty = y - x * sin - y * cos;
        let [a, b, c, d, ..] = self.0;
        self.0[0] = cos * a + sin * c;
        self.0[1] = cos * b + sin * d;
        self.0[2] = -sin * a + cos * c;
        self.0[3] = -sin * b + cos * d;
        self.0[4] += tx * a + ty * c;
        self.0[5] += tx * b + ty * d;
    }
}

/// `cameraMatrixAtFrame(ks, frameIdx, canvasW, canvasH)` as `[a,b,c,d,tx,ty]`.
pub(crate) fn camera_matrix_at_frame(
    keys: Option<&[CameraKey]>,
    frame: f64,
    canvas_w: f64,
    canvas_h: f64,
) -> Option<[f64; 6]> {
    let cam = camera_at_frame(keys, frame)?;
    let s = canvas_w / cam.w;
    let mut m = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    m.translate(canvas_w / 2.0 - cam.x, canvas_h / 2.0 - cam.y);
    m.scale(s, cam.x, cam.y);
    if !(cam.rot == 0.0 || cam.rot.is_nan()) {
        m.rotate(-cam.rot, cam.x, cam.y);
    }
    Some(m.0)
}

#[cfg(test)]
#[path = "../tests/property_camera_eval.rs"]
mod tests;
