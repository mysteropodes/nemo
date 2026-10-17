//! Oracle row parser for the widget-record fixture (L03-K, F19).
//!
//! `fixtures/object-widget-record/oracle.txt` is produced by `tools/gen.cjs` from the baseline JS at
//! fbe4b8a3 (verbatim extracts executed in a Node `vm`). One tab-separated row per case:
//! `mk` what `addWidgetLayer` creates, `imp` import/`widgetOf`/transplant/duplicate verdicts on a
//! stored widget, `ctl` control lookup and `axisLabel`, `nr` the two "never rendered" conditions.

pub(crate) const ORACLE: &str = include_str!("fixtures/object-widget-record/oracle.txt");

/// `addWidgetLayer(kindArg)` on a canvas of `canvas`: the widget, controls and static seeds it made.
pub(crate) struct MakeRow {
    pub(crate) name: String,
    pub(crate) canvas: (u32, u32),
    pub(crate) widget: String,
    pub(crate) controls: String,
    pub(crate) statics: String,
    pub(crate) color: String,
    pub(crate) flag: bool,
}

/// Baseline verdicts on one stored widget value (raw JSON text).
pub(crate) struct ImportRow {
    pub(crate) name: String,
    pub(crate) widget: String,
    pub(crate) import: bool,
    pub(crate) widget_of: bool,
    pub(crate) transplant: bool,
    pub(crate) copy: Option<String>,
}

/// `controlByKey` type (None when it finds nothing) and `axisLabel` result for one axis.
pub(crate) struct ControlRow {
    pub(crate) name: String,
    pub(crate) controls: Option<String>,
    pub(crate) axis: String,
    pub(crate) found_type: Option<String>,
    pub(crate) label: String,
}

/// The two baseline conditions that make a widget layer render nothing.
pub(crate) struct StackRow {
    pub(crate) name: String,
    pub(crate) flag: Option<bool>,
    pub(crate) stack_empty: bool,
    pub(crate) scene_empty: bool,
}

fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .filter(|fields| fields[0] == tag)
        .collect()
}

fn flag(text: &str) -> bool {
    match text {
        "1" => true,
        "0" => false,
        other => panic!("oracle flag must be 0 or 1, got {other}"),
    }
}

fn dash(text: &str) -> Option<String> {
    if text == "-" {
        None
    } else {
        Some(text.to_string())
    }
}

pub(crate) fn make_rows() -> Vec<MakeRow> {
    rows("mk")
        .into_iter()
        .map(|f| MakeRow {
            name: f[1].to_string(),
            canvas: (f[3].parse().unwrap(), f[4].parse().unwrap()),
            widget: f[5].to_string(),
            controls: f[6].to_string(),
            statics: f[7].to_string(),
            color: f[9].to_string(),
            flag: flag(f[10]),
        })
        .collect()
}

pub(crate) fn import_rows() -> Vec<ImportRow> {
    rows("imp")
        .into_iter()
        .map(|f| ImportRow {
            name: f[1].to_string(),
            widget: f[2].to_string(),
            import: flag(f[3]),
            widget_of: flag(f[4]),
            transplant: flag(f[5]),
            copy: dash(f[6]),
        })
        .collect()
}

pub(crate) fn control_rows() -> Vec<ControlRow> {
    rows("ctl")
        .into_iter()
        .map(|f| ControlRow {
            name: f[1].to_string(),
            controls: dash(f[2]),
            axis: f[3].to_string(),
            found_type: dash(f[4]),
            label: serde_json::from_str::<String>(f[5]).unwrap(),
        })
        .collect()
}

pub(crate) fn stack_rows() -> Vec<StackRow> {
    rows("nr")
        .into_iter()
        .map(|f| StackRow {
            name: f[1].to_string(),
            flag: if f[2] == "-" { None } else { Some(flag(f[2])) },
            stack_empty: flag(f[3]),
            scene_empty: flag(f[4]),
        })
        .collect()
}

/// The row of `name` among `rows`, panicking when the oracle lacks it.
pub(crate) fn import_row<'a>(rows: &'a [ImportRow], name: &str) -> &'a ImportRow {
    rows.iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no imp row {name}"))
}

// ---- document builders shared by the test files -------------------------------------------

/// A layer carrying `widget` (raw JSON text).
pub(crate) fn layer(uid: &str, widget: &str) -> String {
    format!("{{\"layerUid\":\"{uid}\",\"widget\":{widget}}}")
}

/// Holder JSON for the layer-level holder of `uid`, with `controls` and one static seed per
/// control that has an entry in `statics` (the shape `addWidgetLayer` leaves behind).
pub(crate) fn holders_for(uid: &str, controls: &str, statics: &str) -> String {
    let list: Vec<serde_json::Value> = serde_json::from_str(controls).unwrap();
    let seeds: serde_json::Value = serde_json::from_str(statics).unwrap();
    let properties: Vec<String> = list
        .iter()
        .filter_map(|control| {
            let key = control["key"].as_str().unwrap();
            seeds
                .get(key)
                .map(|value| format!("{{\"property\":\"{key}\",\"static\":{value}}}"))
        })
        .collect();
    format!(
        "[{{\"target\":{{\"contextId\":\"scene-root\",\"layerUid\":\"{uid}\"}},\"controls\":{controls},\"properties\":[{}]}}]",
        properties.join(",")
    )
}

/// A closed-solid-fill object record from the frozen application example, retargeted.
pub(crate) fn object_on(uid: &str) -> String {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = serde_json::json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":uid,"strokeId":"s1"});
    record.to_string()
}

/// Explicit order plan listing `uids` in one context with no scoped elements.
pub(crate) fn plan(uids: &[&str]) -> String {
    let layers: Vec<String> = uids
        .iter()
        .enumerate()
        .map(|(position, uid)| {
            format!("{{\"layerUid\":\"{uid}\",\"naturalPosition\":{position},\"order\":{{}}}}")
        })
        .collect();
    format!(
        "{{\"format\":\"nemo.native-object-order-plan\",\"formatVersion\":1,\"documentFormatVersion\":2,\"contexts\":[{{\"contextId\":\"scene-root\",\"layers\":[{}],\"scopes\":[]}}],\"migration\":\"explicit-plan-required\",\"referencePolicy\":\"unresolved\"}}",
        layers.join(",")
    )
}

fn holders_member(holders: Option<&str>) -> String {
    holders.map_or(String::new(), |text| format!(",\"propertyHolders\":{text}"))
}

/// Canonical v1 document text (member order = native serialization order).
pub(crate) fn v1(layers: &[String], objects: &[String], holders: Option<&str>) -> String {
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":1,\"totalFrames\":2,\"layers\":[{}],\"objects\":[{}]{}}}",
        layers.join(","),
        objects.join(","),
        holders_member(holders)
    )
}

/// v2 document text over the same layers; `uids` must list every layer once.
pub(crate) fn v2(
    layers: &[String],
    uids: &[&str],
    objects: &[String],
    holders: Option<&str>,
) -> String {
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":2,\"totalFrames\":2,\"layers\":[{}],\"objects\":[{}]{},\"compositionOrder\":{}}}",
        layers.join(","),
        objects.join(","),
        holders_member(holders),
        plan(uids)
    )
}
