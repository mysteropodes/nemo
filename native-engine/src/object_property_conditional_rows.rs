//! #2520 / #2525: the conditional Motion rows of a layer holder (`propsFor`, motion.js@1065e9d8), read from the retained snapshot.
use crate::object_codec::PropertyHolderTarget;
use crate::revision::ObjectSnapshot;

/// Baseline `SMEffectorLayer.propKeysFor`: the radius and strength of an effector layer, the angle of a linear one, then one offset per channel.
const EFFECTOR_ROWS: [&str; 10] = [
    "effRadius",
    "effStrength",
    "effAngle",
    "effOffPosition",
    "effOffRotation",
    "effOffScale",
    "effOffOpacity",
    "effOffPositionZ",
    "effOffRotationX",
    "effOffRotationY",
];

impl ObjectSnapshot {
    /// Baseline `propsFor` (motion.js@1065e9d8), the rows a layer lists only once their prerequisite is set, in its order:
    /// the rows of an effector layer (radius, strength, the angle of a linear falloff, one offset per channel), Matte On once the layer has
    /// a matte, Position on path (and Influence once Align is on) once it follows a path, and the In / Out offsets of Parent in Time for
    /// the edge(s) its link mode drives. A second parent (`parentBlend`, the weights), the exposed properties of a component and the
    /// rows of a parametric shape are not listed here: the native layer refuses a second parent, and the other two have no row label
    /// or consumer to show yet.
    pub(crate) fn conditional_layer_rows(&self, target: &PropertyHolderTarget) -> Vec<&'static str> {
        let document = self.document();
        let mut rows: Vec<&'static str> = Vec::new();
        if document.duplicator_set().is_some_and(|set| {
            set.effector_layers.iter().any(|e| {
                e.context_id == target.context_id() && e.layer_uid == target.layer_uid()
            })
        }) {
            let linear = document.duplicator_set().is_some_and(|set| {
                set.effector_layers.iter().any(|e| {
                    e.context_id == target.context_id()
                        && e.layer_uid == target.layer_uid()
                        && e.effector.falloff.value().is_some_and(|f| f == "linear")
                })
            });
            rows.extend(EFFECTOR_ROWS.iter().filter(|k| linear || **k != "effAngle"));
        }
        let layers = document
            .component_set()
            .and_then(|set| set.component(target.context_id()))
            .map_or(document.layers(), |component| component.layers());
        let Some(layer) = layers
            .iter()
            .find(|l| l.layer_uid() == target.layer_uid())
        else {
            return rows;
        };
        if layer
            .matte_mode
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|mode| mode != "none")
        {
            rows.push("matteOn");
        }
        if let Some(follow) = layer.follow_path.value().and_then(Option::as_ref) {
            rows.push("pathPercent");
            if follow.align() {
                rows.push("pathInfluence");
            }
        }
        if let Some(link) = layer
            .range
            .value()
            .and_then(|range| range.time_link.value())
            .and_then(Option::as_ref)
        {
            let mode = link.mode.value().map_or("both", String::as_str);
            if mode != "out" {
                rows.push("timeLinkInOffset");
            }
            if mode != "in" {
                rows.push("timeLinkOutOffset");
            }
        }
        rows
    }
}
