//! Kernel tests for the immutable scene resource declaration set (L08-D).
//! Scheduler/permit behaviour lives in `object_scene_resource_set_permit.rs`,
//! mutation controls in `object_scene_resource_set_controls.rs`.
//!
//! `fixtures/object-scene-resource-set/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_object_scene_resource_set_oracle.js` against baseline commit
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc by running the unmodified
//! `src/js/application/render/image-budget.js` (`NemoImageBudget`) in a Node
//! `vm`. It pins the decoded-image charge (`w * h * 4`) and the running store
//! total/count, including same-id replacement. Everything else here (set
//! invariants, declaration comparison, retention lifetime) has no baseline-JS
//! analogue: those literals are hand-derived from the contract.
use super::*;
use std::sync::Weak;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-scene-resource-set/oracle.txt");

pub(crate) struct Payload(pub(crate) u32);

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|cells| cells[0] == kind)
        .collect()
}

pub(crate) fn keep(id: &str, version: &str, token: &Arc<Payload>) -> SceneResourceEntry {
    let retention: SceneResourceRetention = token.clone();
    SceneResourceEntry::new(id, version, SceneResourceKind::Raster, 10, retention)
}

pub(crate) fn set_of(pairs: &[(&str, &str)]) -> ObjectSceneResourceSet {
    let entries = pairs
        .iter()
        .map(|(id, version)| keep(id, version, &Arc::new(Payload(0))))
        .collect();
    ObjectSceneResourceSet::new(entries).unwrap()
}

pub(crate) fn declared_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(id, version)| (id.to_string(), version.to_string()))
        .collect()
}

pub(crate) type Expect = Option<(
    SceneResourceMismatchKind,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
)>;
pub(crate) type Pairs = &'static [(&'static str, &'static str)];
use super::SceneResourceMismatchKind::{Extra, Missing, VersionSkew};

/// (name, held, declared, expected first difference as
/// (kind, id, declared version, held version)).
pub(crate) const VERIFY_CASES: &[(&str, Pairs, Pairs, Expect)] = &[
    ("both-empty", &[], &[], None),
    (
        "exact-pair",
        &[("a", "1"), ("b", "7")],
        &[("a", "1"), ("b", "7")],
        None,
    ),
    (
        "missing-last",
        &[("a", "1")],
        &[("a", "1"), ("b", "1")],
        Some((Missing, "b", Some("1"), None)),
    ),
    (
        "extra-last",
        &[("a", "1"), ("b", "1")],
        &[("a", "1")],
        Some((Extra, "b", None, Some("1"))),
    ),
    (
        "version-skew",
        &[("a", "1")],
        &[("a", "2")],
        Some((VersionSkew, "a", Some("2"), Some("1"))),
    ),
    (
        "empty-held",
        &[],
        &[("a", "1")],
        Some((Missing, "a", Some("1"), None)),
    ),
    (
        "empty-declared",
        &[("a", "1")],
        &[],
        Some((Extra, "a", None, Some("1"))),
    ),
    (
        "held-id-sorts-first",
        &[("a", "1")],
        &[("b", "1")],
        Some((Extra, "a", None, Some("1"))),
    ),
    (
        "declared-id-sorts-first",
        &[("b", "1")],
        &[("a", "1")],
        Some((Missing, "a", Some("1"), None)),
    ),
    (
        "first-fault-wins",
        &[("a", "1"), ("c", "1"), ("d", "9")],
        &[("a", "1"), ("b", "1"), ("d", "1")],
        Some((Missing, "b", Some("1"), None)),
    ),
    (
        "skew-before-later-extra",
        &[("a", "1"), ("b", "1"), ("z", "1")],
        &[("a", "2")],
        Some((VersionSkew, "a", Some("2"), Some("1"))),
    ),
    (
        "version-case-sensitive",
        &[("a", "v1")],
        &[("a", "V1")],
        Some((VersionSkew, "a", Some("V1"), Some("v1"))),
    ),
    (
        "prefix-ids-are-distinct",
        &[("a", "1"), ("ab", "1")],
        &[("a", "1")],
        Some((Extra, "ab", None, Some("1"))),
    ),
    (
        "unicode-ids-order-bytewise",
        &[("z", "1"), ("\u{e9}", "1")],
        &[("z", "1"), ("\u{e9}", "1")],
        None,
    ),
    (
        "whitespace-is-not-trimmed",
        &[("a ", "1")],
        &[("a", "1")],
        Some((Missing, "a", Some("1"), None)),
    ),
];

pub(crate) fn observed(
    result: Result<(), SceneResourceMismatch>,
) -> Option<(
    SceneResourceMismatchKind,
    String,
    Option<String>,
    Option<String>,
)> {
    result
        .err()
        .map(|m| (m.kind, m.resource_id, m.declared_version, m.held_version))
}

pub(crate) fn expected(
    expect: &Expect,
) -> Option<(
    SceneResourceMismatchKind,
    String,
    Option<String>,
    Option<String>,
)> {
    (*expect).map(|(kind, id, declared, held)| {
        (
            kind,
            id.to_string(),
            declared.map(str::to_string),
            held.map(str::to_string),
        )
    })
}

#[test]
fn oracle_header_and_rows_are_well_formed() {
    assert!(ORACLE.contains("# baseline commit: fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains("src/js/application/render/image-budget.js sha256 "));
    assert_eq!((rows("px").len(), rows("op").len()), (18, 55));
    for line in ORACLE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let count = line.split_whitespace().count();
        assert!(
            matches!((&line[..2], count), ("px", 4) | ("op", 8)),
            "{line}"
        );
    }
}

#[test]
fn decoded_rgba8_bytes_reproduces_the_baseline_px_rows() {
    for row in rows("px") {
        let (w, h) = (
            row[1].parse::<u32>().unwrap(),
            row[2].parse::<u32>().unwrap(),
        );
        assert_eq!(
            decoded_rgba8_bytes(w, h),
            Some(row[3].parse().unwrap()),
            "{w}x{h}"
        );
    }
}

#[test]
fn decoded_rgba8_bytes_literals_and_overflow_boundary() {
    assert_eq!(decoded_rgba8_bytes(64, 64), Some(16_384));
    assert_eq!(decoded_rgba8_bytes(1920, 1080), Some(8_294_400));
    assert_eq!(decoded_rgba8_bytes(8192, 8192), Some(268_435_456));
    assert_eq!(decoded_rgba8_bytes(0, 4096), Some(0));
    assert_eq!(decoded_rgba8_bytes(u32::MAX, 1), Some(17_179_869_180));
    // (2^32 - 1)^2 * 4 > 2^64: no baseline double could hold it exactly.
    assert_eq!(decoded_rgba8_bytes(u32::MAX, u32::MAX), None);
    // (2^32 - 1) * 2^30 * 4 = 2^64 - 2^32: the largest case that still fits.
    assert_eq!(
        decoded_rgba8_bytes(u32::MAX, 1 << 30),
        Some(18_446_744_069_414_584_320)
    );
}

#[test]
fn set_totals_and_counts_follow_the_baseline_scenarios() {
    let mut state: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut scenario = String::new();
    for row in rows("op") {
        if row[1] != scenario {
            scenario = row[1].to_string();
            state.clear();
        }
        let (w, h) = (
            row[4].parse::<u32>().unwrap(),
            row[5].parse::<u32>().unwrap(),
        );
        state.insert(row[3].to_string(), (w, h));
        let entries = state
            .iter()
            .map(|(id, &(w, h))| {
                let retention: SceneResourceRetention = Arc::new(Payload(0));
                let bytes = decoded_rgba8_bytes(w, h).unwrap();
                SceneResourceEntry::new(
                    id,
                    format!("{w}x{h}"),
                    SceneResourceKind::Raster,
                    bytes,
                    retention,
                )
            })
            .collect();
        let set = ObjectSceneResourceSet::new(entries).unwrap();
        let (total, count) = (
            row[6].parse::<u64>().unwrap(),
            row[7].parse::<usize>().unwrap(),
        );
        assert_eq!(
            set.total_charged_bytes(),
            total,
            "{} step {}",
            row[1],
            row[2]
        );
        assert_eq!(set.len(), count, "{} step {}", row[1], row[2]);
        let touched = set.get(row[3]).unwrap();
        assert_eq!(touched.charged_bytes(), decoded_rgba8_bytes(w, h).unwrap());
        assert_eq!(touched.version(), format!("{w}x{h}"));
    }
    assert_eq!(scenario, "many");
}

#[test]
fn entries_are_sorted_unique_and_looked_up_by_id() {
    let set = set_of(&[("m", "3"), ("a", "1"), ("z", "9"), ("b", "2")]);
    let ids: Vec<&str> = set.entries().iter().map(|e| e.resource_id()).collect();
    assert_eq!(ids, ["a", "b", "m", "z"]);
    assert_eq!((set.len(), set.is_empty()), (4, false));
    assert_eq!(
        set.get("m").map(|e| (e.version(), e.kind())),
        Some(("3", SceneResourceKind::Raster))
    );
    assert!(set.get("nope").is_none() && set.get("").is_none() && set.get("mm").is_none());
    assert!(ObjectSceneResourceSet::empty().is_empty());
    assert_eq!(ObjectSceneResourceSet::empty().total_charged_bytes(), 0);
}

#[test]
fn construction_refusals_are_atomic_and_release_every_token() {
    let tokens: Vec<Arc<Payload>> = (0..3).map(|n| Arc::new(Payload(n))).collect();
    let entries = vec![
        keep("b", "1", &tokens[0]),
        keep("a", "1", &tokens[1]),
        keep("b", "2", &tokens[2]),
    ];
    assert!(tokens.iter().all(|t| Arc::strong_count(t) == 2));
    assert_eq!(
        ObjectSceneResourceSet::new(entries).unwrap_err(),
        SceneResourceSetError::DuplicateResourceId {
            resource_id: "b".into()
        }
    );
    assert!(tokens.iter().all(|t| Arc::strong_count(t) == 1));
    for (id, version, error) in [
        ("", "1", SceneResourceSetError::EmptyResourceId),
        (
            "x",
            "",
            SceneResourceSetError::EmptyVersion {
                resource_id: "x".into(),
            },
        ),
    ] {
        let entries = vec![keep("ok", "1", &tokens[0]), keep(id, version, &tokens[1])];
        assert_eq!(ObjectSceneResourceSet::new(entries).unwrap_err(), error);
        assert!(tokens.iter().all(|t| Arc::strong_count(t) == 1));
    }
}

#[test]
fn fault_precedence_is_first_faulty_entry_then_duplicate_then_overflow() {
    let t = Arc::new(Payload(0));
    let big = |id: &str, version: &str, bytes: u64| {
        let retention: SceneResourceRetention = t.clone();
        SceneResourceEntry::new(id, version, SceneResourceKind::Media, bytes, retention)
    };
    let version_first = vec![
        big("dup", "1", 0),
        big("dup", "2", 0),
        big("late", "", 0),
        big("", "1", 0),
    ];
    assert_eq!(
        ObjectSceneResourceSet::new(version_first).unwrap_err(),
        SceneResourceSetError::EmptyVersion {
            resource_id: "late".into()
        }
    );
    let id_first = vec![big("", "1", 0), big("late", "", 0)];
    assert_eq!(
        ObjectSceneResourceSet::new(id_first).unwrap_err(),
        SceneResourceSetError::EmptyResourceId
    );
    let both_empty = vec![big("", "", 0)];
    assert_eq!(
        ObjectSceneResourceSet::new(both_empty).unwrap_err(),
        SceneResourceSetError::EmptyResourceId
    );
    let dup_and_overflow = vec![big("dup", "1", u64::MAX), big("dup", "2", 1)];
    assert_eq!(
        ObjectSceneResourceSet::new(dup_and_overflow).unwrap_err(),
        SceneResourceSetError::DuplicateResourceId {
            resource_id: "dup".into()
        }
    );
    let overflow = vec![big("a", "1", u64::MAX), big("b", "1", 1)];
    assert_eq!(
        ObjectSceneResourceSet::new(overflow).unwrap_err(),
        SceneResourceSetError::ChargedBytesOverflow
    );
    assert_eq!(Arc::strong_count(&t), 1);
}

#[test]
fn charged_byte_sum_is_checked_not_wrapped() {
    let t = Arc::new(Payload(0));
    let with = |a: u64, b: u64| {
        let r: SceneResourceRetention = t.clone();
        let s: SceneResourceRetention = t.clone();
        ObjectSceneResourceSet::new(vec![
            SceneResourceEntry::new("a", "1", SceneResourceKind::Raster, a, r),
            SceneResourceEntry::new("b", "1", SceneResourceKind::Raster, b, s),
        ])
    };
    assert_eq!(
        with(u64::MAX - 1, 1).unwrap().total_charged_bytes(),
        u64::MAX
    );
    assert_eq!(
        with(u64::MAX, 1).unwrap_err(),
        SceneResourceSetError::ChargedBytesOverflow
    );
    assert_eq!(with(0, 0).unwrap().total_charged_bytes(), 0);
}

#[test]
fn declared_versions_are_the_exact_key_declaration() {
    let set = set_of(&[("raster/2", "v9"), ("brush/1", "v3"), ("media/1", "v1")]);
    let declared = set.declared_versions();
    assert_eq!(
        declared,
        declared_of(&[("brush/1", "v3"), ("media/1", "v1"), ("raster/2", "v9")])
    );
    assert!(set.verify_declared(&declared).is_ok());
    assert!(ObjectSceneResourceSet::empty()
        .declared_versions()
        .is_empty());
}

#[test]
fn verify_declared_literal_matrix() {
    for (name, held, declared, expect) in VERIFY_CASES {
        let got = observed(set_of(held).verify_declared(&declared_of(declared)));
        assert_eq!(got, expected(expect), "{name}");
    }
}

#[test]
fn same_as_requires_identical_retention_allocations() {
    let (a, b) = (Arc::new(Payload(1)), Arc::new(Payload(1)));
    let build = |token: &Arc<Payload>, version: &str| {
        ObjectSceneResourceSet::new(vec![keep("r", version, token)]).unwrap()
    };
    assert!(build(&a, "1").same_as(&build(&a, "1")));
    assert!(
        !build(&a, "1").same_as(&build(&b, "1")),
        "equal payloads, different tokens"
    );
    assert!(!build(&a, "1").same_as(&build(&a, "2")));
    assert!(!build(&a, "1").same_as(&ObjectSceneResourceSet::empty()));
    let empty = ObjectSceneResourceSet::empty();
    let held = build(&a, "1");
    assert!(same_scene_resources(None, None));
    assert!(same_scene_resources(None, Some(&empty)) && same_scene_resources(Some(&empty), None));
    assert!(!same_scene_resources(None, Some(&held)) && !same_scene_resources(Some(&held), None));
    assert!(same_scene_resources(Some(&held), Some(&held)));
}

#[test]
fn retention_lives_exactly_as_long_as_the_set_and_its_arc_clones() {
    let payload = Arc::new(Payload(7));
    let weak: Weak<Payload> = Arc::downgrade(&payload);
    let set = Arc::new(ObjectSceneResourceSet::new(vec![keep("r", "1", &payload)]).unwrap());
    assert_eq!(weak.strong_count(), 2);
    drop(payload);
    assert_eq!(weak.strong_count(), 1, "only the set retains the token");
    let shared = Arc::clone(&set);
    drop(set);
    assert_eq!(
        weak.strong_count(),
        1,
        "a clone keeps the same single token"
    );
    drop(shared);
    assert_eq!(weak.strong_count(), 0);
    assert!(weak.upgrade().is_none());
}

#[test]
fn retention_payload_is_returned_unmodified_through_any() {
    let set = set_of(&[("r", "1")]);
    let retention = set.get("r").unwrap().retention();
    assert_eq!(retention.downcast_ref::<Payload>().map(|p| p.0), Some(0));
    assert!(retention.downcast_ref::<String>().is_none());
}

#[test]
fn set_and_entry_are_send_and_sync_and_kind_names_are_stable() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ObjectSceneResourceSet>();
    assert_send_sync::<SceneResourceEntry>();
    let names: Vec<&str> = [
        SceneResourceKind::Raster,
        SceneResourceKind::BitmapBrush,
        SceneResourceKind::Media,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();
    assert_eq!(names, ["raster", "bitmap-brush", "media"]);
}
