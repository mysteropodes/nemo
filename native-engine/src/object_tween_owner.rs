//! Sealed tween edit using the existing native object revision owner.
use crate::commands::object_fill_owner::object_context_projection::{context_of, embed, project};
use crate::commands::ObjectFillOwner;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use crate::tween_kernels::{
    plan_tween, prepare_tween_resources, PreparedTweenResources, TweenPlan, TweenRequest,
};
#[derive(Debug)]
pub(crate) struct PreparedTween {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    resources: Option<PreparedTweenResources>,
}
impl ObjectFillOwner {
    pub(crate) fn prepare_tween_json(
        &self,
        operation: &str,
        bytes: &[u8],
    ) -> Result<PreparedTween, DispatchErrorCode> {
        if bytes.len() > crate::tween_kernels::limits::REQUEST_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let request: TweenRequest =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if operation.strip_prefix("command.document.") != Some(request.command.as_str()) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let custom = origin
            .project()
            .and_then(|p| p.project().custom_brush_presets());
        // #2358: a declared component is the "scene" of the tween kernels, exactly as the baseline's enterSymbol swapped `state.layers`
        // for the component's: project, plan, embed (the scene, the other components and the placement tables stay as they were).
        // A tween that needs a bitmap-brush re-bake publishes a project resource, which has no component embedding: typed unavailable.
        let context = context_of(&request.context_id)?;
        let planned = if context == "scene-root" {
            plan_tween(origin.document(), &request, custom)?
        } else {
            let view = project(origin.document(), context)?;
            match plan_tween(&view, &request, custom)? {
                None => None,
                Some(plan) if !plan.bakes.is_empty() => return Err(DispatchErrorCode::Unavailable),
                Some(plan) => Some(TweenPlan {
                    document: embed(origin.document(), context, &view, plan.document)?,
                    bakes: Vec::new(),
                }),
            }
        };
        let (document, resources) = match planned {
            None => (None, None),
            Some(plan) => {
                let bitmap = |d: &ObjectDocument| {
                    d.objects.iter().any(|r| {
                        r.vector_ext
                            .value()
                            .is_some_and(|v| !v.bitmap_brush.missing())
                    })
                };
                if bitmap(origin.document()) || bitmap(&plan.document) || !plan.bakes.is_empty() {
                    (
                        None,
                        Some(prepare_tween_resources(&origin, plan.document, plan.bakes)?),
                    )
                } else {
                    (Some(plan.document), None)
                }
            }
        };
        Ok(PreparedTween {
            origin,
            resources,
            document,
        })
    }
    pub(crate) fn commit_tween(
        &mut self,
        candidate: PreparedTween,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if let Some(resources) = candidate.resources {
            return self
                .revisions
                .publish_tween_resources(resources.into_project())
                .map_err(|_| DispatchErrorCode::Unavailable);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}
