//! Oracle rows and comparison harness for the shot chain kernel (L10-B, I17/I18/I42).
//!
//! `fixtures/object-shot-chain/oracle.txt` was produced by `tools/gen.mjs`, which
//! cuts `newId`, `moduleById`, `symbolDuration`, `ensureRetime`, `chainMods`,
//! `montageTotal`, `chainSplitPoint`, `splitChainClip`, `splitSound`,
//! `addInstance`, `addInstanceAuto`, `addMontage`, the sound-menu condition and
//! the "reset retiming" statement verbatim from `src/js/storyboard.js` at
//! baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc and runs them in a Node vm.
//!
//! Rows (space separated; `~` is undefined, `-` an empty result): `D sym n`,
//! `Z sym tin,tout,dur`, `R sym tin tout dur tin',tout',dur'`,
//! `P chain playhead target local`, `S chain target local L= R= C= T=`,
//! `Q chain target local reason baseline` (inputs the kernel refuses), and the
//! sound / placement rows read by `object_shot_modules_rows.rs`. A symbol is `M`
//! (missing) or `P-<totalFrames>-<layers[0] frames>-<name>`; a chain is `_`
//! (empty), `!` (no `chain` field) or `|`-joined slots `X` (dangling id), `N`
//! (sound module), `R<k>` (module `m<k>` listed again) and
//! `I:<sym>:<trimIn>:<trimOut>:<duration>`; slot `i` is module `m<i>` over
//! symbol `s<i>` at x `0.1 + i * 7.25`, y `5.5 - i * 2.75`.

use super::modules::ModuleRef;
use super::{
    chain_split_point_with, chain_total, ensure_retime, left_src_len, reset_retime,
    split_chain_clip_with, symbol_duration, ChainEntry, ChainSplit, EntryKind, InstanceModule,
    LeftSrcLen, Retime, ShotChainError, ShotHost, SplitRules, SymbolView, BASELINE_RULES,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-shot-chain/oracle.txt");

/// The fields after the kind letter of every oracle row of `kind`.
pub(crate) fn oracle_rows(kind: char) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#') && line.starts_with(kind))
        .filter(|line| line[1..].starts_with(' '))
        .map(|line| line[2..].split(' ').collect())
        .collect()
}

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
        total_frames: opt(parts[1]).unwrap_or(0),
        first_layer_frame_count: opt(parts[2]),
    }
}

fn instance_of(i: usize, slot: &str) -> InstanceModule {
    let f: Vec<&str> = slot.split(':').collect();
    InstanceModule {
        symbol_id: format!("s{}", i),
        x: 0.1 + (i as f64) * 7.25,
        y: 5.5 - (i as f64) * 2.75,
        trim_in: opt(f[2]),
        trim_out: opt(f[3]),
        duration: opt(f[4]),
        symbol: sym_view(f[1]),
    }
}

/// The host a chain token describes.
pub(crate) fn host_of(token: &str, playhead: Option<i64>) -> ShotHost {
    let mut by_slot: Vec<EntryKind> = Vec::new();
    let mut chain = Vec::new();
    if token != "_" && token != "!" {
        for (i, slot) in token.split('|').enumerate() {
            let repeat = slot.strip_prefix('R').and_then(|n| n.parse::<usize>().ok());
            let (module_id, kind) = match (repeat, slot) {
                (Some(k), _) => (format!("m{}", k), by_slot[k].clone()),
                (None, "X") => (format!("m{}", i), EntryKind::Dangling),
                (None, "N") => (format!("m{}", i), EntryKind::NotInstance),
                (None, _) => (format!("m{}", i), EntryKind::Instance(instance_of(i, slot))),
            };
            by_slot.push(kind.clone());
            chain.push(ChainEntry { module_id, kind });
        }
    }
    ShotHost { playhead, chain }
}

/// The member a module id names in `host`.
pub(crate) fn member_of<'a>(host: &'a ShotHost, id: &str) -> &'a InstanceModule {
    match host.chain.iter().find(|entry| entry.module_id == id) {
        Some(ChainEntry {
            kind: EntryKind::Instance(member),
            ..
        }) => member,
        _ => panic!("no instance member {}", id),
    }
}

/// The host after the split edit is published (the new module is `N`).
pub(crate) fn apply_split(host: &ShotHost, split: &ChainSplit) -> ShotHost {
    let source = member_of(host, &split.left.module_id);
    let mut chain = Vec::new();
    for item in &split.chain {
        match item {
            ModuleRef::Existing(id) => {
                let mut entry = host
                    .chain
                    .iter()
                    .find(|candidate| &candidate.module_id == id)
                    .expect("existing entry")
                    .clone();
                if entry.module_id == split.left.module_id {
                    if let EntryKind::Instance(member) = &mut entry.kind {
                        member.trim_in = Some(split.left.trim_in);
                        member.trim_out = Some(split.left.trim_out);
                        member.duration = Some(split.left.duration);
                    }
                }
                chain.push(entry);
            }
            ModuleRef::New(_) => chain.push(ChainEntry {
                module_id: "N".to_string(),
                kind: EntryKind::Instance(InstanceModule {
                    symbol_id: split.right.symbol_id.clone(),
                    x: split.right.x,
                    y: split.right.y,
                    trim_in: split.right.trim_in,
                    trim_out: split.right.trim_out,
                    duration: split.right.duration,
                    symbol: source.symbol.clone(),
                }),
            }),
        }
    }
    ShotHost {
        playhead: host.playhead,
        chain,
    }
}

pub(crate) type SplitFn =
    fn(&ShotHost, &str, i64, LeftSrcLen) -> Result<ChainSplit, ShotChainError>;

/// The pieces a control may replace; `PRODUCTION` is the shipped kernel.
#[derive(Clone, Copy)]
pub(crate) struct Impls {
    pub rules: SplitRules,
    pub pick: LeftSrcLen,
    pub split: SplitFn,
}

pub(crate) const PRODUCTION: Impls = Impls {
    rules: BASELINE_RULES,
    pick: left_src_len,
    split: split_chain_clip_with,
};

/// Rows compared and the rows that disagreed (kind, text).
pub(crate) struct Report {
    pub checked: usize,
    pub bad: Vec<(char, String)>,
}

fn row_text(kind: char, fields: &[&str]) -> String {
    format!("{} {}", kind, fields.join(" "))
}

fn render_chain(split: &ChainSplit) -> String {
    let ids: Vec<String> = split
        .chain
        .iter()
        .map(|item| match item {
            ModuleRef::Existing(id) => id.clone(),
            ModuleRef::New(_) => "N".to_string(),
        })
        .collect();
    ids.join(",")
}

fn split_matches(fields: &[&str], host: &ShotHost, got: &ChainSplit) -> bool {
    let left = format!(
        "L={},{},{}",
        got.left.trim_in, got.left.trim_out, got.left.duration
    );
    let right: Vec<&str> = fields[4].trim_start_matches("R=").split(',').collect();
    let int = |text: &str| Some(text.parse::<i64>().expect("integer"));
    let float = |text: &str| text.parse::<f64>().expect("float").to_bits();
    let right_ok = got.right.symbol_id == right[0]
        && got.right.x.to_bits() == float(right[1])
        && got.right.y.to_bits() == float(right[2])
        && got.right.trim_in == int(right[3])
        && got.right.trim_out == int(right[4])
        && got.right.duration == int(right[5]);
    let totals: Vec<&str> = fields[6].trim_start_matches("T=").split('>').collect();
    let before = chain_total(host).ok().map(|n| n.to_string());
    let after = chain_total(&apply_split(host, got))
        .ok()
        .map(|n| n.to_string());
    left == fields[3]
        && right_ok
        && format!("C={}", render_chain(got)) == fields[5]
        && before.as_deref() == Some(totals[0])
        && after.as_deref() == Some(totals[1])
}

/// Run the `P` and accepted `S` rows through `impls`.
pub(crate) fn run(impls: &Impls) -> Report {
    let mut report = Report {
        checked: 0,
        bad: Vec::new(),
    };
    for fields in oracle_rows('P') {
        let host = host_of(fields[0], opt(fields[1]));
        let got = match chain_split_point_with(&host, fields[2], &impls.rules) {
            Ok(Some(local)) => local.to_string(),
            Ok(None) => "-".to_string(),
            Err(error) => format!("{:?}", error),
        };
        report.checked += 1;
        if got != fields[3] {
            report.bad.push(('P', row_text('P', &fields)));
        }
    }
    for fields in oracle_rows('S') {
        let host = host_of(fields[0], None);
        let local: i64 = fields[2].parse().expect("local");
        let ok = match (impls.split)(&host, fields[1], local, impls.pick) {
            Ok(got) => split_matches(&fields, &host, &got),
            Err(_) => false,
        };
        report.checked += 1;
        if !ok {
            report.bad.push(('S', row_text('S', &fields)));
        }
    }
    report
}

/// The retime defaults a control may replace; `DEFAULTS` is the shipped kernel.
#[derive(Clone, Copy)]
pub(crate) struct DefaultImpls {
    pub duration: fn(&SymbolView) -> i64,
    pub ensure: fn(&InstanceModule) -> Result<Retime, ShotChainError>,
    pub reset: fn(&SymbolView) -> Result<Retime, ShotChainError>,
}

pub(crate) const DEFAULTS: DefaultImpls = DefaultImpls {
    duration: symbol_duration,
    ensure: ensure_retime,
    reset: reset_retime,
};

fn triple(retime: Result<Retime, ShotChainError>) -> Option<String> {
    retime
        .ok()
        .map(|r| format!("{},{},{}", r.trim_in, r.trim_out, r.duration))
}

/// The `D`, `Z` and `R` rows through `impls`.
pub(crate) fn run_defaults(impls: &DefaultImpls) -> Report {
    let mut report = Report {
        checked: 0,
        bad: Vec::new(),
    };
    for fields in oracle_rows('D') {
        report.checked += 1;
        if (impls.duration)(&sym_view(fields[0])).to_string() != fields[1] {
            report.bad.push(('D', row_text('D', &fields)));
        }
    }
    for fields in oracle_rows('Z') {
        report.checked += 1;
        if triple((impls.reset)(&sym_view(fields[0]))).as_deref() != Some(fields[1]) {
            report.bad.push(('Z', row_text('Z', &fields)));
        }
    }
    for fields in oracle_rows('R') {
        report.checked += 1;
        let member = InstanceModule {
            symbol_id: "s0".to_string(),
            x: 0.0,
            y: 0.0,
            trim_in: opt(fields[1]),
            trim_out: opt(fields[2]),
            duration: opt(fields[3]),
            symbol: sym_view(fields[0]),
        };
        if triple((impls.ensure)(&member)).as_deref() != Some(fields[4]) {
            report.bad.push(('R', row_text('R', &fields)));
        }
    }
    report
}
