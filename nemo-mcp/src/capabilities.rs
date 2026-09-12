//! Registered feature capability descriptors (capability-v1.schema.json), embedded at
//! compile time so the bundled binary can validate and advertise them without a
//! filesystem dependency at runtime. Registering a new property-style capability is
//! adding its JSON to `CAPABILITY_SOURCES` below, not writing a new operation match arm
//! (P07/#1009) — the descriptor's own `input`/`fixture`/`examples`/`availability` drive
//! `contract.rs`'s payload validation and advertised examples generically.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

const CAPABILITY_SOURCES: &[&str] = &[
    include_str!("../../engineering/application/capabilities/opacity.json"),
    include_str!("../../engineering/application/capabilities/export-job.json"),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CapabilityAvailability {
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CapabilityExample {
    #[serde(default)]
    pub label: String,
    pub input: Value,
    #[serde(default)]
    pub output: Value,
}

/// The complete feature-owned descriptor. Keeping every public field here is
/// deliberate: MCP discovery and its generated JSON Schema advertise this value
/// verbatim, rather than a Rust-side summary that can silently lose input/output
/// constraints, units, handler identity, or examples.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDescriptor {
    #[serde(default)]
    pub schema_version: u32,
    pub id: String,
    #[serde(default)]
    pub version: u32,
    pub input: Value,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub units: Value,
    pub effects: Value,
    pub availability: CapabilityAvailability,
    #[serde(default)]
    pub handler_key: String,
    pub fixture: CapabilityExample,
    #[serde(default)]
    pub examples: Vec<CapabilityExample>,
}

impl CapabilityDescriptor {
    /// JSON Schema `required` keys this capability's own `input` fragment declares,
    /// independent of any transport verb. A capability whose requirement varies by
    /// lifecycle stage (for example export-job.json) declares none here and leaves
    /// per-stage requiredness to its own handler, per that shape's own description.
    pub fn declared_required(&self) -> Vec<String> {
        self.input
            .get("required")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn is_available(&self) -> bool {
        self.availability.state == "available"
    }

    fn lifecycle(&self) -> Vec<&str> {
        self.effects
            .get("lifecycle")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    pub fn supports_stage(&self, stage: &str) -> bool {
        self.lifecycle().contains(&stage)
    }

    /// The fixture first, then every further example, in declaration order — a caller
    /// building a template picks the first one that already carries the fields it needs.
    pub fn example_inputs(&self) -> impl Iterator<Item = &Value> {
        std::iter::once(&self.fixture.input)
            .chain(self.examples.iter().map(|example| &example.input))
    }
}

pub struct CapabilityCatalog {
    descriptors: Vec<CapabilityDescriptor>,
}

impl CapabilityCatalog {
    pub fn find(&self, id: &str) -> Option<&CapabilityDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.id == id)
    }

    /// Full registered declarations in reviewed source order. This is a read-only
    /// contract projection for discovery/schema clients, never application state.
    pub fn descriptors(&self) -> &[CapabilityDescriptor] {
        &self.descriptors
    }

    /// The first registered capability whose lifecycle answers to `stage`, used to build
    /// a representative advertised example. Registration order is source-list order,
    /// which is stable and reviewed, not alphabetical or runtime-dependent.
    pub fn first_supporting(&self, stage: &str) -> Option<&CapabilityDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.supports_stage(stage))
    }
}

impl CapabilityCatalog {
    /// Builds a catalog from arbitrary descriptor JSON instead of the embedded
    /// production set — for exercising a resolution outcome (an unavailable or
    /// stage-unsupported capability) that no currently registered descriptor
    /// produces, without inventing a fake entry in the real registered set.
    #[cfg(test)]
    pub(crate) fn from_sources(sources: &[&str]) -> Self {
        let descriptors = sources
            .iter()
            .map(|source| {
                serde_json::from_str(source).expect("test capability descriptor is valid JSON")
            })
            .collect();
        CapabilityCatalog { descriptors }
    }
}

pub fn catalog() -> &'static CapabilityCatalog {
    static CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let descriptors = CAPABILITY_SOURCES
            .iter()
            .map(|source| {
                serde_json::from_str(source).expect("embedded capability descriptor is valid JSON")
            })
            .collect();
        CapabilityCatalog { descriptors }
    })
}
