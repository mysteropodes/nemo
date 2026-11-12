//! Native object command-family registry (CS2/#2345). A family is one file in this directory that
//! defines `pub(crate) const FAMILY: CommandFamily`; build.rs discovers the files and generates
//! the `mod` declarations and `FAMILIES`, so adding a family never edits central dispatch.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::transaction::CompactResult;

/// What a family hands back for the shared envelope: `Ok(Err(..))` is a typed refusal that is
/// retained as a receipt; the outer error aborts dispatch before any receipt (as before CS2).
pub(crate) type FamilyOutcome = Result<CompactResult, (DispatchErrorCode, &'static str)>;
pub(crate) type FamilyDispatch =
    fn(&mut NativeObjectHistory, &OpacityRequest) -> Result<FamilyOutcome, DispatchErrorCode>;

/// The default bound on an encoded request envelope.
pub(crate) const DEFAULT_REQUEST_BYTES: usize = 4096;

/// The shared envelope (identity, bounds, replay, revision, cancel) runs first; `dispatch` only
/// prepares and commits the typed transaction.
pub(crate) struct CommandFamily {
    pub(crate) name: &'static str,
    /// Whether this family owns the operation name.
    pub(crate) owns: fn(&str) -> bool,
    /// Receipt message for envelope-level refusals.
    pub(crate) refusal: &'static str,
    /// Encoded request bound for the family's operations.
    pub(crate) request_limit: usize,
    pub(crate) dispatch: FamilyDispatch,
}

include!(concat!(env!("OUT_DIR"), "/command_families.rs"));

/// The registered family owning `operation`, if any.
pub(crate) fn owner(operation: &str) -> Option<&'static CommandFamily> {
    FAMILIES
        .iter()
        .find(|family| (family.owns)(operation))
        .copied()
}
