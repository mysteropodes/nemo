//! T12 consumer join on the real project codec, history owner, frame packet,
//! scene, paint plan and witness: two linked sources with independently known
//! pixels, the exact source frame of every exported frame, typed missing links
//! before any session, source replacement, and exactly-once release.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectSceneResourceSet, PaintEntry,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

// ---- a decoder with independently known pixels -------------------------------

#[derive(Default)]
pub(crate) struct Fake {
    /// path -> (width, height, frame count)
    sources: BTreeMap<String, (u32, u32, u32)>,
    open: BTreeMap<u64, String>,
    next: u64,
    pub(crate) opened: Vec<String>,
    pub(crate) closed: Vec<u64>,
    pub(crate) decoded: Vec<(String, u32)>,
    /// The source frame whose decode fails.
    fail_at: Option<u32>,
}

/// Every pixel of `(path, frame)`: `[path tag, frame low, frame high, 255]`.
pub(crate) fn known(path: &str, frame: u32, len: usize) -> Vec<u8> {
    let tag = path
        .bytes()
        .fold(7u8, |acc, b| acc.wrapping_mul(31).wrapping_add(b));
    let [low, high, ..] = frame.to_le_bytes();
    [tag, low, high, 255]
        .into_iter()
        .cycle()
        .take(len)
        .collect()
}

impl Fake {
    pub(crate) fn with(sources: &[(&str, u32, u32, u32)]) -> Self {
        Self {
            sources: sources
                .iter()
                .map(|(path, w, h, n)| (path.to_string(), (*w, *h, *n)))
                .collect(),
            ..Self::default()
        }
    }
}

impl NativeVideoDecoder for Fake {
    fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String> {
        let (width, height, frame_count) = *self.sources.get(path).ok_or("no such file")?;
        self.next += 1;
        self.open.insert(self.next, path.to_owned());
        self.opened.push(path.to_owned());
        Ok(NativeVideoInfo {
            session: self.next,
            width,
            height,
            frame_count,
        })
    }
    fn decode(&mut self, session: u64, source_frame: u32) -> Result<Vec<u8>, String> {
        let path = self.open.get(&session).ok_or("closed session")?.clone();
        let (width, height, _) = self.sources[&path];
        self.decoded.push((path.clone(), source_frame));
        if self.fail_at == Some(source_frame) {
            return Err(format!("decoder refused frame {source_frame}"));
        }
        Ok(known(&path, source_frame, (width * height * 4) as usize))
    }
    fn close(&mut self, session: u64) -> Result<(), String> {
        self.open.remove(&session).ok_or("double close")?;
        self.closed.push(session);
        Ok(())
    }
}

// ---- the project: a drawn layer under a video layer ---------------------------

pub(crate) fn wire(asset: &str, offset: i64, video_visible: bool) -> Value {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let record = json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"D","strokeId":"s"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
        {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}});
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":120,
            "layers":[{"layerUid":"D","visible":true,"timing":timing},
                      {"layerUid":"V","visible":video_visible,"timing":timing}],
            "objects":[record],
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[
                    {"layerUid":"D","naturalPosition":0,"order":{}},{"layerUid":"V","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":[{"layerUid":"D","strokeId":"s","naturalPosition":0,"order":{}}]}]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/one.mov"}},
            {"id":"v2","name":"take-2","kind":"video","source":{"kind":"linked","locator":"clips/two.mov"}}],
        "assetInventory":{"assetIds":["v1","v2"],"placements":[
            {"id":"p1","assetId":asset,"target":{"kind":"drawing","contextId":"scene-root","layerUid":"V"},
             "offsetFrames":offset}]}})
}

fn owner(asset: &str, offset: i64, visible: bool) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "media-frames",
        wire(asset, offset, visible).to_string().as_bytes(),
    )
    .unwrap()
}

pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

/// The scene producer's witness, exactly as the desktop export builds it.
pub(crate) fn base(snapshot: &ObjectSnapshot, frame: u32) -> ObjectCompositionWitness {
    let selector = selector(snapshot, frame);
    let packet = prepare_object_exposure_frame(snapshot, &selector).unwrap();
    let scene = prepare_object_render_scene(&packet, &selector).unwrap();
    let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
    prepare_object_composition(&plan, &selector).unwrap()
}

pub(crate) const BOTH: &[(&str, u32, u32, u32)] =
    &[("clips/one.mov", 160, 120, 40), ("clips/two.mov", 8, 6, 12)];

/// Export `frames` and return, per frame, the drawn video bytes (or the error).
pub(crate) fn export(
    snapshot: &ObjectSnapshot,
    decoder: &mut Fake,
    frames: &[u32],
) -> (Vec<Vec<u8>>, NativeMediaRelease) {
    let mut port = NativeVideoPort::new(decoder);
    let mut media = begin_native_media_frames(
        &mut port,
        snapshot,
        &selector(snapshot, frames[0]),
        frames,
        true,
        &|_| true,
        0,
    )
    .unwrap();
    let drawn = frames
        .iter()
        .map(|&frame| {
            let witness = media.witness(frame, base(snapshot, frame)).unwrap();
            let (image, _, _) = witness.video_image(0).expect("decoded video slot");
            image.data.data().to_vec()
        })
        .collect();
    (drawn, media.finish())
}

#[test]
fn every_exported_frame_draws_the_exact_offset_source_frame() {
    let history = owner("v1", 5, true);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    // Timeline 5 -> source 0, 6 -> 1, 30 -> 25; before the offset clamps to 0,
    // past the last source frame (39) clamps to 39.
    let frames = [5, 6, 30, 2, 90];
    let (drawn, release) = export(&snapshot, &mut decoder, &frames);
    let expected = [0, 1, 25, 0, 39];
    for (bytes, source) in drawn.iter().zip(expected) {
        assert_eq!(*bytes, known("clips/one.mov", source, 160 * 120 * 4));
    }
    assert_eq!(
        decoder.decoded,
        expected.map(|f| ("clips/one.mov".to_string(), f))
    );
    assert!(release.complete);
    assert_eq!((release.frames_acquired, release.frames_released), (5, 5));
    assert_eq!((release.sessions_opened, release.sessions_closed), (1, 1));
    assert_eq!(decoder.closed.len(), decoder.opened.len());
}

#[test]
fn replacing_the_source_changes_the_drawn_pixels_and_their_size() {
    let mut first = Fake::with(BOTH);
    let one = owner("v1", 0, true);
    let (drawn_one, _) = export(&one.acquire_snapshot(0).unwrap(), &mut first, &[3]);
    let mut second = Fake::with(BOTH);
    let two = owner("v2", 0, true);
    let (drawn_two, release) = export(&two.acquire_snapshot(0).unwrap(), &mut second, &[3]);
    assert_eq!(drawn_one[0], known("clips/one.mov", 3, 160 * 120 * 4));
    assert_eq!(drawn_two[0], known("clips/two.mov", 3, 8 * 6 * 4));
    assert_ne!(drawn_one[0][..4], drawn_two[0][..4]);
    assert_eq!(second.opened, ["clips/two.mov"]);
    assert!(release.complete);
}

#[test]
fn the_video_is_contain_fit_on_the_canvas_and_paints_at_its_layer_slot() {
    let history = owner("v1", 0, true);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = begin_native_media_frames(
        &mut port,
        &snapshot,
        &selector(&snapshot, 4),
        &[4],
        true,
        &|_| true,
        0,
    )
    .unwrap();
    let witness = media.witness(4, base(&snapshot, 4)).unwrap();
    // 160x120 on 320x180: scale min(2, 1.5) = 1.5 -> 240x180 at (40, 0).
    let (image, placement, opacity) = witness.video_image(0).unwrap();
    assert_eq!((image.width, image.height), (160, 120));
    let [a, b, c, d, e, f] = placement.as_coeffs();
    assert_eq!([a, b, c, d, e, f], [1.5, 0.0, 0.0, 1.5, 40.0, 0.0]);
    assert_eq!(opacity, 1.0);
    // D (natural position 0) paints first, the video layer V above it.
    let order: Vec<&str> = ordered_entries(witness.plan())
        .map(|entry| match entry {
            PaintEntry::Contour(_) => "contour",
            PaintEntry::Video(_) => "video",
            _ => "other",
        })
        .collect();
    assert_eq!(order, ["contour", "video"]);
    // The key the scheduler leases declares the decoded frame.
    let declared =
        crate::export_job::native_media_export::native_media_export_job::declared_resources(
            &witness,
        );
    assert_eq!(
        declared,
        [("media:v1:4".to_string(), snapshot.snapshot_id().to_string())]
    );
    drop(witness);
    assert!(media.finish().complete);
}

#[test]
fn a_missing_link_refuses_typed_before_any_session_or_decode() {
    let history = owner("v1", 0, true);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let refused = begin_native_media_frames(
        &mut port,
        &snapshot,
        &selector(&snapshot, 0),
        &[0, 1, 2],
        true,
        &|path| path != "clips/one.mov",
        1_000,
    )
    .err();
    assert_eq!(
        refused,
        Some(NativeMediaRefusal::MissingLink {
            frame: 0,
            asset_id: "v1".into(),
            placement_id: "p1".into(),
            reason: "missing",
        })
    );
    assert!(decoder.opened.is_empty() && decoder.decoded.is_empty());
}

#[test]
fn a_frame_without_its_decoded_video_never_draws_blank() {
    let history = owner("v1", 0, true);
    let snapshot = history.acquire_snapshot(0).unwrap();
    // The scene producer's own witness has the slot but no frame: the compositor
    // entry has nothing to draw and the export must refuse (see compositor).
    let witness = base(&snapshot, 2);
    assert_eq!(witness.plan().video_slots().len(), 1);
    assert!(witness.video_image(0).is_none());
    // Attaching a set that lacks the slot's frame is a typed refusal.
    let empty = std::sync::Arc::new(ObjectSceneResourceSet::empty());
    assert!(witness
        .with_media_frames(
            &empty,
            &[VideoFrameDraw {
                placement_id: "p1".into(),
                frame: "media:v1:2".into(),
                blend: None
            }]
        )
        .is_err());
}

#[test]
fn a_hidden_video_layer_opens_and_decodes_nothing() {
    let history = owner("v1", 0, false);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = begin_native_media_frames(
        &mut port,
        &snapshot,
        &selector(&snapshot, 1),
        &[1],
        true,
        &|_| false,
        0,
    )
    .unwrap();
    let witness = media.witness(1, base(&snapshot, 1)).unwrap();
    assert!(witness.plan().video_slots().is_empty());
    let release = media.finish();
    drop(media);
    assert!(release.complete);
    assert_eq!(release.sessions_opened, 0);
    assert!(decoder.opened.is_empty());
}

#[test]
fn a_decode_failure_mid_range_is_typed_and_still_closes_the_session_once() {
    let history = owner("v1", 0, true);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    decoder.fail_at = Some(2);
    {
        let mut port = NativeVideoPort::new(&mut decoder);
        let mut media = begin_native_media_frames(
            &mut port,
            &snapshot,
            &selector(&snapshot, 1),
            &[1, 2],
            true,
            &|_| true,
            0,
        )
        .unwrap();
        let ok = media.witness(1, base(&snapshot, 1)).unwrap();
        let failed = media.witness(2, base(&snapshot, 2));
        assert_eq!(
            failed.err(),
            Some("decoding media:v1:2 failed: decoder refused frame 2".to_string())
        );
        drop(ok);
        let release = media.finish();
        assert!(release.complete);
        assert_eq!((release.frames_acquired, release.frames_released), (1, 1));
        assert_eq!((release.sessions_opened, release.sessions_closed), (1, 1));
    }
    assert_eq!(decoder.closed.len(), 1);
}
