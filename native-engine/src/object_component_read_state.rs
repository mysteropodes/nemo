//! C-comp-owned-state: the pages of a component's stored markers and camera keys of `query.document.object.component.read`
//! (selectors `markers` and `cameraKeys`), in stored order. The scene is refused (`invalid_request`): its markers belong to the
//! composition owner and its camera to `query.document.object.project.camera.read`.
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

/// A frame number as a whole JSON integer when it is one.
fn whole(frame: f64) -> Value {
    if frame.fract() == 0.0 && frame.abs() < 9.0e15 {
        json!(frame as i64)
    } else {
        json!(frame)
    }
}

impl ObjectSnapshot {
    /// The page head (with `contextId`) and every item of `kind` (`markers` or `cameraKeys`) of the named component.
    pub(super) fn component_state_page(
        &self,
        source: &ContextSource<'_>,
        id: &str,
        kind: &str,
    ) -> Result<(Value, Vec<Value>), DispatchErrorCode> {
        let context = source
            .context(id)
            .map_err(|_| DispatchErrorCode::NotFound)?;
        let component = context.component().ok_or(DispatchErrorCode::InvalidRequest)?;
        let items = if kind == "markers" {
            let markers = component.markers().iter();
            markers
                .map(|m| json!({"frame":m.frame,"name":m.name,"color":m.color}))
                .collect()
        } else {
            let keys = component.camera_keys().unwrap_or(&[]).iter();
            keys.map(|k| {
                json!({"frame":whole(k.frame),"x":k.x,"y":k.y,"w":k.w,"rot":k.rot.value(),
                    "hOut":k.h_out.value(),"hIn":k.h_in.value(),"ease":k.ease.value()})
            })
            .collect()
        };
        let mut head = self.component_head(kind);
        head["contextId"] = json!(context.id());
        Ok((head, items))
    }
}
