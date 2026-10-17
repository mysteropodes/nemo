//! Oracle rows and comparison harness for the montage layer join (L10-I, I18/I19/I20/I36/I42).
//!
//! `fixtures/object-montage-layer/oracle.txt` (`ORACLE`) was produced by `tools/gen.mjs` of the L10-I
//! leaf (kept outside the repo) from baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc: it cuts
//! `symbolDuration`, `ensureRetime`, `moduleById`, `chainMods`, `montageTotal`, `montageStrokesAt`,
//! `montageById`, `placeMontageAsLayer`, `chainModsForView` (`src/js/storyboard.js`) and the
//! `if(ld.montageId){...}` branch of `getEffectiveStrokes`, `createUserLayer` (`src/js/app.js`)
//! verbatim and runs them in a Node vm (`SM.t` returns its key; `symbolStrokesAt` keeps its own first
//! guard `if (!sym) return []` and otherwise returns `{id, fi}`).
//!
//! Rows are TAB separated, `#` lines are header. `W world valid storyboard-json components-json`
//! (the same world in the native record shapes), `T world montage total`, `F world montage placed
//! frame res` (`placed`: `~` absent, `n` null, else a number; `res`: `-` empty or `symbol@frame`),
//! `P world montage existing-layers-json effects` (the observable effects of the gesture).
use super::{
    locate_montage_frame, plan_place_montage, Located, MontageLayer, MontageLayerError,
    MontageLayerSet, PlaceOutcome, ResolvedMontage,
};
use crate::evaluation::object_montage_timing::montage_total;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use crate::object_document::ObjectLayer;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-montage-layer/oracle.txt");

pub(crate) struct World {
    pub(crate) id: &'static str,
    pub(crate) valid: bool,
    pub(crate) storyboard: StoryboardRecord,
    pub(crate) components: ComponentSet,
}

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}

pub(crate) fn worlds() -> Vec<World> {
    rows("W")
        .into_iter()
        .map(|f| World {
            id: f[1],
            valid: f[2] == "1",
            storyboard: serde_json::from_str(f[3]).expect("world storyboard"),
            components: serde_json::from_str(f[4]).expect("world components"),
        })
        .collect()
}

pub(crate) fn world<'a>(all: &'a [World], id: &str) -> &'a World {
    all.iter().find(|w| w.id == id).expect("world")
}

/// The montage layer record a row describes (`~` absent, `n` null, else a number).
pub(crate) fn layer_of(montage: &str, placed: &str) -> MontageLayer {
    let tail = match placed {
        "~" => String::new(),
        "n" => ",\"symPlacedAt\":null".to_string(),
        number => format!(",\"symPlacedAt\":{number}"),
    };
    serde_json::from_str(&format!(
        "{{\"layerUid\":\"ly\",\"montageId\":\"{montage}\"{tail}}}"
    ))
    .expect("montage layer")
}

pub(crate) fn res_of(located: &Located) -> String {
    match located {
        Located::Empty(_) => "-".to_string(),
        Located::Source(s) => format!("{}@{}", s.symbol_id, s.src_frame),
    }
}

pub(crate) fn locate_row(w: &World, f: &[&str]) -> String {
    let layer = layer_of(f[2], f[3]);
    let frame: i64 = f[4].parse().expect("frame");
    let located = locate_montage_frame(&layer, &w.storyboard, &w.components, frame)
        .expect("baseline rows are all admissible");
    res_of(&located)
}

pub(crate) fn total_of(w: &World, montage: &str) -> i64 {
    let resolved = ResolvedMontage::resolve(&w.storyboard, &w.components, montage)
        .expect("resolve")
        .expect("montage");
    montage_total(&resolved.members().expect("members")).expect("total")
}

/// The scene layers and the montage layer set of an `existing-layers-json` column.
pub(crate) struct Existing {
    pub(crate) scene: Vec<ObjectLayer>,
    pub(crate) placed: MontageLayerSet,
    pub(crate) names: Vec<String>,
    pub(crate) montage_ids: Vec<Option<String>>,
}

pub(crate) fn existing_of(json: &str) -> Existing {
    let list: Vec<Value> = serde_json::from_str(json).expect("existing layers");
    let mut scene = Vec::new();
    let mut placed = Vec::new();
    let mut names = Vec::new();
    let mut montage_ids = Vec::new();
    for entry in &list {
        let uid = entry["uid"].as_str().expect("uid");
        let name = entry["name"].as_str().expect("name");
        scene.push(
            serde_json::from_value(serde_json::json!({"layerUid": uid, "name": name}))
                .expect("scene layer"),
        );
        names.push(name.to_string());
        let montage = entry["montageId"].as_str().map(str::to_string);
        if let Some(id) = &montage {
            placed.push(
                serde_json::from_value(serde_json::json!({"layerUid": uid, "montageId": id}))
                    .expect("montage layer"),
            );
        }
        montage_ids.push(montage);
    }
    Existing {
        scene,
        placed: MontageLayerSet::new(placed),
        names,
        montage_ids,
    }
}

fn last_of(existing: &Existing) -> String {
    match existing.names.last() {
        None => "null".to_string(),
        Some(name) => {
            let id = existing.montage_ids.last().and_then(|m| m.as_deref());
            serde_json::to_string(&serde_json::json!([name, id])).expect("last")
        }
    }
}

/// The effects the JS adapter performs for a planned gesture, in the baseline's observable form:
/// toasts (`SM.t` is the identity), layer count, active index, undo pushes, frame saves, calls, and
/// the last layer.
pub(crate) fn effects_of(
    outcome: &Result<PlaceOutcome, MontageLayerError>,
    existing: &Existing,
) -> String {
    let n = existing.scene.len();
    let identity = |key: &str| key.to_string();
    let (toast, count, act, undo, calls, last) = match outcome {
        Err(error) => (
            error
                .toast_key()
                .expect("refusals of the gesture have a key")
                .to_string(),
            n,
            0,
            0,
            String::new(),
            last_of(existing),
        ),
        Ok(PlaceOutcome::Focus { layer_uid, .. }) => {
            let index = existing
                .scene
                .iter()
                .position(|l| l.layer_uid() == layer_uid)
                .expect("focused layer exists");
            (
                outcome.as_ref().expect("ok").toast(&identity),
                n,
                index,
                0,
                format!("mode:anim2d,ul:{index}"),
                last_of(existing),
            )
        }
        Ok(PlaceOutcome::Create(plan)) => (
            outcome.as_ref().expect("ok").toast(&identity),
            n + 1,
            n,
            1,
            format!("mode:anim2d,ul:{n}"),
            serde_json::to_string(&serde_json::json!([plan.layer_name, plan.montage_id]))
                .expect("last"),
        ),
    };
    format!(
        "t={};n={};act={};undo={};saves={};calls={};last={}",
        serde_json::to_string(&[toast]).expect("toasts"),
        count,
        act,
        undo,
        undo,
        calls,
        last
    )
}

pub(crate) fn place_row(w: &World, f: &[&str]) -> String {
    let existing = existing_of(f[3]);
    let outcome = plan_place_montage(
        &w.storyboard,
        &w.components,
        &existing.placed,
        &existing.scene,
        f[2],
    );
    effects_of(&outcome, &existing)
}
