//! Oracle tests for the native duplicator and effector-layer records (L04-G, I07).
//!
//! `fixtures/object-duplicator-records/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_oracle.cjs` of the L04-G leaf from baseline fbe4b8a3: the
//! `ly_mig_dup` duplicator of `tests/fixtures/migration/project.json` (legacy
//! effector, `sourceMode` shape, `endAngle` null), the default descriptor made
//! by the verbatim `toggleLayerDuplicator` (`Math.random` pinned to 0.5), the
//! object-duplicator host literal of `convertSelectionToObjectDuplicator`, the
//! add-effector literal of the duplicator panel, the `ld.effector` literal of
//! `addEffectorLayer`, and the result of the verbatim `_duplicatorCount` over a
//! 551-row descriptor matrix. The header lists the baseline blob ids.
//!
//! Rows are TAB separated. `D` label, layerUid, descriptor JSON; `E` effector
//! layer JSON; `C` descriptor JSON, effective clone count. The JSON columns are
//! the native record shape; every count was computed by the baseline function.
use super::*;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-duplicator-records/oracle.txt");
pub(crate) const LAYER_UIDS: [&str; 9] = [
    "ly_mig_dup",
    "ly_toggle",
    "ly_objhost",
    "ly_pathdup",
    "ly_effector",
    "ly_path_src",
    "ly_pool_a",
    "ly_pool_b",
    "ly_pool_c",
];

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}
pub(crate) fn layers() -> Vec<LayerRef<'static>> {
    LAYER_UIDS
        .iter()
        .map(|uid| LayerRef {
            context_id: "scene-root",
            layer_uid: *uid,
        })
        .collect()
}
pub(crate) fn record_json(row: &[&str]) -> String {
    format!(
        "{{\"contextId\":\"scene-root\",\"layerUid\":\"{}\",\"duplicator\":{}}}",
        row[2], row[3]
    )
}
pub(crate) fn set_json() -> String {
    let records: Vec<String> = rows("D").iter().map(|r| record_json(r)).collect();
    format!(
        "{{\"format\":\"{}\",\"formatVersion\":{},\"duplicators\":[{}],\"effectorLayers\":[{}]}}",
        DUPLICATOR_SET_FORMAT,
        DUPLICATOR_SET_FORMAT_VERSION,
        records.join(","),
        rows("E")[0][1]
    )
}
pub(crate) fn set_value() -> Value {
    serde_json::from_str(&set_json()).expect("set json")
}
pub(crate) fn baseline() -> DuplicatorSet {
    serde_json::from_str(&set_json()).expect("baseline set")
}
fn descriptor(label: &str) -> DuplicatorDescriptor {
    let row = rows("D")
        .into_iter()
        .find(|r| r[1] == label)
        .expect("descriptor row");
    serde_json::from_str(row[3]).expect("descriptor")
}
fn count_of(json: &str) -> f64 {
    serde_json::from_str::<DuplicatorDescriptor>(json)
        .unwrap()
        .effective_clone_count()
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains("tests/fixtures/migration/project.json"));
    assert_eq!(rows("D").len(), 4);
    assert_eq!(rows("E").len(), 1);
    assert_eq!(rows("C").len(), 551);
}

#[test]
fn baseline_descriptors_round_trip_to_the_exact_authored_bytes() {
    for row in rows("D") {
        let parsed: DuplicatorDescriptor = serde_json::from_str(row[3]).unwrap();
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            row[3],
            "{}",
            row[1]
        );
        let record: DuplicatorRecord = serde_json::from_str(&record_json(&row)).unwrap();
        assert_eq!(serde_json::to_string(&record).unwrap(), record_json(&row));
        assert_eq!(record.duplicator, parsed);
    }
    let layer: EffectorLayerRecord = serde_json::from_str(rows("E")[0][1]).unwrap();
    assert_eq!(serde_json::to_string(&layer).unwrap(), rows("E")[0][1]);
}

#[test]
fn effective_clone_count_equals_the_verbatim_baseline_for_every_row() {
    for row in rows("C") {
        let want: f64 = row[2].parse().expect("count");
        assert_eq!(count_of(row[1]), want, "{}", row[1]);
    }
}

#[test]
fn caps_are_the_baseline_ones_and_authored_values_are_never_clamped() {
    assert_eq!((GRID_CAP, OTHER_CAP), (900.0, 500.0));
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":30,\"cols\":30}"),
        900.0
    );
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":30,\"cols\":31}"),
        900.0
    );
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":29,\"cols\":31}"),
        899.0
    );
    assert_eq!(count_of("{\"mode\":\"radial\",\"count\":500}"), 500.0);
    assert_eq!(count_of("{\"mode\":\"path\",\"count\":501}"), 500.0);
    let huge = "{\"mode\":\"grid\",\"rows\":10000,\"cols\":10000}";
    let record: DuplicatorDescriptor = serde_json::from_str(huge).unwrap();
    assert_eq!(record.effective_clone_count(), 900.0);
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        huge,
        "the record keeps what was authored"
    );
    // The grid cap is not applied to the other modes' count and vice versa.
    assert_eq!(
        count_of("{\"mode\":\"radial\",\"rows\":9,\"cols\":9,\"count\":4}"),
        4.0
    );
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":2,\"cols\":3,\"count\":99}"),
        6.0
    );
}

#[test]
fn mode_defaults_read_like_the_baseline() {
    assert_eq!(count_of("{}"), 1.0);
    assert_eq!(count_of("{\"rows\":3,\"cols\":4}"), 12.0);
    assert_eq!(
        count_of("{\"mode\":\"\",\"rows\":3,\"cols\":4}"),
        12.0,
        "the empty label is grid"
    );
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":0,\"cols\":5}"),
        5.0,
        "0 reads as 1"
    );
    assert_eq!(
        count_of("{\"mode\":\"grid\",\"rows\":-4,\"cols\":5}"),
        5.0,
        "negatives clamp to 1"
    );
    assert_eq!(
        count_of("{\"mode\":\"path\",\"count\":2.5}"),
        2.5,
        "fractions are not rounded"
    );
}

#[test]
fn whole_table_validates_against_its_layers() {
    let set = baseline();
    assert_eq!((set.duplicators.len(), set.effector_layers.len()), (4, 1));
    set.validate(&layers())
        .expect("baseline table is admissible");
    assert_eq!(serde_json::to_string(&set).unwrap(), set_json());
    assert!(set.duplicator("scene-root", "ly_toggle").is_some());
    assert!(set.duplicator("sym_comp", "ly_toggle").is_none());
    assert_eq!(set.effector_layers_for("scene-root", "ly_toggle").len(), 1);
    assert!(set
        .effector_layers_for("scene-root", "ly_pathdup")
        .is_empty());
    assert!(set.effector_layers_for("sym_comp", "ly_toggle").is_empty());
}

#[test]
fn legacy_effector_fields_are_kept_as_authored() {
    let legacy = descriptor("migration-fixture-legacy-effector");
    let eff = &legacy.effectors()[0];
    let offset: Vec<String> = eff
        .offset_pos
        .value()
        .unwrap()
        .iter()
        .map(|n| n.to_string())
        .collect();
    assert_eq!(offset, ["10", "0"]);
    assert_eq!(
        eff.offset_rot,
        Presence::Present(serde_json::Number::from(5))
    );
    assert_eq!(
        (eff.channels.clone(), eff.pos.clone()),
        (Presence::Missing, Presence::Missing)
    );
    assert_eq!(
        eff.falloff,
        Presence::Missing,
        "no falloff is stored; the kernel reads radial"
    );
    // The literal the panel pushes today uses channels and pos instead.
    let added = descriptor("path-with-add-effector-literal");
    let eff = &added.effectors()[0];
    assert_eq!(
        eff.pos.value().map(|p| (p.x.to_string(), p.y.to_string())),
        Some(("320".into(), "180".into()))
    );
    assert_eq!(eff.offset_pos, Presence::Missing);
    let channels = eff.channels.value().unwrap().as_ref().unwrap();
    assert_eq!(
        (
            channels[0].prop.as_str(),
            channels[0].value.value().map(Vec::len)
        ),
        ("position", Some(2))
    );
}

#[test]
fn null_end_angle_and_path_layer_stay_distinct_from_absent() {
    let toggled = descriptor("toggle-default-grid");
    assert_eq!(toggled.end_angle, Presence::Present(None));
    assert_eq!(toggled.path_layer_uid, Presence::Present(None));
    assert_eq!(toggled.path_layer_uid(), None);
    assert_eq!(
        toggled.source_mode,
        Presence::Missing,
        "the shape-mode default writes no sourceMode"
    );
    assert_eq!(toggled.effectors, Presence::Missing);
    assert!(toggled.effectors().is_empty() && toggled.source_layer_uids().is_empty());
    assert_eq!(
        toggled.seed.value().map(|n| n.to_string()),
        Some("500000".into())
    );
    let mut value: Value = serde_json::from_str(rows("D")[1][3]).unwrap();
    value.as_object_mut().unwrap().remove("endAngle");
    let absent: DuplicatorDescriptor = serde_json::from_value(value).unwrap();
    assert_eq!(absent.end_angle, Presence::Missing);
    assert!(!serde_json::to_string(&absent).unwrap().contains("endAngle"));
    let path = descriptor("path-with-add-effector-literal");
    assert_eq!(
        (path.mode_label(), path.path_layer_uid()),
        (Some("path"), Some("ly_path_src"))
    );
}

#[test]
fn partial_stagger_and_object_pool_come_through_as_authored() {
    let host = descriptor("object-duplicator-host");
    assert_eq!(
        host.source_layer_uids(),
        ["ly_pool_a", "ly_pool_b", "ly_pool_c"]
    );
    assert_eq!(host.source_mode, Presence::Present("objects".to_string()));
    let lazy = "{\"staggerRandom\":{\"position\":true},\"timeOffset\":{\"direction\":\"random\"}}";
    let parsed: DuplicatorDescriptor = serde_json::from_str(lazy).unwrap();
    let stagger = parsed.stagger_random.value().unwrap();
    assert_eq!(stagger.position, Presence::Present(true));
    assert_eq!(stagger.rotation, Presence::Missing);
    assert_eq!(serde_json::to_string(&parsed).unwrap(), lazy);
    let path = descriptor("path-with-add-effector-literal");
    let offset = path.time_offset.value().unwrap();
    assert_eq!(offset.direction, Presence::Present("centerOut".to_string()));
}

#[test]
fn re_applying_identical_records_is_byte_identical() {
    let set = baseline();
    let first = serde_json::to_vec(&set).unwrap();
    let again: DuplicatorSet = serde_json::from_slice(&first).unwrap();
    assert_eq!(again, set);
    assert_eq!(serde_json::to_vec(&again).unwrap(), first);
    again.validate(&layers()).expect("still admissible");
}
