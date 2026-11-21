//! tehas-plugin: the plugins' program. It has no command yet: the model
//! socket is a library (src/model.rs) that later commands start.
//!
//!   tehas-plugin
//!
//! Exit status: 64, the usage.

fn main() {
    eprintln!("tehas-plugin: no command yet");
    std::process::exit(64);
}
