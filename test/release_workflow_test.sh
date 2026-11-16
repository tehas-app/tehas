#!/usr/bin/env bash
# Tests for .github/workflows/release.yml: its first step, that the tagged
# commit is on main and the account that pushed the tag is an admin of the
# repository, and the clean hosted runner it runs on, which gets every tool
# from the job's own steps. The steps' own scripts run here against scratch
# repositories and trees, with curl stubbed. Nothing here reaches the network.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

workflow=$T_ROOT/.github/workflows/release.yml
name='the tag is a commit on main, pushed by an admin of the repository'

t_case "the check is the first step after the checkout, which fetches all of history"
steps=$(grep -E '^      - (name|uses):' "$workflow")
t_eq "$(sed -n 2p <<<"$steps")" "      - name: $name"
t_contains "$(sed -n '/- uses: actions\/checkout/,/- name:/p' "$workflow")" 'fetch-depth: 0'

# The step's run script, as the runner gets it: from `run: |` to the next step,
# with the step's indent taken off.
awk -v n="      - name: $name" '$0 == n {s = 1; next}
  s && /^      - (name|uses):/ {exit}
  s == 2 {print substr($0, 11)}
  s && /^        run: \|$/ {s = 2}' "$workflow" >"$T/guard.sh"
t_contains "$(cat "$T/guard.sh")" 'git merge-base --is-ancestor HEAD refs/remotes/origin/main'

# origin with main and a side branch, and a clone of it as the checkout makes it.
git init -q -b main "$T/origin"
g() { git -C "$1" -c user.name=t -c user.email=t@example.invalid "${@:2}"; }
g "$T/origin" commit -q --allow-empty -m one
g "$T/origin" commit -q --allow-empty -m two
g "$T/origin" switch -q -c side
g "$T/origin" commit -q --allow-empty -m three
g "$T/origin" switch -q main
git clone -q "$T/origin" "$T/checkout"
main=$(git -C "$T/origin" rev-parse main)
side=$(git -C "$T/origin" rev-parse side)

# guard SHA ACTOR: the step on the checkout at SHA, as ACTOR pushed it, with
# the shell options of the workflow's bash. The calls of curl are the run's own.
guard() {
  rm -f "$T/calls/curl"
  git -C "$T/checkout" checkout -q "$1"
  t_run "cd '$T/checkout' && TAG=v1.2.3 ACTOR='$2' GH_TOKEN=x REPO=acme/widget bash -eo pipefail '$T/guard.sh'"
}

t_case "a commit on main pushed by an admin passes"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main" alice
t_eq "$t_rc" 0
t_contains "$(t_calls curl)" 'https://api.github.com/repos/acme/widget/collaborators/alice/permission'

t_case "an older commit of main passes"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main~1" alice
t_eq "$t_rc" 0

t_case "a commit that is not on main stops the release before the API is asked"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$side" alice
t_eq "$t_rc" 1
t_contains "$t_out" "v1.2.3 is not on main: $side"
t_eq "$(t_calls curl)" ""

t_case "a tag pushed by a bot stops the release"
t_stub curl 'echo "{\"permission\":\"admin\"}"'
guard "$main" 'tehas-app-bot[bot]'
t_eq "$t_rc" 1
t_contains "$t_out" "not by a person"
t_eq "$(t_calls curl)" ""

t_case "a tag pushed by an account with write access stops the release"
t_stub curl 'echo "{\"permission\":\"write\"}"'
guard "$main" bob
t_eq "$t_rc" 1
t_contains "$t_out" "bob, who is not an admin of acme/widget"

t_case "an API that does not answer stops the release"
t_stub curl 'exit 22'
guard "$main" alice
t_eq "$t_rc" 1
t_contains "$t_out" "who is not an admin"

# The steps in order, by name.
order=$(grep -E '^      - name:' "$workflow" | sed 's/^      - name: //')
pos() { grep -nxF -- "$1" <<<"$order" | cut -d: -f1; }

t_case "the release job runs on a clean GitHub-hosted runner, not on a self-hosted one"
t_eq "$(grep -E '^    runs-on:' "$workflow")" "    runs-on: ubuntu-24.04"
t_lacks "$(cat "$workflow")" 'self-hosted'

t_case "each tool the job calls is installed by the job, after the check and before the build"
build=$(pos 'build the console, the analytics program, the MCP server, the store, the attachment checker and the two shadows')
for step in 'minisign at a pinned version' 'jq at the pinned version' 'rust at the pinned version'; do
  [ -n "$(pos "$step")" ] || t_bad "no step: $step"
  [ "$(pos "$step")" -gt "$(pos "$name")" ] || t_bad "$step runs before the check"
  [ "$(pos "$step")" -lt "$build" ] || t_bad "$step runs after the build"
done

# step NAME: the run script of the step NAME, as the runner gets it.
step() {
  awk -v n="      - name: $1" '$0 == n {s = 1; next}
    s && /^      - (name|uses):/ {exit}
    s == 2 {print substr($0, 11)}
    s && /^        run: \|$/ {s = 2}' "$workflow"
}

# A tree with its own versions.env, and a rustup-init that curl "downloads":
# it records its arguments and leaves a cargo that names its version.
step 'rust at the pinned version' >"$T/rust.sh"
mkdir -p "$T/tree/host" "$T/rt"
printf '%s\n' RUSTUP_VERSION=9.8.7 RUSTUP_SHA256=feed RUST_VERSION=1.100.0 >"$T/tree/host/versions.env"
cat >"$T/cargo" <<STUB
#!$T_BASH
echo "\$*" >>"$T/cargo.args"
[ "\$1" != --version ] || echo "cargo \${CARGO_VERSION:-1.100.0} (abc 2026-01-01)"
STUB
cat >"$T/rustup-init" <<STUB
#!$T_BASH
echo "\$*" >"$T/rustup-init.args"
mkdir -p "\$CARGO_HOME/bin"
cp "$T/cargo" "\$CARGO_HOME/bin/cargo"
chmod 755 "\$CARGO_HOME/bin/cargo"
STUB
# shellcheck disable=SC2016  # the stub's own arguments
t_stub curl 'while [ "$#" -gt 0 ]; do [ "$1" = -o ] && cp "$T/rustup-init" "$2"; shift; done'
rust() {
  rm -rf "$T/rt" "$T/cargo.args" "$T/rustup-init.args" "$T/calls/curl"
  mkdir -p "$T/rt"
  t_run "cd '$T/tree' && RUNNER_TEMP='$T/rt' GITHUB_ENV='$T/rt/env' GITHUB_PATH='$T/rt/path' $1 bash -eo pipefail '$T/rust.sh'"
}

t_case "rust comes from rustup at its pinned version and hash, at RUST_VERSION with the musl target"
# shellcheck disable=SC2016  # the stub's own $T
t_stub sha256sum 'cat >"$T/sha.in"'
rust ''
t_eq "$t_rc" 0
t_contains "$(t_calls curl)" 'https://static.rust-lang.org/rustup/archive/9.8.7/x86_64-unknown-linux-gnu/rustup-init'
t_eq "$(cat "$T/sha.in")" "feed  $T/rt/rustup-init"
t_contains "$(cat "$T/rustup-init.args")" '--default-toolchain 1.100.0'
t_contains "$(cat "$T/rustup-init.args")" '--target x86_64-unknown-linux-musl'
t_contains "$(cat "$T/rustup-init.args")" '--no-modify-path'
t_contains "$(cat "$T/rt/env")" "CARGO_HOME=$T/rt/cargo"
t_contains "$(cat "$T/rt/env")" "RUSTUP_HOME=$T/rt/rustup"
t_eq "$(cat "$T/rt/path")" "$T/rt/cargo/bin"

t_case "every crate of the lockfile is fetched, since the pack builds its task offline"
t_contains "$(cat "$T/cargo.args")" 'fetch --locked --manifest-path console/Cargo.toml'

t_case "a rustup-init that does not match its hash is not run"
t_stub sha256sum 'exit 1'
rust ''
t_eq "$t_rc" 1
t_eq "$(cat "$T/rustup-init.args" 2>/dev/null)" ""

t_case "a cargo of another version stops the job"
t_stub sha256sum 'exit 0'
rust 'CARGO_VERSION=1.99.0'
t_eq "$t_rc" 1
t_contains "$t_out" 'not cargo 1.100.0'
t_lacks "$(cat "$T/cargo.args")" 'fetch'

# A jq that curl "downloads": it names the version JQ_FAKE gives, or the pin.
step 'jq at the pinned version' >"$T/jq.sh"
printf '%s\n' JQ_VERSION=1.8.1 JQ_SHA256=beef >>"$T/tree/host/versions.env"
cat >"$T/fakejq" <<STUB
#!$T_BASH
echo "jq-\${JQ_FAKE:-1.8.1}"
STUB
# shellcheck disable=SC2016  # the stub's own arguments
t_stub curl 'while [ "$#" -gt 0 ]; do [ "$1" = -o ] && cp "$T/fakejq" "$2"; shift; done'
jqstep() {
  rm -rf "$T/rt" "$T/calls/curl"
  mkdir -p "$T/rt"
  t_run "cd '$T/tree' && RUNNER_TEMP='$T/rt' GITHUB_PATH='$T/rt/path' $1 bash -eo pipefail '$T/jq.sh'"
}

t_case "jq is the pinned release, checked against its hash and put on the job's path"
# shellcheck disable=SC2016  # the stub's own $T
t_stub sha256sum 'cat >"$T/sha.in"'
jqstep ''
t_eq "$t_rc" 0
t_contains "$(t_calls curl)" 'https://github.com/jqlang/jq/releases/download/jq-1.8.1/jq-linux-amd64'
t_eq "$(cat "$T/sha.in")" "beef  $T/rt/jq/jq"
t_eq "$(cat "$T/rt/path")" "$T/rt/jq"

t_case "a jq that does not match its hash is not put on the path"
t_stub sha256sum 'exit 1'
jqstep ''
t_eq "$t_rc" 1
t_eq "$(cat "$T/rt/path" 2>/dev/null)" ""

t_case "a jq of another version stops the job"
t_stub sha256sum 'exit 0'
jqstep 'JQ_FAKE=1.7.1'
t_eq "$t_rc" 1

t_done
