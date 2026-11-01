//! Code the workspace's programs share. Each part here holds one rule that
//! more than one program keeps, so the rule is written once.

pub mod disk;
pub mod file;
pub mod forge;
pub mod grammar;
pub mod hold;
pub mod lock;
#[cfg(feature = "mask")]
pub mod mask;
pub mod plan;
#[cfg(feature = "mask")]
pub mod question;
pub mod review;
#[cfg(feature = "mask")]
pub mod size;
pub mod slow;
pub mod time;
