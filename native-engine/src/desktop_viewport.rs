//! Staged native viewport policy; N16 owns real window/surface binding.
//!
//! This host borrows N12's device, queue, texture and view. A `SurfacePort`
//! returns an owned acquired-frame token which must be consumed before recovery.
//! N13 creates no GPU context, desktop window, renderer, or pixel readback path.

use crate::compositor::{CompositionResult, Compositor};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::{ViewGeneration, WorkId};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalExtent { pub width: u32, pub height: u32 }
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssBounds { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssPoint { pub x: f64, pub y: f64 }
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompositionEdges { pub x0: f64, pub y0: f64, pub x1: f64, pub y1: f64 }
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssEdges { pub x0: f64, pub y0: f64, pub x1: f64, pub y1: f64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceScalePolicy { StretchToSurface }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMapping { css_bounds: CssBounds, physical: PhysicalExtent, composition: PhysicalExtent, reported_dpr: f64 }

impl ViewportMapping {
    pub fn new(css_bounds: CssBounds, physical: PhysicalExtent, composition: PhysicalExtent, reported_dpr: f64) -> Result<Self, ViewportError> {
        if !css_bounds.x.is_finite() || !css_bounds.y.is_finite() || !css_bounds.width.is_finite() || !css_bounds.height.is_finite()
            || css_bounds.width <= 0.0 || css_bounds.height <= 0.0 || physical.width == 0 || physical.height == 0
            || composition.width == 0 || composition.height == 0 || !reported_dpr.is_finite() || reported_dpr <= 0.0 { return Err(ViewportError::InvalidBounds); }
        Ok(Self { css_bounds, physical, composition, reported_dpr })
    }
    pub fn physical_extent(self) -> PhysicalExtent { self.physical }
    pub fn composition_extent(self) -> PhysicalExtent { self.composition }
    pub fn reported_dpr(self) -> f64 { self.reported_dpr }
    pub fn pointer_to_local_css(self, client: CssPoint) -> CssPoint { CssPoint { x: client.x - self.css_bounds.x, y: client.y - self.css_bounds.y } }
    pub fn pointer_to_physical(self, client: CssPoint) -> CssPoint { self.scale_point(client, self.physical) }
    pub fn pointer_to_composition(self, client: CssPoint) -> CssPoint { self.scale_point(client, self.composition) }
    pub fn selection_to_client(self, edges: CompositionEdges) -> Result<CssEdges, ViewportError> {
        let local = self.selection_to_local_css(edges)?;
        Ok(CssEdges { x0: self.css_bounds.x + local.x0, y0: self.css_bounds.y + local.y0, x1: self.css_bounds.x + local.x1, y1: self.css_bounds.y + local.y1 })
    }
    pub fn selection_to_local_css(self, edges: CompositionEdges) -> Result<CssEdges, ViewportError> {
        if ![edges.x0, edges.y0, edges.x1, edges.y1].iter().all(|value| value.is_finite()) || edges.x1 < edges.x0 || edges.y1 < edges.y0 { return Err(ViewportError::InvalidSelection); }
        let scale_x = self.css_bounds.width / f64::from(self.composition.width);
        let scale_y = self.css_bounds.height / f64::from(self.composition.height);
        Ok(CssEdges { x0: edges.x0 * scale_x, y0: edges.y0 * scale_y, x1: edges.x1 * scale_x, y1: edges.y1 * scale_y })
    }
    fn scale_point(self, client: CssPoint, target: PhysicalExtent) -> CssPoint {
        CssPoint { x: (client.x - self.css_bounds.x) * f64::from(target.width) / self.css_bounds.width, y: (client.y - self.css_bounds.y) * f64::from(target.height) / self.css_bounds.height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAttempt<F> { Acquired(F), Lost, Outdated, Timeout, Occluded }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettledSurfaceFrame { Presented, ValidationFailure, DeviceLost }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredAction { Timeout, Occluded }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatalAction { ValidationFailure, DeviceLost, RetryLost, RetryOutdated, RecreateFailed, ReconfigureFailed }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewportStatus { Presented, StaleDiscarded, Deferred(DeferredAction), Failed(FatalAction) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationReceipt { identity: ScheduledFrameIdentity, status: ViewportStatus }
impl PresentationReceipt { pub fn identity(&self) -> &ScheduledFrameIdentity { &self.identity } pub fn status(&self) -> &ViewportStatus { &self.status } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewportError { InvalidBounds, InvalidPointer, InvalidSelection, Disposed, Unregistered(WorkId), IdentityMismatch(WorkId), CompositionExtentMismatch { expected: PhysicalExtent, actual: PhysicalExtent }, Fatal(FatalAction) }

/// Borrowed N12 resources; source and target extents/policy are explicit for N16.
pub struct SurfacePresentation<'a> { pub device: &'a wgpu::Device, pub queue: &'a wgpu::Queue, pub texture: &'a wgpu::Texture,
    pub texture_view: &'a wgpu::TextureView, pub source_extent: PhysicalExtent, pub surface_extent: PhysicalExtent,
    pub scale_policy: SurfaceScalePolicy, pub identity: &'a ScheduledFrameIdentity }

/// A frame has been acquired. Consuming it is the only way to obtain its result.
pub trait AcquiredSurfaceFrame { fn settle(self) -> SettledSurfaceFrame; }

/// N16 implements this with a same-GPU-context surface. The GAT ties an acquired
/// token to the mutable port borrow, so recovery cannot run while it still exists.
pub trait SurfacePort {
    type Frame<'port>: AcquiredSurfaceFrame where Self: 'port;
    fn acquire<'port>(&'port mut self, source: SurfacePresentation<'_>) -> SurfaceAttempt<Self::Frame<'port>>;
    fn recreate(&mut self, extent: PhysicalExtent) -> Result<(), ()>;
    fn reconfigure(&mut self, extent: PhysicalExtent) -> Result<(), ()>;
    fn dispose(&mut self);
    #[cfg(test)] fn simulated_acquire<'port>(&'port mut self, identity: &ScheduledFrameIdentity) -> SurfaceAttempt<Self::Frame<'port>>;
}

enum SurfaceEvent { Settled(SettledSurfaceFrame), Lost, Outdated, Timeout, Occluded }
fn consume_attempt<F: AcquiredSurfaceFrame>(attempt: SurfaceAttempt<F>) -> SurfaceEvent {
    match attempt { SurfaceAttempt::Acquired(frame) => SurfaceEvent::Settled(frame.settle()), SurfaceAttempt::Lost => SurfaceEvent::Lost,
        SurfaceAttempt::Outdated => SurfaceEvent::Outdated, SurfaceAttempt::Timeout => SurfaceEvent::Timeout, SurfaceAttempt::Occluded => SurfaceEvent::Occluded }
}

pub struct DesktopViewportHost<P> {
    port: P, mapping: ViewportMapping, pending: BTreeMap<WorkId, ScheduledFrameIdentity>, receipts: BTreeMap<WorkId, PresentationReceipt>,
    newest_generation: Option<ViewGeneration>, fatal: Option<FatalAction>, disposed: bool,
}

impl<P: SurfacePort> DesktopViewportHost<P> {
    /// Policy-only construction; injection cannot activate Tauri. N16 proves its same-context seam.
    pub fn new(port: P, mapping: ViewportMapping) -> Self { Self { port, mapping, pending: BTreeMap::new(), receipts: BTreeMap::new(), newest_generation: None, fatal: None, disposed: false } }
    pub fn surface_scale_policy(&self) -> SurfaceScalePolicy { SurfaceScalePolicy::StretchToSurface }
    pub fn mapping(&self) -> ViewportMapping { self.mapping }
    pub fn fatal_action(&self) -> Option<FatalAction> { self.fatal }
    pub fn register(&mut self, identity: ScheduledFrameIdentity) -> Result<(), ViewportError> { self.register_as(identity.work_id(), identity) }
    pub fn resize(&mut self, mapping: ViewportMapping) -> Result<(), ViewportError> {
        self.ensure_active()?;
        if self.mapping.physical_extent() == mapping.physical_extent() { self.mapping = mapping; return Ok(()); }
        if self.port.reconfigure(mapping.physical_extent()).is_err() { return Err(self.latch(FatalAction::ReconfigureFailed)); }
        self.mapping = mapping; Ok(())
    }
    pub fn pointer_intent(&self, client: CssPoint) -> Result<CssPoint, ViewportError> {
        self.ensure_active()?;
        if !client.x.is_finite() || !client.y.is_finite() { return Err(ViewportError::InvalidPointer); }
        Ok(self.mapping.pointer_to_composition(client))
    }
    pub fn selection_intent(&self, edges: CompositionEdges) -> Result<CssEdges, ViewportError> { self.ensure_active()?; self.mapping.selection_to_client(edges) }
    pub fn present_composition(&mut self, compositor: &Compositor, result: &CompositionResult) -> Result<PresentationReceipt, ViewportError> {
        let identity = result.scheduled_identity().clone();
        if let Some(receipt) = self.prepare_present(identity.clone())? { return Ok(receipt); }
        let actual = PhysicalExtent { width: result.dimensions().0, height: result.dimensions().1 };
        if actual != self.mapping.composition_extent() { return Err(ViewportError::CompositionExtentMismatch { expected: self.mapping.composition_extent(), actual }); }
        let mapping = self.mapping;
        let first = consume_attempt(self.port.acquire(SurfacePresentation { device: compositor.device(), queue: compositor.queue(), texture: result.texture(), texture_view: result.texture_view(), source_extent: mapping.composition_extent(), surface_extent: mapping.physical_extent(), scale_policy: SurfaceScalePolicy::StretchToSurface, identity: &identity }));
        self.complete_event(identity.clone(), first, |port| consume_attempt(port.acquire(SurfacePresentation { device: compositor.device(), queue: compositor.queue(), texture: result.texture(), texture_view: result.texture_view(), source_extent: mapping.composition_extent(), surface_extent: mapping.physical_extent(), scale_policy: SurfaceScalePolicy::StretchToSurface, identity: &identity })))
    }
    pub fn pending_work_ids(&self) -> Vec<WorkId> { self.pending.keys().copied().collect() }
    pub fn dispose(&mut self) -> Vec<WorkId> { if self.disposed { return Vec::new(); } let pending = self.pending_work_ids(); self.pending.clear(); self.port.dispose(); self.disposed = true; pending }
    fn register_as(&mut self, work_id: WorkId, identity: ScheduledFrameIdentity) -> Result<(), ViewportError> {
        self.ensure_active()?;
        if let Some(existing) = self.pending.get(&work_id).or_else(|| self.receipts.get(&work_id).map(PresentationReceipt::identity)) { return if existing == &identity { Ok(()) } else { Err(ViewportError::IdentityMismatch(work_id)) }; }
        self.newest_generation = Some(self.newest_generation.map_or(identity.view_generation(), |value| value.max(identity.view_generation())));
        self.pending.insert(work_id, identity); Ok(())
    }
    fn prepare_present(&mut self, identity: ScheduledFrameIdentity) -> Result<Option<PresentationReceipt>, ViewportError> {
        if self.disposed { return Err(ViewportError::Disposed); }
        if let Some(receipt) = self.receipts.get(&identity.work_id()) { return if receipt.identity() == &identity { Ok(Some(receipt.clone())) } else { Err(ViewportError::IdentityMismatch(identity.work_id())) }; }
        if let Some(action) = self.fatal { return Err(ViewportError::Fatal(action)); }
        let registered = self.pending.get(&identity.work_id()).ok_or(ViewportError::Unregistered(identity.work_id()))?;
        if registered != &identity { return Err(ViewportError::IdentityMismatch(identity.work_id())); }
        if self.newest_generation.is_some_and(|latest| identity.view_generation() < latest) { return Ok(Some(self.terminal(identity, ViewportStatus::StaleDiscarded))); }
        Ok(None)
    }
    fn complete_event<F>(&mut self, identity: ScheduledFrameIdentity, event: SurfaceEvent, mut retry: F) -> Result<PresentationReceipt, ViewportError> where F: FnMut(&mut P) -> SurfaceEvent {
        match event {
            SurfaceEvent::Settled(SettledSurfaceFrame::Presented) => Ok(self.terminal(identity, ViewportStatus::Presented)),
            SurfaceEvent::Settled(SettledSurfaceFrame::ValidationFailure) => Ok(self.fatal_receipt(identity, FatalAction::ValidationFailure)),
            SurfaceEvent::Settled(SettledSurfaceFrame::DeviceLost) => Ok(self.fatal_receipt(identity, FatalAction::DeviceLost)),
            SurfaceEvent::Timeout => Ok(self.deferred(identity, DeferredAction::Timeout)),
            SurfaceEvent::Occluded => Ok(self.deferred(identity, DeferredAction::Occluded)),
            SurfaceEvent::Lost | SurfaceEvent::Outdated => {
                let initial = event;
                let recovered = if matches!(initial, SurfaceEvent::Lost) { self.port.recreate(self.mapping.physical_extent()).map_err(|_| FatalAction::RecreateFailed) } else { self.port.reconfigure(self.mapping.physical_extent()).map_err(|_| FatalAction::ReconfigureFailed) };
                if let Err(action) = recovered { return Ok(self.fatal_receipt(identity, action)); }
                match retry(&mut self.port) {
                    SurfaceEvent::Settled(SettledSurfaceFrame::Presented) => Ok(self.terminal(identity, ViewportStatus::Presented)),
                    SurfaceEvent::Settled(SettledSurfaceFrame::ValidationFailure) => Ok(self.fatal_receipt(identity, FatalAction::ValidationFailure)),
                    SurfaceEvent::Settled(SettledSurfaceFrame::DeviceLost) => Ok(self.fatal_receipt(identity, FatalAction::DeviceLost)),
                    SurfaceEvent::Timeout => Ok(self.deferred(identity, DeferredAction::Timeout)), SurfaceEvent::Occluded => Ok(self.deferred(identity, DeferredAction::Occluded)),
                    SurfaceEvent::Lost => Ok(self.fatal_receipt(identity, FatalAction::RetryLost)), SurfaceEvent::Outdated => Ok(self.fatal_receipt(identity, FatalAction::RetryOutdated)),
                }
            }
        }
    }
    fn deferred(&self, identity: ScheduledFrameIdentity, action: DeferredAction) -> PresentationReceipt { PresentationReceipt { identity, status: ViewportStatus::Deferred(action) } }
    fn terminal(&mut self, identity: ScheduledFrameIdentity, status: ViewportStatus) -> PresentationReceipt { let receipt = PresentationReceipt { identity: identity.clone(), status }; self.pending.remove(&identity.work_id()); self.receipts.insert(identity.work_id(), receipt.clone()); receipt }
    fn fatal_receipt(&mut self, identity: ScheduledFrameIdentity, action: FatalAction) -> PresentationReceipt { self.fatal = Some(action); self.terminal(identity, ViewportStatus::Failed(action)) }
    fn latch(&mut self, action: FatalAction) -> ViewportError { self.fatal = Some(action); ViewportError::Fatal(action) }
    fn ensure_active(&self) -> Result<(), ViewportError> { if self.disposed { Err(ViewportError::Disposed) } else if let Some(action) = self.fatal { Err(ViewportError::Fatal(action)) } else { Ok(()) } }
}

#[cfg(test)]
impl<P: SurfacePort> DesktopViewportHost<P> {
    pub(crate) fn simulate_port_completion(&mut self, identity: ScheduledFrameIdentity) -> Result<PresentationReceipt, ViewportError> {
        if let Some(receipt) = self.prepare_present(identity.clone())? { return Ok(receipt); }
        let first = consume_attempt(self.port.simulated_acquire(&identity));
        self.complete_event(identity.clone(), first, |port| consume_attempt(port.simulated_acquire(&identity)))
    }
    pub(crate) fn test_port(&self) -> &P { &self.port }
    pub(crate) fn test_port_mut(&mut self) -> &mut P { &mut self.port }
    pub(crate) fn register_under_for_test(&mut self, work_id: WorkId, identity: ScheduledFrameIdentity) -> Result<(), ViewportError> { self.register_as(work_id, identity) }
}
