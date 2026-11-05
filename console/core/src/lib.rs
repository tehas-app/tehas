//! Code the workspace's programs share. Each part here holds one rule that
//! more than one program keeps, so the rule is written once.

#[cfg(feature = "mask")]
pub mod approval;
#[cfg(feature = "mask")]
pub mod call;
pub mod disk;
pub mod env_file;
pub mod events;
pub mod file;
pub mod forge;
pub mod grammar;
pub mod hold;
pub mod install;
#[cfg(feature = "mask")]
pub mod journal;
pub mod jq;
pub mod limit;
pub mod lock;
#[cfg(feature = "mask")]
pub mod mask;
pub mod models;
pub mod out;
pub mod overlap;
#[cfg(test)]
mod overlap_tests;
pub mod plan;
#[cfg(feature = "mask")]
pub mod question;
pub mod review;
pub mod rukas;
pub mod sandbox;
pub mod secret;
pub mod settings;
#[cfg(feature = "mask")]
pub mod size;
pub mod slice;
pub mod slow;
pub mod snapshot;
pub mod testdir;
pub mod textcheck;
pub mod time;
