//! Code the workspace's programs share. Each part here holds one rule that
//! more than one program keeps, so the rule is written once.

pub mod file;
pub mod forge;
pub mod grammar;
#[cfg(feature = "mask")]
pub mod mask;
#[cfg(feature = "mask")]
pub mod question;
pub mod review;
pub mod slow;
pub mod time;
