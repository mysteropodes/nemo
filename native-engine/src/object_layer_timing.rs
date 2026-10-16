//! Sparse authored drawing timing; document data, never a second live owner.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};

// L05-A (#2132): private sparse frame-edit kernel over the private LayerTiming fields.
#[path = "object_frame_edit_sparse.rs"]
#[allow(dead_code)]
mod object_frame_edit_sparse;
#[allow(unused_imports)]
pub(crate) use object_frame_edit_sparse::TimingEdit;

const MAX_ENTRIES: usize = 4096;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerTimingError {
    Invalid,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayerTiming {
    format: String,
    format_version: u32,
    family: String,
    context_id: String,
    keys: Vec<TimingKey>,
    samples: Vec<TimingSample>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TimingKey {
    frame: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimingSample {
    frame: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    manual_edit: Presence<bool>,
}
object_deserialize!(LayerTiming { format: String, format_version: u32, family: String,
    context_id: String, #[serde(deserialize_with = "bounded_entries")] keys: Vec<TimingKey>,
    #[serde(deserialize_with = "bounded_entries")] samples: Vec<TimingSample> });
object_deserialize!(TimingKey { frame: u32 });
object_deserialize!(TimingSample { frame: u32, #[serde(default)] manual_edit: Presence<bool> });

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
            f.write_str("at most 4096 sparse timing entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == MAX_ENTRIES {
                    return Err(serde::de::Error::custom("too many sparse timing entries"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries(std::marker::PhantomData))
}

impl LayerTiming {
    pub(crate) fn blank_drawing() -> Self {
        Self {
            format: "nemo.native-layer-timing".into(),
            format_version: 1,
            family: "drawing".into(),
            context_id: "scene-root".into(),
            keys: vec![TimingKey { frame: 0 }],
            samples: vec![],
        }
    }
    pub(crate) fn validate(&self, total: u32) -> Result<(), LayerTimingError> {
        let invalid = || LayerTimingError::Invalid;
        if self.format != "nemo.native-layer-timing"
            || self.format_version != 1
            || self.family != "drawing"
            || self.context_id != "scene-root"
        {
            return Err(LayerTimingError::Unsupported);
        }
        if self
            .keys
            .len()
            .checked_add(self.samples.len())
            .is_none_or(|n| n > MAX_ENTRIES)
        {
            return Err(invalid());
        }
        fn ordered(frames: impl Iterator<Item = u32>, total: u32) -> bool {
            let mut prior = None;
            for frame in frames {
                if frame >= total || prior.is_some_and(|previous| frame <= previous) {
                    return false;
                }
                prior = Some(frame);
            }
            true
        }
        if !ordered(self.keys.iter().map(|key| key.frame), total)
            || !ordered(self.samples.iter().map(|sample| sample.frame), total)
            || self.samples.iter().any(|sample| {
                self.keys
                    .binary_search_by_key(&sample.frame, |key| key.frame)
                    .is_ok()
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Two sparse binary lookups: interpolated samples never become hold keys.
    pub(crate) fn exposure(&self, frame: u32) -> (&'static str, Option<u32>, bool) {
        let after = self.keys.partition_point(|key| key.frame <= frame);
        let key = after.checked_sub(1).map(|index| self.keys[index].frame);
        if key == Some(frame) {
            return ("key", key, false);
        }
        if let Ok(index) = self
            .samples
            .binary_search_by_key(&frame, |sample| sample.frame)
        {
            return (
                "interpolated",
                Some(frame),
                self.samples[index]
                    .manual_edit
                    .value()
                    .copied()
                    .unwrap_or(false),
            );
        }
        if key.is_some() {
            ("held", key, false)
        } else {
            ("unexposed", None, false)
        }
    }
}
