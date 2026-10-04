//! Document construction is the sole point that can grant catalog provenance.
use crate::application::{
    diagnostics, reproduction_catalog, ExportResourceResolver, NativeApplication,
    ReproductionCatalogError, ReproductionFixture,
};
use crate::document::OpacityDocument;
use crate::export_job::{ExportCompositor, ExportJobManager, StagedArtifactPort};
use crate::history::NativeOpacityHistory;
use std::collections::HashMap;

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    pub fn new(
        instance_id: impl Into<String>,
        document: OpacityDocument,
        artifact_port: P,
        compositor: C,
        resources: R,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            history: NativeOpacityHistory::new(instance_id, document)?,
            exports: ExportJobManager::new(artifact_port, compositor),
            resources,
            requests: HashMap::new(),
            release: None,
            replacement: None,
            diagnostics: diagnostics::RecentDiagnostics::default(),
            reproduction_origin: None,
            reproduction: Default::default(),
        })
    }

    /// Constructs a separate fresh authority from embedded bytes. A matching
    /// label/hash on a caller-supplied document never enters this path.
    pub fn from_reproduction_fixture(
        instance_id: impl Into<String>,
        fixture: ReproductionFixture<'_>,
        artifact_port: P,
        compositor: C,
        resources: R,
    ) -> Result<Self, ReproductionCatalogError> {
        let (document, origin) = reproduction_catalog::load(fixture)?;
        let mut application =
            Self::new(instance_id, document, artifact_port, compositor, resources)
                .map_err(|_| ReproductionCatalogError::InvalidInstance)?;
        application.reproduction_origin = Some(origin);
        Ok(application)
    }
}
