//! Oracle rows and comparison harness for the montage timing kernel (L10-A, I18/I19/I20/I36).
//!
//! `fixtures/object-montage-timing/oracle.txt` (`ORACLE`, sha256
//! 2cbd114cc2213e24a73416281406826306989fd8e39a5e9956cc1cdece4c885e) was produced by `tools/gen.mjs`
//! (sha256 64bd8f3d9b6ee870789e1d559acbb373bd8fb7b8b4b6fd7c6d3ca7fcb102acfc, kept outside the repo; Node v25.9.0), which cuts
//! `symbolDuration`, `ensureRetime`, `moduleById`, `chainMods`, `montageTotal`,
//! `montageStrokesAt`, `chainModsForView` (`src/js/storyboard.js`) and
//! `resolveSymbolFrameIdx`, the `if(ld.montageId){...}` branch of
//! `getEffectiveStrokes` and the `mods.forEach` block of `enterMontageView`
//! (`src/js/app.js`) verbatim from baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc
//! and runs them in a Node vm (`symbolStrokesAt` stubbed to return the
//! requested symbol id and frame).
//!
//! Rows (space separated; `~` is null/undefined, `-` an empty result): `D sym n`,
//! `R sym tin tout dur tin',tout',dur'`, `T chain total`, `F chain f res`,
//! `L chain placed frame res`, `V chain res`, `O total tin tout placed speed
//! main bits`, `C tin srcLen dur sbFnv onceFnv diffs`. A symbol is `M`
//! (missing) or `P-<totalFrames>-<layers[0] frames>-<name>`; a chain is `_`
//! (empty), `!` (no `chain` field) or `|`-joined slots `X` (dangling id), `N`
//! (sound module) and `I:<sym>:<trimIn>:<trimOut>:<duration>`; slot `i` is
//! module `m<i>` over symbol `s<i>`. `res` is `<symbol id>@<source frame>`.

use super::{
    ensure_retime, montage_layer_frame, montage_source_frame_with, montage_total,
    montage_view_segments, once_symbol_frame, storyboard_source_frame, symbol_duration,
    ChainMember, ChainSlot, LayerFrame, MontageMember, MontageModules, MontageTimingError,
    OnceLayer, Retime, SourceFrame, SourcePick, SymbolView,
};
use std::collections::{BTreeMap, HashMap};

/// Three members: trimmed + defaulted + missing symbol (total 64).
pub(crate) const WALK_TOKEN: &str = "I:P-24-24-a:0:23:24|I:P-12-12-b:2:9:16|I:M:~:~:~";

pub(crate) const ORACLE: &str = include_str!("fixtures/object-montage-timing/oracle.txt");

pub(crate) fn opt(token: &str) -> Option<i64> {
    if token == "~" {
        None
    } else {
        Some(token.parse().expect("integer token"))
    }
}

pub(crate) fn sym_view(token: &str) -> SymbolView {
    if token == "M" {
        return SymbolView::Missing;
    }
    let parts: Vec<&str> = token.splitn(4, '-').collect();
    SymbolView::Present {
        name: if parts[3] == "~" {
            None
        } else {
            Some(parts[3].to_string())
        },
        total_frames: opt(parts[1]).unwrap_or(0),
        first_layer_frame_count: opt(parts[2]),
    }
}

/// A fixture chain: the `chain` id list plus the modules the ids resolve to.
pub(crate) struct Chain {
    pub ids: Vec<String>,
    slots: HashMap<String, ChainSlot>,
}

impl MontageModules for Chain {
    fn slot(&self, module_id: &str) -> ChainSlot {
        self.slots
            .get(module_id)
            .cloned()
            .unwrap_or(ChainSlot::Dangling)
    }
}

pub(crate) fn chain_of(token: &str) -> Chain {
    let mut ids = Vec::new();
    let mut slots = HashMap::new();
    if token != "_" && token != "!" {
        for (i, slot) in token.split('|').enumerate() {
            let id = format!("m{i}");
            ids.push(id.clone());
            match slot {
                "X" => {}
                "N" => {
                    slots.insert(id, ChainSlot::NotInstance);
                }
                _ => {
                    let f: Vec<&str> = slot.split(':').collect();
                    slots.insert(
                        id,
                        ChainSlot::Instance(MontageMember {
                            symbol_id: format!("s{i}"),
                            trim_in: opt(f[2]),
                            trim_out: opt(f[3]),
                            duration: opt(f[4]),
                            symbol: sym_view(f[1]),
                        }),
                    );
                }
            }
        }
    }
    Chain { ids, slots }
}

/// The pieces a control may replace; `PRODUCTION` is the shipped kernel.
#[derive(Clone, Copy)]
pub(crate) struct Impls {
    pub retime: fn(&MontageMember) -> Result<Retime, MontageTimingError>,
    pub pick: SourcePick,
    pub layer_frame: fn(i64, Option<i64>, i64) -> Result<LayerFrame, MontageTimingError>,
}

pub(crate) const PRODUCTION: Impls = Impls {
    retime: ensure_retime,
    pick: storyboard_source_frame,
    layer_frame: montage_layer_frame,
};

/// `resolve_chain` with an injectable `ensureRetime`.
pub(crate) fn resolve(chain: &Chain, impls: &Impls) -> Vec<ChainMember> {
    let mut members = Vec::new();
    for id in &chain.ids {
        if let ChainSlot::Instance(member) = chain.slot(id) {
            let retime = (impls.retime)(&member).expect("retime");
            members.push(ChainMember {
                module_id: id.clone(),
                symbol_id: member.symbol_id,
                retime,
                symbol: member.symbol,
            });
        }
    }
    members
}

pub(crate) fn show(found: Option<SourceFrame>) -> String {
    match found {
        None => "-".to_string(),
        Some(s) => format!("{}@{}", s.symbol_id, s.src_frame),
    }
}

pub(crate) fn bits_hex(x: f64) -> String {
    let bits = if x.is_nan() {
        0x7ff8_0000_0000_0000u64
    } else {
        x.to_bits()
    };
    format!("{bits:016x}")
}

pub(crate) fn fnv(list: &[i64]) -> String {
    let text = list
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub(crate) fn view_row(members: &[ChainMember]) -> String {
    match montage_view_segments(members) {
        Err(MontageTimingError::EmptyChain) => "E".to_string(),
        Err(other) => panic!("view failed: {other:?}"),
        Ok(spec) => {
            let segments: Vec<String> = spec
                .segments
                .iter()
                .map(|s| {
                    let name: String = s.name.bytes().map(|b| format!("{b:02x}")).collect();
                    format!(
                        "{name},{},{},{},{},{},{},{},{},{},{},{}",
                        s.symbol_id,
                        s.sym_play_mode,
                        bits_hex(s.sym_speed),
                        s.sym_placed_at,
                        s.sym_trim_in,
                        s.sym_trim_out,
                        s.sym_single_frame,
                        s.in_point,
                        s.out_point,
                        u8::from(s.locked),
                        u8::from(s.visible)
                    )
                })
                .collect();
            format!("{};{}", spec.total_frames, segments.join(";"))
        }
    }
}

/// Index of the first result token of each row kind.
fn result_start(kind: char) -> usize {
    match kind {
        'D' | 'T' | 'V' => 2,
        'F' => 3,
        'L' | 'C' => 4,
        'R' => 5,
        'O' => 7,
        other => panic!("unknown row kind {other}"),
    }
}

fn eval_row(impls: &Impls, t: &[&str]) -> String {
    match t[0] {
        "D" => symbol_duration(&sym_view(t[1])).to_string(),
        "R" => {
            let member = MontageMember {
                symbol_id: "s0".to_string(),
                trim_in: opt(t[2]),
                trim_out: opt(t[3]),
                duration: opt(t[4]),
                symbol: sym_view(t[1]),
            };
            let r = (impls.retime)(&member).expect("retime");
            format!("{},{},{}", r.trim_in, r.trim_out, r.duration)
        }
        "T" => montage_total(&resolve(&chain_of(t[1]), impls))
            .expect("total")
            .to_string(),
        "F" => {
            let members = resolve(&chain_of(t[1]), impls);
            let frame: i64 = t[2].parse().expect("frame");
            show(montage_source_frame_with(&members, frame, impls.pick).expect("source"))
        }
        "L" => {
            let members = resolve(&chain_of(t[1]), impls);
            let total = montage_total(&members).expect("total");
            let frame: i64 = t[3].parse().expect("frame");
            show(
                match (impls.layer_frame)(frame, opt(t[2]), total).expect("layer frame") {
                    LayerFrame::Local(local) => {
                        montage_source_frame_with(&members, local, impls.pick).expect("source")
                    }
                    LayerFrame::None(_) => None,
                },
            )
        }
        "V" => view_row(&resolve(&chain_of(t[1]), impls)),
        "O" => {
            let layer = OnceLayer {
                sym_trim_in: opt(t[2]),
                sym_trim_out: opt(t[3]),
                sym_placed_at: opt(t[4]),
                sym_speed: if t[5] == "~" {
                    None
                } else {
                    Some(t[5].parse().expect("speed"))
                },
            };
            let total: i64 = t[1].parse().expect("total frames");
            bits_hex(once_symbol_frame(
                total,
                &layer,
                t[6].parse().expect("main frame"),
            ))
        }
        "C" => {
            let trim_in: i64 = t[1].parse().expect("trimIn");
            let src_len: i64 = t[2].parse().expect("srcLen");
            let duration: i64 = t[3].parse().expect("duration");
            let (mut sb, mut once, mut diffs) = (Vec::new(), Vec::new(), Vec::new());
            for local in 0..duration {
                let a = (impls.pick)(trim_in, src_len, local, duration).expect("pick");
                let b = once_for(trim_in, src_len, duration, local);
                if a != b {
                    diffs.push(format!("{local}:{a}:{b}"));
                }
                sb.push(a);
                once.push(b);
            }
            let diffs = if diffs.is_empty() {
                "-".to_string()
            } else {
                diffs.join(",")
            };
            format!("{} {} {diffs}", fnv(&sb), fnv(&once))
        }
        other => panic!("unknown row {other}"),
    }
}

/// The `once` replay of a montage-view segment, as `enterMontageView` builds it.
pub(crate) fn once_for(trim_in: i64, src_len: i64, duration: i64, local: i64) -> i64 {
    let layer = OnceLayer {
        sym_trim_in: Some(trim_in),
        sym_trim_out: Some(trim_in + src_len - 1),
        sym_placed_at: Some(0),
        sym_speed: Some(src_len as f64 / duration as f64),
    };
    once_symbol_frame(trim_in + src_len + 5, &layer, local) as i64
}

/// Rows that disagreed, and how many rows of each kind ran.
pub(crate) struct Report {
    pub bad: Vec<(char, String)>,
    pub kinds: BTreeMap<char, usize>,
}

pub(crate) fn run(impls: &Impls) -> Report {
    let mut report = Report {
        bad: Vec::new(),
        kinds: BTreeMap::new(),
    };
    for (number, line) in ORACLE.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let t: Vec<&str> = line.split(' ').collect();
        let kind = t[0].chars().next().expect("kind");
        *report.kinds.entry(kind).or_insert(0) += 1;
        let want = t[result_start(kind)..].join(" ");
        let got = eval_row(impls, &t);
        if got != want {
            report
                .bad
                .push((kind, format!("line {}: {line} => got [{got}]", number + 1)));
        }
    }
    report
}
