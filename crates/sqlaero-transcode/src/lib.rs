//! Format-neutral native contracts and writing primitives.
//!
//! Python ownership and source protocols belong to the binding adapter. Target
//! formats consume native facts without introducing a complete value tree.

pub mod buffer;
pub mod diagnostic;
pub mod identifier;
pub mod integer;
pub mod number;
pub mod structure;
pub mod temporal;
pub mod utf8;
pub mod value;
