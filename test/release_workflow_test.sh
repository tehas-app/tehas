#!/usr/bin/env bash
# Tests for .github/workflows/release.yml: its first step, that the tagged
# commit is on main and the account that pushed the tag is an admin of the
# repository, and the clean hosted runner it runs on, which gets every tool
# from the job's own steps, the pi-rukas pin its pack step passes, and the
# notes its publish step takes. The steps' own scripts run here against
# scratch repositories and trees, with curl stubbed. Nothing here reaches the network.
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

t_case "the signing key comes from the release environment, which waits for a reviewer"
t_eq "$(grep -E '^    environment:' "$workflow")" "    environment: release"

t_case "each tool the job calls is installed by the job, after the check and before the build"
build=$(pos 'build the programs')
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
t_contains "$(t_calls curl)" '--retry 5 --retry-all-errors --retry-delay 5 -o'
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

t_case "the build takes no lab marker, rustflags or release profile setting from the runner"
step 'build the programs' >"$T/build.sh"
# A cargo that writes down what each build is given, and leaves the programs.
# shellcheck disable=SC2016  # the stub's own variables
t_stub cargo '[ "$1" = build ] || exit 0
{ env | grep -E "^([A-Z0-9_]*RUSTFLAGS|CARGO_PROFILE_[A-Z0-9_]*|TEHAS_LAB_BUILD)=" | LC_ALL=C sort
  echo "encoded rustflags ${CARGO_ENCODED_RUSTFLAGS+set}"; } >>"$T/build.env"
dir="" tr="" ps=()
while [ "$#" -gt 0 ]; do
  case $1 in --target-dir) dir=$2; shift ;; --target) tr=$2/; shift ;; -p) ps+=("$2"); shift ;; esac
  shift
done
mkdir -p "$dir/${tr}release"
for p in "${ps[@]}"; do : >"$dir/${tr}release/$p"; done'
t_stub rustup 'exit 0'
t_stub od 'echo " 3e 00"'
rm -rf "$T/rt" "$T/build.env"
mkdir -p "$T/rt"
t_run "cd '$T/tree' && RUNNER_TEMP='$T/rt' TEHAS_LAB_BUILD=on RUSTFLAGS='-C debug-assertions' \
  CARGO_ENCODED_RUSTFLAGS=-Cdebug-assertions CARGO_BUILD_RUSTFLAGS='-C debug-assertions' \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS='-C debug-assertions' CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true \
  CARGO_PROFILE_RELEASE_PACKAGE_STORE_DEBUG_ASSERTIONS=true bash -eo pipefail '$T/build.sh'"
t_eq "$t_rc" 0
t_eq "$(cat "$T/build.env")" "CARGO_ENCODED_RUSTFLAGS=
encoded rustflags set
CARGO_ENCODED_RUSTFLAGS=
encoded rustflags set"

t_case "a prerelease tag starts the release, and is published as a prerelease, never the latest"
on=$(sed -n '/^on:/,/^permissions:/p' "$workflow")
for stage in alpha beta rc; do
  t_contains "$on" "- 'v[0-9]+.[0-9]+.[0-9]+-$stage.[0-9]+'"
done
t_contains "$on" "- 'v[0-9]+.[0-9]+.[0-9]+'"
# The tag check of the build step and the marking of the publish step, run
# for each kind of tag.
check=$(grep -F 'not a release tag' "$workflow" | sed 's/^ *//')
# shellcheck disable=SC2016  # the workflow's own text, not to expand here
mark=$(grep -E '^ *(pre=false latest=true|if \[\[ \$TAG == )' "$workflow" | sed 's/^ *//')
t_eq "$(wc -l <<<"$mark")" 2
for tag in v1.0.0 v1.0.0-alpha.1 v1.0.0-beta.2 v1.0.0-rc.3 v1.0.0-gamma.1 v1.0.0-rc1 v1.0; do
  # shellcheck disable=SC2016  # the workflow's own text, not to expand here
  printf '%s\n' "TAG=$tag" "$check" "$mark" 'echo "$pre $latest"' >"$T/mark.sh"
  t_run "bash '$T/mark.sh'"
  case $tag in
    v1.0.0) t_eq "$t_rc $t_out" "0 false true" ;;
    *-alpha.* | *-beta.* | *-rc.*) t_eq "$t_rc $t_out" "0 true false" ;;
    *) t_eq "$t_rc" 1 ;;
  esac
done
# shellcheck disable=SC2016  # the workflow's own text, not to expand here
t_contains "$(cat "$workflow")" 'prerelease: $pre}'
# shellcheck disable=SC2016  # the workflow's own text, not to expand here
t_contains "$(cat "$workflow")" '\"make_latest\": \"$latest\"'

# The publish step in a tree of its own: scripts/release-notes.sh, which execs
# the xtask of this checkout, and a CHANGELOG.md whose Unreleased section is
# the only one with text. curl records what it is asked and answers a release.
t_xtask || t_bad "no xtask: $t_xtask_why"
step 'publish the release' >"$T/publish.sh"
mkdir -p "$T/pub/scripts" "$T/pub/dist"
cp "$T_ROOT/scripts/release-notes.sh" "$T/pub/scripts/"
printf '%s\n' '# Changelog' '' '## Unreleased' '' '### Changes' '' '- every change since the last release' '' \
  '## v1.0.0-rc.1 (2026-11-19)' '' '### Changes' '' '- the candidate' >"$T/pub/CHANGELOG.md"
# shellcheck disable=SC2016  # the stub's own variables
publish_curl='echo "$*" >>"$T/curl.args"; echo "{\"id\": 7, \"html_url\": \"https://example.invalid/r\"}"'
publish() {
  rm -f "$T/curl.args"
  t_stub curl "$publish_curl"
  t_run "cd '$T/pub' && TAG=$1 DIST='$T/pub/dist' GH_TOKEN=x REPO=acme/widget bash -eo pipefail '$T/publish.sh'"
}

t_case "a release without a section of its own is not published with Unreleased in its place"
t_lacks "$(cat "$T/publish.sh")" 'release-notes.sh Unreleased'
publish v1.0.0
t_eq "$t_rc" 1
t_contains "$t_out" "no section for v1.0.0"
t_contains "$t_out" "no release notes for v1.0.0, as said above; nothing was published"
t_eq "$(cat "$T/curl.args" 2>/dev/null)" ""

t_case "a tag with a section of its own is published with it"
publish v1.0.0-rc.1
t_eq "$t_rc" 0
t_contains "$(cat "$T/curl.args")" '"body": "### Changes\n\n- the candidate"'
t_lacks "$(cat "$T/curl.args")" "every change since the last release"

# The pack step in a tree of its own: a console that answers `rukas target`
# and a pack-release.sh that records what it is given.
step 'pack' >"$T/pack.sh"
mkdir -p "$T/pk/scripts" "$T/pk/host" "$T/pk/rt/programs" "$T/pk/dist"
printf '%s\n' PI_RUKAS_REPO=https://example.invalid/pi-rukas.git PI_RUKAS_FOLLOW=release:0 >"$T/pk/host/versions.env"
pin=$(printf 'c%.0s' {1..40})
cat >"$T/pk/rt/programs/console" <<STUB
#!$T_BASH
echo "\$*" >>"$T/console.args"
[ -z "\${CONSOLE_FAILS:-}" ] || exit 1
echo "$pin release v0.14.94"
STUB
cat >"$T/pk/scripts/pack-release.sh" <<STUB
#!$T_BASH
echo "\$*" >"$T/pack.args"
STUB
chmod 755 "$T/pk/rt/programs/console" "$T/pk/scripts/pack-release.sh"
packstep() {
  rm -f "$T/console.args" "$T/pack.args"
  t_stub git 'echo 1111111111111111111111111111111111111111'
  t_stub sha256sum 'exit 0'
  t_run "cd '$T/pk' && TAG=v1.2.3 DIST='$T/pk/dist' RUNNER_TEMP='$T/pk/rt' $1 bash -eo pipefail '$T/pack.sh'"
}

t_case "a release pins the pi-rukas commit the tag's own PI_RUKAS_FOLLOW names, and no digest"
[ "$(pos pack)" -lt "$(pos 'publish the release')" ] || t_bad "the pack runs after the publish"
packstep ''
t_eq "$t_rc" 0
t_eq "$(cat "$T/console.args")" "rukas target release:0 https://example.invalid/pi-rukas.git"
t_contains "$(cat "$T/pack.args")" "--rukas-commit $pin"
t_lacks "$(cat "$T/pack.args")" "--rukas-digest"
t_contains "$t_out" "pi-rukas pinned at $pin, release v0.14.94"

t_case "a follow upstream does not answer stops the release before anything is packed"
packstep 'CONSOLE_FAILS=1'
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=release:0 in https://example.invalid/pi-rukas.git; nothing was published"
t_eq "$(cat "$T/pack.args" 2>/dev/null)" ""

t_done
