//! #2255: the widget presentation view borrows the actual successful presentation, like the Mesh and subselect reads.
use crate::native_dispatch::NativeObjectHost;
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use native_engine::object_snapshot::ObjectSnapshot;
impl NativeObjectHost {
    pub(crate) fn dispatch_widget_view(
        &self,
        snapshot: &ObjectSnapshot,
        request: OpacityRequest,
    ) -> Result<ResponseEnvelope, String> {
        let answer = match self.presented_object_source(snapshot) {
            Ok((receipt, witness)) => {
                let identity = receipt.identity();
                let work = format!("native-work-{}", identity.work_id().value());
                snapshot.dispatch_with_widget_view(
                    request,
                    Ok((
                        &work,
                        identity.view_generation().value(),
                        witness.context_id(),
                        witness.frame(),
                    )),
                )
            }
            Err(_) => {
                snapshot.dispatch_with_widget_view(request, Err("Widget presentation unavailable"))
            }
        };
        answer.map_err(|_| "native widget read correlation is invalid".into())
    }
}
