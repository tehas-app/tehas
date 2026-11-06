//! The rule of which GitHub read is tried again lives in tehas-core, so the
//! poll, `tehasbot status` and `tehas tidy` read through the same one.

pub use tehas_core::retry::*;
