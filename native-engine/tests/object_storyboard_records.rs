//! Oracle tests for the native StoryBoard records (L04-G, I14/I35).
//!
//! `fixtures/object-storyboard-records/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_oracle.cjs` of the L04-G leaf from baseline fbe4b8a3
//! `src/js/storyboard.js`: a legacy v1 storyboard (a montage with `items` and
//! an inline-audio entry) is run through the verbatim `migrateLegacy`, then the
//! verbatim `addInstanceAuto`, `addMontage` and `ensureRetime` (with the join
//! statements of `tryJoinInstance`), and the resulting wire value is
//! transcribed 1:1 into the native record (`_v2` dropped, `dataB64` replaced by
//! an `assetId`). Rows are TAB separated, `-` is absent.
//! `S` storyboard JSON; `M` id, type, symbolId, trimIn, trimOut, duration, chain
//! (`-` absent, `=` empty, comma list), audio (`moduleId@offset` list), x, y;
//! `N` nextId, activeMontageId, module count; `R` symbol id, `symbolDuration`.
//! Every expected token was read from the baseline objects, not from the JSON.
use super::*;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-storyboard-records/oracle.txt");
pub(crate) const SYMBOLS: [&str; 1] = ["sym_comp"];

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}
pub(crate) fn storyboard_json() -> &'static str {
    rows("S")[0][1]
}
pub(crate) fn storyboard_value() -> Value {
    serde_json::from_str(storyboard_json()).expect("storyboard json")
}
pub(crate) fn baseline() -> StoryboardRecord {
    serde_json::from_str(storyboard_json()).expect("baseline storyboard")
}
fn tok(token: &str) -> Option<f64> {
    if token == "-" {
        None
    } else {
        Some(token.parse::<f64>().expect("number token"))
    }
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains("migrateLegacy"));
    assert_eq!(rows("S").len(), 1);
    assert_eq!(rows("M").len(), 6);
    assert_eq!(rows("N").len(), 1);
    assert_eq!(rows("R")[0][2], "12");
}

#[test]
fn storyboard_decodes_and_re_encodes_to_the_exact_authored_bytes() {
    let record = baseline();
    assert_eq!(serde_json::to_string(&record).unwrap(), storyboard_json());
    let again: StoryboardRecord =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(again, record);
    assert_eq!(
        serde_json::to_vec(&again).unwrap(),
        storyboard_json().as_bytes()
    );
}

#[test]
fn modules_match_the_baseline_state_field_by_field() {
    let record = baseline();
    for row in rows("M") {
        let module = record
            .module(row[1])
            .unwrap_or_else(|| panic!("module {}", row[1]));
        assert_eq!(module.id(), row[1]);
        assert_eq!(module.kind(), row[2]);
        assert_eq!(
            module.symbol_id(),
            if row[3] == "-" { None } else { Some(row[3]) }
        );
        assert_eq!(module.trim_in(), tok(row[4]), "trimIn {}", row[1]);
        assert_eq!(module.trim_out(), tok(row[5]), "trimOut {}", row[1]);
        assert_eq!(module.duration(), tok(row[6]), "duration {}", row[1]);
        match row[7] {
            "-" => assert_eq!(module.chain, Presence::Missing, "{}", row[1]),
            "=" => assert_eq!(module.chain, Presence::Present(Vec::new()), "{}", row[1]),
            list => assert_eq!(
                module.chain(),
                list.split(',').collect::<Vec<_>>(),
                "{}",
                row[1]
            ),
        }
        match row[8] {
            "-" => assert_eq!(module.audio, Presence::Missing, "{}", row[1]),
            "=" => assert_eq!(module.audio, Presence::Present(Vec::new()), "{}", row[1]),
            list => {
                let joins: Vec<String> = module
                    .audio()
                    .iter()
                    .map(|a| format!("{}@{}", a.module_id, a.offset_frames))
                    .collect();
                assert_eq!(joins.join(","), list, "{}", row[1]);
            }
        }
        assert_eq!(module.x(), tok(row[9]), "x {}", row[1]);
        assert_eq!(module.y(), tok(row[10]), "y {}", row[1]);
    }
    assert_eq!(record.modules().len(), 6);
    assert!(record.module("sbm99").is_none());
}

#[test]
fn counter_and_active_montage_match_the_baseline() {
    let n = &rows("N")[0];
    let record = baseline();
    assert_eq!(record.next_id().to_string(), n[1]);
    assert_eq!(record.active_montage_id(), Some(n[2]));
    assert_eq!(record.modules().len().to_string(), n[3]);
}

#[test]
fn baseline_storyboard_is_admissible_against_its_symbols() {
    baseline().validate(&SYMBOLS).expect("baseline storyboard");
}

#[test]
fn retime_fields_are_absent_until_the_baseline_sets_them() {
    let record = baseline();
    let total: f64 = rows("R")[0][2].parse().unwrap();
    // sbm13 joined a chain: ensureRetime wrote trimIn 0, trimOut symbolDuration - 1, duration = span.
    let joined = record.module("sbm13").unwrap();
    assert_eq!(joined.trim_in(), Some(0.0));
    assert_eq!(joined.trim_out(), Some(total - 1.0));
    assert_eq!(
        joined.duration(),
        Some(joined.trim_out().unwrap() - joined.trim_in().unwrap() + 1.0)
    );
    // sbm11 was migrated from an item without retime and never joined through ensureRetime.
    let bare = record.module("sbm11").unwrap();
    assert_eq!(
        (bare.trim_in(), bare.trim_out(), bare.duration()),
        (None, None, None)
    );
    assert_eq!(bare.trim_in, Presence::Missing);
    // sbm10 is a STRETCH: 8 source frames squeezed to a different duration (speed != 1).
    let stretched = record.module("sbm10").unwrap();
    let source = stretched.trim_out().unwrap() - stretched.trim_in().unwrap() + 1.0;
    assert_eq!((source, stretched.duration()), (8.0, Some(16.0)));
}

#[test]
fn chain_order_is_the_sequence_and_audio_lives_on_the_montage() {
    let record = baseline();
    assert_eq!(record.module("sbm9").unwrap().chain(), ["sbm10", "sbm11"]);
    assert_eq!(record.module("sbm14").unwrap().chain(), ["sbm13"]);
    let join = &record.module("sbm9").unwrap().audio()[0];
    assert_eq!(
        (join.module_id.as_str(), join.offset_frames.to_string()),
        ("sbm12", "3".to_string())
    );
    assert!(record.module("sbm13").unwrap().chain().is_empty());
    assert!(record.module("sbm13").unwrap().audio().is_empty());
}

#[test]
fn a_sound_references_an_asset_instead_of_embedding_audio() {
    let record = baseline();
    let sound = record.module("sbm12").unwrap();
    assert_eq!(
        sound.asset_id,
        Presence::Present("asset-audio-sbm12".to_string())
    );
    assert_eq!(
        sound.in_sec,
        Presence::Present(Some(serde_json::Number::from(0)))
    );
    assert_eq!(
        sound
            .out_sec
            .value()
            .map(|v| v.as_ref().map(|n| n.to_string())),
        Some(Some("1.5".to_string()))
    );
    assert_eq!(
        sound.cursor_sec,
        Presence::Present(None),
        "cursorSec null is kept, not dropped"
    );
    assert!(!storyboard_json().contains("dataB64"));
}

#[test]
fn active_montage_null_and_absent_stay_distinct() {
    let mut value = storyboard_value();
    value["activeMontageId"] = Value::Null;
    let null: StoryboardRecord = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(null.active_montage_id, Presence::Present(None));
    assert_eq!(null.active_montage_id(), None);
    assert!(serde_json::to_string(&null)
        .unwrap()
        .contains("\"activeMontageId\":null"));
    value.as_object_mut().unwrap().remove("activeMontageId");
    let absent: StoryboardRecord = serde_json::from_value(value).unwrap();
    assert_eq!(absent.active_montage_id, Presence::Missing);
    assert!(!serde_json::to_string(&absent)
        .unwrap()
        .contains("activeMontageId"));
    null.validate(&SYMBOLS).unwrap();
    absent.validate(&SYMBOLS).unwrap();
}

#[test]
fn dangling_chain_and_audio_ids_stay_representable_because_the_kernel_skips_them() {
    let mut value = storyboard_value();
    let at = |v: &Value, id: &str| {
        v["modules"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| m["id"] == id)
            .unwrap()
    };
    let montage = at(&value, "sbm14");
    value["modules"][montage]["chain"] = serde_json::json!(["sbm13", "sbm404", "sbm12"]);
    value["modules"][montage]["audio"] =
        serde_json::json!([{"moduleId": "sbm405", "offsetFrames": 0}]);
    let record: StoryboardRecord = serde_json::from_value(value).unwrap();
    record
        .validate(&SYMBOLS)
        .expect("chainMods skips ids that name no instance");
    assert_eq!(
        record.module("sbm14").unwrap().chain(),
        ["sbm13", "sbm404", "sbm12"]
    );
}

#[test]
fn re_applying_identical_records_is_byte_identical() {
    let first = serde_json::to_vec(&baseline()).unwrap();
    let again: StoryboardRecord = serde_json::from_slice(&first).unwrap();
    assert_eq!(serde_json::to_vec(&again).unwrap(), first);
    assert_eq!(again, baseline());
}

#[test]
fn a_storyboard_with_only_the_required_members_round_trips() {
    let text =
        "{\"format\":\"nemo.native-storyboard\",\"formatVersion\":1,\"nextId\":1,\"modules\":[]}";
    let record: StoryboardRecord = serde_json::from_str(text).unwrap();
    assert_eq!(
        (record.pan.clone(), record.zoom.clone()),
        (Presence::Missing, Presence::Missing)
    );
    assert_eq!(serde_json::to_string(&record).unwrap(), text);
    record.validate(&[]).unwrap();
}
