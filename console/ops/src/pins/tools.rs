//! Each pinned tool, where its versions come from, and the values that move
//! with its version: a checksum, a digest, a lockfile and its hash.

use super::text::Ver;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

/// The outside world a move reads. The tests give it a fake.
pub trait World {
    /// The body of a GET.
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
    /// One file of a .tar.gz, as text.
    fn unpack(&self, archive: &[u8], member: &str) -> Result<String, String>;
    /// `npm install --package-lock-only` in that directory.
    fn npm_lock(&self, dir: &Path) -> Result<(), String>;
    /// The newest publish time npm will install, `YYYY-MM-DDTHH:MM:SS`, when
    /// its `min-release-age` holds new releases back.
    fn npm_cutoff(&self) -> Option<String>;
}

pub enum Source {
    /// GitHub releases of a repository, the tag being the prefix and X.Y.Z.
    Releases(&'static str, &'static str),
    /// GitHub tags, for a repository that makes no releases.
    Tags(&'static str),
    Node,
    Crate(&'static str),
    /// An npm package and the tree, under the root, that pins it.
    Npm(&'static str, &'static str),
    Caddy,
}

pub struct Tool {
    pub key: &'static str,
    pub name: &'static str,
    pub source: Source,
}

pub const TOOLS: &[Tool] = &[
    Tool {
        key: "NODE_VERSION",
        name: "Node",
        source: Source::Node,
    },
    Tool {
        key: "GH_VERSION",
        name: "gh",
        source: Source::Releases("cli/cli", "v"),
    },
    Tool {
        key: "BUN_VERSION",
        name: "bun",
        source: Source::Releases("oven-sh/bun", "bun-v"),
    },
    Tool {
        key: "SHELLCHECK_VERSION",
        name: "shellcheck",
        source: Source::Releases("koalaman/shellcheck", "v"),
    },
    Tool {
        key: "JQ_VERSION",
        name: "jq",
        source: Source::Releases("jqlang/jq", "jq-"),
    },
    Tool {
        key: "PI_VERSION",
        name: "Pi",
        source: Source::Npm("@earendil-works/pi-coding-agent", "host/npm/pi"),
    },
    Tool {
        key: "CTX7_VERSION",
        name: "ctx7",
        source: Source::Npm("ctx7", "host/npm/ctx7"),
    },
    Tool {
        key: "PARALLEL_CLI_VERSION",
        name: "parallel-cli",
        source: Source::Npm("parallel-web-cli", "host/npm/parallel-web-cli"),
    },
    Tool {
        key: "PLAYWRIGHT_VERSION",
        name: "Playwright",
        source: Source::Npm("playwright", "host/sandbox/npm/playwright"),
    },
    Tool {
        key: "RUST_VERSION",
        name: "Rust",
        source: Source::Releases("rust-lang/rust", ""),
    },
    Tool {
        key: "RUSTUP_VERSION",
        name: "rustup",
        source: Source::Tags("rust-lang/rustup"),
    },
    Tool {
        key: "VIPUNE_VERSION",
        name: "vipune",
        source: Source::Crate("vipune"),
    },
    Tool {
        key: "OO_VERSION",
        name: "oo",
        source: Source::Crate("double-o"),
    },
    Tool {
        key: "CBM_VERSION",
        name: "codebase-memory-mcp",
        source: Source::Releases("DeusData/codebase-memory-mcp", "v"),
    },
    Tool {
        key: "OSV_SCANNER_VERSION",
        name: "OSV-Scanner",
        source: Source::Releases("google/osv-scanner", "v"),
    },
    Tool {
        key: "CADDY_VERSION",
        name: "Caddy",
        source: Source::Caddy,
    },
];

/// The files besides versions.env that name Caddy's digest and version.
pub const CADDY_FILES: &[&str] = &[
    "host/edge/docs.compose.yaml",
    "host/edge/router.compose.yaml",
    "preview/Dockerfile",
];

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn json(world: &dyn World, url: &str) -> Result<Value, String> {
    let body = world.get(url)?;
    serde_json::from_slice(&body).map_err(|e| format!("{url} is not JSON: {e}"))
}

fn text(world: &dyn World, url: &str) -> Result<String, String> {
    String::from_utf8(world.get(url)?).map_err(|_| format!("{url} is not text"))
}

fn strs<'a>(v: &'a Value, field: &str) -> impl Iterator<Item = &'a str> {
    let field = field.to_string();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(move |e| e[&field].as_str())
}

/// Every release version the tool's source has.
pub fn versions(tool: &Tool, world: &dyn World) -> Result<Vec<Ver>, String> {
    let mut found: Vec<Ver> = Vec::new();
    match tool.source {
        Source::Releases(repo, prefix) => {
            let url = format!("https://api.github.com/repos/{repo}/releases?per_page=100");
            let all = json(world, &url)?;
            let published = all.as_array().into_iter().flatten().filter(|r| {
                r["draft"].as_bool() == Some(false) && r["prerelease"].as_bool() == Some(false)
            });
            found.extend(
                published.filter_map(|r| Ver::parse(r["tag_name"].as_str()?.strip_prefix(prefix)?)),
            );
        }
        Source::Tags(repo) => {
            let all = json(
                world,
                &format!("https://api.github.com/repos/{repo}/tags?per_page=100"),
            )?;
            found.extend(strs(&all, "name").filter_map(Ver::parse));
        }
        Source::Node => {
            let all = json(world, "https://nodejs.org/dist/index.json")?;
            found.extend(strs(&all, "version").filter_map(|v| Ver::parse(v.strip_prefix('v')?)));
        }
        Source::Crate(name) => {
            let all = json(world, &format!("https://crates.io/api/v1/crates/{name}"))?;
            let kept = all["versions"].as_array().into_iter().flatten();
            found.extend(
                kept.filter(|v| v["yanked"].as_bool() == Some(false))
                    .filter_map(|v| Ver::parse(v["num"].as_str()?)),
            );
        }
        Source::Npm(package, _) => {
            let url = format!("https://registry.npmjs.org/{}", package.replace('/', "%2F"));
            let all = json(world, &url)?;
            let listed = all["versions"].as_object().into_iter().flatten();
            // A version npm would refuse to lock is not one to move to.
            let cutoff = world.npm_cutoff();
            let old_enough = |k: &str| match &cutoff {
                None => true,
                Some(c) => all["time"][k]
                    .as_str()
                    .is_some_and(|t| t.get(..19).is_some_and(|t| t <= c.as_str())),
            };
            found.extend(
                listed
                    .filter(|(k, v)| v.get("deprecated").is_none() && old_enough(k))
                    .filter_map(|(k, _)| Ver::parse(k)),
            );
        }
        Source::Caddy => {
            let mut url = Some(
                "https://hub.docker.com/v2/repositories/library/caddy/tags?page_size=100&name=2."
                    .to_string(),
            );
            let mut pages = 0;
            while let Some(u) = url.take() {
                let page = json(world, &u)?;
                found.extend(strs(&page["results"], "name").filter_map(Ver::parse));
                pages += 1;
                url = page["next"]
                    .as_str()
                    .filter(|_| pages < 20)
                    .map(str::to_string);
            }
        }
    }
    if found.is_empty() {
        return Err(format!("{} lists no release", tool.name));
    }
    Ok(found)
}

/// The hash a publisher's checksum list gives for one file.
fn listed(list: &str, file: &str, url: &str) -> Result<String, String> {
    list.lines()
        .find_map(|l| {
            let mut words = l.split_whitespace();
            let hash = words.next()?;
            let name = words
                .next()?
                .trim_start_matches('*')
                .trim_start_matches("./");
            (name == file && hash.len() == 64).then(|| hash.to_ascii_lowercase())
        })
        .ok_or_else(|| format!("{url} lists no sha256 for {file}"))
}

/// The versions.env values, other than the version, that are true of the
/// tool at `v`.
pub fn companions(
    tool: &Tool,
    v: &str,
    world: &dyn World,
) -> Result<Vec<(String, String)>, String> {
    let one = |key: &str, value: String| Ok(vec![(key.to_string(), value)]);
    let from_list = |url: String, file: String| -> Result<String, String> {
        listed(&text(world, &url)?, &file, &url)
    };
    match tool.key {
        "NODE_VERSION" => one(
            "NODE_SHA256",
            from_list(
                format!("https://nodejs.org/dist/v{v}/SHASUMS256.txt"),
                format!("node-v{v}-linux-x64.tar.xz"),
            )?,
        ),
        "GH_VERSION" => one(
            "GH_SHA256",
            from_list(
                format!("https://github.com/cli/cli/releases/download/v{v}/gh_{v}_checksums.txt"),
                format!("gh_{v}_linux_amd64.tar.gz"),
            )?,
        ),
        "BUN_VERSION" => one(
            "BUN_SHA256",
            from_list(
                format!("https://github.com/oven-sh/bun/releases/download/bun-v{v}/SHASUMS256.txt"),
                "bun-linux-x64.zip".to_string(),
            )?,
        ),
        "JQ_VERSION" => one(
            "JQ_SHA256",
            from_list(
                format!("https://github.com/jqlang/jq/releases/download/jq-{v}/sha256sum.txt"),
                "jq-linux-amd64".to_string(),
            )?,
        ),
        "RUSTUP_VERSION" => one(
            "RUSTUP_SHA256",
            from_list(
                format!("https://static.rust-lang.org/rustup/archive/{v}/x86_64-unknown-linux-gnu/rustup-init.sha256"),
                "rustup-init".to_string(),
            )?,
        ),
        // shellcheck publishes no checksums: the hash is of the download.
        "SHELLCHECK_VERSION" => one(
            "SHELLCHECK_SHA256",
            sha256(&world.get(&format!(
                "https://github.com/koalaman/shellcheck/releases/download/v{v}/shellcheck-v{v}.linux.x86_64.tar.xz"
            ))?),
        ),
        "CBM_VERSION" => one(
            "CBM_INSTALLER_SHA256",
            sha256(&world.get(&format!(
                "https://raw.githubusercontent.com/DeusData/codebase-memory-mcp/v{v}/install.sh"
            ))?),
        ),
        // The sum of osv-scanner_linux_amd64, the asset the gate's step
        // downloads. The name is written in that step too, and
        // test/advisories_test.sh checks the two agree.
        "OSV_SCANNER_VERSION" => one(
            "OSV_SCANNER_SHA256",
            from_list(
                format!("https://github.com/google/osv-scanner/releases/download/v{v}/osv-scanner_SHA256SUMS"),
                "osv-scanner_linux_amd64".to_string(),
            )?,
        ),
        "CADDY_VERSION" => {
            let url = format!("https://hub.docker.com/v2/repositories/library/caddy/tags/{v}");
            let digest = json(world, &url)?["digest"].as_str().map(str::to_string);
            let digest = digest
                .filter(|d| d.strip_prefix("sha256:").is_some_and(|h| h.len() == 64))
                .ok_or_else(|| format!("{url} names no sha256 digest"))?;
            one("CADDY_DIGEST", digest)
        }
        "VIPUNE_VERSION" => model(v, world),
        _ => Ok(Vec::new()),
    }
}

/// The embedding model vipune `v` asks for, from its crate, and the hashes of
/// the two files of it the sandbox image carries, from Hugging Face.
fn model(v: &str, world: &dyn World) -> Result<Vec<(String, String)>, String> {
    let url = format!("https://crates.io/api/v1/crates/vipune/{v}/download");
    let engine = world.unpack(&world.get(&url)?, "src/embedding/engine.rs")?;
    let constant = |name: &str| {
        engine.lines().find_map(|l| {
            let rest = l
                .trim()
                .strip_prefix(&format!("pub const {name}: &str = \""))?;
            rest.strip_suffix("\";").map(str::to_string)
        })
    };
    let (Some(id), Some(rev)) = (constant("EMBED_MODEL_ID"), constant("EMBED_MODEL_REVISION"))
    else {
        return Err(format!("vipune {v} names no EMBED_MODEL_ID and EMBED_MODEL_REVISION in src/embedding/engine.rs"));
    };
    let good_id = id.split('/').count() == 2
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b));
    if !good_id || rev.len() != 40 || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "vipune {v} names the model {id} at {rev}, which is not a model and a commit"
        ));
    }
    let tree = format!("https://huggingface.co/api/models/{id}/tree/{rev}/onnx");
    let onnx = json(world, &tree)?
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["path"] == "onnx/model.onnx")
        .and_then(|e| e["lfs"]["oid"].as_str().map(str::to_string))
        .filter(|h| h.len() == 64)
        .ok_or_else(|| format!("{tree} has no sha256 of onnx/model.onnx"))?;
    let tokenizer = sha256(&world.get(&format!(
        "https://huggingface.co/{id}/resolve/{rev}/tokenizer.json"
    ))?);
    Ok(vec![
        ("VIPUNE_MODEL".into(), id),
        ("VIPUNE_MODEL_REVISION".into(), rev),
        ("VIPUNE_MODEL_SHA256_ONNX".into(), onnx),
        ("VIPUNE_MODEL_SHA256_TOKENIZER".into(), tokenizer),
    ])
}
