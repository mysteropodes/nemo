//! Controls for the StoryBoard chain selection kernel: each named deviation from
//! the baseline must be caught by the oracle rows AND by a pinned literal.
//!
//! The fixture header carries, per deviation, the number of oracle rows that the
//! equivalent edit of the baseline JavaScript changes (`# mutant <name> <rows>`,
//! measured by the generator). The deviations here are run through the real seams
//! of the kernel (the source-frame `pick`, the `MontageModules` port, the module
//! list) or, for the backfill, a one-line model of `symbolStrokesAt`; each must
//! miss at least that many rows and at least one pinned case. The undeviated
//! runs must miss none, which proves the harness is the baseline and nothing else.

use super::tests::{chain_of, eval, opt, rows, tag_list, tags, world, World, ORACLE, PINNED};
use super::{
    layer_selection_with, layer_source_frame, montage_selection_with, LayerSource, StoryModule,
    StoryboardChain, SymbolFrame, SymbolTable,
};
use crate::evaluation::object_montage_timing::{
    montage_layer_frame, montage_source_frame_with, montage_total, resolve_chain,
    storyboard_source_frame, symbol_duration, ChainSlot, LayerFrame, MontageMember, MontageModules,
    MontageTimingError, SourcePick, SymbolView,
};

fn measured(name: &str) -> usize {
    let key = format!("# mutant {name} ");
    ORACLE
        .lines()
        .find_map(|l| l.strip_prefix(key.as_str()))
        .unwrap_or_else(|| panic!("no measurement for {name}"))
        .parse()
        .unwrap()
}

// ---- symbolStrokesAt backfill -------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Backfill {
    Baseline,
    /// A preceding INTERPOLATED frame also counts as a backfill source.
    InterpolatedSources,
    /// The scan starts two frames back instead of one.
    SkipNearest,
    /// `idx = fi`: the frame index is not clamped to the last frame.
    NoClamp,
}

fn backfill_model(m: Backfill, frames: &[Option<SymbolFrame>], fi: i64) -> Option<usize> {
    let len = frames.len() as i64;
    let idx = if m == Backfill::NoClamp {
        fi
    } else {
        fi.min(len - 1)
    };
    if idx < 0 {
        return None;
    }
    let at = |k: i64| -> Option<SymbolFrame> {
        if k >= 0 && k < len {
            frames[k as usize]
        } else {
            None
        }
    };
    if at(idx).is_some_and(|f| f.is_keyframe || f.is_interpolated) {
        return Some(idx as usize);
    }
    let mut k = if m == Backfill::SkipNearest {
        idx - 2
    } else {
        idx - 1
    };
    while k >= 0 {
        let hit = at(k).is_some_and(|f| {
            f.is_keyframe || (m == Backfill::InterpolatedSources && f.is_interpolated)
        });
        if hit {
            return Some(k as usize);
        }
        k -= 1;
    }
    None
}

/// `symbolStrokesAt(symbolId, fi)` through a model of the per-layer frame choice.
fn layers_tag<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    symbol_id: &str,
    fi: i64,
    pick: &P,
) -> String {
    match w.symbols.get(symbol_id) {
        None => "~".to_string(),
        Some(symbol) => {
            let layers: Vec<LayerSource> = symbol
                .layers
                .iter()
                .enumerate()
                .filter_map(|(layer_index, l)| {
                    pick(l.frames.as_slice(), fi).map(|frame_index| LayerSource {
                        layer_index,
                        frame_index,
                    })
                })
                .collect();
            tag_list(symbol_id, &layers)
        }
    }
}

fn s_row<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    f: &[&str],
    pick: &P,
) -> String {
    layers_tag(w, f[1], f[2].parse().unwrap(), pick)
}

/// `montageStrokesAt` with the frame choice replaced by the model.
fn m_model<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    ids: &[String],
    frame: i64,
    pick: &P,
) -> String {
    let members = chain_of(w).members(ids).unwrap();
    match montage_source_frame_with(&members, frame, storyboard_source_frame).unwrap() {
        None => "~".to_string(),
        Some(source) => layers_tag(w, &source.symbol_id, source.src_frame, pick),
    }
}

/// The montage-layer path with the frame choice replaced by the model.
fn l_model<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    ids: &[String],
    frame: i64,
    placed: Option<i64>,
    pick: &P,
) -> String {
    let members = chain_of(w).members(ids).unwrap();
    let total = montage_total(&members).unwrap();
    match montage_layer_frame(frame, placed, total).unwrap() {
        LayerFrame::None(_) => "~".to_string(),
        LayerFrame::Local(local) => m_model(w, ids, local, pick),
    }
}

/// Mismatching `S`, `M` and `L` oracle rows when the frame choice is `pick`.
fn backfill_misses<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    pick: P,
) -> usize {
    let s = rows("S")
        .iter()
        .filter(|f| s_row(w, f, &pick) != f[3])
        .count();
    let m = rows("M")
        .iter()
        .filter(|f| m_model(w, &w.chains[f[1]], f[2].parse().unwrap(), &pick) != f[3])
        .count();
    let l = rows("L")
        .iter()
        .filter(|f| l_model(w, &w.chains[f[1]], f[2].parse().unwrap(), opt(f[3]), &pick) != f[4])
        .count();
    s + m + l
}

fn s_pinned_misses<P: Fn(&[Option<SymbolFrame>], i64) -> Option<usize>>(
    w: &World,
    pick: P,
) -> usize {
    PINNED
        .iter()
        .filter(|(prefix, _)| prefix.starts_with("S "))
        .filter(|(prefix, expected)| {
            let f: Vec<&str> = prefix.split(' ').collect();
            s_row(w, &f, &pick) != *expected
        })
        .count()
}

// ---- chain walk with injected seams ---------------------------------------------------

struct Run<'a, M: MontageModules> {
    modules: &'a M,
    symbols: &'a SymbolTable,
    pick: SourcePick,
}

fn t_row<M: MontageModules>(r: &Run<M>, ids: &[String]) -> String {
    let members = resolve_chain(ids, r.modules).unwrap();
    montage_total(&members).unwrap().to_string()
}

fn m_row<M: MontageModules>(r: &Run<M>, ids: &[String], frame: i64) -> String {
    tags(&montage_selection_with(r.modules, r.symbols, ids, frame, r.pick).unwrap())
}

fn l_row<M: MontageModules>(r: &Run<M>, ids: &[String], frame: i64, placed: Option<i64>) -> String {
    tags(&layer_selection_with(r.modules, r.symbols, ids, frame, placed, r.pick).unwrap())
}

fn eval_run<M: MontageModules>(r: &Run<M>, w: &World, prefix: &str) -> String {
    let f: Vec<&str> = prefix.split(' ').collect();
    match f[0] {
        "T" => t_row(r, &w.chains[f[1]]),
        "M" => m_row(r, &w.chains[f[1]], f[2].parse().unwrap()),
        "L" => l_row(r, &w.chains[f[1]], f[2].parse().unwrap(), opt(f[3])),
        _ => eval(w, prefix),
    }
}

/// Mismatching `T`, `M` and `L` oracle rows of a run.
fn counts<M: MontageModules>(r: &Run<M>, w: &World) -> (usize, usize, usize) {
    let t = rows("T")
        .iter()
        .filter(|f| t_row(r, &w.chains[f[1]]) != f[2])
        .count();
    let m = rows("M")
        .iter()
        .filter(|f| m_row(r, &w.chains[f[1]], f[2].parse().unwrap()) != f[3])
        .count();
    let l = rows("L")
        .iter()
        .filter(|f| l_row(r, &w.chains[f[1]], f[2].parse().unwrap(), opt(f[3])) != f[4])
        .count();
    (t, m, l)
}

fn pinned_misses<M: MontageModules>(r: &Run<M>, w: &World) -> usize {
    PINNED
        .iter()
        .filter(|(prefix, expected)| eval_run(r, w, prefix) != *expected)
        .count()
}

/// `Math.ceil` where the baseline has `Math.floor`.
fn pick_ceil(
    trim_in: i64,
    src_len: i64,
    local: i64,
    duration: i64,
) -> Result<i64, MontageTimingError> {
    let scaled = (local as f64 * src_len as f64 / duration as f64).ceil();
    let cap = (src_len - 1) as f64;
    let picked = if scaled < cap { scaled } else { cap };
    Ok(trim_in + picked as i64)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Seam {
    Baseline,
    /// `trimOut = symbolDuration` instead of `symbolDuration - 1`.
    TrimOutOffByOne,
    /// A dangling id becomes a default member instead of being skipped.
    DanglingDefault,
    /// A sound/montage/unknown module becomes a default member.
    NonInstanceCounted,
}

struct Seamed<'a> {
    inner: StoryboardChain<'a>,
    seam: Seam,
}

fn default_member() -> MontageMember {
    MontageMember {
        symbol_id: String::new(),
        trim_in: None,
        trim_out: None,
        duration: None,
        symbol: SymbolView::Missing,
    }
}

impl MontageModules for Seamed<'_> {
    fn slot(&self, module_id: &str) -> ChainSlot {
        match (self.seam, self.inner.slot(module_id)) {
            (Seam::TrimOutOffByOne, ChainSlot::Instance(mut m)) => {
                if m.trim_out.is_none() {
                    m.trim_out = Some(symbol_duration(&m.symbol));
                }
                ChainSlot::Instance(m)
            }
            (Seam::DanglingDefault, ChainSlot::Dangling) => ChainSlot::Instance(default_member()),
            (Seam::NonInstanceCounted, ChainSlot::NotInstance) => {
                ChainSlot::Instance(default_member())
            }
            (_, slot) => slot,
        }
    }
}

fn seam_counts(w: &World, seam: Seam) -> ((usize, usize, usize), usize) {
    let modules = Seamed {
        inner: StoryboardChain::baseline(&w.modules, &w.symbols),
        seam,
    };
    let run = Run {
        modules: &modules,
        symbols: &w.symbols,
        pick: storyboard_source_frame,
    };
    (counts(&run, w), pinned_misses(&run, w))
}

fn edited_counts(
    w: &World,
    edit: impl Fn(&mut Vec<StoryModule>),
) -> ((usize, usize, usize), usize) {
    let mut modules = w.modules.clone();
    edit(&mut modules);
    let chain = StoryboardChain::baseline(&modules, &w.symbols);
    let run = Run {
        modules: &chain,
        symbols: &w.symbols,
        pick: storyboard_source_frame,
    };
    (counts(&run, w), pinned_misses(&run, w))
}

fn total((t, m, l): (usize, usize, usize)) -> usize {
    t + m + l
}

// ---- tests ------------------------------------------------------------------------------

#[test]
fn the_undeviated_runs_reproduce_every_oracle_row_and_pinned_case() {
    let w = world();
    assert_eq!(backfill_misses(&w, layer_source_frame), 0);
    assert_eq!(
        backfill_misses(&w, |f, fi| backfill_model(Backfill::Baseline, f, fi)),
        0
    );
    assert_eq!(s_pinned_misses(&w, layer_source_frame), 0);
    assert_eq!(seam_counts(&w, Seam::Baseline), ((0, 0, 0), 0));
}

#[test]
fn the_fixture_header_measurements_are_pinned() {
    for (name, rows_changed) in [
        ("ceil", 124),
        ("trimOutOffByOne", 452),
        ("interpBackfill", 459),
        ("skipOne", 668),
        ("noClamp", 27),
        ("nonInstance", 240),
        ("zeroTrimOut", 208),
        ("zeroDuration", 99),
        ("zeroTrimIn", 0),
        ("lastDup", 168),
        ("danglingTRows", 5),
    ] {
        assert_eq!(measured(name), rows_changed, "{name}");
    }
}

#[test]
fn backfill_deviations_are_caught() {
    let w = world();
    for (m, name) in [
        (Backfill::InterpolatedSources, "interpBackfill"),
        (Backfill::SkipNearest, "skipOne"),
        (Backfill::NoClamp, "noClamp"),
    ] {
        let floor = measured(name);
        assert!(floor >= 27, "{name}");
        let rows_missed = backfill_misses(&w, |f, fi| backfill_model(m, f, fi));
        assert!(rows_missed >= floor, "{name}: {rows_missed} < {floor}");
        let pinned = s_pinned_misses(&w, |f, fi| backfill_model(m, f, fi));
        assert!(pinned >= 1, "{name} must break a pinned case");
    }
}

#[test]
fn ceil_for_floor_is_caught() {
    let w = world();
    let chain = StoryboardChain::baseline(&w.modules, &w.symbols);
    let run = Run {
        modules: &chain,
        symbols: &w.symbols,
        pick: pick_ceil,
    };
    let (t, m, l) = counts(&run, &w);
    assert_eq!(t, 0, "totals do not depend on the source-frame formula");
    assert!(m + l >= measured("ceil"));
    assert!(pinned_misses(&run, &w) >= 1);
}

#[test]
fn trim_out_off_by_one_is_caught() {
    let w = world();
    let (c, pinned) = seam_counts(&w, Seam::TrimOutOffByOne);
    assert!(total(c) >= measured("trimOutOffByOne"));
    assert!(c.0 >= 1, "an unset trimOut changes montage totals");
    assert!(pinned >= 1);
}

#[test]
fn a_dangling_id_must_be_skipped_not_counted() {
    let w = world();
    let (c, pinned) = seam_counts(&w, Seam::DanglingDefault);
    // every T row of a chain holding a dangling id grows by a 24-frame default member.
    assert!(c.0 >= measured("danglingTRows"));
    assert!(pinned >= 1);
}

#[test]
fn non_instance_modules_must_be_skipped_not_counted() {
    let w = world();
    let (c, pinned) = seam_counts(&w, Seam::NonInstanceCounted);
    assert!(total(c) >= measured("nonInstance"));
    assert!(pinned >= 1);
}

#[test]
fn an_explicit_zero_trim_out_or_duration_is_not_unset() {
    let w = world();
    let (c, pinned) = edited_counts(&w, |modules| {
        for m in modules.iter_mut().filter(|m| m.trim_out == Some(0)) {
            m.trim_out = None;
        }
    });
    assert!(total(c) >= measured("zeroTrimOut"));
    assert!(pinned >= 1);
    let (c, pinned) = edited_counts(&w, |modules| {
        for m in modules.iter_mut().filter(|m| m.duration == Some(0)) {
            m.duration = None;
        }
    });
    assert!(total(c) >= measured("zeroDuration"));
    assert!(pinned >= 1);
}

/// The contract's "explicit 0 trimIn treated as unset" is an EQUIVALENT mutation:
/// the default of an unset trimIn is 0, so no row can tell them apart. The baseline
/// measurement is zero rows and the native run agrees.
#[test]
fn an_explicit_zero_trim_in_is_indistinguishable_from_unset() {
    let w = world();
    let (c, pinned) = edited_counts(&w, |modules| {
        for m in modules.iter_mut().filter(|m| m.trim_in == Some(0)) {
            m.trim_in = None;
        }
    });
    assert_eq!(measured("zeroTrimIn"), 0);
    assert_eq!((c, pinned), ((0, 0, 0), 0));
    assert!(w.modules.iter().any(|m| m.trim_in == Some(0)));
}

#[test]
fn the_first_module_with_an_id_must_win() {
    let w = world();
    let (c, pinned) = edited_counts(&w, |modules| modules.reverse());
    assert!(total(c) >= measured("lastDup"));
    assert!(pinned >= 1);
}
