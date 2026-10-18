//! Expression lookup kernel over supplied immutable records (D16-K1, #2278).
//!
//! Mirrors the name, uid, control, keyframe and marker lookups of
//! `src/js/motion.js`: `findLayerIndexByRef`, `findLayerIndexByUid`, the
//! resolution order of `exprLayerControl`, `normalizeControlName`,
//! `findControl`, `exprKeyAt`, `exprNumKeys`, `nearestKeyIndex`,
//! `exprNearestKeyAtFrame`, `_markerEntry`, `_markerApi` and
//! `_aliasMarkerApi`. Every input is a caller-owned record copied into a
//! validated table; nothing here samples a track, evaluates a guest, builds a
//! snapshot, persists, or owns document state.
//!
//! Motion semantics kept on purpose:
//! - a layer reference is the FIRST exact (case-sensitive) name match, then
//!   the first uid match; `layerControl` tries the uid FIRST, so a layer whose
//!   name equals another layer's uid resolves differently through the two;
//! - control names compare after JS `trim()` + `toLowerCase()`, reproduced
//!   here exactly (see [`normalize_control_name`]), and the first match wins;
//! - `keys.at(i)` is 1-based and does not floor: an in-range non-integer index
//!   makes Motion throw, which is [`LookupError::KeyIndexNotInteger`];
//! - the nearest key is a binary search where the earlier key wins a tie; the
//!   nearest marker is a linear scan where the first marker wins a tie, and
//!   markers may be unsorted;
//! - marker indexes and frames go through Motion's `_num` (a non-finite number
//!   reads 0) and `Math.floor`; the alias `nearestKey` takes seconds × fps and
//!   re-applies `_num`, so an overflowing product reads frame 0.
//!
//! Bounds are this kernel's own contract, not Motion's: records and queries
//! beyond them are refused with a typed [`LookupRefusal`].

/// Most layers a table may hold.
pub(crate) const MAX_LAYERS: usize = 4096;
/// Most expression controls one layer may declare.
pub(crate) const MAX_CONTROLS: usize = 256;
/// Most keys one track may hold.
pub(crate) const MAX_KEYS: usize = 65_536;
/// Most components one key value may hold.
pub(crate) const MAX_VALUE_DIMS: usize = 16;
/// Most markers one list may hold.
pub(crate) const MAX_MARKERS: usize = 4096;
/// Longest name, uid, color or query string, in UTF-8 bytes.
pub(crate) const MAX_NAME_BYTES: usize = 1024;

/// Input this kernel refuses before looking anything up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookupRefusal {
    TooManyLayers,
    TooManyControls,
    TooManyKeys,
    TooManyValueDims,
    TooManyMarkers,
    NameTooLong,
    /// A key frame, key value or marker frame is NaN or infinite.
    NonFiniteRecord,
    /// A key index or key frame query is NaN or infinite.
    NonFiniteQuery,
    /// The frame rate is not finite and positive.
    InvalidFps,
}

/// Why a lookup has no answer where Motion would throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookupError {
    Refused(LookupRefusal),
    /// `layerControl` named no layer by uid or by reference.
    UnknownLayer,
    /// `exprKeyAt` read `keys[i - 1]` with a non-integer in-range `i`.
    KeyIndexNotInteger,
}

impl From<LookupRefusal> for LookupError {
    fn from(r: LookupRefusal) -> Self {
        Self::Refused(r)
    }
}

fn bounded(s: &str) -> Result<&str, LookupRefusal> {
    (s.len() <= MAX_NAME_BYTES)
        .then_some(s)
        .ok_or(LookupRefusal::NameTooLong)
}

fn bounded_opt(s: &Option<String>) -> Result<(), LookupRefusal> {
    s.as_deref().map_or(Ok(()), |s| bounded(s).map(|_| ()))
}

/// A validated frame rate (`_exprFps`); Motion's `|| 24` stays with the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fps(f64);

impl Fps {
    pub(crate) fn new(fps: f64) -> Result<Self, LookupRefusal> {
        let valid = fps.is_finite() && fps > 0.0;
        valid.then_some(Self(fps)).ok_or(LookupRefusal::InvalidFps)
    }
    pub(crate) fn get(self) -> f64 {
        self.0
    }
}

/// Motion's `_num` for a numeric input: a non-finite number reads 0.
fn js_num(v: f64) -> f64 {
    Some(v).filter(|v| v.is_finite()).unwrap_or(0.0)
}

// ---- control names ---------------------------------------------------------

/// The JS `String.prototype.trim` set: WhiteSpace plus LineTerminator. Unlike
/// Rust's `char::is_whitespace` it includes U+FEFF and excludes U+0085.
#[rustfmt::skip]
const JS_SPACE: [(u32, u32); 10] = [
    (0x9, 0xd), (0x20, 0x20), (0xa0, 0xa0), (0x1680, 0x1680), (0x2000, 0x200a), (0x2028, 0x2029),
    (0x202f, 0x202f), (0x205f, 0x205f), (0x3000, 0x3000), (0xfeff, 0xfeff),
];

/// Code points Rust (Unicode 17) lowercases but the pinned JS engine
/// (Node 22.22, Unicode 16.0) leaves unchanged.
const JS_UNMAPPED: [(u32, u32); 4] = [
    (0xa7ce, 0xa7ce),
    (0xa7d2, 0xa7d2),
    (0xa7d4, 0xa7d4),
    (0x16ea0, 0x16eb8),
];

/// Final-sigma context class: 0 neither, 1 cased, 2 case-ignorable.
type SigmaClass = u8;

/// Code points whose final-sigma class differs between Rust and the pinned
/// JS engine, with the JS class (measured exhaustively over every scalar).
#[rustfmt::skip]
const JS_SIGMA_CLASS: [(u32, u32, SigmaClass); 21] = [
    (0x295, 0x295, 1), (0x1acf, 0x1add, 0), (0x1ae0, 0x1aeb, 0), (0xa7ce, 0xa7cf, 0),
    (0xa7d2, 0xa7d2, 0), (0xa7d4, 0xa7d4, 0), (0xa7f1, 0xa7f1, 0), (0x10ec5, 0x10ec5, 0),
    (0x10efa, 0x10efb, 0), (0x11b60, 0x11b60, 0), (0x11b62, 0x11b64, 0), (0x11b66, 0x11b66, 0),
    (0x11dd9, 0x11dd9, 0), (0x16ea0, 0x16eb8, 0), (0x16ebb, 0x16ed3, 0), (0x16ff2, 0x16ff3, 0),
    (0x1e6e3, 0x1e6e3, 0), (0x1e6e6, 0x1e6e6, 0), (0x1e6ee, 0x1e6ef, 0), (0x1e6f5, 0x1e6f5, 0),
    (0x1e6ff, 0x1e6ff, 0),
];

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&(c as u32)))
}

/// Class of `c` as Rust's own Final_Sigma rule sees it, probed through it:
/// a cased neighbour after the sigma blocks it, a cased or ignorable one
/// before it (followed by the cased `Α`) allows it.
fn rust_sigma_class(c: char) -> SigmaClass {
    let sigma_final = |s: String, at: usize| s.to_lowercase().chars().nth(at) == Some('\u{3c2}');
    let after = sigma_final(format!("\u{391}\u{3a3}{c}"), 1);
    let before = sigma_final(format!("\u{391}{c}\u{3a3}"), 1 + c.to_lowercase().count());
    match (after, before) {
        (false, _) => 1,
        (true, true) => 2,
        (true, false) => 0,
    }
}

fn js_sigma_class(c: char) -> SigmaClass {
    JS_SIGMA_CLASS
        .iter()
        .find(|&&(a, b, _)| (a..=b).contains(&(c as u32)))
        .map_or_else(|| rust_sigma_class(c), |&(_, _, k)| k)
}

/// JS `toLowerCase` (root locale): per-code-point full lowercase mapping
/// plus the Final_Sigma context rule, both pinned to the JS engine's tables.
fn js_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let sigma_ctx: Option<Vec<char>> = s.contains('\u{3a3}').then(|| {
        // Each neighbour is replaced by a representative of its JS class,
        // so Rust's own Final_Sigma rule decides with JS's classification.
        let ctx: String = s
            .chars()
            .map(|c| match c {
                '\u{3a3}' => c,
                _ => ['\u{20}', 'a', '\''][js_sigma_class(c) as usize],
            })
            .collect();
        ctx.to_lowercase().chars().collect()
    });
    for (i, c) in s.chars().enumerate() {
        match (&sigma_ctx, c) {
            (Some(ctx), '\u{3a3}') => out.push(ctx[i]),
            _ if in_ranges(c, &JS_UNMAPPED) => out.push(c),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// `normalizeControlName`: `String(n == null ? '' : n).trim().toLowerCase()`.
pub(crate) fn normalize_control_name(name: Option<&str>) -> Result<String, LookupRefusal> {
    let name = bounded(name.unwrap_or(""))?;
    Ok(js_lower(name.trim_matches(|c| in_ranges(c, &JS_SPACE))))
}

/// One declared expression control; only its name takes part in lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlRecord {
    pub(crate) name: Option<String>,
}

/// `findControl` over one layer's controls: index of the first match.
pub(crate) fn find_control(
    controls: &[ControlRecord],
    name: Option<&str>,
) -> Result<Option<usize>, LookupRefusal> {
    let want = normalize_control_name(name)?;
    if want.is_empty() {
        return Ok(None);
    }
    for (i, c) in controls.iter().enumerate() {
        if normalize_control_name(c.name.as_deref())? == want {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

// ---- layers ----------------------------------------------------------------

/// One layer as the lookups see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerRecord {
    pub(crate) name: Option<String>,
    pub(crate) layer_uid: Option<String>,
    pub(crate) controls: Vec<ControlRecord>,
}

/// The validated, immutable layer list in stack order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerTable {
    layers: Vec<LayerRecord>,
}

/// A resolved `layerControl(ref, name)`; `control` is `None` where Motion's
/// `_controlValue` would throw its unknown-control error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlResolution {
    pub(crate) layer_index: usize,
    pub(crate) control: Option<usize>,
}

impl LayerTable {
    pub(crate) fn new(layers: Vec<LayerRecord>) -> Result<Self, LookupRefusal> {
        if layers.len() > MAX_LAYERS {
            return Err(LookupRefusal::TooManyLayers);
        }
        for l in &layers {
            if l.controls.len() > MAX_CONTROLS {
                return Err(LookupRefusal::TooManyControls);
            }
            bounded_opt(&l.name)?;
            bounded_opt(&l.layer_uid)?;
            l.controls.iter().try_for_each(|c| bounded_opt(&c.name))?;
        }
        Ok(Self { layers })
    }

    pub(crate) fn layers(&self) -> &[LayerRecord] {
        &self.layers
    }

    /// `findLayerIndexByUid`: the first exact uid; `""` names nothing.
    pub(crate) fn find_by_uid(&self, uid: &str) -> Result<Option<usize>, LookupRefusal> {
        if bounded(uid)?.is_empty() {
            return Ok(None);
        }
        Ok(self
            .layers
            .iter()
            .position(|l| l.layer_uid.as_deref() == Some(uid)))
    }

    /// `findLayerIndexByRef`: the first exact name, else [`Self::find_by_uid`].
    pub(crate) fn find_by_ref(&self, r: &str) -> Result<Option<usize>, LookupRefusal> {
        if bounded(r)?.is_empty() {
            return Ok(None);
        }
        let by_name = self
            .layers
            .iter()
            .position(|l| l.name.as_deref() == Some(r));
        by_name.map_or_else(|| self.find_by_uid(r), |i| Ok(Some(i)))
    }

    /// `exprLayerControl`'s resolution: uid first, then by reference.
    pub(crate) fn layer_control(
        &self,
        r: &str,
        name: Option<&str>,
    ) -> Result<ControlResolution, LookupError> {
        let layer_index = match self.find_by_uid(r)? {
            Some(i) => i,
            None => self.find_by_ref(r)?.ok_or(LookupError::UnknownLayer)?,
        };
        let control = find_control(&self.layers[layer_index].controls, name)?;
        Ok(ControlResolution {
            layer_index,
            control,
        })
    }
}

// ---- keys ------------------------------------------------------------------

/// One stored key: its frame and its value components `v`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyRecord {
    pub(crate) frame: f64,
    pub(crate) value: Vec<f64>,
}

/// A key's value: a bare number when it has one component, else a copy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeyValue {
    Scalar(f64),
    Vector(Vec<f64>),
}

/// `exprKeyAt`'s `{frame, time, value, index}`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyEntry {
    pub(crate) frame: f64,
    pub(crate) time: f64,
    pub(crate) value: KeyValue,
    pub(crate) index: usize,
}

/// A validated, immutable key list in stored order (sorting is not assumed).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyTrack {
    keys: Vec<KeyRecord>,
}

impl KeyTrack {
    pub(crate) fn new(keys: Vec<KeyRecord>) -> Result<Self, LookupRefusal> {
        if keys.len() > MAX_KEYS {
            return Err(LookupRefusal::TooManyKeys);
        }
        for k in &keys {
            if k.value.len() > MAX_VALUE_DIMS {
                return Err(LookupRefusal::TooManyValueDims);
            }
            if !k.frame.is_finite() || k.value.iter().any(|v| !v.is_finite()) {
                return Err(LookupRefusal::NonFiniteRecord);
            }
        }
        Ok(Self { keys })
    }
}

/// `exprNumKeys`; a missing track counts 0.
pub(crate) fn num_keys(track: Option<&KeyTrack>) -> usize {
    track.map_or(0, |t| t.keys.len())
}

/// `exprKeyAt`: 1-based and unfloored; out of range or no track is `None`.
pub(crate) fn key_at(
    track: Option<&KeyTrack>,
    i: f64,
    fps: Fps,
) -> Result<Option<KeyEntry>, LookupError> {
    if !i.is_finite() {
        return Err(LookupRefusal::NonFiniteQuery.into());
    }
    let Some(track) = track else { return Ok(None) };
    if i < 1.0 || i > track.keys.len() as f64 {
        return Ok(None);
    }
    if i.fract() != 0.0 {
        return Err(LookupError::KeyIndexNotInteger);
    }
    let index = i as usize;
    let k = &track.keys[index - 1];
    let value = match k.value.as_slice() {
        [v] => KeyValue::Scalar(*v),
        v => KeyValue::Vector(v.to_vec()),
    };
    Ok(Some(KeyEntry {
        frame: k.frame,
        time: k.frame / fps.get(),
        value,
        index,
    }))
}

/// `nearestKeyIndex` over a non-empty track: the earlier key wins a tie.
fn nearest_key_index(keys: &[KeyRecord], target: f64) -> usize {
    let (mut lo, mut hi) = (0, keys.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if keys[mid].frame < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        return 0;
    }
    if lo == keys.len() {
        return keys.len() - 1;
    }
    let prev = target - keys[lo - 1].frame;
    let next = keys[lo].frame - target;
    // Only a strictly closer later key replaces the earlier one.
    lo - usize::from(next >= prev)
}

/// `exprNearestKeyAtFrame`; an empty or missing track is `None`.
pub(crate) fn nearest_key_at_frame(
    track: Option<&KeyTrack>,
    frame: f64,
    fps: Fps,
) -> Result<Option<KeyEntry>, LookupError> {
    if !frame.is_finite() {
        return Err(LookupRefusal::NonFiniteQuery.into());
    }
    match track {
        Some(t) if !t.keys.is_empty() => {
            key_at(Some(t), (nearest_key_index(&t.keys, frame) + 1) as f64, fps)
        }
        _ => Ok(None),
    }
}

// ---- markers ---------------------------------------------------------------

/// One stored marker (`{frame, name, color}`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MarkerRecord {
    pub(crate) frame: f64,
    pub(crate) name: Option<String>,
    pub(crate) color: Option<String>,
}

/// `_markerEntry`'s `{frame, time, name, color, index}`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MarkerEntry {
    pub(crate) frame: f64,
    pub(crate) time: f64,
    /// `name || ''`.
    pub(crate) name: String,
    /// `color || null`: an empty color is `None`.
    pub(crate) color: Option<String>,
    pub(crate) index: usize,
}

/// A marker argument: a name, or a number taken through `_num`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MarkerQuery<'a> {
    Name(&'a str),
    Number(f64),
}

/// `_markerApi(list)`; a missing list is empty. Order is as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MarkerList {
    markers: Vec<MarkerRecord>,
}

impl MarkerList {
    pub(crate) fn new(markers: Vec<MarkerRecord>) -> Result<Self, LookupRefusal> {
        if markers.len() > MAX_MARKERS {
            return Err(LookupRefusal::TooManyMarkers);
        }
        for m in &markers {
            if !m.frame.is_finite() {
                return Err(LookupRefusal::NonFiniteRecord);
            }
            bounded_opt(&m.name)?;
            bounded_opt(&m.color)?;
        }
        Ok(Self { markers })
    }

    /// `count` (and the alias `numKeys`).
    pub(crate) fn count(&self) -> usize {
        self.markers.len()
    }

    /// `_markerEntry(list, i)` for an already-floored `i`.
    fn entry(&self, i: f64, fps: Fps) -> Option<MarkerEntry> {
        if i < 1.0 || i > self.markers.len() as f64 {
            return None;
        }
        let index = i as usize;
        let m = &self.markers[index - 1];
        Some(MarkerEntry {
            frame: m.frame,
            time: m.frame / fps.get(),
            name: m.name.clone().unwrap_or_default(),
            color: m.color.clone().filter(|c| !c.is_empty()),
            index,
        })
    }

    /// `at(i)` (and the alias `key`): the first marker named exactly `i`
    /// (no name reads `''`), or `Math.floor(_num(i))`, 1-based.
    pub(crate) fn at(
        &self,
        q: MarkerQuery<'_>,
        fps: Fps,
    ) -> Result<Option<MarkerEntry>, LookupRefusal> {
        Ok(match q {
            MarkerQuery::Name(n) => {
                bounded(n)?;
                let found = self
                    .markers
                    .iter()
                    .position(|m| m.name.as_deref().unwrap_or("") == n);
                found.and_then(|j| self.entry((j + 1) as f64, fps))
            }
            MarkerQuery::Number(i) => self.entry(js_num(i).floor(), fps),
        })
    }

    /// `nearest(frame)`: a linear scan, the first marker wins a tie.
    pub(crate) fn nearest(&self, frame: f64, fps: Fps) -> Option<MarkerEntry> {
        if self.markers.is_empty() {
            return None;
        }
        let target = js_num(frame);
        let (mut best, mut best_d) = (0, f64::INFINITY);
        for (j, m) in self.markers.iter().enumerate() {
            let d = (m.frame - target).abs();
            if d < best_d {
                best_d = d;
                best = j;
            }
        }
        self.entry((best + 1) as f64, fps)
    }

    /// The alias `nearestKey(t)`: `nearest(_num(t) * fps)`.
    pub(crate) fn nearest_key_seconds(&self, t: f64, fps: Fps) -> Option<MarkerEntry> {
        self.nearest(js_num(t) * fps.get(), fps)
    }
}
