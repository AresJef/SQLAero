//! Classic-protocol framing and checked use of mysql_common packet parsers.

pub mod auth;
pub mod framing;
pub mod packets;
/// Shared protocol capability type; this is not a public raw client_flags input.
pub use mysql_common::constants::CapabilityFlags;
