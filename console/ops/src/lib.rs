//! The operations of the console, one module per subject. The console in
//! ../src declares none of them and takes each from here.

pub mod account;
pub mod clean;
pub mod disk;
pub mod edge;
pub mod flags;
pub mod forge;
pub mod freeze;
pub mod github;
pub mod health;
pub mod host_message;
pub mod ids;
pub mod mcp;
pub mod panel;
pub mod pins;
pub mod rukas;
pub mod sandbox;
pub mod settings;
pub mod setup;
pub mod shipped;
pub mod size;
pub mod slice;
pub mod stop;
pub mod store;
pub mod tidy;
pub mod update;

use tehas_core::jq;
