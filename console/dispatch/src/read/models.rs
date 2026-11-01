//! What a snapshot asks of the model endpoints and the sandbox image, as
//! libexec/dispatch asks it: mesh_check over lib/models.sh, image_ok's
//! image and model, and whether a refresh can continue. curl and docker do the
//! asking, under the bash's bounds.

use super::run::{run, run_input};
use serde_json::{json, Value};
use std::time::Duration;
use tehas_core::models::{self, auth, host_path, ModelFile};

/// A png of one pixel, as libexec/dispatch's mesh_pixel.
const PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// The instance's providers and the models its agents use.
pub struct Models {
    pub file: ModelFile,
    /// tehas_models: (provider, model), the subagent's first, each pair once.
    pub used: Vec<(String, String)>,
    pub provider: String,
    pub model: String,
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_default()
}

impl Models {
    pub fn read() -> Models {
        let file = ModelFile::read(&host_path());
        let used = models::used();
        Models {
            file,
            used,
            provider: env("PI_ENSEMBLE_SUBAGENT_PROVIDER"),
            model: env("PI_ENSEMBLE_SUBAGENT_MODEL"),
        }
    }
}

fn ask(url: &str, key: &str, wanted: &[String]) -> Result<(), String> {
    models::ask(url, key, wanted, &|args, input| {
        let r = run_input("curl", args, Some(input), Duration::from_secs(15));
        r.ok().then_some(r.out)
    })
}

/// What mesh_check settles: the snapshot's endpoint ({hold, end, log}) and
/// the subagent provider's url and key, empty when none was asked.
pub fn endpoint(m: &Models) -> (Value, String, String) {
    let mut log: Vec<String> = Vec::new();
    let (mut url_used, mut key_used, mut end) = (String::new(), String::new(), String::new());
    if m.file.url(&m.provider, "main").is_empty() || m.model.is_empty() {
        return (
            json!({"hold": "", "end": "", "log": []}),
            url_used,
            key_used,
        );
    }
    let mut providers: Vec<&str> = Vec::new();
    for (p, _) in &m.used {
        if !providers.contains(&p.as_str()) {
            providers.push(p);
        }
    }
    for p in providers {
        let url = m.file.url(p, "main");
        if url.is_empty() {
            continue;
        }
        let models: Vec<String> = m
            .used
            .iter()
            .filter(|(x, _)| x == p)
            .map(|(_, y)| y.clone())
            .collect();
        let why = match ask(&url, &m.file.key(p, "main"), &models) {
            Ok(()) => {
                if p == m.provider {
                    (url_used, key_used, end) = (url, m.file.key(p, "main"), "main".into());
                }
                continue;
            }
            Err(w) => w,
        };
        let backup = m.file.url(p, "backup");
        let hold = if backup.is_empty() {
            format!("holding: no resources: the model endpoint {why}")
        } else {
            match ask(&backup, &m.file.key(p, "backup"), &models) {
                Ok(()) => {
                    log.push(format!(
                        "model endpoint: provider {p} is on its backup, the main endpoint {why}"
                    ));
                    if p == m.provider {
                        (url_used, key_used, end) =
                            (backup, m.file.key(p, "backup"), "backup".into());
                    }
                    continue;
                }
                Err(w2) => {
                    format!("holding: no resources: the model endpoint {why}, and its backup {w2}")
                }
            }
        };
        return (
            json!({"hold": hold, "end": end, "log": log}),
            url_used,
            key_used,
        );
    }
    (
        json!({"hold": "", "end": end, "log": log}),
        url_used,
        key_used,
    )
}

/// What image_ok asks: the sandbox image's labels, and, with an endpoint
/// asked, whether models.json says the model takes images and whether it
/// answers one.
pub fn images(m: &Models, url: &str, key: &str) -> Value {
    let image = env("PI_ENSEMBLE_IMAGE");
    let r = run(
        "docker",
        &[
            "image".into(),
            "inspect".into(),
            "--format".into(),
            "{{index .Config.Labels \"tehas.rukas\"}}|{{index .Config.Labels \"tehas.rukas.images\"}}".into(),
            image,
        ],
        Duration::from_secs(30),
    );
    let line = if r.ok() {
        r.out.lines().next().unwrap_or("").to_string()
    } else {
        String::new()
    };
    let (commit, reads) = line.split_once('|').unwrap_or((&line, ""));
    let commit = if (7..=40).contains(&commit.len())
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        commit
    } else {
        ""
    };
    let reads = if matches!(reads, "0" | "1") {
        reads
    } else {
        ""
    };
    let declares = m
        .file
        .json
        .get("providers")
        .and_then(|x| x.get(&m.provider))
        .and_then(|x| x.get("models"))
        .and_then(Value::as_array)
        .is_some_and(|a| {
            a.iter().any(|x| {
                x.get("id").and_then(Value::as_str) == Some(&m.model)
                    && x.get("input")
                        .and_then(Value::as_array)
                        .is_some_and(|i| i.iter().any(|v| v == "image"))
            })
        });
    // Asked only where image_ok would ask it: a reading image, an endpoint,
    // a model that says it takes images.
    let answers = reads != "0"
        && !url.is_empty()
        && declares
        && {
            let body = json!({"model": m.model, "max_tokens": 1, "messages": [{"role": "user", "content": [
            {"type": "text", "text": "."},
            {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{PIXEL}")}}]}]});
            let config = format!(
                "{}data = {}\n",
                auth(key),
                serde_json::to_string(&body.to_string()).unwrap_or_default()
            );
            run_input(
                "curl",
                &[
                    "-fsS".into(),
                    "-K".into(),
                    "-".into(),
                    "-m".into(),
                    "30".into(),
                    "-H".into(),
                    "Content-Type: application/json".into(),
                    format!("{url}/chat/completions"),
                ],
                Some(&config),
                Duration::from_secs(35),
            )
            .ok()
        };
    json!({"rukas": reads, "commit": commit, "declares": declares, "answers": answers})
}

/// Whether the image's pi-rukas takes --continue-pr
/// natively.
pub fn continues() -> bool {
    let wait = std::env::var("TEHAS_CAPS_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let r = run(
        "docker",
        &[
            "run".into(),
            "--rm".into(),
            "--init".into(),
            env("PI_ENSEMBLE_IMAGE"),
            "/opt/pi-rukas/bin/pi-rukas-headless".into(),
            "--capabilities".into(),
        ],
        Duration::from_secs(wait),
    );
    let caps: Value = serde_json::from_str(&r.out).unwrap_or(Value::Null);
    r.ok() && caps.pointer("/features/continuePr") == Some(&json!(true))
}
