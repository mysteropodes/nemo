//! Controls for the drop router: each deliberate mistake, run through the same
//! oracle comparison as the shipped kernel, must be rejected by at least as many
//! rows as the equivalent source edit changes in the JS baseline (the counts are
//! the `# control` lines of `oracle.txt`, measured by `tools/gen.mjs`).

use super::rows::{
    control_counts, file_of, oracle_rows, run, split_arrow, Impls, Report, PRODUCTION,
};
use super::*;

/// One mutated classifier/planner configuration.
#[derive(Clone, Copy)]
pub(crate) struct Cfg {
    pub(crate) svg: fn(&DropFile) -> bool,
    pub(crate) image_keeps_svg: bool,
    pub(crate) other_keeps_svg: bool,
    pub(crate) order: [DropGroup; 5],
}

pub(crate) const BASE: Cfg = Cfg {
    svg: is_svg_file,
    image_keeps_svg: false,
    other_keeps_svg: false,
    order: DropGroup::EXECUTION_ORDER,
};

fn classify(file: &DropFile, cfg: Cfg) -> Memberships {
    let svg = (cfg.svg)(file);
    let audio = file.mime.starts_with("audio/");
    let image_prefix = file.mime.starts_with("image/");
    let video = file.mime.starts_with("video/");
    Memberships {
        audio,
        svg,
        image: image_prefix && (cfg.image_keeps_svg || !svg),
        video,
        other: !audio && !image_prefix && !video && (cfg.other_keeps_svg || !svg),
    }
}

pub(crate) fn plan_cfg(files: &[DropFile], cfg: Cfg) -> Option<ImportBatchPlan> {
    if files.is_empty() {
        return None;
    }
    let all: Vec<Memberships> = files.iter().map(|file| classify(file, cfg)).collect();
    let mut groups = Vec::new();
    for group in cfg.order {
        let file_indices: Vec<usize> = (0..files.len()).filter(|&i| all[i].has(group)).collect();
        if !file_indices.is_empty() {
            groups.push(GroupPlan {
                group,
                file_indices,
            });
        }
    }
    Some(ImportBatchPlan {
        groups,
        file_count: files.len(),
    })
}

fn toast_order(plan: &ImportBatchPlan, order: [DropGroup; 5]) -> String {
    let parts: Vec<String> = order
        .iter()
        .filter(|g| !plan.indices(**g).is_empty())
        .map(|g| g.toast_part(plan.indices(*g).len()))
        .collect();
    format!("{}{}", TOAST_PREFIX, parts.join(" + "))
}

const SVG_FIRST_TOAST: [DropGroup; 5] = [
    DropGroup::Audio,
    DropGroup::Svg,
    DropGroup::Image,
    DropGroup::Video,
    DropGroup::Other,
];
const IMAGE_FIRST_EXEC: [DropGroup; 5] = [
    DropGroup::Audio,
    DropGroup::Image,
    DropGroup::Svg,
    DropGroup::Video,
    DropGroup::Other,
];

/// Run `impls`; every `S/G/L/P/R` count the baseline edit changes must be reached.
pub(crate) fn rejected(name: &str, impls: Impls) -> Report {
    let report = run(&impls);
    for (kind, least) in control_counts(name) {
        if "DSGLPR".contains(kind) {
            let got = report.count(kind);
            assert!(
                got >= least,
                "{} {}: {} rows differ, wanted at least {}",
                name,
                kind,
                got,
                least
            );
        }
    }
    report
}

fn names_row(report: &Report, prefix: &str) -> bool {
    report.bad.iter().any(|row| row.starts_with(prefix))
}

#[test]
fn image_pile_that_keeps_svg_is_rejected() {
    let impls = Impls {
        plan: |files| {
            plan_cfg(
                files,
                Cfg {
                    image_keeps_svg: true,
                    ..BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("svg-after-image-pile", impls);
    assert!(
        names_row(&report, "G image/svg+xml|a.svg =>"),
        "{:?}",
        report.bad
    );
}

#[test]
fn other_group_that_keeps_svg_is_rejected() {
    let impls = Impls {
        plan: |files| {
            plan_cfg(
                files,
                Cfg {
                    other_keeps_svg: true,
                    ..BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("other-keeps-svg", impls);
    assert!(
        names_row(&report, "G text/xml|b.svg =>"),
        "{:?}",
        report.bad
    );
}

#[test]
fn toast_in_execution_order_is_rejected() {
    let impls = Impls {
        toast: |plan| toast_order(plan, SVG_FIRST_TOAST),
        ..PRODUCTION
    };
    let report = rejected("toast-order-swapped", impls);
    assert_eq!(
        report.count('S') + report.count('L') + report.count('P') + report.count('R'),
        0
    );
}

#[test]
fn execution_in_toast_order_is_rejected() {
    let impls = Impls {
        plan: |files| {
            plan_cfg(
                files,
                Cfg {
                    order: IMAGE_FIRST_EXEC,
                    ..BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("exec-order-swapped", impls);
    assert_eq!(
        report.count('S') + report.count('L') + report.count('P') + report.count('R'),
        0
    );
}

#[test]
fn oracle_covers_the_named_drop_classes() {
    let singles: Vec<String> = oracle_rows('G')
        .iter()
        .filter(|f| f.len() == 4)
        .map(|f| f[0].to_string())
        .collect();
    for want in [
        "audio/mpeg|x.svg",
        "video/mp4|y.svg",
        "text/xml|b.svg",
        "image/png|A.SVG",
        "IMAGE/PNG|a.png",
    ] {
        assert!(
            singles.iter().any(|s| s == want),
            "no single-file G row for {}",
            want
        );
    }
    assert!(oracle_rows('G')
        .iter()
        .any(|f| f.len() == 3 && f[0] == "=>"));
}

/// Whether the router blocked `group` for a reason that is NOT the missing reference owner.
fn expected_blocked(plan: &ImportBatchPlan, ports: &DropPorts) -> Vec<DropGroup> {
    plan.groups
        .iter()
        .map(|g| g.group)
        .filter(|g| match g {
            DropGroup::Audio => ports.audio == ProviderState::Unavailable,
            DropGroup::Svg => ports.svg == ProviderState::Unavailable,
            DropGroup::Image => ports.image == ProviderState::Unavailable,
            DropGroup::Video => ports.video == ProviderState::Unavailable,
            DropGroup::Other => true,
        })
        .collect()
}

#[test]
fn every_port_combination_is_all_or_nothing() {
    let state = |bit: bool| {
        if bit {
            ProviderState::Available
        } else {
            ProviderState::Unavailable
        }
    };
    let mut combos = 0;
    for row in oracle_rows('G') {
        let (inputs, _) = split_arrow(&row);
        let files: Vec<DropFile> = inputs.iter().map(|t| file_of(t)).collect();
        for mask in 0..32u32 {
            let ports = DropPorts {
                audio: state(mask & 1 != 0),
                svg: state(mask & 2 != 0),
                image: state(mask & 4 != 0),
                video: state(mask & 8 != 0),
                reference: if mask & 16 != 0 {
                    ReferenceOwner::NativePngOnly
                } else {
                    ReferenceOwner::Absent
                },
            };
            combos += 1;
            match (plan_groups(&files), route_drop(&files, &ports)) {
                (None, DropRoute::Empty) => assert!(files.is_empty()),
                (Some(plan), DropRoute::Ready(ready)) => {
                    assert_eq!(plan, ready);
                    assert!(expected_blocked(&plan, &ports).is_empty());
                }
                (Some(plan), DropRoute::Unavailable(u)) => {
                    let got: Vec<DropGroup> = u.blocked.iter().map(|b| b.group).collect();
                    assert_eq!(got, expected_blocked(&plan, &ports));
                    assert_eq!(u.file_count, files.len());
                    // a blocked group lists exactly the indices the plan holds for it
                    for b in &u.blocked {
                        assert_eq!(b.file_indices, plan.indices(b.group));
                    }
                }
                (plan, route) => panic!("plan {:?} and route {:?} disagree", plan, route),
            }
        }
    }
    assert_eq!(combos, 617 * 32);
}

#[test]
fn drag_gate_that_accepts_everything_is_rejected() {
    let impls = Impls {
        drag: |_| true,
        ..PRODUCTION
    };
    let report = rejected("drag-always-true", impls);
    assert_eq!(report.count('D'), 6);
    assert_eq!(report.total(), 6);
}
