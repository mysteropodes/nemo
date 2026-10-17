//! Oracle tests for the media binding plan (G42, L09-G), plus the builders the
//! pin, link and control files share.
//!
//! `fixtures/object-media-link-plan/oracle.txt` was produced by running the
//! SHIPPED `_targetFor`/`_targetExactFor`/`_interpMode`/`_blendWeight` and the
//! frame-selection region of `_layerFrameSyncAsync` (`src/js/native-video-bridge.js`)
//! and the shipped `startTrack` (`src/js/audio-bridge.js`), cut verbatim at
//! fbe4b8a3 and evaluated in a Node `vm` (generator `tools/gen.js`). The
//! baseline keeps the offset in `nativeVideo.offsetFrames` and
//! `track.offsetFrames`; every row feeds it the SAME decimal i64 the native
//! catalog stores in `AssetPlacement::offset_frames`, so the plan must produce
//! both outcomes from that one field. Values are IEEE 754 bit patterns in hex.
//! Rows: `V fc off tr imode fb rv frame S:<f>|B:<from>:<to>:<w>`,
//! `A off buf ts te anim ct fps frame N:<Reason>|Y:<when>:<offset>:<dur>:<gain>`.

use std::collections::HashMap;
use std::sync::Arc;

use super::controls::Mutation;
use super::{plan_frame, AudioFacts, BoundMedia, MediaFacts, PlanEntry, PlanInput, VideoFacts};
use crate::object_document::media_timing::object_media_link_join::LocatorNamespace;
use crate::object_document::media_timing::object_media_link_state::ResolveTable;
use crate::object_document::media_timing::object_media_timing_audio::{
    PlaybackEnv, StartOutcome, TrackGain, TrackVolume, VolumeSampler,
};
use crate::object_document::media_timing::object_media_timing_video::{
    FrameSelection, TimeRemapPort,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
};

const ORACLE: &str = include_str!("fixtures/object-media-link-plan/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn h(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

// ---- shared builders ------------------------------------------------------

pub(crate) fn aid(s: &str) -> AssetId {
    AssetId::new(s.to_string()).expect("valid asset id")
}

pub(crate) fn embedded(kind: AssetKind, id: &str) -> Arc<AssetRecord> {
    let media_type = match kind {
        AssetKind::Image => "image/png",
        AssetKind::Video => "video/mp4",
        AssetKind::Audio => "audio/mpeg",
        AssetKind::Font => "font/ttf",
    };
    let source =
        AssetSource::admitted_embedded(media_type.to_string(), Arc::new(vec![1, 2, 3]), 1 << 16)
            .expect("embedded source");
    Arc::new(AssetRecord::new(aid(id), format!("{id}.bin"), kind, source).expect("record"))
}

pub(crate) fn linked(kind: AssetKind, id: &str, locator: &str) -> Arc<AssetRecord> {
    let source = AssetSource::linked(locator.to_string(), 4096).expect("linked source");
    Arc::new(AssetRecord::new(aid(id), format!("{id}.bin"), kind, source).expect("record"))
}

pub(crate) fn drawing(layer: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "ctx".to_string(),
        layer_uid: layer.to_string(),
        stroke_id: None,
    }
}

pub(crate) fn track(uid: &str) -> AssetTarget {
    AssetTarget::AudioTrack {
        track_uid: uid.to_string(),
    }
}

pub(crate) fn font(layer: &str) -> AssetTarget {
    AssetTarget::Text {
        context_id: "ctx".to_string(),
        layer_uid: layer.to_string(),
        stroke_id: None,
    }
}

pub(crate) fn place(pid: &str, asset: &str, target: AssetTarget, off: i64) -> AssetPlacement {
    AssetPlacement::new(pid.to_string(), aid(asset), target, off).expect("placement")
}

pub(crate) fn inventory(
    records: Vec<Arc<AssetRecord>>,
    placements: Vec<AssetPlacement>,
) -> AssetInventory {
    AssetInventory::new(records, placements, 64, 64).expect("inventory")
}

/// A time-remap port with a fixed answer.
pub(crate) struct Fixed(pub Option<f64>);

impl TimeRemapPort for Fixed {
    fn remap_value(&self, _frame: f64) -> Option<f64> {
        self.0
    }
}

/// The generator's `SMMotion.valueAtFrame` stub: four pure profiles of the frame.
pub(crate) struct Profile(pub u8);

impl VolumeSampler for Profile {
    fn volume_at(&self, f: f64) -> Option<f64> {
        Some(match self.0 {
            0 => f,
            1 => f * 0.02 - 0.5,
            2 => (f % 7.0) / 7.0 - 0.2,
            3 => 0.1,
            other => panic!("unknown profile {other}"),
        })
    }
}

#[derive(Default)]
pub(crate) struct FactsEntry {
    pub video: Option<VideoFacts>,
    pub remap: Option<Fixed>,
    pub audio: Option<AudioFacts>,
    pub sampler: Option<Profile>,
}

/// Facts keyed by placement id; a placement without an entry gets `None`.
#[derive(Default)]
pub(crate) struct Facts(pub HashMap<String, FactsEntry>);

impl Facts {
    pub(crate) fn entry(&mut self, pid: &str) -> &mut FactsEntry {
        self.0.entry(pid.to_string()).or_default()
    }
}

impl MediaFacts for Facts {
    fn video(&self, id: &str) -> Option<VideoFacts> {
        self.0.get(id)?.video.clone()
    }
    fn time_remap(&self, id: &str) -> Option<&dyn TimeRemapPort> {
        self.0
            .get(id)?
            .remap
            .as_ref()
            .map(|p| p as &dyn TimeRemapPort)
    }
    fn audio(&self, id: &str) -> Option<AudioFacts> {
        self.0.get(id)?.audio
    }
    fn volume_sampler(&self, id: &str) -> Option<&dyn VolumeSampler> {
        self.0
            .get(id)?
            .sampler
            .as_ref()
            .map(|p| p as &dyn VolumeSampler)
    }
}

pub(crate) fn env(fps: f64) -> PlaybackEnv {
    PlaybackEnv {
        fps,
        context_isolated: false,
        audio_context: true,
    }
}

/// Plan against an empty resolve table (embedded assets need no link).
pub(crate) fn plan_embedded(
    inv: &AssetInventory,
    facts: &Facts,
    fps: f64,
    frame: f64,
) -> Vec<PlanEntry> {
    let table = ResolveTable::new();
    let input = PlanInput {
        inventory: inv,
        links: &table,
        namespace: LocatorNamespace::DesktopPath,
        now_ms: 0,
        env: env(fps),
        facts,
    };
    plan_frame(&input, frame).expect("finite frame")
}

// ---- oracle driver ----------------------------------------------------------

fn render_selection(s: &FrameSelection) -> String {
    match s {
        FrameSelection::Single { frame } => format!("S:{}", h(*frame)),
        FrameSelection::Blend(b) => format!("B:{}:{}:{}", h(b.from), h(b.to), h(b.weight)),
    }
}

/// FNV-1a over the little-endian bytes of each `f32`, as the generator hashes.
fn fnv(values: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for v in values {
        for b in v.to_bits().to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

fn render_start(o: &StartOutcome, ct: f64) -> String {
    let s = match o {
        StartOutcome::Silent(why) => return format!("N:{why:?}"),
        StartOutcome::Play(s) => s,
    };
    let gain = match &s.gain {
        TrackGain::Value(v) => format!("v:{}", h(*v)),
        TrackGain::Curve(c) => format!(
            "c:{}:{}:{}:{:016x}:{:08x}:{:08x}",
            h(s.audio_time(ct)),
            h(c.duration_s),
            c.values.len(),
            fnv(&c.values),
            c.values[0].to_bits(),
            c.values[c.values.len() - 1].to_bits()
        ),
    };
    format!(
        "Y:{}:{}:{}:{}",
        h(s.src_when(ct)),
        h(s.offset_s),
        h(s.duration_s),
        gain
    )
}

#[derive(Default)]
pub(crate) struct Run {
    pub video: usize,
    pub audio: usize,
    pub single: usize,
    pub blend: usize,
    pub silent: usize,
    pub flat: usize,
    pub curves: usize,
    pub bad: Vec<String>,
}

fn video_row(t: &[&str], m: Mutation) -> (String, String) {
    let off: i64 = t[2].parse().expect("decimal offset");
    let record = embedded(AssetKind::Video, "vid");
    let inv = inventory(
        vec![record],
        vec![place("pv", "vid", drawing("lv"), m.offset(off))],
    );
    let mut facts = Facts::default();
    let e = facts.entry("pv");
    e.video = Some(VideoFacts {
        frame_count: hex(t[1]),
        time_remap: t[3] == "1",
        interp_mode: match t[4] {
            "u" => None,
            other => Some(other.strip_prefix("s:").expect("s: prefix").to_string()),
        },
        frame_blend: t[5] == "1",
    });
    e.remap = Some(Fixed(if t[6] == "-" { None } else { Some(hex(t[6])) }));
    let entries = plan_embedded(&inv, &facts, 24.0, hex(t[7]));
    let got = match entries.as_slice() {
        [PlanEntry::Bound(b)] => match &b.media {
            BoundMedia::Video(sel) => render_selection(sel),
            other => format!("not a video binding: {other:?}"),
        },
        other => format!("not one bound entry: {other:?}"),
    };
    (got, t[8].to_string())
}

fn audio_row(t: &[&str], m: Mutation) -> (String, String) {
    let off: i64 = t[1].parse().expect("decimal offset");
    let anim: Option<u8> = if t[5] == "-" {
        None
    } else {
        Some(t[5].parse().expect("profile digit"))
    };
    let inv = inventory(
        vec![embedded(AssetKind::Audio, "aud")],
        vec![place("pa", "aud", track("ta"), m.offset(off))],
    );
    let mut facts = Facts::default();
    let e = facts.entry("pa");
    e.audio = Some(AudioFacts {
        muted: false,
        buffer_duration: Some(hex(t[2])),
        trim_start: hex(t[3]),
        trim_end: if t[4] == "u" { None } else { Some(hex(t[4])) },
        // The generator leaves every volume property absent: the flat default is 1.
        volume: if anim.is_some() {
            TrackVolume::Animated
        } else {
            TrackVolume::Flat(1.0)
        },
    });
    e.sampler = anim.map(Profile);
    let entries = plan_embedded(&inv, &facts, hex(t[7]), hex(t[8]));
    let got = match entries.as_slice() {
        [PlanEntry::Bound(b)] => match &b.media {
            BoundMedia::Audio(o) => render_start(o, hex(t[6])),
            other => format!("not an audio binding: {other:?}"),
        },
        other => format!("not one bound entry: {other:?}"),
    };
    (got, t[9].to_string())
}

pub(crate) fn run(m: Mutation) -> Run {
    let mut r = Run::default();
    for line in ORACLE.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        let (got, want) = match t[0] {
            "V" => {
                r.video += 1;
                if t[8].starts_with("S:") {
                    r.single += 1;
                } else {
                    r.blend += 1;
                }
                video_row(&t, m)
            }
            "A" => {
                r.audio += 1;
                match t[9].as_bytes()[0] {
                    b'N' => r.silent += 1,
                    _ if t[9].contains(":c:") => r.curves += 1,
                    _ => r.flat += 1,
                }
                audio_row(&t, m)
            }
            other => panic!("unknown oracle row kind {other}"),
        };
        if got != want {
            r.bad.push(format!("{line}\n  got  {got}"));
        }
    }
    r
}

#[test]
fn generated_oracle_matches_baseline() {
    let r = run(Mutation::None);
    assert!(
        r.bad.is_empty(),
        "{} mismatches, first: {}",
        r.bad.len(),
        r.bad[0]
    );
}

#[test]
fn oracle_has_the_documented_row_counts() {
    let r = run(Mutation::None);
    assert_eq!((r.video, r.single, r.blend), (1168, 1072, 96));
    assert_eq!((r.audio, r.silent, r.flat, r.curves), (1032, 344, 608, 80));
}

#[test]
fn oracle_pins_the_catalog_offset_extremes_for_both_media() {
    let max = "9007199254740991".to_string();
    let min = "-9007199254740991".to_string();
    for (kind, col) in [("V", 2), ("A", 1)] {
        for want in [&max, &min] {
            let n = ORACLE
                .lines()
                .map(|l| l.split(' ').collect::<Vec<_>>())
                .filter(|t| t[0] == kind && t[col] == want.as_str())
                .count();
            assert!(n > 0, "no {kind} row with offset {want}");
        }
    }
}
