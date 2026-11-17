//! The attachment library: the one link rule ([`link`]), the one fetch of a
//! link and of a pinned link ([`fetch`]), and the versioned manifest.json of a
//! cycle's files ([`manifest`]). The attach program (src/main.rs), the
//! dispatcher, the console and the cycle supervisor read links, fetch them and
//! write manifests through it.

pub mod comments;
pub mod fetch;
pub mod given;
pub mod handoff;
pub mod link;
pub mod manifest;
pub mod sniff;
pub mod take;
pub mod unpack;
pub mod zip;

#[cfg(test)]
mod given_tests;
#[cfg(test)]
mod take_tests;
#[cfg(test)]
mod tests;
