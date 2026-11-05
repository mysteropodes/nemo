//! #2372 A-flip-selection: Shift+X / Shift+Alt+X send a MIRROR (scale -1 about the selection centre) through the same batch `reshape` with a `matrix`
//! as a move or a resize. The mapping takes any affine: a stroke's points, a ribbon's centreline with its handles (baseline: the flip also
//! mirrored `centerSegments`), the linked fill and a gradient follow, all in ONE history entry; undo and redo restore it exactly.
use super::create_many::pressure_record;
use super::support::*;
use serde_json::{Value, json};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn reshape(id: &str, m: [f64; 6]) -> Value {
    json!({"op":"reshape","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":id},"matrix":m})
}
fn at(owner: &crate::history::NativeObjectHistory, id: &str) -> Value {
    objects(&state(owner))
        .iter()
        .find(|r| r["target"]["strokeId"] == id)
        .unwrap()
        .clone()
}
fn xy(v: &Value) -> [f64; 2] {
    [v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()]
}
fn corner(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
/// The mirror about x = `cx` (horizontal) or y = `cy` (vertical), as the client builds it: scale(-1, 1) / (1, -1) about the bounds centre.
const FLIP_H: [f64; 6] = [-1.0, 0.0, 0.0, 1.0, 100.0, 0.0];
const FLIP_V: [f64; 6] = [1.0, 0.0, 0.0, -1.0, 0.0, 60.0];

fn square(id: &str, ext: Value) -> Value {
    let mut r = record_at("A", "L1", 0, "");
    r["target"]["strokeId"] = json!(id);
    r["geometry"]["closed"] = json!(true);
    r["geometry"]["segments"] = json!([
        corner(0., 0.),
        corner(100., 0.),
        corner(100., 60.),
        corner(0., 60.)
    ]);
    if !ext.is_null() {
        r["vectorExt"] = ext;
    }
    r
}

#[test]
fn a_plain_stroke_is_mirrored_in_one_entry_and_undo_redo_restore_it() {
    let mut owner = owner_of(&document(1, vec![square("s-a", Value::Null)], None));
    let before = state(&owner);
    let pts = |owner: &crate::history::NativeObjectHistory| -> Vec<[f64; 2]> {
        at(owner, "s-a")["geometry"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| xy(&s["point"]))
            .collect()
    };
    let r = send(&mut owner, "h", batch(vec![reshape("s-a", FLIP_H)]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        pts(&owner),
        vec![[100., 0.], [0., 0.], [0., 60.], [100., 60.]],
        "mirrored about x = 50"
    );
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "one gesture, one history entry"
    );
    let r = send(&mut owner, "v", batch(vec![reshape("s-a", FLIP_V)]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        pts(&owner),
        vec![[100., 60.], [0., 60.], [0., 0.], [100., 0.]],
        "then about y = 30"
    );
    assert_eq!(owner.history_depths(), (2, 0));
    let after = state(&owner);
    for _ in 0..2 {
        let revision = owner.content_revision();
        owner.undo(revision).unwrap();
    }
    assert_eq!(
        state(&owner),
        before,
        "two undos restore the record exactly"
    );
    for _ in 0..2 {
        let revision = owner.content_revision();
        owner.redo(revision).unwrap();
    }
    assert_eq!(
        objects(&state(&owner)),
        objects(&after),
        "two redos bring the mirror back"
    );
}

#[test]
fn a_ribbon_is_mirrored_with_its_centreline_handles_widths_and_linked_fill() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let center = json!([{"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":5,"y":3},"width":2},
        {"point":{"x":40,"y":10},"handleIn":{"x":-5,"y":-3},"handleOut":{"x":0,"y":0},"width":4}]);
    let companion = pressure_record(
        [0.2, 0.4, 0.6],
        json!({"linkedFillId":"lf","isLinkedFillCompanion":true}),
    );
    let ribbon = pressure_record(
        [0.1, 0.1, 0.1],
        json!({"linkedFillId":"lf","pressure":{"centerSegments":center,"widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}),
    );
    let made = send(
        &mut owner,
        "set",
        json!({"command":"object.vector.createMany","records":[companion, ribbon]}),
    );
    assert_eq!(made["ok"], true, "{made}");
    let before = state(&owner);
    // The centreline spans x 0..40, so the client's mirror axis is x = 20.
    let r = send(
        &mut owner,
        "h",
        batch(vec![reshape(
            "native-stroke:1:1",
            [-1., 0., 0., 1., 40., 0.],
        )]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let moved = at(&owner, "native-stroke:1:1")["vectorExt"]["pressure"]["centerSegments"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        (
            xy(&moved[0]["point"]),
            xy(&moved[0]["handleIn"]),
            xy(&moved[0]["handleOut"])
        ),
        ([40., 10.], [0., 0.], [-5., 3.]),
        "the first point and its handle are mirrored"
    );
    assert_eq!(
        (
            xy(&moved[1]["point"]),
            xy(&moved[1]["handleIn"]),
            xy(&moved[1]["handleOut"])
        ),
        ([0., 10.], [5., -3.], [0., 0.]),
        "so is the second"
    );
    assert_eq!(
        (moved[0]["width"].as_f64(), moved[1]["width"].as_f64()),
        (Some(2.), Some(4.)),
        "a mirror does not change the widths"
    );
    let fill: Vec<[f64; 2]> = at(&owner, "native-stroke:1:0")["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| xy(&s["point"]))
        .collect();
    assert_eq!(
        fill,
        vec![[40., 10.], [0., 10.]],
        "the linked fill follows the centreline"
    );
    assert_eq!(
        owner.history_depths(),
        (2, 0),
        "the ribbon and its fill are one entry"
    );
    let revision = owner.content_revision();
    owner.undo(revision).unwrap();
    assert_eq!(state(&owner), before, "one undo restores both");
}

#[test]
fn a_gradient_fill_mirrors_its_end_points() {
    let ext = json!({"fillGradient":{"kind":"linear","from":[0,0],"to":[100,0],"stops":[{"offset":0,"color":"#ff0000ff"},{"offset":1,"color":"#0000ffff"}]}});
    let mut owner = owner_of(&document(1, vec![square("s-a", ext)], None));
    assert_eq!(
        send(&mut owner, "h", batch(vec![reshape("s-a", FLIP_H)]))["ok"],
        true
    );
    let g = at(&owner, "s-a")["vectorExt"]["fillGradient"].clone();
    assert_eq!(
        (g["from"].clone(), g["to"].clone()),
        (json!([100., 0.]), json!([0., 0.]))
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn a_dynamic_shape_takes_a_mirror_through_its_scale_kernel() {
    let ext = json!({"paramShape":{"kind":"rect","box":{"x1":0,"y1":0,"x2":100,"y2":60}}});
    let mut owner = owner_of(&document(1, vec![square("s-a", ext)], None));
    // A rect mirrored about its own centre is the same rect: the owner writes nothing for it (and the client's flip is an honest no-op).
    let same = send(&mut owner, "centre", batch(vec![reshape("s-a", FLIP_H)]));
    assert_eq!(same["ok"], true, "{same}");
    assert_eq!(owner.history_depths(), (0, 0));
    // Mirrored about x = 0 (a selection whose centre is elsewhere) it lands on the other side, box and anchors together.
    let r = send(
        &mut owner,
        "left",
        batch(vec![reshape("s-a", [-1., 0., 0., 1., 0., 0.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let shape = at(&owner, "s-a");
    let mut xs: Vec<f64> = shape["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| xy(&s["point"])[0])
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, vec![-100., -100., 0., 0.]);
    assert_eq!(shape["vectorExt"]["paramShape"]["kind"], "rect");
    assert_eq!(
        shape["vectorExt"]["paramShape"]["box"],
        json!({"x1":-100.,"y1":0.,"x2":0.,"y2":60.}),
        "the box stays normalised"
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

/// Motion mode edits the key a held frame inherits (select ops: "Animation 2D promotes the cell, Motion edits the key it inherits"): the step
/// names the key's own cell and no copy key is sent, so the shared key is mirrored in place and no new key appears.
#[test]
fn a_flip_in_motion_mirrors_the_inherited_key_in_place_without_keying_the_held_frame() {
    let mut source = document(2, vec![square("s-a", Value::Null)], None);
    source["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&source);
    let before = state(&owner);
    let mut step = reshape("s-a", FLIP_H);
    step["frame"] = json!(0);
    let r = send(&mut owner, "motion", batch(vec![step]));
    assert_eq!(r["ok"], true, "{r}");
    let after = state(&owner);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":0}]), "no key was added at the held frame");
    let xs: Vec<f64> = at(&owner, "s-a")["geometry"]["segments"].as_array().unwrap().iter().map(|s| xy(&s["point"])[0]).collect();
    assert_eq!(xs, vec![100., 0., 0., 100.], "the key every held frame shows is mirrored");
    assert_eq!(owner.history_depths(), (1, 0), "one gesture, one history entry");
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
    // A held frame named without the copy key still has no records of its own: the step must name the key's cell.
    let mut held = reshape("s-a", FLIP_H);
    held["frame"] = json!(3);
    assert_eq!(send(&mut owner, "held", batch(vec![held]))["ok"], false);
}
