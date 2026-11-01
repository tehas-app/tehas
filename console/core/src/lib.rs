//! Code the workspace's programs share. Each part here holds one rule that
//! more than one program keeps, so the rule is written once.

#[cfg(feature = "mask")]
pub mod call;
pub mod file;
pub mod forge;
pub mod grammar;
pub mod hold;
#[cfg(feature = "mask")]
pub mod journal;
pub mod lock;
#[cfg(feature = "mask")]
pub mod mask;
pub mod plan;
#[cfg(feature = "mask")]
pub mod question;
pub mod review;
pub mod slow;
pub mod time;
