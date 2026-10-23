//! Persisted baseline tween arcs and standing key locks in the existing document.
//! Literal UID/frame-pair keys and insertion order survive codec/rekey; no live state or cache.
use crate::object_document::{object_deserialize, ObjectDocument, Presence};
use serde::de::{Error as _, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;
use std::collections::HashSet;
use std::fmt;

const MAX_ENTRIES: usize = 4096;
const MAX_KEY_BYTES: usize = 1024;
const MAX_STORE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SAFE: i64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MotionArcError {
    Invalid,
}

/// JSON must be the original mode string, never Serde's externally tagged object form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum KeyLockMode {
    In,
    Out,
    Layer,
}
impl<'de> Deserialize<'de> for KeyLockMode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Mode;
        impl Visitor<'_> for Mode {
            type Value = KeyLockMode;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("in, out or layer mode string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "in" => Ok(KeyLockMode::In),
                    "out" => Ok(KeyLockMode::Out),
                    "layer" => Ok(KeyLockMode::Layer),
                    _ => Err(E::custom("unknown key lock mode")),
                }
            }
        }
        d.deserialize_str(Mode)
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
struct ArcRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    out: Presence<Option<[Number; 2]>>,
    #[serde(rename = "in", skip_serializing_if = "Presence::missing")]
    incoming: Presence<Option<[Number; 2]>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    cx: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    cy: Presence<Option<Number>>,
}
object_deserialize!(ArcRecord {
    #[serde(default)] out: Presence<Option<[Number;2]>>,
    #[serde(default,rename="in")] incoming: Presence<Option<[Number;2]>>,
    #[serde(default)] cx: Presence<Option<Number>>, #[serde(default)] cy: Presence<Option<Number>>
});
fn finite(n: &Number) -> bool {
    n.as_f64().is_some_and(f64::is_finite)
}
impl ArcRecord {
    fn valid(&self) -> bool {
        [&self.out, &self.incoming].iter().all(|slot| {
            slot.value()
                .and_then(Option::as_ref)
                .is_none_or(|point| point.iter().all(finite))
        }) && [&self.cx, &self.cy]
            .iter()
            .all(|slot| slot.value().and_then(Option::as_ref).is_none_or(finite))
    }
}
/// Ordered JSON object, not a Value mirror: strict records plus exactly retained authored key order.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MotionArcs {
    entries: Vec<(String, Option<ArcRecord>)>,
}
impl MotionArcs {
    fn checked(&self) -> Result<(), MotionArcError> {
        if self.entries.len() > MAX_ENTRIES {
            return Err(MotionArcError::Invalid);
        }
        let mut seen = HashSet::new();
        let mut bytes = 2usize;
        for (i, (key, value)) in self.entries.iter().enumerate() {
            if key.len() > MAX_KEY_BYTES
                || !seen.insert(key)
                || value.as_ref().is_some_and(|v| !v.valid())
            {
                return Err(MotionArcError::Invalid);
            }
            let k = serde_json::to_vec(key)
                .map_err(|_| MotionArcError::Invalid)?
                .len();
            let v = serde_json::to_vec(value)
                .map_err(|_| MotionArcError::Invalid)?
                .len();
            bytes = bytes
                .checked_add(k + v + 1 + usize::from(i > 0))
                .ok_or(MotionArcError::Invalid)?;
            if bytes > MAX_STORE_BYTES {
                return Err(MotionArcError::Invalid);
            }
        }
        Ok(())
    }
    fn retimed(&self, layer: &str, pairs: &[(i64, i64, i64, i64)]) -> Result<Self, MotionArcError> {
        check_pairs(layer, pairs)?;
        let mut next = self.clone();
        for &(a, b, new_a, new_b) in pairs {
            if (a, b) == (new_a, new_b) {
                continue;
            }
            let old = format!("{layer}:{a}-{b}-");
            let new = format!("{layer}:{new_a}-{new_b}-");
            let keys = next
                .entries
                .iter()
                .filter(|(key, _)| key.starts_with(&old))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            for key in keys {
                let index = next
                    .entries
                    .iter()
                    .position(|(k, _)| k == &key)
                    .ok_or(MotionArcError::Invalid)?;
                let (_, value) = next.entries.remove(index);
                let destination = format!("{new}{}", &key[old.len()..]);
                // JS delete+assignment: overwrite keeps an existing target's position; new target appends.
                if let Some((_, existing)) =
                    next.entries.iter_mut().find(|(key, _)| key == &destination)
                {
                    *existing = value;
                } else {
                    next.entries.push((destination, value));
                }
            }
        }
        next.checked()?;
        Ok(next)
    }
    fn handles(
        &self,
        layer: &str,
        a_frame: i64,
        b_frame: i64,
        index: u32,
        a: [f64; 2],
        b: [f64; 2],
    ) -> Result<MotionArcHandles, MotionArcError> {
        if !a.into_iter().chain(b).all(f64::is_finite) {
            return Err(MotionArcError::Invalid);
        }
        let key = format!("{layer}:{a_frame}-{b_frame}-{index}");
        let record = self
            .entries
            .iter()
            .find(|(k, _)| k == &key)
            .and_then(|(_, value)| value.as_ref());
        let out = record.and_then(|r| r.out.value()).and_then(Option::as_ref);
        let incoming = record
            .and_then(|r| r.incoming.value())
            .and_then(Option::as_ref);
        let out = out
            .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
            .unwrap_or([(b[0] - a[0]) / 3., (b[1] - a[1]) / 3.]);
        let incoming = incoming
            .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
            .unwrap_or([-(b[0] - a[0]) / 3., -(b[1] - a[1]) / 3.]);
        let handles = MotionArcHandles {
            out: [a[0] + out[0], a[1] + out[1]],
            incoming: [b[0] + incoming[0], b[1] + incoming[1]],
        };
        if !handles
            .out
            .into_iter()
            .chain(handles.incoming)
            .all(f64::is_finite)
        {
            return Err(MotionArcError::Invalid);
        }
        Ok(handles)
    }
}
impl Serialize for MotionArcs {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.checked()
            .map_err(|_| <S::Error as serde::ser::Error>::custom("invalid bounded motion arcs"))?;
        let mut map = s.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for MotionArcs {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Arcs;
        impl<'de> Visitor<'de> for Arcs {
            type Value = MotionArcs;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded motion arc object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = vec![];
                let mut seen = HashSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if entries.len() >= MAX_ENTRIES
                        || key.len() > MAX_KEY_BYTES
                        || !seen.insert(key.clone())
                    {
                        return Err(A::Error::custom("invalid or duplicate motion arc key"));
                    }
                    entries.push((key, map.next_value::<Option<ArcRecord>>()?));
                }
                let store = MotionArcs { entries };
                store
                    .checked()
                    .map_err(|_| A::Error::custom("invalid bounded motion arc record"))?;
                Ok(store)
            }
        }
        d.deserialize_map(Arcs)
    }
}
fn check_pairs(layer: &str, pairs: &[(i64, i64, i64, i64)]) -> Result<(), MotionArcError> {
    if layer.is_empty()
        || pairs.len() > MAX_ENTRIES
        || pairs.iter().any(|p| {
            [p.0, p.1, p.2, p.3]
                .iter()
                .any(|v| !(-MAX_SAFE..=MAX_SAFE).contains(v))
        })
    {
        return Err(MotionArcError::Invalid);
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct MotionArcHandles {
    pub(crate) out: [f64; 2],
    #[serde(rename = "in")]
    pub(crate) incoming: [f64; 2],
}
impl ObjectDocument {
    /// Missing/null/empty stores need no UI context to preserve an old request.
    pub(crate) fn has_motion_arc_entries(&self) -> bool {
        self.motion_arcs
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|arcs| !arcs.entries.is_empty())
    }
    /// Native snapshot reader of the baseline handle values; old cx/cy entries retain their bytes but use defaults.
    pub(crate) fn motion_arc_handles(
        &self,
        layer: &str,
        a_frame: i64,
        b_frame: i64,
        index: u32,
        a: [f64; 2],
        b: [f64; 2],
    ) -> Result<MotionArcHandles, MotionArcError> {
        check_pairs(layer, &[(a_frame, b_frame, a_frame, b_frame)])?;
        match self.motion_arcs.value().and_then(Option::as_ref) {
            Some(arcs) => arcs.handles(layer, a_frame, b_frame, index, a, b),
            None => MotionArcs::default().handles(layer, a_frame, b_frame, index, a, b),
        }
    }
    /// Prepare the entire Arc replacement before field assignment; caller later seals the SAME document with content/range/Motion.
    pub(crate) fn retime_motion_arcs(
        &mut self,
        layer: &str,
        pairs: &[(i64, i64, i64, i64)],
    ) -> Result<(), MotionArcError> {
        check_pairs(layer, pairs)?;
        if let Some(Some(arcs)) = self.motion_arcs.value() {
            let next = arcs.retimed(layer, pairs)?;
            self.motion_arcs = Presence::Present(Some(next));
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "../tests/object_motion_arcs.rs"]
mod tests;
