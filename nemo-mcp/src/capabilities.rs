//! Registered feature capability descriptors (capability-v1.schema.json), embedded at
//! compile time so the bundled binary can validate and advertise them without a
//! filesystem dependency at runtime. Registering a new property-style capability is
//! adding its JSON to `CAPABILITY_SOURCES` below, not writing a new operation match arm
//! (P07/#1009) — the descriptor's own `input`/`fixture`/`examples`/`availability` drive
//! `contract.rs`'s payload validation and advertised examples generically.
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

const CAPABILITY_SOURCES: &[&str] = &[
    include_str!("../../engineering/application/capabilities/opacity.json"),
    include_str!("../../engineering/application/capabilities/export-job.json"),
];

#[derive(Clone, Debug, Deserialize)]
pub struct CapabilityAvailability {
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CapabilityExample {
    pub input: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: String,
    pub input: Value,
    pub effects: Value,
    pub availability: CapabilityAvailability,
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

    /// The first registered capability whose lifecycle answers to `stage`, used to build
    /// a representative advertised example. Registration order is source-list order,
    /// which is stable and reviewed, not alphabetical or runtime-dependent.
    pub fn first_supporting(&self, stage: &str) -> Option<&CapabilityDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.supports_stage(stage))
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
