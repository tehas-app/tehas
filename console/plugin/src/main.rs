//! tehas-plugin: the plugins' program. `tehasbot plugin` runs it under the
//! instance's settings, and the units run that.
//!
//!   tehas-plugin run <name> --kind refresh [--trigger timer|host|action [--by <login>]]
//!   tehas-plugin run <name> --kind hook --pr <n> [--trigger cycle|host]
//!   tehas-plugin run <name>-<pr> --kind hook     what tehas-plugin-hook@.service gives
//!   tehas-plugin tick
//!
//! `run` is one run of the plugin's container (src/run.rs). With no
//! `--trigger`, what asked for it is read from the request it left. `tick` is
//! the timer's step (src/tick.rs).
//!
//! Exit, run: 0 done; 1 failed or out of time; 3 running, or not ready to
//! run; 64 usage; 78 the feature is off or the settings name no product
//! mirror. tick: 0.

use std::time::Duration;
use tehas_plugin::host::Real;
use tehas_plugin::run::{self, Setup};
use tehas_plugin::tick;

const USAGE: &str = "usage: tehas-plugin run <name> --kind refresh|hook [--pr N] [--trigger timer|host|action|cycle] [--by <login>] | tehas-plugin tick";

fn main() {
    std::process::exit(i32::from(go()));
}

fn go() -> u8 {
    let given: Vec<String> = std::env::args().skip(1).collect();
    let get = |name: &str| std::env::var(name).unwrap_or_default();
    let ask = match given.split_first() {
        Some((verb, rest)) if verb == "run" => match run::words(rest) {
            Some(a) => Some(a),
            None => {
                eprintln!("{USAGE}");
                return 64;
            }
        },
        Some((verb, rest)) if verb == "tick" && rest.is_empty() => None,
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    let Some(home) = run::home(&get) else {
        eprintln!("tehas-plugin: TEHAS_HOME is not set");
        return 78;
    };
    let models = std::path::PathBuf::from(tehas_core::models::host_path());
    let setup = Setup {
        home: &home,
        get: &get,
        models: &models,
        poll: Duration::from_secs(5),
    };
    let mut host = Real {
        home: home.clone(),
        exec: tehas_ops::plugin::exec::Host,
    };
    match ask {
        Some(a) => run::run(&mut host, &setup, &a),
        None => tick::tick(&mut host, &setup),
    }
}
