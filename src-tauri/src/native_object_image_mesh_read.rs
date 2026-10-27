//! The existing owner lends its actual successful preview to a bounded immutable query.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use native_engine::object_snapshot::{
    ImageMeshReadFailure, ImageMeshReadProjection, ObjectSnapshot,
};
use native_engine::{
    desktop_viewport::PresentationReceipt, object_render_scene::ObjectCompositionWitness,
};
use serde_json::Value;

impl NativeObjectHost {
    pub(super) fn presented_object_source(
        &self,
        snapshot: &ObjectSnapshot,
    ) -> Result<(&PresentationReceipt, &ObjectCompositionWitness), ImageMeshReadFailure> {
        use ImageMeshReadFailure::{Stale, Unpresented};
        let preview = &self.preview;
        if self.released.is_some()
            || preview.released
            || preview.blocked.is_some()
            || preview.pending.is_some()
            || preview.prepared.is_some()
        {
            return Err(Unpresented);
        }
        let receipt = preview.presented_mesh_view.as_ref().ok_or(Unpresented)?;
        let witness = preview.reference_cache.as_ref()
            .or(preview.presented_vectors.as_ref()).ok_or(Unpresented)?;
        let identity = receipt.identity();
        let key = identity.evaluation_key();
        if receipt.status() != &native_engine::desktop_viewport::ViewportStatus::Presented {
            return Err(Unpresented);
        }
        if self.content_revision() != snapshot.content_revision()
            || witness.instance_id() != snapshot.instance_id()
            || witness.document_id() != snapshot.document_id()
            || witness.content_revision() != snapshot.content_revision()
            || witness.document_snapshot_id() != snapshot.snapshot_id()
            || key.document_snapshot_id() != snapshot.snapshot_id()
            || key.context_id() != witness.context_id()
            || key.frame() != witness.frame()
        {
            return Err(Stale);
        }
        Ok((receipt, witness))
    }
    fn image_mesh_projection(
        &self,
        snapshot: &ObjectSnapshot,
        view: &Value,
    ) -> Result<ImageMeshReadProjection, ImageMeshReadFailure> {
        use ImageMeshReadFailure::{Stale, Unsupported};
        let (receipt, witness) = self.presented_object_source(snapshot)?;
        let identity = receipt.identity();
        let work_id = format!("native-work-{}", identity.work_id().value());
        if view["workId"].as_str() != Some(work_id.as_str())
            || view["viewGeneration"].as_u64() != Some(identity.view_generation().value())
            || view["frame"].as_u64() != Some(u64::from(witness.frame()))
            || view["contextId"].as_str() != Some(witness.context_id())
        {
            return Err(Stale);
        }
        let layer = view["layerUid"].as_str().ok_or(Unsupported)?;
        witness
            .image_mesh_read_projection(layer)?
            .with_presentation(work_id, identity.view_generation().value())
            .map_err(|_| Unsupported)
    }

    pub(crate) fn dispatch_image_mesh_view(
        &self,
        snapshot: &ObjectSnapshot,
        request: OpacityRequest,
    ) -> Result<ResponseEnvelope, String> {
        let wire =
            serde_json::to_value(&request).map_err(|_| "native image mesh selector is invalid")?;
        if wire["payload"]["view"]["kind"] == "presentation" {
            let result = match self.presented_object_source(snapshot) {
                Ok((receipt, witness)) => snapshot.dispatch_presentation_view(
                    request,
                    Ok((
                        witness.context_id(),
                        witness.frame(),
                        &format!("native-work-{}", receipt.identity().work_id().value()),
                        receipt.identity().view_generation().value(),
                    )),
                ),
                Err(reason) => snapshot.dispatch_presentation_view(request, Err(reason)),
            };
            return result.map_err(|_| "native presentation read correlation is invalid".into());
        }
        let projection = self.image_mesh_projection(snapshot, &wire["payload"]["view"]);
        snapshot
            .dispatch_with_image_mesh_view(request, projection.as_ref().map_err(|failure| *failure))
            .map_err(|_| "native image mesh read correlation is invalid".into())
    }
}

#[cfg(test)]
#[path = "native_image_mesh_read_public_tests.rs"]
mod tests;

#[cfg(test)]
mod presentation_guards {
    use super::super::tests::{output, present, selector, snapshot, viewport, Mode};
    use super::tests::{host_info, host_read, host_saved, mutate, presented_project};
    use super::*;
    use native_engine::compositor::Compositor;
    use serde_json::json;

    fn request(owner: &NativeObjectHost) -> Value {
        let info = host_info(owner);
        json!({"apiVersion":2,"requestId":"presentation-guard-read","instanceId":info["instanceId"],
            "documentId":info["documentId"],"operation":"query.document.object",
            "payload":{"atRevision":info["contentRevision"],"view":{"kind":"presentation"}}})
    }
    fn counts(owner: &NativeObjectHost) -> (u64, u64) {
        let counts = owner.preview.scheduler.lease_counters();
        (counts.live(), counts.acquired())
    }
    fn read_without_spending(owner: &mut NativeObjectHost, query: &Value) -> Value {
        let saved = host_saved(owner);
        let history = owner.history.history_depths();
        let counters = counts(owner);
        let receipt = owner.preview.presented_mesh_view.clone();
        let answer = host_read(owner, query);
        assert_eq!(host_saved(owner), saved);
        assert_eq!(owner.history.history_depths(), history);
        assert_eq!(counts(owner), counters);
        assert_eq!(
            owner
                .preview
                .presented_mesh_view
                .as_ref()
                .map(|v| v.identity()),
            receipt.as_ref().map(|v| v.identity())
        );
        answer
    }
    #[test]
    fn gpu_bootstrap_guards_current_main_receipt_without_state_or_resource_spending() {
        let bytes = presented_project().to_string();
        let mut owner =
            NativeObjectHost::from_project_file("bootstrap-guard-owner".into(), bytes.as_bytes())
                .unwrap();
        let query = request(&owner);
        assert_eq!(
            read_without_spending(&mut owner, &query)["error"]["code"],
            "unavailable"
        );
        let pin = snapshot(&owner);
        owner
            .with_prepared_object_preview(&selector(&pin, 0), output(), |owner| {
                assert_eq!(
                    read_without_spending(owner, &query)["error"]["code"],
                    "unavailable"
                );
                Ok(())
            })
            .unwrap();
        let mut gpu = Compositor::new().unwrap();
        let (mut surface, trace) = viewport(&gpu, 64);
        trace.borrow_mut().mode = Mode::Defer;
        let pending = present(&mut owner, &mut gpu, &mut surface).unwrap();
        assert!(matches!(
            pending.status(),
            native_engine::desktop_viewport::ViewportStatus::Deferred(_)
        ));
        assert_eq!(
            read_without_spending(&mut owner, &query)["error"]["code"],
            "unavailable"
        );
        trace.borrow_mut().mode = Mode::Present;
        let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
        let acquired = read_without_spending(&mut owner, &query);
        assert_eq!(acquired["ok"], true);
        assert_eq!(
            acquired["result"]["workId"],
            format!("native-work-{}", shown.identity().work_id().value())
        );
        assert_eq!(
            acquired["result"]["viewGeneration"],
            shown.identity().view_generation().value()
        );
        assert_eq!(
            acquired["result"]["frame"],
            shown.identity().evaluation_key().frame()
        );
        let mut stale = query.clone();
        stale["payload"]["atRevision"] = json!(owner.content_revision() + 1);
        assert_eq!(
            read_without_spending(&mut owner, &stale)["error"]["code"],
            "stale_revision"
        );
        owner.preview.blocked = Some("test cleanup debt".into());
        assert_eq!(
            read_without_spending(&mut owner, &query)["error"]["code"],
            "unavailable"
        );
        owner.preview.blocked = None;
        let receipt = owner.preview.presented_mesh_view.clone();
        let witness = owner.preview.reference_cache.clone();
        mutate(&mut owner, "bootstrap-detach", false);
        let current = request(&owner);
        assert_eq!(
            read_without_spending(&mut owner, &current)["error"]["code"],
            "unavailable"
        );
        owner.preview.presented_mesh_view = receipt;
        owner.preview.reference_cache = witness;
        assert_eq!(
            read_without_spending(&mut owner, &current)["error"]["code"],
            "stale_revision",
            "historical caches are not current presentation proof"
        );
        owner.preview.invalidate();
        mutate(&mut owner, "bootstrap-undo", true);
        let restored = request(&owner);
        assert_eq!(
            read_without_spending(&mut owner, &restored)["error"]["code"],
            "unavailable"
        );
        present(&mut owner, &mut gpu, &mut surface).unwrap();
        assert_eq!(read_without_spending(&mut owner, &restored)["ok"], true);
        let mut replacement =
            NativeObjectHost::from_project_file(owner.instance_id().into(), bytes.as_bytes())
                .unwrap();
        assert_eq!(
            read_without_spending(&mut replacement, &restored)["error"]["code"],
            "wrong_document"
        );
        owner.release_project().unwrap();
        let counters = counts(&owner);
        let history = owner.history.history_depths();
        assert!(owner
            .dispatch(serde_json::from_value(restored).unwrap())
            .is_err());
        assert_eq!(counts(&owner), counters);
        assert_eq!(owner.history.history_depths(), history);
    }
}

#[cfg(test)]
#[path = "native_subselect_read_public_tests.rs"]
mod subselect_tests;
