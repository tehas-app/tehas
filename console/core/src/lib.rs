//! Code the workspace's programs share. Each part here holds one rule that
//! more than one program keeps, so the rule is written once.

pub mod file;
pub mod grammar;
#[cfg(feature = "mask")]
pub mod mask;
pub mod time;
