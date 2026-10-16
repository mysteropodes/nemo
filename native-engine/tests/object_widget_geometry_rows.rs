//! Oracle row parsing and comparison harness for the rig-widget geometry kernel (L03-B,
//! F19). Production and the mutants in `_controls` run through the same row checks.
//!
//! `fixtures/object-widget-geometry/oracle.txt` (`ORACLE`) is produced by
//! `tools/gen.cjs` (kept outside the repo; sha256 recorded in the leaf HANDOFF) from
//! `src/js/rig-widget.js` at fbe4b8a3: the widget functions are cut VERBATIM out of that
//! file and executed in a Node `vm` against a stubbed `window`/`state`/`SMMotion` that only
//! delivers the already composed anchor and angle, a recorded `valueAtFrame` and a recorded
//! `setLayerValue`. There is no existing golden suite for the widget; this oracle is the
//! executed baseline.
//!
//! Tokens (space separated rows, f64 as 16 hex digits, `-` none, `_` empty). A CASE is
//! `kind/size/anchor/angle/x/y/frame` (`kind` `~` is the empty string, `size` `-` is absent,
//! points `x,y`, a space in `kind` is `%20`); an AXIS is `key,min,max,rest` with key `-` null and `~` empty, rest `-`
//! absent and `n` null (a lone `-` is a null axis); a SAMPLE is `u` undefined, `n` null, `e`
//! empty, `v<hex>` one value, `m<hex>,<hex>` two values, `P` no sampling provider.
//! Rows: `c01 t out`, `spn min max out`, `vfl min max t out`, `geo case w,h,cx,cy,cos,sin|bad`,
//! `tw|tl case in out`, `rl case out`, `pk case xSample ySample out log`, `wfl case lx,ly writes`,
//! `axv axis sample frame out log` (`log` is the recorded sampler calls `key@frame;...`).
//!
//! Tolerance: every row is compared bit for bit (`-0` included) except values that pass
//! through `cos`/`sin` of a non-zero angle (`geo` cos/sin, `tw`, `tl`), which may differ by
//! `TRIG_ULPS` units of roundoff of the group's input scale (macOS libm differs from V8 by
//! an ulp).

use std::cell::RefCell;

use super::{
    geom_for, AxisRest, AxisSample, AxisSlot, AxisSpec, AxisUnavailable, AxisValueSource,
    Placement, WidgetGeom, WidgetGeometryError, WidgetKind, WidgetSpec, WidgetWrites,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-widget-geometry/oracle.txt");
const TRIG_ULPS: f64 = 16.0;

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

pub(crate) fn same_pts(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(*x, *y))
}

/// Bit-equal, or within `TRIG_ULPS` of `scale` (at least 1) and of the values themselves.
fn near(a: &[f64], b: &[f64], scale: f64) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            same(*x, *y)
                || (x.is_finite()
                    && y.is_finite()
                    && (x - y).abs()
                        <= TRIG_ULPS * f64::EPSILON * scale.max(1.0).max(x.abs()).max(y.abs()))
        })
}

pub(crate) fn point(tok: &str) -> [f64; 2] {
    let (x, y) = tok.split_once(',').expect("point");
    [hex(x), hex(y)]
}

/// One parsed CASE: the widget as the baseline stored it plus its composed placement.
pub(crate) struct Case {
    pub(crate) kind: Result<WidgetKind, WidgetGeometryError>,
    pub(crate) spec: WidgetSpec,
    pub(crate) placement: Placement,
    pub(crate) frame: f64,
}

pub(crate) fn axis(tok: &str) -> Option<AxisSpec> {
    if tok == "-" {
        return None;
    }
    let f: Vec<&str> = tok.split(',').collect();
    assert_eq!(f.len(), 4, "axis token {tok}");
    let key = match f[0] {
        "-" => None,
        "~" => Some(String::new()),
        k => Some(k.to_string()),
    };
    let rest = match f[3] {
        "-" => AxisRest::Absent,
        "n" => AxisRest::Null,
        h => AxisRest::Value(hex(h)),
    };
    Some(AxisSpec {
        key,
        min: hex(f[1]),
        max: hex(f[2]),
        rest,
    })
}

pub(crate) fn case(tok: &str) -> Case {
    let f: Vec<&str> = tok.split('/').collect();
    assert_eq!(f.len(), 7, "case token {tok}");
    let kind_text = if f[0] == "~" {
        String::new()
    } else {
        f[0].replace("%20", " ")
    };
    let kind = WidgetKind::parse(&kind_text);
    let size = if f[1] == "-" { None } else { Some(point(f[1])) };
    let spec = WidgetSpec {
        kind: *kind.as_ref().unwrap_or(&WidgetKind::Joystick),
        size,
        x: axis(f[4]).expect("every case has an x axis"),
        y: axis(f[5]),
    };
    Case {
        kind,
        spec,
        placement: Placement {
            anchor: point(f[2]),
            angle_degrees: hex(f[3]),
        },
        frame: hex(f[6]),
    }
}

pub(crate) fn geom_of(c: &Case) -> WidgetGeom {
    geom_for(&c.spec, &c.placement, c.frame)
}

fn sample(tok: &str) -> AxisSample {
    match tok.as_bytes()[0] {
        b'P' => AxisSample::ProviderMissing,
        b'u' | b'n' | b'-' => AxisSample::Absent,
        b'e' => AxisSample::Values(Vec::new()),
        b'v' => AxisSample::Values(vec![hex(&tok[1..])]),
        b'm' => AxisSample::Values(tok[1..].split(',').map(hex).collect()),
        _ => panic!("sample token {tok}"),
    }
}

/// First element of a sample token (the value the baseline reads), 0 when there is none.
fn first_value(tok: &str) -> f64 {
    match sample(tok) {
        AxisSample::Values(v) => v.first().copied().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The reason a sample token / axis combination has no value, from the contract alone.
fn unavailable(axis: Option<&AxisSpec>, tok: &str) -> Option<AxisUnavailable> {
    let Some(axis) = axis else {
        return Some(AxisUnavailable::NoAxis);
    };
    if axis.key.as_deref().map_or(true, str::is_empty) {
        return Some(AxisUnavailable::NoKey);
    }
    match tok.as_bytes()[0] {
        b'P' => Some(AxisUnavailable::ProviderMissing),
        b'u' | b'n' | b'-' => Some(AxisUnavailable::ControlAbsent),
        b'e' => Some(AxisUnavailable::EmptySample),
        _ => None,
    }
}

/// Recording stand-in for the sampler port: answers from a key table and logs each call.
pub(crate) struct TableSource {
    entries: Vec<(String, AxisSample)>,
    provider: bool,
    log: RefCell<Vec<String>>,
}

impl TableSource {
    pub(crate) fn new(entries: Vec<(String, AxisSample)>, provider: bool) -> TableSource {
        TableSource {
            entries,
            provider,
            log: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn log_token(&self) -> String {
        let log = self.log.borrow();
        if log.is_empty() {
            "_".to_string()
        } else {
            log.join(";")
        }
    }
}

impl AxisValueSource for TableSource {
    fn sample(&self, key: &str, frame: f64) -> AxisSample {
        if !self.provider {
            return AxisSample::ProviderMissing;
        }
        self.log
            .borrow_mut()
            .push(format!("{key}@{:016x}", frame.to_bits()));
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map_or(AxisSample::Absent, |(_, s)| s.clone())
    }
}

fn entries(pairs: &[(Option<&AxisSpec>, &str)]) -> Vec<(String, AxisSample)> {
    let mut out = Vec::new();
    for (axis, tok) in pairs {
        if let Some(key) = axis.and_then(|a| a.key.as_deref()) {
            if !key.is_empty() && *tok != "-" {
                out.push((key.to_string(), sample(tok)));
            }
        }
    }
    out
}

pub(crate) fn rows(op: &'static str) -> impl Iterator<Item = (usize, Vec<&'static str>)> {
    ORACLE
        .lines()
        .enumerate()
        .filter(move |(_, l)| l.starts_with(op) && l.as_bytes().get(op.len()) == Some(&b' '))
        .map(|(n, l)| (n + 1, l.split(' ').collect::<Vec<&'static str>>()))
}

fn label(line: usize, t: &[&str]) -> String {
    let head: String = t[1].chars().take(60).collect();
    format!("line {line}: {} {head}", t[0])
}

/// `(rows seen, labels of rows the function got wrong)`.
pub(crate) type Run = (usize, Vec<String>);
pub(crate) type UnaryFn<'a> = &'a dyn Fn(f64) -> f64;
pub(crate) type SpanFn<'a> = &'a dyn Fn(f64, f64) -> f64;
pub(crate) type ValueFn<'a> = &'a dyn Fn(&AxisSpec, f64) -> f64;
pub(crate) type GeomFn<'a> = &'a dyn Fn(&WidgetSpec, &Placement, f64) -> WidgetGeom;
pub(crate) type MapFn<'a> = &'a dyn Fn(&WidgetGeom, f64, f64) -> [f64; 2];
pub(crate) type RestFn<'a> = &'a dyn Fn(&WidgetSpec, &WidgetGeom) -> [f64; 2];
pub(crate) type LocalFn<'a> = &'a dyn Fn(&WidgetSpec, &WidgetGeom, f64, f64) -> [f64; 2];
pub(crate) type PuckFn<'a> = &'a dyn Fn(
    &WidgetSpec,
    &WidgetGeom,
    &dyn AxisValueSource,
) -> Result<[f64; 2], WidgetGeometryError>;
pub(crate) type WriteFn<'a> =
    &'a dyn Fn(&WidgetSpec, &WidgetGeom, f64, f64) -> Result<WidgetWrites, WidgetGeometryError>;
pub(crate) type AxisFn<'a> =
    &'a dyn Fn(Option<&AxisSpec>, f64, &dyn AxisValueSource) -> Result<f64, AxisUnavailable>;

pub(crate) fn c01_run(f: UnaryFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("c01") {
        n += 1;
        if !same(f(hex(t[1])), hex(t[2])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn span_run(f: SpanFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("spn") {
        n += 1;
        if !same(f(hex(t[1]), hex(t[2])), hex(t[3])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn value_run(f: ValueFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("vfl") {
        n += 1;
        let ax = AxisSpec {
            key: Some("k".to_string()),
            min: hex(t[1]),
            max: hex(t[2]),
            rest: AxisRest::Absent,
        };
        if !same(f(&ax, hex(t[3])), hex(t[4])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn geo_run(f: GeomFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("geo") {
        n += 1;
        let c = case(t[1]);
        let ok = if t[2] == "bad" {
            c.kind == Err(WidgetGeometryError::InvalidKind)
        } else {
            let want: Vec<f64> = t[2].split(',').map(hex).collect();
            let g = f(&c.spec, &c.placement, c.frame);
            let exact = [g.w, g.h, g.cx, g.cy];
            let trig = [g.cos, g.sin];
            c.kind.is_ok()
                && same_pts(&exact, &want[..4])
                && if c.placement.angle_degrees == 0.0 {
                    same_pts(&trig, &want[4..])
                } else {
                    near(&trig, &want[4..], 1.0)
                }
        };
        if !ok {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

/// `tw` (local to world) or `tl` (world to local) rows through `f`.
pub(crate) fn map_run(op: &'static str, f: MapFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows(op) {
        n += 1;
        let c = case(t[1]);
        let (input, want) = (point(t[2]), point(t[3]));
        let got = f(&geom_of(&c), input[0], input[1]);
        let scale = input[0].abs().max(input[1].abs());
        let ok = if c.placement.angle_degrees == 0.0 {
            same_pts(&got, &want)
        } else {
            near(&got, &want, scale)
        };
        if !ok {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn rest_run(f: RestFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("rl") {
        n += 1;
        let c = case(t[1]);
        if !same_pts(&f(&c.spec, &geom_of(&c)), &point(t[2])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

/// `pk` rows. `puck` is the port-driven function; `local` is the explicit pure core that
/// the baseline's silent `0` for an unavailable axis is routed through.
pub(crate) fn puck_run(puck: PuckFn, local: LocalFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("pk") {
        n += 1;
        let c = case(t[1]);
        let g = geom_of(&c);
        let y_read = if c.spec.kind == WidgetKind::Joystick {
            c.spec.y.as_ref()
        } else {
            None
        };
        let src = TableSource::new(
            entries(&[(Some(&c.spec.x), t[2]), (c.spec.y.as_ref(), t[3])]),
            true,
        );
        let want = point(t[4]);
        let got = puck(&c.spec, &g, &src);
        let ex = unavailable(Some(&c.spec.x), t[2]);
        let ey = y_read.and_then(|y| unavailable(Some(y), t[3]));
        let expected_err = ex
            .map(|reason| (AxisSlot::X, reason))
            .or_else(|| ey.map(|reason| (AxisSlot::Y, reason)));
        let verdict = match (&got, expected_err) {
            (Ok(p), None) => same_pts(p, &want),
            (Err(e), Some((axis, reason))) => {
                *e == WidgetGeometryError::AxisUnavailable { axis, reason }
            }
            _ => false,
        };
        let vx = if ex.is_some() { 0.0 } else { first_value(t[2]) };
        let vy = if y_read.is_none() || ey.is_some() {
            0.0
        } else {
            first_value(t[3])
        };
        let characterized = same_pts(&local(&c.spec, &g, vx, vy), &want);
        if !(verdict && characterized && src.log_token() == t[5]) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

fn writes_token(w: &WidgetWrites) -> String {
    let mut parts = vec![format!("{}={:016x}", w.x.key, w.x.value.to_bits())];
    if let Some(y) = &w.y {
        parts.push(format!("{}={:016x}", y.key, y.value.to_bits()));
    }
    parts.join(";")
}

pub(crate) fn write_run(f: WriteFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("wfl") {
        n += 1;
        let c = case(t[1]);
        let l = point(t[2]);
        let ok = matches!(f(&c.spec, &geom_of(&c), l[0], l[1]), Ok(w) if writes_token(&w) == t[3]);
        if !ok {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn axis_run(f: AxisFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("axv") {
        n += 1;
        let ax = axis(t[1]);
        let frame = hex(t[3]);
        let src = TableSource::new(entries(&[(ax.as_ref(), t[2])]), t[2] != "P");
        let want = hex(t[4]);
        let got = f(ax.as_ref(), frame, &src);
        let verdict = match (got, unavailable(ax.as_ref(), t[2])) {
            (Ok(v), None) => same(v, want),
            // The baseline reads every unavailable axis as a positive zero.
            (Err(reason), Some(expected)) => reason == expected && want.to_bits() == 0,
            _ => false,
        };
        if !(verdict && src.log_token() == t[5]) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}
