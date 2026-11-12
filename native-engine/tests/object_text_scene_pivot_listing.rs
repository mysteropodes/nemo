//! #2372 A-select-rest: the hit-test listing carries the stored Select pivot of an image and of a text block, as it does a vector record's.
use super::*;

#[test]
fn the_listing_reads_back_the_stored_pivot_of_an_image_and_a_text_block() {
    let mut doc = drawing();
    doc["items"]["raster"][0]["item"]["xformAnchorKey"] = json!("tl");
    doc["items"]["text"][0]["block"]["xformAnchorCustom"] = json!([12.5, 34.5]);
    let result = super::hit_listing::listing_of(&doc);
    let items = result["items"].as_array().unwrap();
    let by = |role: &str| items.iter().find(|i| i["role"] == role).unwrap().clone();
    let (image, block) = (by("raster"), by("text"));
    assert_eq!((image["xformAnchorKey"].clone(), image.get("xformAnchorCustom").cloned()), (json!("tl"), None));
    assert_eq!((block["xformAnchorCustom"].clone(), block.get("xformAnchorKey").cloned()), (json!([12.5, 34.5]), None));
    // A turned block lists it too (its listing item is built by the turned-frame branch).
    doc["items"]["text"][0]["block"]["rotation"] = json!(30);
    let turned = super::hit_listing::listing_of(&doc)["items"].as_array().unwrap().iter().find(|i| i["role"] == "text").unwrap().clone();
    assert_eq!(turned["xformAnchorCustom"], json!([12.5, 34.5]), "{turned}");
    // An item that never had one lists none.
    let plain = super::hit_listing::listing_of(&drawing());
    assert!(plain["items"].as_array().unwrap().iter().all(|i| i.get("xformAnchorKey").is_none() && i.get("xformAnchorCustom").is_none()));
}
