//! Island styling and identity for a boolean result (`insertBooleanResult` and
//! `carryBooleanData` of `src/js/tools.js`, L02-D).
//!
//! The plan says where each island lands (`insertAt + k`, clamped like
//! `Layer.insertChild`), which paint it receives and which data it keeps. It
//! does not mint ids: the first island keeps the source `strokeId`, every
//! later island asks for a fresh one (`StrokeIdAction::Fresh`) that the
//! transaction leaf allocates. Pre-existing island data is assumed empty (a
//! provider ring is bare); the baseline's Paper.js results could arrive with a
//! cloned copy of an operand's data, which is a legacy detail not reproduced.
use std::collections::BTreeMap;

use crate::object_document::object_group_kernels::object_boolean_topology::{
    classify, IslandPlan, ResultShape, TopologyError, TopologyPlan,
};

/// `BOOL_KEEP_DATA_ALL`: copied onto every island when the source defines them.
pub(crate) const KEEP_DATA_ALL: [&str; 13] = [
    "groupId",
    "ownerId",
    "ownerName",
    "ownerColor",
    "channelTag",
    "shadowSwatchId",
    "tweenOn",
    "boxAngle",
    "xformAnchorKey",
    "xformAnchorCustom",
    "effects",
    "fillGradient",
    "isFillShape",
];

/// `BOOL_KEEP_DATA_FIRST`: copied onto the first island only.
const STROKE_ID_KEY: &str = "strokeId";

/// One `data.*` value with JS truthiness. Arrays and objects are opaque text
/// (carried by value, never inspected).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DataValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Opaque(String),
}

impl DataValue {
    pub(crate) fn truthy(&self) -> bool {
        match self {
            DataValue::Null => false,
            DataValue::Bool(value) => *value,
            DataValue::Number(value) => *value != 0.0 && !value.is_nan(),
            DataValue::Text(value) => !value.is_empty(),
            DataValue::Opaque(_) => true,
        }
    }
}

/// The source item's `data`: a key is present iff the JS value is not `undefined`
/// (`null`, `0`, `false` and `""` are present and are copied).
pub(crate) type SourceData = BTreeMap<String, DataValue>;

/// What the first or a later island does about the stroke identity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StrokeIdAction {
    Absent,
    /// First island: the source value, copied whatever its truthiness.
    Carry(DataValue),
    /// Later island with a truthy source id: `ensureStrokeId` mints a new one.
    Fresh,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DataCarry {
    pub(crate) fields: BTreeMap<String, DataValue>,
    pub(crate) stroke_id: StrokeIdAction,
}

/// `carryBooleanData(isl, srcData, isFirst)`.
pub(crate) fn carry_boolean_data(src: Option<&SourceData>, is_first: bool) -> DataCarry {
    let mut carry = DataCarry {
        fields: BTreeMap::new(),
        stroke_id: StrokeIdAction::Absent,
    };
    let src = match src {
        Some(data) => data,
        None => return carry,
    };
    for key in KEEP_DATA_ALL.iter() {
        if let Some(value) = src.get(*key) {
            carry.fields.insert((*key).to_string(), value.clone());
        }
    }
    match src.get(STROKE_ID_KEY) {
        Some(value) if is_first => carry.stroke_id = StrokeIdAction::Carry(value.clone()),
        Some(value) if value.truthy() => carry.stroke_id = StrokeIdAction::Fresh,
        _ => {}
    }
    carry
}

/// `fillColor` argument: `undefined` leaves the island unstyled, `null` clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaintArg {
    Leave,
    Clear,
    Set(String),
}

/// `{color, width, cap, join}` of a redrawn outline.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StrokeInfo {
    pub(crate) color: String,
    pub(crate) width: f64,
    pub(crate) cap: String,
    pub(crate) join: String,
}

pub(crate) struct InsertRequest<'a> {
    pub(crate) insert_at: usize,
    /// Children in the layer when insertion starts (sources already removed).
    pub(crate) layer_len: usize,
    pub(crate) fill: PaintArg,
    /// `None` leaves the island opacity alone (`opacity !== undefined`).
    pub(crate) opacity: Option<f64>,
    /// Applied only when its colour is non-empty; otherwise the outline is cleared.
    pub(crate) stroke: Option<&'a StrokeInfo>,
    pub(crate) src: Option<&'a SourceData>,
    /// Passed to `_eraseDegenerateSelfLoops` as `skipRefit`.
    pub(crate) preserve_geometry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedIsland {
    pub(crate) island: IslandPlan,
    /// Final index in the layer: `min(insert_at, layer_len) + k`.
    pub(crate) index: usize,
    pub(crate) fill: PaintArg,
    /// `None` means `strokeColor = null`.
    pub(crate) stroke: Option<StrokeInfo>,
    pub(crate) opacity: Option<f64>,
    pub(crate) carry: DataCarry,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsertPlan {
    pub(crate) topology: TopologyPlan,
    pub(crate) islands: Vec<PlacedIsland>,
    pub(crate) skip_refit: bool,
}

/// `insertBooleanResult`: classify, then style and place every island in order.
pub(crate) fn plan_insert(
    shape: &ResultShape,
    request: &InsertRequest,
) -> Result<InsertPlan, TopologyError> {
    let topology = classify(shape)?;
    let base = request.insert_at.min(request.layer_len);
    let stroke = request
        .stroke
        .filter(|info| !info.color.is_empty())
        .cloned();
    let islands: Vec<PlacedIsland> = topology
        .islands
        .iter()
        .enumerate()
        .map(|(k, island)| PlacedIsland {
            island: island.clone(),
            index: base + k,
            fill: request.fill.clone(),
            stroke: stroke.clone(),
            opacity: request.opacity,
            carry: carry_boolean_data(request.src, k == 0),
        })
        .collect();
    Ok(InsertPlan {
        topology,
        islands,
        skip_refit: request.preserve_geometry,
    })
}
