//! Layer colour label (#2371 U11): `applyLayerColor` of `src/js/timeline.js` at 1065e9d8 as one sealed lifecycle edit.
//! The request names the clicked layer, the `#rrggbb` label and, optionally, the editor selection; the plan kernel
//! (`plan_recolor`) paints the whole selection when it holds more than one layer and the clicked one, otherwise only the
//! clicked layer. Renaming and recolouring are label edits: the baseline `renameLayer` and `applyLayerColor` never checked a
//! lock or a transform parent, so the baseline policy permits them. The kernel owns no history or id state.
use crate::commands::object_fill_owner::layer_lifecycle::{parse, LayerLifecycleKind, PreparedLayerLifecycle};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_recolor, BaselineLayerEditPolicy,
};
use crate::object_document::object_deserialize;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

#[derive(Debug)]
struct ColorPayload {
    command: String,
    layer_uid: String,
    color: String,
    selected: Option<Vec<String>>,
}
object_deserialize!(ColorPayload {
    command: String,
    layer_uid: String,
    color: String,
    selected: Option<Vec<String>>
});

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_color_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: ColorPayload = parse(bytes)?;
        let selected = payload.selected.unwrap_or_default();
        if payload.command != "object.layer.color.set"
            || payload.layer_uid.is_empty()
            || selected.iter().any(String::is_empty)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan_recolor(
            origin.document(),
            &payload.layer_uid,
            &selected,
            &payload.color,
            &BaselineLayerEditPolicy,
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Color, plan)
    }
}
