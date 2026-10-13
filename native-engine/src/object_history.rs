//! Staged object fill/history authority; no transport, retries or replacement port.
use crate::commands::{ObjectFillOwner, PreparedObjectFill};
use crate::object_document::{ObjectDocument, ProjectEnvelope};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use crate::transaction::StageReceipts;

#[path = "object_command_dispatch.rs"]
mod object_command_dispatch;

#[derive(Debug, Clone)]
struct ObjectHistoryEntry {
    before_revision: u64,
    after_revision: u64,
}

#[cfg(test)]
mod project_tests {
    use super::*;
    use crate::object_document::{ProjectMetadata, ProjectReferenceState};
    use serde_json::{json, Value};

    fn cases() -> Value {
        serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap()
    }
    fn drawing(version: u32) -> Value {
        let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":21,
            "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]});
        if version == 2 {
            value["compositionOrder"] = json!({
            "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{"static":[0]}},
                {"layerUid":"0","naturalPosition":1,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"});
        }
        value
    }
    fn project(version: u32, attached: bool) -> ProjectEnvelope {
        let document: ObjectDocument = serde_json::from_value(drawing(version)).unwrap();
        ObjectSnapshot::validate_drawing(&document).unwrap();
        let project =
            ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), document);
        if !attached {
            return project;
        }
        // Already-admitted encoded fixture: these are owner-retention controls,
        // not a claim that this private constructor performs PNG decoding.
        let hex="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
        let bytes = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        project.with_reference(
            ProjectReferenceState::admitted_still(
                "media-1".into(),
                ".plate.png".into(),
                false,
                0.5,
                -7,
                bytes,
                1024,
            )
            .unwrap(),
        )
    }
    fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
        owner.acquire_snapshot(owner.content_revision()).unwrap()
    }
    fn unchanged_project(snapshot: &ObjectSnapshot, original: &ProjectEnvelope, expected: &Value) {
        let project = snapshot.project().unwrap();
        assert_eq!(project.project(), original.project());
        assert_eq!(project.reference_state(), original.reference_state());
        if let (ProjectReferenceState::StillImage(a), ProjectReferenceState::StillImage(b)) =
            (project.reference_state(), original.reference_state())
        {
            assert!(std::sync::Arc::ptr_eq(&a, &b));
            assert_eq!(a.encoded_png().as_ptr(), b.encoded_png().as_ptr());
        }
        assert!(std::ptr::eq(snapshot.document(), project.drawing()));
        assert_eq!(
            serde_json::to_value(snapshot.document()).unwrap(),
            *expected
        );
    }

    #[test]
    fn real_fill_noop_undo_redo_and_pins_preserve_composite_v1_v2_values() {
        for version in [1, 2] {
            for attached in [false, true] {
                let original = project(version, attached);
                let before = drawing(version);
                let mut after = before.clone();
                after["objects"][0] = cases()["command"]["afterRecord"].clone();
                let mut owner =
                    NativeObjectHistory::from_admitted_project("project-history", original.clone())
                        .unwrap();
                let zero = current(&owner);
                assert_eq!(owner.history_depths(), (0, 0));
                let payload = cases()["command"]["request"]["payload"].clone();
                let prepared = owner
                    .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
                    .unwrap();
                let stale = prepared.clone();
                assert_eq!(owner.commit_fill(prepared), Ok(1));
                assert_eq!(owner.history_depths(), (1, 0));
                let one = current(&owner);
                unchanged_project(&one, &original, &after);
                assert_eq!(
                    owner.commit_fill(stale),
                    Err(DispatchErrorCode::StaleRevision)
                );
                assert_eq!(owner.undo(1), Ok(2));
                assert_eq!(owner.history_depths(), (0, 1));
                let two = current(&owner);
                unchanged_project(&two, &original, &before);
                let mut noop = payload.clone();
                noop["fill"] = before["objects"][0]["fill"].clone();
                let prepared = owner
                    .prepare_fill_json(&serde_json::to_vec(&noop).unwrap())
                    .unwrap();
                assert_eq!(owner.commit_fill(prepared), Ok(2));
                assert_eq!(owner.history_depths(), (0, 1));
                assert_eq!(owner.redo(2), Ok(3));
                assert_eq!(owner.history_depths(), (1, 0));
                unchanged_project(&current(&owner), &original, &after);
                assert_eq!(owner.undo(2), Err(DispatchErrorCode::StaleRevision));
                unchanged_project(&zero, &original, &before);
                unchanged_project(&one, &original, &after);
                drop(owner);
                unchanged_project(&zero, &original, &before);
                unchanged_project(&two, &original, &before);
            }
        }
    }

    #[test]
    fn fresh_incarnations_and_failed_origins_do_not_publish_project_revisions() {
        let original = project(1, true);
        let mut a = NativeObjectHistory::from_admitted_project("same", original.clone()).unwrap();
        let b = NativeObjectHistory::from_admitted_project("same", original.clone()).unwrap();
        let c = NativeObjectHistory::from_admitted_project("other", original.clone()).unwrap();
        assert_ne!(a.document_id(), b.document_id());
        let payload = serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap();
        assert_eq!(
            a.commit_fill(b.prepare_fill_json(&payload).unwrap()),
            Err(DispatchErrorCode::WrongDocument)
        );
        assert_eq!(
            a.commit_fill(c.prepare_fill_json(&payload).unwrap()),
            Err(DispatchErrorCode::WrongInstance)
        );
        assert_eq!(a.content_revision(), 0);
        assert_eq!(a.history_depths(), (0, 0));
        assert!(a.acquire_snapshot(1).is_none());
        unchanged_project(&current(&a), &original, &drawing(1));
    }

    #[test]
    fn arbitrary_project_drawings_are_revalidated_before_owner_construction() {
        for version in [1, 2] {
            let good = project(version, true);
            for path in ["/totalFrames", "/objects/1/target/frameScope/frame"] {
                let mut raw = drawing(version);
                *raw.pointer_mut(path).unwrap() =
                    json!(if path == "/totalFrames" { 0 } else { 21 });
                let invalid: ObjectDocument = serde_json::from_value(raw).unwrap();
                assert!(NativeObjectHistory::from_admitted_project(
                    "invalid",
                    good.with_drawing(invalid)
                )
                .is_err());
            }
            if version == 2 {
                let mut invalid = good.drawing().clone();
                invalid.composition_order = Default::default();
                assert!(NativeObjectHistory::from_admitted_project(
                    "invalid",
                    good.with_drawing(invalid)
                )
                .is_err());
            }
            assert!(NativeObjectHistory::from_admitted_project("", good.clone()).is_err());
            let owner = NativeObjectHistory::from_admitted_project("good", good.clone()).unwrap();
            unchanged_project(&current(&owner), &good, &drawing(version));
        }
    }

    #[test]
    fn drawing_only_serialize_refuses_projects_and_bare_compatibility_stays() {
        for attached in [false, true] {
            let owner =
                NativeObjectHistory::from_admitted_project("serialize", project(1, attached))
                    .unwrap();
            let snapshot = current(&owner);
            let request = serde_json::from_value(
                json!({"apiVersion":2,"requestId":"serialize", "instanceId":snapshot.instance_id(),
                "documentId":snapshot.document_id(),"operation":"query.document.object.serialize",
                "payload":{"atRevision":0}}),
            )
            .unwrap();
            let reply = snapshot.dispatch(request).unwrap();
            assert!(!reply.ok);
            assert!(reply.result.is_none());
            assert_eq!(reply.error.unwrap().code, DispatchErrorCode::Unavailable);
        }
        let bare = NativeObjectHistory::new("bare", project(1, false).drawing().clone()).unwrap();
        let snapshot = current(&bare);
        assert!(snapshot.project().is_none());
        let request = serde_json::from_value(
            json!({"apiVersion":2,"requestId":"bare", "instanceId":snapshot.instance_id(),
            "documentId":snapshot.document_id(),"operation":"query.document.object.serialize",
            "payload":{"atRevision":0}}),
        )
        .unwrap();
        assert!(snapshot.dispatch(request).unwrap().ok);
    }
}

/// One non-Clone owner. Construction always admits a fresh document incarnation.
#[derive(Debug)]
pub struct NativeObjectHistory {
    owner: ObjectFillOwner,
    undo: Vec<ObjectHistoryEntry>,
    redo: Vec<ObjectHistoryEntry>,
    receipts: StageReceipts,
}

impl NativeObjectHistory {
    /// Crate-private only; aggregate/resource admission and host routing are
    /// unavailable. Uses the SAME checked owner and fresh incarnation as bare.
    pub(crate) fn from_admitted_project(
        instance_id: impl Into<String>,
        project: ProjectEnvelope,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: ObjectFillOwner::from_admitted_project(instance_id, project)?,
            undo: vec![],
            redo: vec![],
            receipts: StageReceipts::default(),
        })
    }
    pub fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: ObjectFillOwner::new(instance_id, document)?,
            undo: vec![],
            redo: vec![],
            receipts: StageReceipts::default(),
        })
    }
    pub fn instance_id(&self) -> &str {
        self.owner.instance_id()
    }
    pub fn document_id(&self) -> &str {
        self.owner.document_id()
    }
    pub fn content_revision(&self) -> u64 {
        self.owner.content_revision()
    }
    pub fn acquire_snapshot(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.owner.acquire_snapshot(revision)
    }
    pub fn history_depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    /// Strict D1 payload preparation from this owner's current immutable snapshot.
    pub fn prepare_fill_json(&self, bytes: &[u8]) -> Result<PreparedObjectFill, DispatchErrorCode> {
        self.owner.prepare_fill_json(bytes)
    }
    /// Returns the selected revision; semantic no-op returns the unchanged head.
    pub fn commit_fill(&mut self, candidate: PreparedObjectFill) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_fill(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Restores full retained before-content into a new monotonic revision.
    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, true)
    }
    /// Restores full retained after-content into a new monotonic revision.
    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, false)
    }
    fn move_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let source = if undo { &self.undo } else { &self.redo };
        let entry = source.last().ok_or(DispatchErrorCode::Unavailable)?.clone();
        let revision = if undo {
            entry.before_revision
        } else {
            entry.after_revision
        };
        let restored = self.owner.restore(revision)?;
        if undo {
            self.undo.pop();
            self.redo.push(entry);
        } else {
            self.redo.pop();
            self.undo.push(entry);
        }
        Ok(restored)
    }
}
