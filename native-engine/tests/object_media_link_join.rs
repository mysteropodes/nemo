//! Join tests for the link-state kernel and the native asset catalog (G32,
//! L09-C). Records and placements are built with the catalog's real
//! constructors; the link table is driven through its real transitions.

use super::{
    export_gate, link_ref_of, LinkGateError, LinkReadiness, LocatorNamespace, MissingLink,
    MissingLinkReason,
};
use crate::object_document::{
    AssetId, AssetKind, AssetPlacement, AssetRecord, AssetSource, AssetTarget,
};
use std::sync::Arc;

use super::super::object_media_link_state::{ResolveOutcome, ResolveStart, ResolveTable};

fn id(text: &str) -> AssetId {
    AssetId::new(text.to_owned()).unwrap()
}

fn linked(asset: &str, kind: AssetKind, locator: &str) -> AssetRecord {
    let source = AssetSource::linked(locator.to_owned(), 4096).unwrap();
    AssetRecord::new(id(asset), "clip".to_owned(), kind, source).unwrap()
}

fn embedded(asset: &str, kind: AssetKind) -> AssetRecord {
    let bytes = Arc::new(vec![1u8, 2, 3]);
    let capacity = bytes.capacity();
    let source = AssetSource::admitted_embedded("video/mp4".to_owned(), bytes, capacity).unwrap();
    AssetRecord::new(id(asset), "clip".to_owned(), kind, source).unwrap()
}

fn placement(asset: &str, placement: &str) -> AssetPlacement {
    let target = match asset {
        "snd" => AssetTarget::AudioTrack {
            track_uid: "t1".to_owned(),
        },
        _ => AssetTarget::Drawing {
            context_id: "ctx".to_owned(),
            layer_uid: format!("layer-{placement}"),
            stroke_id: None,
        },
    };
    AssetPlacement::new(placement.to_owned(), id(asset), target, 0).unwrap()
}

const DESKTOP: LocatorNamespace = LocatorNamespace::DesktopPath;
const WEB: LocatorNamespace = LocatorNamespace::WebHandle;

fn settle(
    t: &mut ResolveTable,
    rec: &AssetRecord,
    ns: LocatorNamespace,
    o: ResolveOutcome,
    at: i64,
) {
    let r = link_ref_of(rec, ns).unwrap();
    assert_eq!(t.begin(&r, at), ResolveStart::Started);
    t.finish(&mut [], &r, o, at);
}

fn missing(asset: &str, placement: &str, reason: MissingLinkReason) -> LinkGateError {
    LinkGateError::MissingLink(MissingLink {
        asset_id: id(asset),
        placement_id: placement.to_owned(),
        reason,
    })
}

#[test]
fn the_locator_is_borrowed_unchanged_under_either_namespace() {
    for raw in [
        "C:\\Users\\x y\\clip \u{e9}.mov",
        " lead",
        "trail ",
        "../up/./x//y/",
        "lm1abc",
        "%41",
    ] {
        let rec = linked("v1", AssetKind::Video, raw);
        let d = link_ref_of(&rec, DESKTOP).unwrap();
        let w = link_ref_of(&rec, WEB).unwrap();
        assert_eq!(d.linked_path.unwrap().as_bytes(), raw.as_bytes());
        assert_eq!((d.linked_handle_id, w.linked_path), (None, None));
        assert_eq!(w.linked_handle_id.unwrap().as_bytes(), raw.as_bytes());
        assert_eq!(
            rec.source().locator(),
            Some(raw),
            "the catalog value is untouched"
        );
    }
}

#[test]
fn embedded_bytes_have_no_link_to_resolve() {
    let rec = embedded("v1", AssetKind::Video);
    assert!(link_ref_of(&rec, DESKTOP).is_none());
    let got = export_gate(
        &ResolveTable::new(),
        &rec,
        &placement("v1", "p1"),
        DESKTOP,
        0,
    );
    assert_eq!(got, Ok(LinkReadiness::Embedded));
}

#[test]
fn an_unresolved_link_is_a_typed_missing_link_never_a_success() {
    let (rec, p) = (
        linked("v1", AssetKind::Video, "/m/v.mov"),
        placement("v1", "p1"),
    );
    let mut t = ResolveTable::new();
    let want = |reason| Err(missing("v1", "p1", reason));
    assert_eq!(
        export_gate(&t, &rec, &p, DESKTOP, 0),
        want(MissingLinkReason::NotResolved)
    );
    settle(&mut t, &rec, DESKTOP, ResolveOutcome::HandleMissing, 100);
    assert_eq!(
        export_gate(&t, &rec, &p, DESKTOP, 4099),
        want(MissingLinkReason::Missing)
    );
    assert_eq!(
        export_gate(&t, &rec, &p, DESKTOP, 4100),
        want(MissingLinkReason::Missing)
    );
    let web = linked("v2", AssetKind::Image, "h-1");
    settle(&mut t, &web, WEB, ResolveOutcome::PermissionNeeded, 100);
    let p2 = placement("v2", "p2");
    for now in [100, 4099, 4100, 90_000] {
        assert_eq!(
            export_gate(&t, &web, &p2, WEB, now),
            Err(missing("v2", "p2", MissingLinkReason::PermissionNeeded))
        );
    }
}

#[test]
fn a_resolved_link_passes_and_the_namespace_is_part_of_the_identity() {
    let (rec, p) = (
        linked("v1", AssetKind::Image, "same-text"),
        placement("v1", "p1"),
    );
    let mut t = ResolveTable::new();
    settle(&mut t, &rec, DESKTOP, ResolveOutcome::Resolved, 5);
    assert_eq!(
        export_gate(&t, &rec, &p, DESKTOP, 5),
        Ok(LinkReadiness::Resolved)
    );
    assert_eq!(
        export_gate(&t, &rec, &p, WEB, 5),
        Err(missing("v1", "p1", MissingLinkReason::NotResolved)),
        "'p:same-text' and 'w:same-text' are different references"
    );
}

#[test]
fn every_non_resolved_state_refuses_at_every_instant() {
    let (rec, p) = (
        linked("v1", AssetKind::Video, "/m/v.mov"),
        placement("v1", "p1"),
    );
    for outcome in [ResolveOutcome::HandleMissing, ResolveOutcome::ReadFailed] {
        let mut t = ResolveTable::new();
        settle(&mut t, &rec, DESKTOP, outcome, 1000);
        for now in [-5, 0, 999, 1000, 4999, 5000, 5001, i64::MAX] {
            let got = export_gate(&t, &rec, &p, DESKTOP, now);
            assert!(
                matches!(got, Err(LinkGateError::MissingLink(_))),
                "{outcome:?} at {now}"
            );
        }
    }
}

#[test]
fn mismatched_placement_and_unsupported_kinds_are_typed_refusals() {
    let t = ResolveTable::new();
    let rec = linked("v1", AssetKind::Video, "/m/v.mov");
    assert_eq!(
        export_gate(&t, &rec, &placement("other", "p9"), DESKTOP, 0),
        Err(LinkGateError::PlacementAssetMismatch)
    );
    let audio = linked("snd", AssetKind::Audio, "/m/a.wav");
    assert_eq!(
        export_gate(&t, &audio, &placement("snd", "p3"), DESKTOP, 0),
        Err(LinkGateError::UnsupportedKind(AssetKind::Audio))
    );
    let font = linked("f1", AssetKind::Font, "/m/f.ttf");
    let target = AssetTarget::Text {
        context_id: "c".to_owned(),
        layer_uid: "l".to_owned(),
        stroke_id: None,
    };
    let fp = AssetPlacement::new("p4".to_owned(), id("f1"), target, 0).unwrap();
    assert_eq!(
        export_gate(&t, &font, &fp, DESKTOP, 0),
        Err(LinkGateError::UnsupportedKind(AssetKind::Font))
    );
    let sound = embedded("snd", AssetKind::Audio);
    assert_eq!(
        export_gate(&t, &sound, &placement("snd", "p5"), DESKTOP, 0),
        Ok(LinkReadiness::Embedded),
        "an embedded sound has no link to gate"
    );
}
