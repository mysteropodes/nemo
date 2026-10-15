//! Pure expression-control read rules (R18.2 candidate, local draft).
//!
//! Mirrors `src/js/motion.js`: `normalizeControlName`, `findControl`,
//! `controlsOf`, `_controlValue`, `exprSelfControl`, `exprLayerControl` and
//! the uid-then-name layer lookup it performs through
//! `findLayerIndexByUid`/`findLayerIndexByRef`. The caller owns the layers,
//! the control declarations, the evaluation context and value sampling
//! (`valueAtFrame`, which may itself run an expression); this module only
//! decides which layer, which control track and which frame are read, and
//! how the sampled components are handed back.
//!
//! Strings are the JavaScript values as given; `None` is `null` or
//! `undefined`. Non-string references, names and frame arguments (numbers
//! as layer refs, strings as frames) are outside this kernel.

/// One `ld.exprControls` entry: its stable track key and display name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlDecl<'a> {
    pub key: &'a str,
    pub name: Option<&'a str>,
}

/// The two identifiers a control read can name a layer by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerIds<'a> {
    pub uid: Option<&'a str>,
    pub name: Option<&'a str>,
}

/// `v.length === 1 ? v[0] : v.slice()`: one component reads as a number,
/// any other count (including none) as an array.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Scalar(f64),
    Vector(Vec<f64>),
}

/// The thrown errors, before localisation. The JavaScript message embeds
/// `String(ref)` or `String(name)`; the caller formats it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// `exprErrorUnknownLayer`: no layer has this uid or name.
    UnknownLayer,
    /// `exprErrorUnknownControl`: the layer has no control of this name.
    UnknownControl,
    /// `exprErrorControlNoLayer`: a self read with no owning layer.
    NoLayer,
}

/// `normalizeControlName`: JavaScript `trim()` then `toLowerCase()`.
pub fn normalize_control_name(name: Option<&str>) -> String {
    name.unwrap_or("")
        .trim_matches(is_js_whitespace)
        .to_lowercase()
}

/// `findControl`: the first control whose normalized name equals the
/// normalized request. A blank request matches nothing. `controls` is
/// `controlsOf(ld)`: the caller passes an empty slice when the layer's
/// `exprControls` is missing or not an array.
pub fn find_control<'c, 'a>(
    controls: &'c [ControlDecl<'a>],
    name: Option<&str>,
) -> Option<&'c ControlDecl<'a>> {
    let want = normalize_control_name(name);
    if want.is_empty() {
        return None;
    }
    controls
        .iter()
        .find(|c| normalize_control_name(c.name) == want)
}

/// `findLayerIndexByUid`: the first layer with exactly this uid. A falsy
/// request (`null`, `undefined`, `""`) finds nothing.
pub fn find_layer_by_uid(layers: &[LayerIds<'_>], uid: Option<&str>) -> Option<usize> {
    let uid = uid.filter(|u| !u.is_empty())?;
    layers.iter().position(|l| l.uid == Some(uid))
}

/// `findLayerIndexByRef`: the first layer with exactly this name, else by
/// uid. A falsy request finds nothing.
pub fn find_layer_by_ref(layers: &[LayerIds<'_>], reference: Option<&str>) -> Option<usize> {
    let r = reference.filter(|r| !r.is_empty())?;
    layers
        .iter()
        .position(|l| l.name == Some(r))
        .or_else(|| find_layer_by_uid(layers, Some(r)))
}

/// The lookup order of `exprLayerControl`: uid first (the form the widget
/// and pickwhip emit), then name, then (redundantly) uid again.
pub fn resolve_control_layer(layers: &[LayerIds<'_>], reference: Option<&str>) -> Option<usize> {
    find_layer_by_uid(layers, reference).or_else(|| find_layer_by_ref(layers, reference))
}

/// JavaScript `_num` for a number: non-finite reads as `+0`, `-0` is kept.
pub fn js_num(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// The frame a cross-layer read samples: an explicit argument through
/// `_num`; otherwise the evaluation context's frame, or the document's
/// current frame outside any evaluation. `arg` `None` is `undefined`.
pub fn control_frame(arg: Option<f64>, context_frame: Option<f64>, current_frame: f64) -> f64 {
    match arg {
        Some(f) => js_num(f),
        None => context_frame.unwrap_or(current_frame),
    }
}

/// `_controlValue`: find the control by name, then sample its track at
/// `frame` through `sample(key, frame)`.
pub fn control_value(
    controls: &[ControlDecl<'_>],
    name: Option<&str>,
    frame: f64,
    sample: impl FnOnce(&str, f64) -> Vec<f64>,
) -> Result<ControlValue, ControlError> {
    let control = find_control(controls, name).ok_or(ControlError::UnknownControl)?;
    Ok(shape_value(sample(control.key, frame)))
}

/// `exprLayerControl(ref, name, f)`. `controls_of(i)` is `controlsOf` of
/// layer `i`; `sample(i, key, frame)` is `valueAtFrame` on that layer. An
/// unknown layer fails before the control name is looked at.
#[allow(clippy::too_many_arguments)]
pub fn layer_control<'a>(
    layers: &[LayerIds<'_>],
    controls_of: impl FnOnce(usize) -> &'a [ControlDecl<'a>],
    reference: Option<&str>,
    name: Option<&str>,
    arg: Option<f64>,
    context_frame: Option<f64>,
    current_frame: f64,
    sample: impl FnOnce(usize, &str, f64) -> Vec<f64>,
) -> Result<ControlValue, ControlError> {
    let idx = resolve_control_layer(layers, reference).ok_or(ControlError::UnknownLayer)?;
    let frame = control_frame(arg, context_frame, current_frame);
    control_value(controls_of(idx), name, frame, |key, f| sample(idx, key, f))
}

/// `exprSelfControl(name, f)`: outside an evaluation context it reads `0`
/// without touching anything; inside one it needs the expression's owning
/// layer (an element holder's layer), whose controls it reads at the
/// context frame unless `f` is given.
pub fn self_control(
    context_frame: Option<f64>,
    owning_layer_controls: Option<&[ControlDecl<'_>]>,
    name: Option<&str>,
    arg: Option<f64>,
    sample: impl FnOnce(&str, f64) -> Vec<f64>,
) -> Result<ControlValue, ControlError> {
    let Some(ctx_frame) = context_frame else {
        return Ok(ControlValue::Scalar(0.0));
    };
    let controls = owning_layer_controls.ok_or(ControlError::NoLayer)?;
    let frame = match arg {
        Some(f) => js_num(f),
        None => ctx_frame,
    };
    control_value(controls, name, frame, sample)
}

/// The array-or-number return shape of `_controlValue`.
pub fn shape_value(v: Vec<f64>) -> ControlValue {
    if v.len() == 1 {
        ControlValue::Scalar(v[0])
    } else {
        ControlValue::Vector(v)
    }
}

/// ECMAScript `WhiteSpace` and `LineTerminator`, the set `trim()` strips.
/// Unlike Rust's `char::is_whitespace` it includes U+FEFF and excludes
/// U+0085.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}
