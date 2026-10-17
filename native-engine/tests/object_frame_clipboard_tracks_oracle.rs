//! The oracle rows through the real kernel: oracle.txt was produced by running the VERBATIM baseline `repeatSelection`
//! (src/js/timeline.js at fbe4b8a3) over dense layers whose `ld.motion` tables carry property tracks. Each row is rebuilt
//! as a native document (version 1 and version 2; a row with an `order` table only in version 2, where the layer-level
//! order track lives in the composition-order plan), cycled, and compared with a document built from the baseline's output:
//! every field and number representation, not only the tables.
use super::super::{repeat_motion_tracks, MAX_ROUNDS};
use super::rows::{matches_expected, rows, tables, Row};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Value;

fn run(row: &Row, doc: &mut ObjectDocument) -> Result<(), DispatchErrorCode> {
    repeat_motion_tracks(doc, &row.layers(), row.first, row.last, row.eff)
}

#[test]
fn every_oracle_row_matches_the_baseline_in_each_document_form() {
    let mut pairs = 0;
    let mut refused = 0;
    let mut unchanged = 0;
    for row in rows() {
        for form in row.forms() {
            let who = format!("{} {form:?}", row.label());
            let mut doc = super::rows::document(row.total, &row.input, form);
            let original = doc.clone();
            assert_eq!(tables(&original), row.input, "{who}: the input tables");
            let result = run(&row, &mut doc);
            if row.dup {
                // the baseline keeps two keys on one frame; a native track cannot, so the cycle refuses whole
                assert_eq!(result, Err(DispatchErrorCode::InvalidRequest), "{who}");
                assert_eq!(doc, original, "{who}: a refusal changes nothing");
                refused += 1;
            } else {
                result.unwrap_or_else(|code| panic!("{who}: {code:?}"));
                matches_expected(&row, form, &doc).unwrap_or_else(|message| panic!("{message}"));
                ObjectSnapshot::validate_drawing(&doc)
                    .unwrap_or_else(|error| panic!("{who}: storage admission {error}"));
                let saved = encode_storage_project(&doc).unwrap();
                assert_eq!(
                    decode_storage_project(&saved).unwrap(),
                    doc,
                    "{who}: reopen"
                );
                assert_eq!(doc == original, !row.changed, "{who}: change flag");
                if !row.changed {
                    unchanged += 1;
                    assert_eq!(saved, encode_storage_project(&original).unwrap(), "{who}");
                }
            }
            pairs += 1;
        }
    }
    assert_eq!((pairs, refused, unchanged), (231, 45, 40));
}

#[test]
fn the_raw_count_clamp_of_the_oracle_is_the_one_the_kernel_is_given() {
    let mut clamped = 0;
    for row in rows() {
        assert_eq!(
            row.times.clamp(1, i64::from(MAX_ROUNDS)),
            i64::from(row.eff),
            "{}: eff is Math.max(1, Math.min(50, parseInt(times, 10) || 1))",
            row.label()
        );
        if row.times != i64::from(row.eff) {
            clamped += 1;
        }
    }
    assert_eq!(clamped, 13);
}

fn is_fraction(key: &Value) -> bool {
    !key["frame"].is_i64()
}
/// Every key of a layer's table with the name of its property.
fn track_keys(table: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if let Some(object) = table.as_object() {
        for (name, track) in object {
            for key in track["keys"].as_array().unwrap() {
                out.push((name.clone(), key.clone()));
            }
        }
    }
    out
}

#[test]
fn oracle_reaches_every_branch() {
    let all = rows();
    let (mut sources, mut clones, mut order_sources, mut covered_layers) = (0, 0, 0, 0);
    let (mut hold_true, mut hold_false, mut null, mut empty, mut listed) = (0, 0, 0, 0, 0);
    let (mut hin, mut hout, mut fractional, mut control, mut multi, mut bare) = (0, 0, 0, 0, 0, 0);
    let (mut dup, mut changed, mut clean, mut order) = (0, 0, 0, 0);
    for row in &all {
        dup += usize::from(row.dup);
        changed += usize::from(row.changed);
        clean += usize::from(row.changed && !row.dup);
        order += usize::from(row.order);
        multi += usize::from(row.max_layer > row.min_layer);
        bare += usize::from(row.input.iter().any(Value::is_null));
        fractional += usize::from(
            row.input
                .iter()
                .any(|t| track_keys(t).iter().any(|(_, key)| is_fraction(key))),
        );
        control += usize::from(row.input.iter().any(|t| t.get("xc_amp").is_some()));
        for (layer, table) in row.input.iter().enumerate() {
            if layer < row.min_layer || layer > row.max_layer || table.is_null() {
                continue;
            }
            covered_layers += 1;
            for (name, key) in track_keys(table) {
                let at = key["frame"].as_f64().unwrap();
                if at < f64::from(row.first) || at > f64::from(row.last) {
                    continue;
                }
                sources += 1;
                clones += row.eff;
                order_sources += usize::from(name == "order");
                hold_true += usize::from(key["hold"] == true);
                hold_false += usize::from(key["hold"] == false);
                null += usize::from(key.get("curvePoints").is_some_and(Value::is_null));
                empty += usize::from(key["curvePoints"].as_array().is_some_and(|p| p.is_empty()));
                listed += usize::from(key["curvePoints"].as_array().is_some_and(|p| !p.is_empty()));
                hin += usize::from(key.get("hIn").is_some());
                hout += usize::from(key.get("hOut").is_some());
            }
        }
    }
    assert_eq!(
        (all.len(), dup, changed, clean, order),
        (125, 24, 103, 79, 19)
    );
    assert_eq!((fractional, control, multi, bare), (24, 57, 53, 45));
    assert_eq!(
        (covered_layers, sources, clones, order_sources),
        (163, 325, 1419, 17)
    );
    assert_eq!(
        (hold_true, hold_false, null, empty, listed),
        (54, 41, 34, 44, 109)
    );
    assert_eq!((hin, hout), (50, 46));
}
