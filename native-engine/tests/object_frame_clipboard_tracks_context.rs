//! The cross-context admission matrix: scene-root to scene-root is the only pair a native document can write today (the
//! component provider, I14/I15, is absent), and every other pair is a stable typed code decided from the document alone.
use super::*;

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;

use build::{Build, Cell};

fn pair(source: &str, target: &str) -> ClipboardContexts {
    ClipboardContexts {
        source: source.to_owned(),
        target: target.to_owned(),
    }
}
fn layers() -> Vec<Vec<Cell>> {
    let cell = |flag: char| Cell {
        flag,
        strokes: Vec::new(),
    };
    vec![vec![cell('K'), cell('.')]]
}
/// Version 1: scene-root only. Version 2 with extras: the plan also declares the context `ctx-x`.
fn plain() -> ObjectDocument {
    build::document(&layers(), Build::V1)
}
fn declared_x() -> ObjectDocument {
    build::document(&layers(), Build::V2Extras)
}
fn admit(document: &ObjectDocument, source: &str, target: &str) -> Result<(), DispatchErrorCode> {
    admit_clipboard_contexts(document, &pair(source, target))
}

#[test]
fn scene_root_to_scene_root_is_the_one_admitted_pair() {
    assert_eq!(
        ClipboardContexts::scene_root(),
        pair("scene-root", "scene-root")
    );
    assert_eq!(admit(&plain(), "scene-root", "scene-root"), Ok(()));
    assert_eq!(admit(&declared_x(), "scene-root", "scene-root"), Ok(()));
}

#[test]
fn a_declared_component_context_is_unavailable_in_every_direction_until_the_provider_exists() {
    let document = declared_x();
    for (source, target) in [
        ("scene-root", "ctx-x"),
        ("ctx-x", "scene-root"),
        ("ctx-x", "ctx-x"),
    ] {
        assert_eq!(
            admit(&document, source, target),
            Err(DispatchErrorCode::Unavailable),
            "{source} -> {target}"
        );
    }
}

#[test]
fn an_unknown_context_is_not_found_and_wins_over_an_unavailable_one() {
    let document = declared_x();
    for (source, target) in [
        ("nope", "scene-root"),
        ("scene-root", "nope"),
        ("ctx-x", "nope"),
        ("nope", "ctx-x"),
    ] {
        assert_eq!(
            admit(&document, source, target),
            Err(DispatchErrorCode::NotFound),
            "{source} -> {target}"
        );
    }
    // the answer depends on the document: version 1 does not declare ctx-x at all
    assert_eq!(
        admit(&plain(), "scene-root", "ctx-x"),
        Err(DispatchErrorCode::NotFound)
    );
}

#[test]
fn an_empty_context_id_is_invalid_and_wins_over_everything() {
    let document = declared_x();
    for (source, target) in [
        ("", "scene-root"),
        ("scene-root", ""),
        ("", "nope"),
        ("ctx-x", ""),
        ("", ""),
    ] {
        assert_eq!(
            admit(&document, source, target),
            Err(DispatchErrorCode::InvalidRequest),
            "{source:?} -> {target:?}"
        );
    }
}
