//! SQLAero's native MySQL core, independent of Python and the I/O runtime.
//!
//! Configuration parsing is shared by the binding boundary and native callers.

pub mod config;
pub mod transport;
