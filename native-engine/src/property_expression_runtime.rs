//! Pure property-expression result handling (D15/#2074, under #2072).
//!
//! Mirrors the tail of `evalExpressionFor` and `normalizeExprResult` in
//! `src/js/motion.js`: how a guest expression's return value is folded into
//! a property value, which typed diagnostic each failure produces, when the
//! raw (un-expressed) value is substituted, when a compiled-expression cache
//! entry is reusable, and when a finished evaluation may still publish.
//!
//! The caller owns compilation, the guest, wall-clock budgets and the
//! document; this module receives an already-classified [`Outcome`] and
//! never reads a context, a clock or a global.
//!
//! Deliberately not modelled: JavaScript's `_depthTripped` bookkeeping, which
//! lets a depth warning raised by an inner hop survive an outer success (a
//! reporting detail of the shared `lastError` slot, not a value); and the
//! toast/re-render side effects of a timeout. The `_depthTripped` behavior
//! must be carried into the D10/D11 consumer, which owns the shared slot.

/// Most nested cross-layer evaluations allowed (`EXPR_MAX_DEPTH`).
pub(crate) const EXPR_MAX_DEPTH: u32 = 8;

/// What a guest expression returned, reduced to what the normalizer reads.
/// `Other` is every JavaScript value that is neither a number nor an array:
/// booleans, strings, objects (array-likes included), null, undefined,
/// functions. An array element that is not a number is `Other` as well,
/// holes and `undefined` included, since `typeof` rejects them alike.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GuestValue {
    Number(f64),
    Array(Vec<GuestValue>),
    Other,
}

/// The dimension `normalizeExprResult` works against.
///
/// `PROP_DIM[prop]` is a plain object lookup; a property absent from it is
/// `undefined` unless its name starts with `vtx`, which `normalizeExprResult`
/// alone answers as 2. `Vertex` keeps that origin because the *diagnostic*
/// (`PROP_DIM[prop] >= 2`) has no such fallback: for a vertex it reads
/// `undefined >= 2`, false, and reports the scalar message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dim {
    Known(usize),
    Vertex,
    Undefined,
}

impl Dim {
    /// Resolves a property name the way the JavaScript does; `table_dim` is
    /// the property's own `PROP_DIM` entry, `None` when it has none.
    pub(crate) fn for_prop(prop: &str, table_dim: Option<usize>) -> Dim {
        match table_dim {
            Some(n) => Dim::Known(n),
            None if prop.starts_with("vtx") => Dim::Vertex,
            None => Dim::Undefined,
        }
    }

    /// The dimension the normalizer compares against; `None` is JavaScript's
    /// `undefined`, against which every `>=` is false and a fill loop is empty.
    fn normalizing(self) -> Option<usize> {
        match self {
            Dim::Known(n) => Some(n),
            Dim::Vertex => Some(2),
            Dim::Undefined => None,
        }
    }

    /// `PROP_DIM[prop] >= 2`: which "must return" message applies.
    fn wants_pair_message(self) -> bool {
        matches!(self, Dim::Known(n) if n >= 2)
    }
}

/// `normalizeExprResult`: the property value a guest result stands for, or
/// `None` when the shape is wrong (the caller then reports a diagnostic and
/// falls back to the raw value). The rules apply in this order:
///
/// 1. an array of at least `dim` entries is cut to `dim`, and the *cut*
///    (not the whole array) must be all finite numbers;
/// 2. an array of exactly one finite number with `dim >= 2` fills every
///    component;
/// 3. any other array is rejected;
/// 4. a finite bare number fills every component;
/// 5. anything else is rejected.
///
/// With an `Undefined` dim no array can match rules 1-2 (`n >= undefined`
/// is false), so arrays are rejected, and a finite bare number yields the
/// *empty* vector (`fill` loops zero times): `Some(vec![])`, not `None`.
pub(crate) fn normalize_result(dim: Dim, value: &GuestValue) -> Option<Vec<f64>> {
    let dim = dim.normalizing();
    let fill = |n: f64| vec![n; dim.unwrap_or(0)];
    match value {
        GuestValue::Array(items) => {
            if dim.is_some_and(|d| items.len() >= d) {
                return all_finite(&items[..dim.unwrap_or(0)]);
            }
            if let ([GuestValue::Number(n)], Some(d)) = (items.as_slice(), dim) {
                if d >= 2 && n.is_finite() {
                    return Some(fill(*n));
                }
            }
            None
        }
        GuestValue::Number(n) if n.is_finite() => Some(fill(*n)),
        _ => None,
    }
}

/// `allFinite` over a cut: every entry must be a finite number.
fn all_finite(items: &[GuestValue]) -> Option<Vec<f64>> {
    items
        .iter()
        .map(|item| match item {
            GuestValue::Number(n) if n.is_finite() => Some(*n),
            _ => None,
        })
        .collect()
}

/// How one guest evaluation ended, classified by the caller. The caller
/// must apply the JS precedence: a compile (`Syntax`) failure is checked
/// first, then `TooDeep`, then the guest's own `Thrown`/`Timeout`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// The guest returned a value.
    Returned(GuestValue),
    /// The guest threw; `line` is the user-visible line, if it has one.
    Thrown { message: String, line: Option<u32> },
    /// The source failed to compile.
    Syntax { message: String, line: Option<u32> },
    /// The nesting depth reached [`EXPR_MAX_DEPTH`] before the guest ran.
    TooDeep,
    /// The outermost wall-clock budget was exceeded.
    Timeout,
}

/// The typed reason a property fell back to its raw value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Diagnostic {
    MustReturnNumber,
    MustReturnNumberOrXY,
    Runtime { message: String, line: Option<u32> },
    Syntax { message: String, line: Option<u32> },
    TooDeep,
    Timeout,
}

/// A property value together with why it is not the expression's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fallback {
    pub(crate) value: Vec<f64>,
    pub(crate) diagnostic: Option<Diagnostic>,
    /// Set only by a timeout: the expression switches itself off so it does
    /// not hit the same wall every frame. No other failure disables it.
    pub(crate) disable: bool,
}

/// The tail of `evalExpressionFor` plus `valueAtFrame`'s `?? raw`: a good
/// result is the value with no diagnostic; every failure yields `raw`
/// unchanged and the matching diagnostic. It takes an already-classified
/// outcome; the caller applies the precedence documented on [`Outcome`].
pub(crate) fn evaluate_with_fallback(dim: Dim, raw: &[f64], outcome: Outcome) -> Fallback {
    let failed = |diagnostic, disable| Fallback {
        value: raw.to_vec(),
        diagnostic: Some(diagnostic),
        disable,
    };
    match outcome {
        Outcome::Returned(guest) => match normalize_result(dim, &guest) {
            Some(value) => Fallback {
                value,
                diagnostic: None,
                disable: false,
            },
            None if dim.wants_pair_message() => failed(Diagnostic::MustReturnNumberOrXY, false),
            None => failed(Diagnostic::MustReturnNumber, false),
        },
        Outcome::Thrown { message, line } => failed(Diagnostic::Runtime { message, line }, false),
        Outcome::Syntax { message, line } => failed(Diagnostic::Syntax { message, line }, false),
        Outcome::TooDeep => failed(Diagnostic::TooDeep, false),
        Outcome::Timeout => failed(Diagnostic::Timeout, true),
    }
}

/// The evaluation target a compiled expression belongs to (D10 shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextTarget {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) stroke_id: Option<String>,
}

/// What a compiled-expression cache entry is valid for. Expression controls
/// are deliberately NOT part of the key: the JavaScript compares only
/// `code` and the global preamble (`exprGlobals()`), because controls are
/// read at call time through closures, never baked into the compiled body.
/// The property is part of the key: JS caches per `holder._exprCompiled[prop]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CacheKey {
    pub(crate) code: String,
    pub(crate) preamble: String,
    pub(crate) context: ContextTarget,
    pub(crate) prop: String,
}

/// A cached entry is reusable only when code, preamble, context and prop all match.
pub(crate) fn cache_hit(cached: &CacheKey, wanted: &CacheKey) -> bool {
    cached == wanted
}

/// The gate between a finished evaluation and the document.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Publish<T> {
    Publish(T),
    Stale,
}

/// A candidate computed against `candidate_revision` may publish (or bake)
/// only while that is still the current revision. Any other revision, newer
/// or older, is stale and yields nothing.
pub(crate) fn publish_if_current<T>(
    candidate_revision: u64,
    current_revision: u64,
    value: T,
) -> Publish<T> {
    if candidate_revision == current_revision {
        Publish::Publish(value)
    } else {
        Publish::Stale
    }
}
