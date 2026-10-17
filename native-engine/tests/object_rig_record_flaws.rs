//! Builders and the single-flaw rejection table of the rig record (L03-N, I11).
//!
//! Every rejected rig is the admitted twin with exactly one thing changed, so a rejection can only
//! come from that change. The wire tests pin what admission must NOT refuse.

use super::{RigAdmissionError, RigRecord};
use RigAdmissionError as E;

pub(crate) const WEIGHT: &str = r#"{"boneId":"bone1","ptIndex":0,"w":1}"#;

/// `"id":{...}` with `live` vertices in segments and `rest` in restSegments along the x axis.
pub(crate) fn bone(id: &str, live: usize, rest: usize, extra: &str) -> String {
    let points = |n: usize| -> String {
        (0..n)
            .map(|i| format!("{{\"point\":[{},0]}}", i * 10))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "\"{id}\":{{\"segments\":[{}],\"restSegments\":[{}],\"closed\":false{extra}}}",
        points(live),
        points(rest)
    )
}

pub(crate) fn ik_keyed(
    key: &str,
    root: (&str, u32),
    joint: (&str, u32),
    end: (&str, u32),
) -> String {
    let r = |(b, v): (&str, u32)| format!("{{\"boneId\":\"{b}\",\"vi\":{v}}}");
    format!(
        "\"{key}\":{{\"root\":{},\"joint\":{},\"end\":{},\"l1\":10,\"l2\":10,\"flip\":false}}",
        r(root),
        r(joint),
        r(end)
    )
}

pub(crate) fn ik(root: (&str, u32), joint: (&str, u32), end: (&str, u32)) -> String {
    ik_keyed(&format!("{}:{}", end.0, end.1), root, joint, end)
}

/// A bind on `target` (raw members incl. trailing comma) with `rest` points, `weights` vertex lists
/// (the first holds `entry`), and `handles` restHandles slots when given.
pub(crate) fn bind(
    target: &str,
    rest: usize,
    weights: usize,
    handles: Option<usize>,
    entry: &str,
) -> String {
    let points: Vec<String> = (0..rest).map(|i| format!("[{i},0]")).collect();
    let lists: Vec<String> = (0..weights)
        .map(|i| {
            if i == 0 {
                format!("[{entry}]")
            } else {
                "[]".into()
            }
        })
        .collect();
    let slots = handles.map_or(String::new(), |n| {
        format!("\"restHandles\":[{}],", vec!["{}"; n].join(","))
    });
    format!(
        "{{{target}\"rest\":[{}],{slots}\"weights\":[{}],\"rotate\":true}}",
        points.join(","),
        lists.join(",")
    )
}

pub(crate) const S1: &str = r#""strokeId":"s1","#;

pub(crate) fn rig_text(
    bones: &[String],
    iks: &[String],
    next_id: &str,
    binds: &[String],
) -> String {
    format!(
        "{{\"bones\":{{{}}},\"ikChains\":{{{}}},\"nextId\":{next_id},\"binds\":[{}]}}",
        bones.join(","),
        iks.join(","),
        binds.join(",")
    )
}

pub(crate) fn twin_bones() -> Vec<String> {
    vec![
        bone("bone1", 2, 2, ""),
        bone("bone2", 2, 2, ",\"radius\":50"),
    ]
}

pub(crate) fn twin_ik() -> String {
    ik(("bone1", 0), ("bone1", 1), ("bone2", 1))
}

pub(crate) fn twin_bind() -> String {
    bind(S1, 3, 3, None, WEIGHT)
}

pub(crate) fn twin() -> String {
    rig_text(&twin_bones(), &[twin_ik()], "3", &[twin_bind()])
}

pub(crate) fn with_bones(bones: &[String]) -> String {
    rig_text(bones, &[], "9", &[])
}

pub(crate) fn with_ik(entries: &[String]) -> String {
    rig_text(&twin_bones(), entries, "3", &[])
}

pub(crate) fn with_binds(binds: &[String]) -> String {
    rig_text(&twin_bones(), &[], "3", binds)
}

pub(crate) fn verdict(text: &str) -> Result<(), RigAdmissionError> {
    serde_json::from_str::<RigRecord>(text)
        .expect("decodes")
        .validate()
}

#[test]
fn the_admitted_twin_passes_so_each_rejection_is_its_single_flaw() {
    assert_eq!(verdict(&twin()), Ok(()));
    let record: RigRecord = serde_json::from_str(&twin()).unwrap();
    assert_eq!(serde_json::to_string(&record).unwrap(), twin());
    assert_eq!(record.bones.len(), 2);
}

fn flaws() -> Vec<(&'static str, String, RigAdmissionError)> {
    let b1 = || bone("bone1", 2, 2, "");
    let two = |text: String| -> Vec<String> { vec![text, bone("bone2", 2, 2, "")] };
    let bad_target = |target: &str| with_binds(&[bind(target, 3, 3, None, WEIGHT)]);
    vec![
        (
            "empty bone id",
            with_bones(&[bone("", 2, 2, "")]),
            E::EmptyBoneId,
        ),
        (
            "duplicate bone id",
            with_bones(&[b1(), b1()]),
            E::DuplicateBoneId,
        ),
        (
            "live pose has one vertex",
            with_bones(&two(bone("bone1", 1, 2, ""))),
            E::BoneTooShort,
        ),
        (
            "rest pose has one vertex",
            with_bones(&two(bone("bone1", 2, 1, ""))),
            E::BoneTooShort,
        ),
        (
            "live pose is empty",
            with_bones(&two(bone("bone1", 0, 2, ""))),
            E::BoneTooShort,
        ),
        (
            "rest pose is empty",
            with_bones(&two(bone("bone1", 2, 0, ""))),
            E::BoneTooShort,
        ),
        (
            "nextId zero",
            rig_text(&twin_bones(), &[], "0", &[]),
            E::NextIdOutOfRange,
        ),
        (
            "nextId beyond 2^53-1",
            rig_text(&twin_bones(), &[], "9007199254740992", &[]),
            E::NextIdOutOfRange,
        ),
        (
            "nextId equals the largest suffix",
            rig_text(&twin_bones(), &[], "2", &[]),
            E::NextIdNotAboveBoneSuffix,
        ),
        (
            "nextId below a larger suffix",
            rig_text(&[bone("bone7", 2, 2, "")], &[], "6", &[]),
            E::NextIdNotAboveBoneSuffix,
        ),
        (
            "suffix with leading zeros counts as its value",
            rig_text(&[bone("bone007", 2, 2, "")], &[], "7", &[]),
            E::NextIdNotAboveBoneSuffix,
        ),
        (
            "suffix beyond u64 saturates",
            rig_text(
                &[bone("bone99999999999999999999999", 2, 2, "")],
                &[],
                "9007199254740991",
                &[],
            ),
            E::NextIdNotAboveBoneSuffix,
        ),
        (
            "duplicate ik key",
            with_ik(&[twin_ik(), twin_ik()]),
            E::DuplicateIkChain,
        ),
        (
            "ik key is not end.boneId:end.vi",
            with_ik(&[ik_keyed(
                "bone2:0",
                ("bone1", 0),
                ("bone1", 1),
                ("bone2", 1),
            )]),
            E::IkKeyMismatch,
        ),
        (
            "ik key with another bone",
            with_ik(&[ik_keyed(
                "bone1:1",
                ("bone1", 0),
                ("bone1", 1),
                ("bone2", 1),
            )]),
            E::IkKeyMismatch,
        ),
        (
            "ik root bone missing",
            with_ik(&[ik(("bone9", 0), ("bone1", 1), ("bone2", 1))]),
            E::IkBoneMissing,
        ),
        (
            "ik joint bone missing",
            with_ik(&[ik(("bone1", 0), ("bone9", 1), ("bone2", 1))]),
            E::IkBoneMissing,
        ),
        (
            "ik end bone missing",
            with_ik(&[ik(("bone1", 0), ("bone1", 1), ("bone9", 1))]),
            E::IkBoneMissing,
        ),
        (
            "ik root vertex beyond rest",
            with_ik(&[ik(("bone1", 2), ("bone1", 1), ("bone2", 1))]),
            E::IkVertexMissing,
        ),
        (
            "ik joint vertex beyond rest",
            with_ik(&[ik(("bone1", 0), ("bone1", 2), ("bone2", 1))]),
            E::IkVertexMissing,
        ),
        (
            "ik end vertex beyond rest",
            with_ik(&[ik(("bone1", 0), ("bone1", 1), ("bone2", 2))]),
            E::IkVertexMissing,
        ),
        ("bind names no target", bad_target(""), E::BindTargetInvalid),
        (
            "bind names both targets",
            bad_target(r#""strokeId":"s1","meshId":"m","#),
            E::BindTargetInvalid,
        ),
        (
            "bind with an empty stroke id",
            bad_target(r#""strokeId":"","#),
            E::BindTargetInvalid,
        ),
        (
            "bind with an empty mesh id",
            bad_target(r#""meshId":"","#),
            E::BindTargetInvalid,
        ),
        (
            "bind without rest points",
            with_binds(&[bind(S1, 0, 0, None, "")]),
            E::BindRestEmpty,
        ),
        (
            "fewer weight lists than rest points",
            with_binds(&[bind(S1, 3, 2, None, WEIGHT)]),
            E::BindWeightsLength,
        ),
        (
            "more weight lists than rest points",
            with_binds(&[bind(S1, 3, 4, None, WEIGHT)]),
            E::BindWeightsLength,
        ),
        (
            "fewer restHandles than rest points",
            with_binds(&[bind(S1, 3, 3, Some(2), WEIGHT)]),
            E::BindRestHandlesLength,
        ),
        (
            "more restHandles than rest points",
            with_binds(&[bind(S1, 3, 3, Some(4), WEIGHT)]),
            E::BindRestHandlesLength,
        ),
        (
            "mesh bind with restHandles",
            with_binds(&[bind(r#""meshId":"m","#, 3, 3, Some(3), WEIGHT)]),
            E::BindRestHandlesOnMeshBind,
        ),
        (
            "weight entry with neither ptIndex nor offset",
            with_binds(&[bind(S1, 3, 3, None, r#"{"boneId":"bone1","w":1}"#)]),
            E::WeightEntryShape,
        ),
    ]
}

#[test]
fn single_flaw_rigs_are_rejected_with_the_typed_error() {
    let table = flaws();
    assert_eq!(table.len(), 32);
    for (name, text, want) in &table {
        assert_ne!(text, &twin(), "{name}: the flaw must change the twin");
        assert_eq!(verdict(text), Err(*want), "{name}");
    }
}

#[test]
fn every_admission_error_is_reached_and_classified() {
    let reached: Vec<RigAdmissionError> = flaws().into_iter().map(|(_, _, e)| e).collect();
    let all = [
        E::EmptyBoneId,
        E::DuplicateBoneId,
        E::BoneTooShort,
        E::NextIdOutOfRange,
        E::NextIdNotAboveBoneSuffix,
        E::DuplicateIkChain,
        E::IkKeyMismatch,
        E::IkBoneMissing,
        E::IkVertexMissing,
        E::BindTargetInvalid,
        E::BindRestEmpty,
        E::BindWeightsLength,
        E::BindRestHandlesOnMeshBind,
        E::BindRestHandlesLength,
        E::WeightEntryShape,
        E::StrokeFamilyUnsupported,
        E::IdSequenceExhausted,
    ];
    for error in all {
        let by_table = reached.contains(&error);
        let elsewhere = matches!(error, E::StrokeFamilyUnsupported | E::IdSequenceExhausted);
        assert!(
            by_table != elsewhere,
            "{error:?} is reached by the flaw table exactly when it is not document/counter level"
        );
        assert_eq!(
            error.is_duplicate(),
            matches!(error, E::DuplicateBoneId | E::DuplicateIkChain),
            "{error:?}"
        );
        assert_eq!(
            error.is_unsupported(),
            error == E::StrokeFamilyUnsupported,
            "{error:?}"
        );
        assert!(!error.message().is_empty());
    }
    let mut messages: Vec<&str> = all.iter().map(|e| e.message()).collect();
    messages.sort_unstable();
    messages.dedup();
    assert_eq!(messages.len(), all.len(), "messages are distinct");
}
