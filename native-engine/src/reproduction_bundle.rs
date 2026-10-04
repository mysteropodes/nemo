//! Closed portable projection: raw bytes are fully admitted before replay.
use crate::application::{ReproductionReason, ReproductionReplayError, REPRODUCTION_FIXTURE};
use serde::de::{value::MapAccessDeserializer, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Number};
use std::{fmt, marker::PhantomData};

pub(super) const MAX_COMMANDS: usize = 32;
pub(super) const MAX_BUNDLE_BYTES: usize = 3072;
pub(super) const TARGET: &str = "r08_curve_layer";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::application) struct CapturedCommand {
    pub id: u8,
    pub expected_revision: u64,
    pub value: Number,
    pub revision: u64,
    pub applied: bool,
}

// Derive alone also admits JSON arrays for structs. Every wire object uses
// this wrapper so array-shaped objects cannot bypass the closed map contract.
struct Object<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(Object)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireBundle {
    format: String,
    format_version: u32,
    api_version: u32,
    fixture: Object<Fixture>,
    command: String,
    stable_target: Object<Target>,
    clock: (),
    seed: (),
    versions: Object<Versions>,
    commands: Vec<Object<CapturedCommand>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    id: String,
    version: u32,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    layer_uid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Versions {
    native_engine: String,
}

// Only decode can construct this value; no caller can manufacture an admitted
// command sequence or replace the pinned catalog origin.
pub(super) struct ValidatedBundle {
    commands: Vec<CapturedCommand>,
}

impl ValidatedBundle {
    pub(super) fn commands(&self) -> &[CapturedCommand] {
        &self.commands
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<ValidatedBundle, ReproductionReplayError> {
    use ReproductionReplayError::{IncompatibleBundle, InvalidBundle, InvalidSequence};
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ReproductionReplayError::ByteLimit);
    }
    // Decode directly from bytes: a Value intermediate would silently discard
    // duplicate keys. Unit fields require explicit null, never missing values.
    let Object(mut wire): Object<WireBundle> =
        serde_json::from_slice(bytes).map_err(|_| InvalidBundle)?;
    let _ = (wire.clock, wire.seed);
    if wire.format != "nemo.native-opacity-reproduction"
        || wire.format_version != 1
        || wire.api_version != 2
        || wire.fixture.0.id != REPRODUCTION_FIXTURE.id
        || wire.fixture.0.version != REPRODUCTION_FIXTURE.version
        || wire.fixture.0.sha256 != REPRODUCTION_FIXTURE.sha256
        || wire.command != "layer.opacity.set"
        || wire.stable_target.0.layer_uid != TARGET
        || wire.versions.0.native_engine != env!("CARGO_PKG_VERSION")
    {
        return Err(IncompatibleBundle);
    }
    if !(1..=MAX_COMMANDS).contains(&wire.commands.len()) {
        return Err(InvalidSequence);
    }
    restore_precise_opacities(bytes, &mut wire.commands)?;
    // This starting value belongs to the exact pinned v1 catalog hash. It is
    // validation data, not another writable document or an authenticity check.
    let (mut opacity, mut revision) = (25.0, 0);
    for (index, Object(command)) in wire.commands.iter().enumerate() {
        let value = command.value.as_f64().ok_or(InvalidBundle)?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(InvalidBundle);
        }
        let applied = value != opacity;
        if usize::from(command.id) != index + 1
            || command.expected_revision != revision
            || command.applied != applied
            || command.revision != revision + u64::from(applied)
        {
            return Err(InvalidSequence);
        }
        opacity = value;
        revision = command.revision;
    }
    Ok(ValidatedBundle {
        commands: wire
            .commands
            .into_iter()
            .map(|Object(command)| command)
            .collect(),
    })
}

fn restore_precise_opacities(
    bytes: &[u8],
    commands: &mut [Object<CapturedCommand>],
) -> Result<(), ReproductionReplayError> {
    // serde_json's default float parser is not round-trip exact. The already
    // validated closed shape guarantees that only command objects have "value"
    // fields. Recover their raw number tokens with Rust's correctly rounded
    // float parser so close values cannot become a different no-op disposition.
    // This scan never admits structure; strict typed decode above has done that.
    let mut commands = commands.iter_mut();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        while bytes[cursor] != b'"' {
            if bytes[cursor] == b'\\' {
                cursor += 1;
            }
            cursor += 1;
        }
        cursor += 1;
        let end = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b':') {
            continue;
        }
        let key: String = serde_json::from_slice(&bytes[start..end])
            .map_err(|_| ReproductionReplayError::InvalidBundle)?;
        if key != "value" {
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_digit() || b"-+.eE".contains(byte))
        {
            cursor += 1;
        }
        let Object(command) = commands
            .next()
            .ok_or(ReproductionReplayError::InvalidBundle)?;
        if command.value.is_f64() {
            let value = std::str::from_utf8(&bytes[start..cursor])
                .ok()
                .and_then(|token| token.parse::<f64>().ok())
                .and_then(Number::from_f64)
                .ok_or(ReproductionReplayError::InvalidBundle)?;
            command.value = value;
        }
    }
    if commands.next().is_some() {
        return Err(ReproductionReplayError::InvalidBundle);
    }
    Ok(())
}

pub(super) fn encode(commands: &[CapturedCommand]) -> Result<Vec<u8>, ReproductionReason> {
    let bytes = serde_json::to_vec(&json!({
        "format": "nemo.native-opacity-reproduction",
        "formatVersion": 1,
        "apiVersion": 2,
        "fixture": {
            "id": REPRODUCTION_FIXTURE.id,
            "version": REPRODUCTION_FIXTURE.version,
            "sha256": REPRODUCTION_FIXTURE.sha256
        },
        "command": "layer.opacity.set",
        "stableTarget": { "layerUid": TARGET },
        "clock": null,
        "seed": null,
        "versions": { "nativeEngine": env!("CARGO_PKG_VERSION") },
        "commands": commands
    }))
    .map_err(|_| ReproductionReason::InvalidProjection)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ReproductionReason::ByteLimit);
    }
    Ok(bytes)
}
