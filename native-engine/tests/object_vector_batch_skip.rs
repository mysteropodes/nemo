//! #2372: `object.vector.batch` is atomic. A restyle the engine cannot do (a texture anchor the edit would unhide) refuses the whole
//! batch with nothing written, unless the request sets the boolean `skipUnavailable`: then the rest restyle and the receipt reports
//! how many steps were skipped (still one id per requested step); a batch where every step is skipped is refused all the same.
use super::support::*;
use super::batch_style::{at, batch, refused, restyle, target};
use serde_json::{json, Value};

fn anchor_record(id: &str) -> Value {
    let mut r = record_at("A", "L1", 0, id);
    r["vectorExt"] = json!({"brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1,"strokeSeed":1}});
    r
}
fn green(id: &str) -> Value {
    restyle(id, json!({"stroke":"#00ff00"}))
}
fn skipping(ops: Vec<Value>) -> Value {
    let mut p = batch(ops);
    p["skipUnavailable"] = json!(true);
    p
}

#[test]
fn a_batch_with_a_restyle_the_engine_cannot_do_is_atomic_by_default() {
    // [valid A, unavailable B] (a texture anchor the edit would unhide) must not half-apply: B refuses the whole batch.
    let doc = document(1, vec![record_at("B", "L1", 0, "s-b"), anchor_record("anchor")], None);
    refused(&doc, vec![green("s-b"), green("anchor")], "unavailable");
    refused(&doc, vec![green("anchor"), green("s-b")], "unavailable");
    let two = document(1, vec![anchor_record("anchor"), anchor_record("anchor-2")], None);
    refused(&two, vec![green("anchor"), green("anchor-2")], "unavailable");
    // An explicit false is the default.
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    let mut p = batch(vec![green("s-b"), green("anchor")]);
    p["skipUnavailable"] = json!(false);
    let r = send(&mut owner, "no", p);
    assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("unavailable")), "{r}");
    assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)), "nothing was written");
}

#[test]
fn an_opted_in_mixed_selection_restyles_what_it_can_and_reports_the_skipped_texture_anchor() {
    let mut owner = owner_of(&document(1, vec![record_at("B", "L1", 0, "s-b"), anchor_record("anchor")], None));
    let before = state(&owner);
    let r = send(&mut owner, "mix", skipping(vec![green("s-b"), green("anchor")]));
    assert_eq!(r["ok"], true, "the plain item restyles, the anchor is skipped: {r}");
    assert_eq!(r["result"]["skippedSteps"], 1, "{r}");
    assert_eq!(r["result"]["strokeIds"], json!(["s-b", "anchor"]), "one id per requested step: {r}");
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(at(&owner, "anchor"), before["objects"][1], "the skipped anchor is untouched");
    assert_ne!(at(&owner, "s-b")["vector"]["stroke"], before["objects"][0]["vector"]["stroke"]);
    // Skipped first: the same outcome, the receipt keeps the request order.
    let mut owner = owner_of(&document(1, vec![record_at("B", "L1", 0, "s-b"), anchor_record("anchor")], None));
    let r = send(&mut owner, "mix", skipping(vec![green("anchor"), green("s-b")]));
    assert_eq!((r["ok"].clone(), r["result"]["skippedSteps"].clone(), r["result"]["strokeIds"].clone()), (json!(true), json!(1), json!(["anchor", "s-b"])), "{r}");
    // A batch that skips nothing reports no skipped steps.
    let mut owner = owner_of(&document(1, vec![record_at("B", "L1", 0, "s-b")], None));
    let r = send(&mut owner, "plain", skipping(vec![green("s-b")]));
    assert_eq!(r["ok"], true, "{r}");
    assert!(r["result"].get("skippedSteps").is_none(), "{r}");
}

#[test]
fn an_opted_in_batch_where_every_restyle_is_unavailable_is_still_refused() {
    let two = document(1, vec![anchor_record("anchor"), anchor_record("anchor-2")], None);
    let mut owner = owner_of(&two);
    let before = state(&owner);
    for (id, ops) in [("all-2", vec![green("anchor"), green("anchor-2")]), ("all-1", vec![green("anchor")])] {
        let r = send(&mut owner, id, skipping(ops));
        assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("unavailable")), "{r}");
    }
    assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)), "nothing was written");
}

#[test]
fn skip_unavailable_is_a_boolean_batch_member_only() {
    let doc = document(1, vec![record_at("B", "L1", 0, "s-b")], None);
    let mut owner = owner_of(&doc);
    for bad in [json!(1), json!("true"), json!(null)] {
        let mut p = batch(vec![green("s-b")]);
        p["skipUnavailable"] = bad.clone();
        let r = send(&mut owner, "bad", p);
        assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("invalid_request")), "{bad}: {r}");
    }
    let mut delete = json!({"command":"object.vector.delete","stableTarget":target(0, "s-b")});
    delete["skipUnavailable"] = json!(true);
    let r = send(&mut owner, "del", delete);
    assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("invalid_request")), "{r}");
    assert_eq!(owner.history_depths(), (0, 0));
}
