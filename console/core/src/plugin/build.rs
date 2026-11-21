//! The commands of a plugin's fetch and build as data
//! (docs/specs/2026-11-21-plugins-security.md, "The build"): the fetch of the
//! pinned commit over https only, the reads of a file at the commit, the
//! context as `git archive`, and the build with no network.

/// The manifest at the root of a plugin's repository.
pub const MANIFEST: &str = "tehas-plugin.json";
/// The seconds a fetch may take.
pub const FETCH_LIMIT: u32 = 600;
/// The seconds a pull of one base image may take.
pub const PULL_LIMIT: u32 = 600;
/// The seconds the build may take.
pub const BUILD_LIMIT: u32 = 1800;
/// The largest Dockerfile read.
pub const DOCKERFILE_MAX: u64 = 1 << 20;

/// What every git command of a plugin's repository is given: https only, no
/// redirect to another address, no hook, no submodule.
pub const GIT_CONFIG: [&str; 5] = [
    "protocol.allow=never",
    "protocol.https.allow=always",
    "http.followRedirects=false",
    "core.hooksPath=/dev/null",
    "fetch.recurseSubmodules=false",
];

/// The image a plugin's commit builds.
pub fn tag(name: &str, commit: &str) -> String {
    format!("tehas-plugin-{name}:{commit}")
}

/// The repository of the image tags of a plugin.
pub fn image_repo(name: &str) -> String {
    format!("tehas-plugin-{name}")
}

/// The ref that keeps a fetched commit.
pub fn keep_ref(commit: &str) -> String {
    format!("refs/tehas/{commit}")
}

/// `git` with the repository and its config, then `args`.
pub fn git(repo: &str, args: &[&str]) -> Vec<String> {
    let mut out = vec!["-C".to_string(), repo.to_string()];
    for c in GIT_CONFIG {
        out.push("-c".to_string());
        out.push(c.to_string());
    }
    out.extend(args.iter().map(|a| a.to_string()));
    out
}

/// The fetch of one commit and nothing else, into a ref that keeps it: the
/// arguments of `timeout`.
pub fn fetch(repo: &str, url: &str, commit: &str) -> Vec<String> {
    let refspec = format!("+{commit}:{}", keep_ref(commit));
    let mut out = vec![FETCH_LIMIT.to_string(), "git".to_string()];
    out.extend(git(
        repo,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-recurse-submodules",
            "--depth=1",
            url,
            &refspec,
        ],
    ));
    out
}

/// The build of the context on standard input: the arguments of `timeout`.
/// BuildKit takes `--memory` and ignores it, so the slice is what bounds the
/// build's memory.
pub fn build(name: &str, commit: &str, dockerfile: &str) -> Vec<String> {
    let t = tag(name, commit);
    [
        &BUILD_LIMIT.to_string(),
        "docker",
        "build",
        "--network",
        "none",
        "--cgroup-parent",
        "tehas.slice",
        "--memory",
        "4g",
        "--no-cache",
        "--pull=false",
        "-f",
        dockerfile,
        "-t",
        &t,
        "-",
    ]
    .map(String::from)
    .to_vec()
}

/// What `docker image inspect --format '{{json .Config.OnBuild}}'` answered:
/// the image's ONBUILD triggers. None: no such answer.
pub fn onbuild(said: &str) -> Option<Vec<String>> {
    match serde_json::from_str::<serde_json::Value>(said.trim()).ok()? {
        serde_json::Value::Null => Some(Vec::new()),
        serde_json::Value::Array(a) => a
            .into_iter()
            .map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn the_fetch_takes_one_commit_over_https_with_no_hook_or_submodule() {
        let got = fetch("/h/plugins/floorplan/repo.git", "https://h.example/p", C).join(" ");
        assert_eq!(
            got,
            format!(
                "600 git -C /h/plugins/floorplan/repo.git -c protocol.allow=never -c protocol.https.allow=always -c http.followRedirects=false -c core.hooksPath=/dev/null -c fetch.recurseSubmodules=false fetch --quiet --no-tags --no-recurse-submodules --depth=1 https://h.example/p +{C}:refs/tehas/{C}"
            )
        );
    }

    #[test]
    fn the_build_has_no_network_and_no_cache_and_runs_in_the_slice() {
        assert_eq!(
            build("floorplan", C, "docker/Dockerfile").join(" "),
            format!("1800 docker build --network none --cgroup-parent tehas.slice --memory 4g --no-cache --pull=false -f docker/Dockerfile -t tehas-plugin-floorplan:{C} -")
        );
    }

    #[test]
    fn the_onbuild_triggers_are_read_from_dockers_answer() {
        assert_eq!(onbuild("null\n"), Some(vec![]));
        assert_eq!(onbuild("[]"), Some(vec![]));
        assert_eq!(
            onbuild(r#"["RUN echo hi"]"#),
            Some(vec!["RUN echo hi".to_string()])
        );
        assert_eq!(onbuild("no such image"), None);
        assert_eq!(onbuild("[1]"), None);
    }
}
