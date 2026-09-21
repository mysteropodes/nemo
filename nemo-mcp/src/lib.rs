//! Transport only. The running application's command service owns all document writes.
pub mod capabilities;
pub(crate) mod capability_contract;
pub mod contract;
pub(crate) mod native_contract;
pub mod registry;
pub mod server;
pub mod wire;

pub const BUILD_SOURCE_ID: &str = env!("NEMO_MCP_SOURCE_ID");
