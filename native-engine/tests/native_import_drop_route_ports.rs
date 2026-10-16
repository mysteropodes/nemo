//! Tests of the provider dispositions of a drop (L10-C, G37/I46): typed
//! unavailability, all-or-nothing routing, the `other` reference group and the
//! baseline characterization rows (`H`, `O`) the dispositions are measured against.

use super::rows::{
    calls_of, escape, f, file_of, groups_of, oracle_has, oracle_rows, split_arrow, tokens, unescape,
};
use super::*;

fn all_available() -> DropPorts {
    DropPorts {
        audio: ProviderState::Available,
        svg: ProviderState::Available,
        image: ProviderState::Available,
        video: ProviderState::Available,
        reference: ReferenceOwner::NativePngOnly,
    }
}

fn blocked(group: DropGroup, provider: Provider, reason: BlockReason, idx: &[usize]) -> Blocked {
    Blocked {
        group,
        provider,
        reason,
        file_indices: idx.to_vec(),
    }
}

fn unavailable(file_count: usize, blocked: Vec<Blocked>) -> DropRoute {
    DropRoute::Unavailable(UnavailableDrop {
        file_count,
        blocked,
    })
}

#[test]
fn provider_dispositions_are_typed_data() {
    let files = [
        f("audio/mpeg", "a.mp3"),
        f("image/svg+xml", "a.svg"),
        f("image/png", "a.png"),
        f("video/mp4", "v.mp4"),
    ];
    match route_drop(&files, &all_available()) {
        DropRoute::Ready(plan) => assert_eq!(plan.groups.len(), 4),
        other => panic!("expected Ready, got {:?}", other),
    }
    let pu = BlockReason::ProviderUnavailable;
    assert_eq!(
        route_drop(&files, &DropPorts::NONE_AVAILABLE),
        unavailable(
            4,
            vec![
                blocked(DropGroup::Audio, Provider::AudioTracks, pu, &[0]),
                blocked(DropGroup::Svg, Provider::VectorImport, pu, &[1]),
                blocked(DropGroup::Image, Provider::RasterAssets, pu, &[2]),
                blocked(DropGroup::Video, Provider::VideoMedia, pu, &[3]),
            ]
        )
    );
    assert_eq!(Provider::AudioTracks.task_ids(), ["G30"]);
    assert_eq!(Provider::VectorImport.task_ids(), ["G35"]);
    assert_eq!(Provider::RasterAssets.task_ids(), ["G02", "G27"]);
    assert_eq!(Provider::VideoMedia.task_ids(), ["G28", "G42"]);
    assert!(Provider::NativeReference.task_ids().is_empty());
}

#[test]
fn one_unavailable_group_blocks_the_whole_drop() {
    let files = [
        f("image/png", "a.png"),
        f("video/mp4", "v.mp4"),
        f("audio/mpeg", "a.mp3"),
    ];
    let mut ports = all_available();
    ports.video = ProviderState::Unavailable;
    let pu = BlockReason::ProviderUnavailable;
    assert_eq!(
        route_drop(&files, &ports),
        unavailable(
            3,
            vec![blocked(DropGroup::Video, Provider::VideoMedia, pu, &[1])]
        )
    );
    // an overlapping file is blocked by the group that is missing; nothing else is applied
    let overlap = [f("audio/mpeg", "x.svg")];
    ports = all_available();
    ports.svg = ProviderState::Unavailable;
    assert_eq!(
        route_drop(&overlap, &ports),
        unavailable(
            1,
            vec![blocked(DropGroup::Svg, Provider::VectorImport, pu, &[0])]
        )
    );
}

#[test]
fn other_group_is_never_success() {
    let files = [f("application/pdf", "a.pdf")];
    let want = |reason| {
        unavailable(
            1,
            vec![blocked(
                DropGroup::Other,
                Provider::NativeReference,
                reason,
                &[0],
            )],
        )
    };
    assert_eq!(
        route_drop(&files, &all_available()),
        want(BlockReason::UseImportReference)
    );
    let mut ports = all_available();
    ports.reference = ReferenceOwner::Absent;
    assert_eq!(
        route_drop(&files, &ports),
        want(BlockReason::ReferenceOwnerAbsent)
    );
    let toast = "Use Import Reference to choose a PNG file for this native project.";
    assert_eq!(
        BlockReason::UseImportReference.baseline_toast(),
        Some(toast)
    );
    assert_eq!(BlockReason::ReferenceOwnerAbsent.baseline_toast(), None);
    assert_eq!(BlockReason::ProviderUnavailable.baseline_toast(), None);
    assert_eq!(
        LEGACY_UNRECOGNIZED_TOAST_KEY,
        "toastFormatNotRecognizedVideoImages"
    );
    // mixed drop: the image would be fine but the reference file blocks everything
    let mixed = [f("image/png", "a.png"), f("application/pdf", "a.pdf")];
    assert!(matches!(
        route_drop(&mixed, &all_available()),
        DropRoute::Unavailable(_)
    ));
}

#[test]
fn baseline_other_rows_agree_with_the_kernel() {
    let mut seen = 0;
    for row in oracle_rows('O') {
        let (inputs, events) = split_arrow(&row);
        let native = inputs[0] == "n";
        let files: Vec<DropFile> = inputs[1..].iter().map(|t| file_of(t)).collect();
        let plan = plan_groups(&files).expect("plan");
        if events[0] == "read" {
            // the legacy importer branches belong to image and video, never to `other`
            assert!(plan.indices(DropGroup::Other).is_empty(), "{:?}", files);
            continue;
        }
        seen += 1;
        let all: Vec<usize> = (0..files.len()).collect();
        assert_eq!(groups_of(&plan), [("other".to_string(), all.clone())]);
        let toast = if native {
            format!("toast:{}", escape(NATIVE_REFERENCE_DROP_TOAST))
        } else {
            format!(
                "toast:{}",
                escape(&format!("KEY:{}", LEGACY_UNRECOGNIZED_TOAST_KEY))
            )
        };
        assert_eq!(events[0], toast);
        if native {
            let want = blocked(
                DropGroup::Other,
                Provider::NativeReference,
                BlockReason::UseImportReference,
                &all,
            );
            assert_eq!(
                route_drop(&files, &all_available()),
                unavailable(files.len(), vec![want])
            );
        }
    }
    assert_eq!(seen, 16);
}

#[test]
fn baseline_skips_absent_importers_silently_and_native_reports_them() {
    let mut checked = 0;
    for row in oracle_rows('H') {
        let (head, tail) = split_arrow(&row);
        let files: Vec<DropFile> = head[1..].iter().map(|t| file_of(t)).collect();
        let (calls, toast) = (calls_of(tail[0]), unescape(tail[1]));
        let plan = plan_groups(&files).expect("plan");
        let imported = |label: &str| calls.iter().any(|c| c.0 == label);
        if head[0] == "audio" && !plan.indices(DropGroup::Audio).is_empty() {
            checked += 1;
            assert!(!imported("audio"), "baseline imported nothing for audio");
            assert!(
                toast.contains("piste(s) audio"),
                "yet the toast announces it: {}",
                toast
            );
            let mut ports = all_available();
            ports.audio = ProviderState::Unavailable;
            match route_drop(&files, &ports) {
                DropRoute::Unavailable(u) => {
                    assert!(u.blocked.iter().any(|b| b.group == DropGroup::Audio
                        && b.reason == BlockReason::ProviderUnavailable))
                }
                other => panic!("expected Unavailable, got {:?}", other),
            }
        }
        if head[0] == "svg" && !plan.indices(DropGroup::Svg).is_empty() {
            checked += 1;
            // without the vector importer the baseline lets svg files fall into the image pile
            assert!(!imported("svg"), "{:?}", calls);
            let fallen: usize = calls
                .iter()
                .filter(|c| c.0 == "image")
                .map(|c| c.1.len())
                .sum();
            assert!(fallen >= 1);
            let mut ports = all_available();
            ports.svg = ProviderState::Unavailable;
            assert!(matches!(
                route_drop(&files, &ports),
                DropRoute::Unavailable(_)
            ));
        }
    }
    assert_eq!(checked, 5);
    let pair = [f("image/svg+xml", "a.svg"), f("image/svg+xml", "a.png")];
    let line = format!(
        "H svg {} => image:0.1 {}",
        tokens(&pair),
        escape("Import : 2 image(s)")
    );
    assert!(oracle_has(&line));
}
