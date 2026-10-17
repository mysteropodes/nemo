//! Shared fixtures of the L04-J tests: the oracle rows and the native world they describe.
//!
//! `fixtures/object-context-resolver/oracle.txt` was produced by `tools/gen_resolver_oracle.cjs`
//! of the L04-J leaf from baseline fbe4b8a3. Rows are TAB separated and `-` is absent:
//! `D`/`D2` document JSON (the v2 world, and the world after the verbatim
//! `importForeignSymbolTree`), `N` expected counts, `S` initial stroke inventory
//! (`sym layer frame stroke fill`), `LO` initial inner layer order, `T` edit script
//! (`scenario step settled op args..`), `V`/`VL` inventory and layer order read back from the
//! baseline after the verbatim `enterSymbol`/`exitToScene` ran, `A` strokes the verbatim
//! `applyOverrides` changed, `P` verbatim `layerElements`/`elementLabel` positional labels,
//! `H` verbatim `hitTestComponentLayers` results, `M`/`MI` the import id map, `X` measured
//! counts of the JS deviants. The documents are the INPUT world; every expected token beside
//! them was read back from the baseline objects.
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::{ObjectLayer, ObjectRecord};
use serde_json::{Number, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-context-resolver/oracle.txt");

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|fields| fields[0] == kind)
        .collect()
}

pub(crate) fn json(kind: &str) -> &'static str {
    rows(kind)[0][1]
}

pub(crate) fn count(name: &str) -> usize {
    rows("N")
        .iter()
        .find(|fields| fields[1] == name)
        .map(|fields| fields[2].parse().expect("count"))
        .expect("count row")
}

/// The document pieces a `ContextSource` borrows, decoded without document admission.
pub(crate) struct World {
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
    pub(crate) set: ComponentSet,
}

impl World {
    pub(crate) fn source(&self) -> ContextSource<'_> {
        ContextSource::new(
            self.total_frames,
            &self.layers,
            &self.objects,
            Some(&self.set),
        )
    }
}

/// `kind` is `D` or `D2`.
pub(crate) fn world(kind: &str) -> World {
    let value: Value = serde_json::from_str(json(kind)).expect("world json");
    World {
        total_frames: value["totalFrames"].as_u64().expect("totalFrames") as u32,
        layers: serde_json::from_value(value["layers"].clone()).expect("layers"),
        objects: serde_json::from_value(value["objects"].clone()).expect("objects"),
        set: serde_json::from_value(value["componentTables"]["components"].clone())
            .expect("component set"),
    }
}

/// `#rrggbb` of a solid fill, rounded like the baseline hex strings.
pub(crate) fn fill_hex(record: &ObjectRecord) -> String {
    let fill = record.solid_fill().expect("solid fill");
    let byte = |n: &Number| (n.as_f64().expect("channel") * 255.0).round() as u32;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(&fill.r),
        byte(&fill.g),
        byte(&fill.b)
    )
}

/// `[sym, layer, frame, stroke, fill]` per stroke in the baseline iteration order: components in
/// table order, inner layers in their CURRENT order, frames ascending, records in stored order.
pub(crate) fn inventory(set: &ComponentSet) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for component in &set.components {
        for layer in component.layers() {
            for frame in 0..component.total_frames() {
                for record in component.objects() {
                    let target = &record.target;
                    if target.layer_uid == layer.layer_uid() && target.frame_scope.frame == frame {
                        out.push(vec![
                            target.context_id.clone(),
                            target.layer_uid.clone(),
                            frame.to_string(),
                            target.stroke_id.clone(),
                            fill_hex(record),
                        ]);
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn owned(fields: &[&str]) -> Vec<String> {
    fields.iter().map(|field| field.to_string()).collect()
}

/// The `V` rows of one settled step, without the scenario and step columns.
pub(crate) fn step_rows(scenario: &str, step: usize) -> Vec<Vec<String>> {
    let step = step.to_string();
    rows("V")
        .iter()
        .filter(|fields| fields[1] == scenario && fields[2] == step)
        .map(|fields| owned(&fields[3..]))
        .collect()
}

/// The `VL` rows of one settled step: `(sym, layer uid csv)`.
pub(crate) fn layer_orders(scenario: &str, step: usize) -> Vec<(String, String)> {
    let step = step.to_string();
    rows("VL")
        .iter()
        .filter(|fields| fields[1] == scenario && fields[2] == step)
        .map(|fields| (fields[3].to_string(), fields[4].to_string()))
        .collect()
}

/// The settled steps of a scenario, ascending (step 0 is the initial state).
pub(crate) fn settled_steps(scenario: &str) -> Vec<usize> {
    let mut steps: Vec<usize> = rows("VL")
        .iter()
        .filter(|fields| fields[1] == scenario)
        .map(|fields| fields[2].parse().expect("step"))
        .collect();
    steps.dedup();
    steps
}
