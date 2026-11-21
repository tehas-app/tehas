//! tehas-plugin-gate: the plugins' gate (../plugin/src/gate.rs,
//! docs/specs/2026-11-21-plugins-web.md).
//!
//!   tehas-plugin-gate
//!
//! The settings are read as tehas_load leaves them in the environment:
//! TEHAS_HOME, TEHAS_DOMAIN, TEHAS_EDGE_ONE_LEVEL, TEHAS_REPO, TEHAS_PLUGINS,
//! each TEHAS_PLUGIN_GRANTS_<NAME>, TEHAS_PLUGIN_ACCESS and
//! TEHAS_PLUGIN_GATE_ADDR. The GitHub App's client id and secret are read
//! from ~/.config/tehas/github-app.client-id and github-app.client-secret,
//! which no one but the host's user may read. It is off unless the feature
//! `plugins` is on: off, it says so and ends 0, and no port is opened.
//!
//! A press is not asked of a runner yet: it answers 503 until the runner
//! implements [`Refresh`].
//!
//! Exit: 0 the feature is off; 78 a setting or the App's client is missing or
//! malformed; 1 it cannot listen.

use std::collections::HashMap;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tehas_core::settings::rules;
use tehas_plugin::gate::catalog::Installed;
use tehas_plugin::gate::github::{Curl, Level};
use tehas_plugin::gate::{self, Gate, Press, Refresh, Settings};

/// Connections served at once; one more is 503.
const MOST: usize = 64;

struct Unwired;

impl Refresh for Unwired {
    fn ask(&self, _: &str, _: &str, _: &str) -> Press {
        Press::Failed("the runner is not built yet".to_string())
    }
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn stop(why: &str) -> ! {
    eprintln!("tehas-plugin-gate: {why}");
    std::process::exit(78);
}

/// A file of the App's client: one line, readable by the host's user only.
fn client(home: &std::path::Path, name: &str) -> String {
    let at = home.join(".config/tehas").join(name);
    let Ok(meta) = std::fs::metadata(&at) else {
        stop(&format!(
            "{} is not there: the sign-in needs the GitHub App's client id and secret",
            at.display()
        ));
    };
    if meta.permissions().mode() & 0o077 != 0 {
        stop(&format!(
            "{} may be read by others: make it mode 600",
            at.display()
        ));
    }
    let text = std::fs::read_to_string(&at).unwrap_or_default();
    let value = text.trim_end_matches('\n');
    if !tehas_core::plugin::is_client_value(value) {
        stop(&format!(
            "{} is not one line of the App's client",
            at.display()
        ));
    }
    value.to_string()
}

fn main() {
    if std::env::args().len() > 1 {
        eprintln!("usage: tehas-plugin-gate");
        std::process::exit(64);
    }
    if !tehas_core::features::on("plugins") {
        tehas_core::outln!("off");
        return;
    }
    let home = PathBuf::from(env("TEHAS_HOME"));
    if !home.is_absolute() {
        stop("TEHAS_HOME is not set");
    }
    let domain = env("TEHAS_DOMAIN");
    if domain.is_empty() {
        stop("TEHAS_DOMAIN is not set: a plugin's view is served only at a name of the domain");
    }
    let repo = env("TEHAS_REPO");
    if !rules::is_repo(&repo) {
        stop("TEHAS_REPO is not set");
    }
    let Some(access) = Level::of_setting(&env("TEHAS_PLUGIN_ACCESS")) else {
        stop("TEHAS_PLUGIN_ACCESS is not set: name the least permission on TEHAS_REPO that lets a person in");
    };
    let addr = env("TEHAS_PLUGIN_GATE_ADDR");
    if rules::check("TEHAS_PLUGIN_GATE_ADDR", &addr).is_err() {
        stop("TEHAS_PLUGIN_GATE_ADDR is not a private address and port");
    }
    let pins = tehas_core::plugin::pins(&env("TEHAS_PLUGINS"))
        .unwrap_or_else(|why| stop(&format!("TEHAS_PLUGINS is not valid: {why}")));
    let mut grants = HashMap::new();
    for pin in &pins {
        let line = env(&tehas_core::plugin::grants_setting(&pin.name));
        if line.is_empty() {
            continue;
        }
        let g = tehas_core::plugin::grants(&line).unwrap_or_else(|why| {
            stop(&format!("the grants of {} are not valid: {why}", pin.name))
        });
        grants.insert(pin.name.clone(), g);
    }
    let user_home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let client_id = client(&user_home, tehas_core::plugin::CLIENT_ID_FILE);
    let client_secret = client(&user_home, tehas_core::plugin::CLIENT_SECRET_FILE);
    let api = Some(env("TEHAS_GITHUB_API"))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_string());
    let web = "https://github.com".to_string();
    let settings = Settings {
        domain,
        one_level: env("TEHAS_EDGE_ONE_LEVEL") == "1",
        plugins: pins.iter().map(|p| p.name.clone()).collect(),
        access,
        home: home.clone(),
        client_id: client_id.clone(),
        web: web.clone(),
    };
    let github = Curl {
        api,
        web,
        repo,
        client_id,
        client_secret,
        console: home.join("libexec/console"),
    };
    let catalog = Installed::new(home, pins, grants);
    let gate = Arc::new(Gate::new(
        settings,
        Box::new(github),
        Box::new(catalog),
        Box::new(Unwired),
    ));
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("tehas-plugin-gate: cannot listen on {addr}: {e}");
        std::process::exit(1);
    });
    eprintln!("tehas-plugin-gate: listening on {addr}");
    gate::serve(listener, gate, MOST);
}
