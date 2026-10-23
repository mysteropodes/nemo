//! Native StoryBoard records (L04-G, I14/I35): `state.storyboard` of baseline
//! `src/js/storyboard.js` at fbe4b8a3 as storage DTOs plus their admission rules.
//!
//! Modules are `instance` (a Component placed in the node space, with optional
//! `trimIn` / `trimOut` / `duration` retiming set when it joins a chain),
//! `montage` (the anchor block whose `chain` ORDER is the sequence and whose
//! `audio` lists the sounds snapped below it) and `sound`. The chain mapping
//! (`montageTotal`, `montageStrokesAt`) is the StoryBoard kernel; this module
//! only decides which records may exist.
//!
//! Faithful to the baseline, not stricter than it: a chain or audio entry that
//! names no module (or a module of another type) is representable because
//! `chainMods` skips it; a module that names a missing symbol is refused.
//! Not carried: `_v2` (a one-way migration marker; native records are always
//! the v2 model) and a sound's inline `dataB64`, which becomes an `assetId`
//! into the project asset inventory (resolved at document admission, L04-I).
use crate::object_document::{object_deserialize, AssetId, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::{HashMap, HashSet};

pub(crate) const STORYBOARD_FORMAT: &str = "nemo.native-storyboard";
pub(crate) const STORYBOARD_FORMAT_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 4096;
const MAX_ID_BYTES: usize = 255;
/// `newId()` is `'sbm' + nextId++`.
const ID_PREFIX: &str = "sbm";
const INSTANCE_FIELDS: [&str; 4] = ["symbolId", "trimIn", "trimOut", "duration"];
const MONTAGE_FIELDS: [&str; 4] = ["name", "playhead", "chain", "audio"];
const SOUND_FIELDS: [&str; 5] = ["name", "assetId", "inSec", "outSec", "cursorSec"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoryboardRecordError {
    Invalid(&'static str),
    Unsupported(&'static str),
    DuplicateId(&'static str),
    Unresolved(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryboardRecord {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pan: Presence<StoryboardPan>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) zoom: Presence<Number>,
    pub(crate) next_id: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) active_montage_id: Presence<Option<String>>,
    pub(crate) modules: Vec<StoryboardModule>,
}
object_deserialize!(StoryboardRecord {
    format: String, format_version: u32, #[serde(default)] pan: Presence<StoryboardPan>,
    #[serde(default)] zoom: Presence<Number>, next_id: u32,
    #[serde(default)] active_montage_id: Presence<Option<String>>,
    #[serde(deserialize_with = "bounded_entries")] modules: Vec<StoryboardModule>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryboardPan {
    pub(crate) x: Number,
    pub(crate) y: Number,
}
object_deserialize!(StoryboardPan {
    x: Number,
    y: Number
});

/// One module. The field set a type may carry is fixed by `validate`; a field
/// of another type is refused rather than ignored.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryboardModule {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) symbol_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    pub(crate) x: Number,
    pub(crate) y: Number,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) trim_in: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) trim_out: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) duration: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) playhead: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) chain: Presence<Vec<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) audio: Presence<Vec<AudioJoin>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) asset_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) in_sec: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) out_sec: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) cursor_sec: Presence<Option<Number>>,
}
object_deserialize!(StoryboardModule {
    id: String, #[serde(rename = "type")] kind: String,
    #[serde(default)] symbol_id: Presence<String>, #[serde(default)] name: Presence<String>,
    x: Number, y: Number,
    #[serde(default)] trim_in: Presence<Number>, #[serde(default)] trim_out: Presence<Number>,
    #[serde(default)] duration: Presence<Number>, #[serde(default)] playhead: Presence<Number>,
    #[serde(default, deserialize_with = "bounded_presence")] chain: Presence<Vec<String>>,
    #[serde(default, deserialize_with = "bounded_presence")] audio: Presence<Vec<AudioJoin>>,
    #[serde(default)] asset_id: Presence<String>,
    #[serde(default)] in_sec: Presence<Option<Number>>,
    #[serde(default)] out_sec: Presence<Option<Number>>,
    #[serde(default)] cursor_sec: Presence<Option<Number>>
});

/// `{ moduleId, offsetFrames }` of `montage.audio`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioJoin {
    pub(crate) module_id: String,
    pub(crate) offset_frames: Number,
}
object_deserialize!(AudioJoin {
    module_id: String,
    offset_frames: Number
});

// Refuse oversized arrays during decoding, before an unbounded Vec allocation.
fn bounded_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Entries<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Entries<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 4096 StoryBoard entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == MAX_ENTRIES {
                    return Err(serde::de::Error::custom("too many StoryBoard entries"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries(std::marker::PhantomData))
}
fn bounded_presence<'de, D, T>(deserializer: D) -> Result<Presence<Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    bounded_entries(deserializer).map(Presence::Present)
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}
fn invalid(message: &'static str) -> StoryboardRecordError {
    StoryboardRecordError::Invalid(message)
}
/// The counter value of an id minted by `newId()`, `None` for any other spelling.
fn allocated_number(id: &str) -> Option<u64> {
    let digits = id.strip_prefix(ID_PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok()
}

impl StoryboardRecord {
    /// `symbol_ids` are the component symbols of the same document.
    pub(crate) fn validate(&self, symbol_ids: &[&str]) -> Result<(), StoryboardRecordError> {
        if self.format != STORYBOARD_FORMAT || self.format_version != STORYBOARD_FORMAT_VERSION {
            return Err(StoryboardRecordError::Unsupported(
                "unsupported StoryBoard format or formatVersion",
            ));
        }
        if self.modules.len() > MAX_ENTRIES {
            return Err(invalid("too many StoryBoard modules"));
        }
        let mut ids = HashSet::new();
        for module in &self.modules {
            if !valid_id(&module.id) {
                return Err(invalid("module id must be a non-empty identifier"));
            }
            if !ids.insert(module.id.as_str()) {
                return Err(StoryboardRecordError::DuplicateId("duplicate module id"));
            }
            if allocated_number(&module.id).is_some_and(|n| n >= u64::from(self.next_id)) {
                return Err(invalid("nextId must exceed every allocated module id"));
            }
            module.validate(symbol_ids)?;
        }
        let kinds: HashMap<&str, &str> = self
            .modules
            .iter()
            .map(|m| (m.id.as_str(), m.kind.as_str()))
            .collect();
        if let Some(Some(active)) = self.active_montage_id.value() {
            if kinds.get(active.as_str()) != Some(&"montage") {
                return Err(StoryboardRecordError::Unresolved(
                    "activeMontageId names no montage module",
                ));
            }
        }
        // `desnap` removes a module from its chain before it joins another, so a
        // module sits in at most one chain once and in at most one audio lane once.
        let mut chained = HashSet::new();
        let mut sounds = HashSet::new();
        for module in &self.modules {
            for id in module.chain() {
                if id.is_empty() || !chained.insert(id.as_str()) {
                    return Err(StoryboardRecordError::DuplicateId(
                        "chain entry is empty or listed twice",
                    ));
                }
            }
            for join in module.audio() {
                let offset = join
                    .offset_frames
                    .as_f64()
                    .filter(|o| o.is_finite() && *o >= 0.0);
                if join.module_id.is_empty() || offset.is_none() {
                    return Err(invalid(
                        "audio join needs a moduleId and a finite offset >= 0",
                    ));
                }
                if !sounds.insert(join.module_id.as_str()) {
                    return Err(StoryboardRecordError::DuplicateId(
                        "sound module joined twice",
                    ));
                }
                if kinds
                    .get(join.module_id.as_str())
                    .is_some_and(|k| *k != "sound")
                {
                    return Err(invalid("audio join names a module that is not a sound"));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn modules(&self) -> &[StoryboardModule] {
        &self.modules
    }
    pub(crate) fn module(&self, id: &str) -> Option<&StoryboardModule> {
        self.modules.iter().find(|m| m.id == id)
    }
    pub(crate) fn next_id(&self) -> u32 {
        self.next_id
    }
    /// `None` for absent and for null.
    pub(crate) fn active_montage_id(&self) -> Option<&str> {
        self.active_montage_id.value().and_then(|a| a.as_deref())
    }
}

impl StoryboardModule {
    fn validate(&self, symbol_ids: &[&str]) -> Result<(), StoryboardRecordError> {
        let present = [
            ("symbolId", !self.symbol_id.missing()),
            ("name", !self.name.missing()),
            ("trimIn", !self.trim_in.missing()),
            ("trimOut", !self.trim_out.missing()),
            ("duration", !self.duration.missing()),
            ("playhead", !self.playhead.missing()),
            ("chain", !self.chain.missing()),
            ("audio", !self.audio.missing()),
            ("assetId", !self.asset_id.missing()),
            ("inSec", !self.in_sec.missing()),
            ("outSec", !self.out_sec.missing()),
            ("cursorSec", !self.cursor_sec.missing()),
        ];
        let allowed: &[&str] = match self.kind.as_str() {
            "instance" => &INSTANCE_FIELDS,
            "montage" => &MONTAGE_FIELDS,
            "sound" => &SOUND_FIELDS,
            _ => {
                return Err(StoryboardRecordError::Unsupported(
                    "module type is not instance, montage or sound",
                ))
            }
        };
        if present
            .iter()
            .any(|(name, here)| *here && !allowed.contains(name))
        {
            return Err(invalid("module carries a field of another module type"));
        }
        match self.kind.as_str() {
            "instance" => self.validate_instance(symbol_ids),
            "sound" => {
                let asset = self
                    .asset_id
                    .value()
                    .ok_or(invalid("a sound needs an assetId"))?;
                AssetId::new(asset.clone())
                    .map_err(|_| invalid("sound assetId is not a valid asset id"))?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
    /// The retime fields feed `trimIn + min(srcLen - 1, floor(local * srcLen / duration))`:
    /// a non-positive duration or a range with `trimOut < trimIn` leaves it undefined.
    fn validate_instance(&self, symbol_ids: &[&str]) -> Result<(), StoryboardRecordError> {
        let symbol = self
            .symbol_id
            .value()
            .ok_or(invalid("an instance module needs a symbolId"))?;
        if !symbol_ids.contains(&symbol.as_str()) {
            return Err(StoryboardRecordError::Unresolved(
                "instance module names a missing symbol",
            ));
        }
        let trim_in = self.trim_in.value().and_then(Number::as_f64);
        let trim_out = self.trim_out.value().and_then(Number::as_f64);
        let duration = self.duration.value().and_then(Number::as_f64);
        if duration.is_some_and(|d| !d.is_finite() || d <= 0.0)
            || trim_in.is_some_and(|t| !t.is_finite() || t < 0.0)
            || trim_out.is_some_and(|t| !t.is_finite() || t < trim_in.unwrap_or(0.0))
        {
            return Err(invalid(
                "retime needs duration > 0 and 0 <= trimIn <= trimOut",
            ));
        }
        Ok(())
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }
    pub(crate) fn symbol_id(&self) -> Option<&str> {
        self.symbol_id.value().map(String::as_str)
    }
    pub(crate) fn x(&self) -> Option<f64> {
        self.x.as_f64()
    }
    pub(crate) fn y(&self) -> Option<f64> {
        self.y.as_f64()
    }
    pub(crate) fn trim_in(&self) -> Option<f64> {
        self.trim_in.value().and_then(Number::as_f64)
    }
    pub(crate) fn trim_out(&self) -> Option<f64> {
        self.trim_out.value().and_then(Number::as_f64)
    }
    pub(crate) fn duration(&self) -> Option<f64> {
        self.duration.value().and_then(Number::as_f64)
    }
    /// The montage sequence in authored order (ids may dangle; the kernel skips those).
    pub(crate) fn chain(&self) -> &[String] {
        match self.chain.value() {
            Some(chain) => chain.as_slice(),
            None => &[],
        }
    }
    pub(crate) fn audio(&self) -> &[AudioJoin] {
        match self.audio.value() {
            Some(audio) => audio.as_slice(),
            None => &[],
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_storyboard_records_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_storyboard_records.rs"]
mod tests;
