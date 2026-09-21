//! Registered feature capability descriptors (capability-v1.schema.json), embedded at
//! compile time so the bundled binary can validate and advertise them without a
//! filesystem dependency at runtime. Registering a new property-style capability is
//! adding its JSON to `CAPABILITY_SOURCES` below, not writing a new operation match arm
//! (P07/#1009) — the descriptor's own `input`/`fixture`/`examples`/`availability` drive
//! `contract.rs`'s payload validation and advertised examples generically.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

// Public so the integration tests READ this declaration instead of copying it.
// Four hand-maintained copies of this list is what made #1310 fail in four
// places at once, inside a crate no job ran (#1318).
pub const CAPABILITY_SOURCES: &[&str] = &[
    include_str!("../../engineering/application/capabilities/opacity.json"),
    include_str!("../../engineering/application/capabilities/export-job.json"),
    include_str!("../../engineering/application/capabilities/timelapse.json"),
];

/// Native v2 declarations are deliberately version-separated from the legacy
/// catalog above. N16 consumes the feature-owned descriptor but does not add it
/// to v1 discovery or turn it into another MCP tool.
pub const NATIVE_CAPABILITY_SOURCES: &[&str] = &[include_str!(
    "../../engineering/application/capabilities-v2/native-opacity.json"
)];

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

/// Deterministic, read-only v2 catalog. The original JSON values are retained so
/// discovery advertises every feature-owned field verbatim; the index exists only
/// to resolve declared operations without a per-feature switch.
#[derive(Debug)]
pub struct NativeCapabilityCatalog {
    descriptors: Vec<Value>,
    operations: BTreeMap<String, usize>,
}

impl NativeCapabilityCatalog {
    pub fn from_sources(sources: &[&str]) -> Result<Self, String> {
        let mut descriptors = Vec::with_capacity(sources.len());
        let mut ids = BTreeMap::<String, usize>::new();
        let mut operations = BTreeMap::<String, usize>::new();
        for (index, source) in sources.iter().enumerate() {
            let descriptor: Value = serde_json::from_str(source)
                .map_err(|error| format!("native capability source {index}: {error}"))?;
            let object = descriptor
                .as_object()
                .ok_or_else(|| format!("native capability source {index} must be a JSON object"))?;
            if object.get("schemaVersion").and_then(Value::as_u64) != Some(2)
                || object.get("apiVersion").and_then(Value::as_u64) != Some(2)
            {
                return Err(format!(
                    "native capability source {index} must declare schemaVersion/apiVersion 2"
                ));
            }
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| format!("native capability source {index} has no id"))?;
            if ids.insert(id.to_owned(), index).is_some() {
                return Err(format!("duplicate native capability id {id}"));
            }
            let declared = object
                .get("operations")
                .and_then(Value::as_array)
                .filter(|values| !values.is_empty())
                .ok_or_else(|| format!("native capability {id} has no operations"))?;
            for operation in declared {
                let operation = operation
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("native capability {id} has a non-string operation"))?;
                if operations.insert(operation.to_owned(), index).is_some() {
                    return Err(format!("duplicate native operation {operation}"));
                }
            }
            descriptors.push(descriptor);
        }
        Ok(Self {
            descriptors,
            operations,
        })
    }

    pub fn descriptors(&self) -> &[Value] {
        &self.descriptors
    }

    pub fn capability_for_operation(&self, operation: &str) -> Option<&Value> {
        self.operations
            .get(operation)
            .and_then(|index| self.descriptors.get(*index))
    }
}

pub fn native_catalog() -> &'static NativeCapabilityCatalog {
    static CATALOG: OnceLock<NativeCapabilityCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        NativeCapabilityCatalog::from_sources(NATIVE_CAPABILITY_SOURCES)
            .expect("embedded native capability catalog is valid and unambiguous")
    })
}
