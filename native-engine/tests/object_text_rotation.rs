//! #2267: a text block's stored `rotation` (degrees, about its anchor) turns the painted block in the one scene every reader shares
//! (preview, export, SVG, the hit-test listing), survives a save and a reopen, and an edit of the block never loses it.
use super::*;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    reader, serialize_svg_frame, SvgFrameSpec,
};

fn turned(degrees: f64) -> Value {
    let mut doc = drawing();
    doc["objects"] = json!([]);
    doc["items"].as_object_mut().unwrap().remove("raster");
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] =
        json!([{"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
    if degrees != 0.0 {
        doc["items"]["text"][0]["block"]["rotation"] = json!(degrees);
    }
    doc
}
fn contours_of(doc: &Value) -> Vec<Vec<[f64; 6]>> {
    let o = owner(doc);
    let p = plan(&support::head(&o), 0);
    ordered_entries(&p)
        .filter_map(|e| match e {
            PaintEntry::Text(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| {
            t.item
                .scene()
                .contours()
                .iter()
                .map(|c| {
                    c.contour
                        .segments()
                        .iter()
                        .map(|s| [s.point[0], s.point[1], s.handle_in[0], s.handle_in[1], s.handle_out[0], s.handle_out[1]])
                        .collect()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
/// `degrees` about `(10, 20)` applied to a point row (handles take the linear part).
fn turn(row: [f64; 6], degrees: f64) -> [f64; 6] {
    let (s, c) = degrees.to_radians().sin_cos();
    let lin = |x: f64, y: f64| (x * c - y * s, x * s + y * c);
    let p = lin(row[0] - 10.0, row[1] - 20.0);
    let (hi, ho) = (lin(row[2], row[3]), lin(row[4], row[5]));
    [10.0 + p.0, 20.0 + p.1, hi.0, hi.1, ho.0, ho.1]
}

#[test]
fn a_turned_block_paints_every_contour_turned_about_its_stored_anchor() {
    let upright = contours_of(&turned(0.0));
    for degrees in [30.0, -90.0, 180.0] {
        let spun = contours_of(&turned(degrees));
        assert_eq!(spun.len(), upright.len());
        assert_eq!(spun.len(), 3, "the three paths of the bundled A");
        for (a, b) in upright.iter().zip(&spun) {
            assert_eq!(a.len(), b.len());
            for (from, to) in a.iter().zip(b) {
                let want = turn(*from, degrees);
                assert!(
                    want.iter().zip(to).all(|(w, g)| (w - g).abs() < 1e-6),
                    "{degrees}: {to:?} != {want:?}"
                );
            }
        }
    }
}

#[test]
fn the_svg_export_draws_the_turned_block() {
    let svg_of = |doc: &Value| {
        let o = owner(doc);
        let p = plan(&support::head(&o), 0);
        let spec = SvgFrameSpec::new(320, 180, 1024 * 1024).unwrap();
        reader::parse(&serialize_svg_frame(&p, spec).unwrap().bytes).unwrap()
    };
    let (flat, spun) = (svg_of(&turned(0.0)), svg_of(&turned(-90.0)));
    assert_eq!(flat.paths.len(), spun.paths.len());
    let want = turn([flat.paths[0].start[0], flat.paths[0].start[1], 0.0, 0.0, 0.0, 0.0], -90.0);
    assert!(
        (spun.paths[0].start[0] - want[0]).abs() < 1e-3 && (spun.paths[0].start[1] - want[1]).abs() < 1e-3,
        "{:?} != {want:?}",
        spun.paths[0].start
    );
    assert_ne!(flat.paths[0].start, spun.paths[0].start);
}

#[test]
fn the_hit_listing_reports_the_turned_frame_and_the_stored_bounds() {
    let listed = |doc: &Value| {
        let mut doc = doc.clone();
        doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] =
            json!([{"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
        super::hit_listing::listing_of(&doc)["items"][0].clone()
    };
    let (flat, spun) = (listed(&turned(0.0)), listed(&turned(90.0)));
    assert_eq!(spun["role"], "text");
    let nums = |v: &Value| v.as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect::<Vec<_>>();
    // The bounds stay the block's own (unturned) frame; the map carries it to the painted block: a quarter turn about (10, 20).
    for (a, b) in nums(&flat["bounds"]).iter().zip(nums(&spun["bounds"])) {
        assert!((a - b).abs() < 1e-6, "{flat} vs {spun}");
    }
    for (got, want) in nums(&spun["matrix"]).iter().zip([0.0, 1.0, -1.0, 0.0, 30.0, 10.0]) {
        assert!((got - want).abs() < 1e-9, "{spun}");
    }
    assert_eq!(nums(&flat["matrix"]), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

fn block_of(o: &crate::history::NativeObjectHistory) -> Value {
    support::items_of(o)["text"][0]["block"].clone()
}

#[test]
fn the_turn_survives_an_edit_of_the_block_undo_and_a_save_reopen() {
    let mut o = owner(&turned(30.0));
    let before = support::bytes(&o);
    support::run(
        &mut o,
        &json!({"command":"object.text.edit","stableTarget":support::target("L0","text"),"patch":{"size":30,"color":"#445566"}}),
    )
    .unwrap();
    let block = block_of(&o);
    assert_eq!((block["rotation"].as_f64(), block["size"].as_f64()), (Some(30.0), Some(30.0)));
    o.undo(o.content_revision()).unwrap();
    assert_eq!(support::bytes(&o), before, "undo is exact, the turn included");
    // Save and reopen: the strict record round trip keeps the member (and an upright block stays without one).
    let head = support::head(&o);
    let stored = o.serialize_project_file(o.content_revision(), head.snapshot_id()).unwrap();
    let reopened = crate::history::NativeObjectHistory::from_project_file("turn-reopen", &stored).unwrap();
    assert_eq!(block_of(&reopened)["rotation"].as_f64(), Some(30.0));
    assert!(block_of(&owner(&turned(0.0))).get("rotation").is_none());
}

#[test]
fn a_rotation_that_is_not_a_finite_turn_is_refused() {
    use crate::object_codec::decode_storage_project;
    for bad in [json!(361), json!(-400), json!("30"), json!(null)] {
        let mut doc = turned(0.0);
        doc["items"]["text"][0]["block"]["rotation"] = bad.clone();
        assert!(decode_storage_project(doc.to_string().as_bytes()).is_err(), "{bad} must be refused");
    }
    let mut ok = turned(0.0);
    ok["items"]["text"][0]["block"]["rotation"] = json!(-360);
    assert!(decode_storage_project(ok.to_string().as_bytes()).is_ok());
}
