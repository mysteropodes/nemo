//! Resolve a frame-independent Motion target to its actual native Order slot.
//! Repeated stroke identities across authored cells are ambiguous, never guessed.
use crate::object_document::{AuthoredOrder, FrameScopeKind, ObjectDocument, PropertyHolderTarget};

enum Location {
    Layer(usize, usize),
    Element(usize, usize, usize),
}

impl ObjectDocument {
    fn property_order_location(&self, target: &PropertyHolderTarget) -> Option<Location> {
        if self.format_version() != 2 || target.context_id() != "scene-root" {
            return None;
        }
        self.layers()
            .iter()
            .find(|l| l.layer_uid() == target.layer_uid())?;
        let plan = self.composition_order.value()?;
        let c = plan
            .contexts
            .iter()
            .position(|c| c.context_id == target.context_id())?;
        let context = &plan.contexts[c];
        let Some(stroke) = target.stroke_id() else {
            return context
                .layers
                .iter()
                .position(|l| l.layer_uid == target.layer_uid())
                .map(|l| Location::Layer(c, l));
        };
        let mut found = None;
        for (s, scope) in context.scopes.iter().enumerate() {
            if scope.frame_scope.kind != FrameScopeKind::Authored {
                continue;
            }
            let actual = |t: &crate::object_document::ObjectTarget| {
                t.context_id() == target.context_id()
                    && t.layer_uid() == target.layer_uid()
                    && t.stroke_id() == stroke
                    && t.scope_kind() == &FrameScopeKind::Authored
                    && t.frame() == scope.frame_scope.frame
            };
            if !self.objects().iter().any(|r| actual(r.target()))
                && !self
                    .item_records()
                    .is_some_and(|items| items.targets().any(|t| actual(&t)))
            {
                continue;
            }
            for (e, element) in scope.elements.iter().enumerate() {
                if element.layer_uid == target.layer_uid() && element.stroke_id == stroke {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(Location::Element(c, s, e));
                }
            }
        }
        found
    }

    pub(crate) fn property_order(&self, target: &PropertyHolderTarget) -> Option<&AuthoredOrder> {
        let location = self.property_order_location(target)?;
        let plan = self.composition_order.value()?;
        Some(match location {
            Location::Layer(c, l) => &plan.contexts[c].layers[l].order,
            Location::Element(c, s, e) => &plan.contexts[c].scopes[s].elements[e].order,
        })
    }

    pub(crate) fn property_order_mut(
        &mut self,
        target: &PropertyHolderTarget,
    ) -> Option<&mut AuthoredOrder> {
        let location = self.property_order_location(target)?;
        let crate::object_document::Presence::Present(plan) = &mut self.composition_order else {
            return None;
        };
        Some(match location {
            Location::Layer(c, l) => &mut plan.contexts[c].layers[l].order,
            Location::Element(c, s, e) => &mut plan.contexts[c].scopes[s].elements[e].order,
        })
    }
}
