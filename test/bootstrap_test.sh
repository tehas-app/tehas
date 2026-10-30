#!/usr/bin/env bash
# Tests for bootstrap.sh, against stub tools that print a version: what
# `check` reports, that `user` skips what is present, how an npm pin is checked
# and installed, the Playwright pin the sandbox image builds from, and the
# platform guard. No download and no install happens here.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

bootstrap="$T_ROOT/bootstrap.sh"
# shellcheck source=host/versions.env
. "$T_ROOT/host/versions.env"
# check_pins reads the pinned trees with jq, so the stub has to be the real one.
jq_real=$(command -v jq)

printf 'ID=ubuntu\nVERSION_ID="26.04"\n' >"$T/os-ubuntu"
printf 'ID=debian\nVERSION_ID="13"\n' >"$T/os-debian"
# Only the stubs and the base system: tools installed for the person running
# the tests must not be found.
T_PATH=$T/bin:/usr/bin:/bin
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# The commit of pi-rukas's newest release tag, as git ls-remote answers it.
rukas_head=$(printf 'e%.0s' {1..40})

# Every tool at its expected version, the npm ones installed from their pinned
# tree: what a host has after one run of `user` with nothing to change.
all_present() {
  t_stub node "echo v$NODE_VERSION"
  t_stub gh "echo 'gh version $GH_VERSION (2026-07-31)'"
  t_stub bun "echo $BUN_VERSION"
  t_stub pi "echo $PI_VERSION"
  t_stub ctx7 "echo $CTX7_VERSION"
  t_stub parallel-cli "echo 'parallel-cli, version $PARALLEL_CLI_VERSION'"
  t_stub rustc "echo 'rustc $RUST_VERSION (abc 2026-09-28)'"
  t_stub rustup 'printf "%s\n" x86_64-unknown-linux-gnu x86_64-unknown-linux-musl'
  t_stub vipune "echo 'vipune $VIPUNE_VERSION'"
  t_stub oo "echo 'oo $OO_VERSION'"
  t_stub codebase-memory-mcp "echo 'codebase-memory-mcp $CBM_VERSION'"
  t_stub git "case \"\$*\" in *ls-remote*) printf '%s\trefs/tags/v0.12.81\n' $rukas_head ;; *rev-parse*) echo $rukas_head ;; *) echo 'git version 2.53.0' ;; esac"
  t_stub docker 'echo "Docker version 29.1.3"'
  t_stub loginctl 'echo yes'
  t_stub curl 'echo "curl 8.0.0"'
  t_stub uname 'echo x86_64'
  for tool in jq openssl flock minisign shellcheck unzip; do t_stub "$tool" 'echo "1.2.3"'; done
  t_stub jq "[ \"\${1:-}\" = --version ] && { echo jq-$JQ_VERSION; exit 0; }; exec $jq_real \"\$@\""
  mkdir -p "$T/home/.pi/agent/extensions"
  ln -sfn "$T" "$T/home/.pi/agent/extensions/pi-rukas"
  pin_trees_in_place
}

# A host that has the tools but not the trees: what an earlier bootstrap left,
# before npm tools were pinned.
pin_trees_absent() {
  rm -rf "$T/home/tehas/tools/npm" "$T/home/.local/bin"
}

# The pinned trees, installed the way bootstrap.sh installs them.
pin_trees_in_place() {
  local name
  for name in pi ctx7 parallel-web-cli; do
    mkdir -p "$T/home/tehas/tools/npm/$name"
    install -m 644 "$T_ROOT/host/npm/$name/package.json" \
      "$T_ROOT/host/npm/$name/package-lock.json" "$T/home/tehas/tools/npm/$name/"
  done
}

# An npm whose `ci` lays down the tree it was given: its one dependency, the
# command that package names, and that command linked in node_modules/.bin. The
# linked command answers with the version the tool stub answers with, so a step
# can check the version through the link.
t_stub_npm_ci() {
  # shellcheck disable=SC2016  # the body is expanded when the stub runs
  t_stub npm 'case "$1" in
ci)
  dep=$(jq -r ".dependencies | keys | .[0]" "$PWD/package.json")
  case "$dep" in
    @earendil-works/pi-coding-agent) bin=pi ;;
    parallel-web-cli) bin=parallel-cli ;;
    *) bin=$dep ;;
  esac
  mkdir -p "$PWD/node_modules/$dep" "$PWD/node_modules/.bin"
  pkgjson=$PWD/node_modules/$dep/package.json
  printf "{\"name\":\"%s\",\"bin\":{\"%s\":\"cli.js\"}}\n" "$dep" "$bin" >"$pkgjson"
  cmd=$PWD/node_modules/.bin/$bin
  printf "#!/bin/sh\nexec \"$T/bin/$bin\" \"\$@\"\n" >"$cmd"
  chmod 755 "$cmd"
  ;;
esac'
}

t_case "check passes when every tool is at its version"
all_present
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_lacks "$t_out" "MISSING"
t_lacks "$t_out" "DIFFERS"
t_contains "$t_out" "node"
t_contains "$t_out" "$rukas_head"

t_case "check changes nothing"
before=$(find "$T/home" | sort)
t_run "$bootstrap check"
t_eq "$(find "$T/home" | sort)" "$before"

# The PATH a run of bootstrap.sh gets is the one lib/paths.sh names, the same
# list install.sh uses: a tool under one of those directories is found.
t_case "check looks in the tool directories lib/paths.sh names"
all_present
t_stub rustc "printf '%s\n' \"\$PATH\" >\"\$T/rustc-path\"
printf '%s\n' \"\$TEHAS_HOME \$NPM_CONFIG_PREFIX \$CARGO_HOME \$RUSTUP_HOME\" >\"\$T/rustc-homes\"
echo 'rustc $RUST_VERSION'"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_eq "$(cat "$T/rustc-path")" "$T/home/tehas/bin:$T/home/tehas/tools/node/bin:$T/home/.local/bin:$T/home/.bun/bin:$T/home/.cargo/bin:$T_PATH"
t_eq "$(cat "$T/rustc-homes")" "$T/home/tehas $T/home/.local $T/home/.cargo $T/home/.rustup"

t_case "check reports a tool at another version and exits 1"
t_stub gh 'echo "gh version 2.0.0 (2022-01-01)"'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^gh ' <<<"$t_out")" "2.0.0"
t_contains "$(grep '^gh ' <<<"$t_out")" "$GH_VERSION"
t_contains "$(grep '^gh ' <<<"$t_out")" "DIFFERS"

t_case "check reports an npm tool at its version but not on the pinned tree, as user would replace it"
all_present
printf ' ' >>"$T/home/tehas/tools/npm/ctx7/package-lock.json"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^ctx7 ' <<<"$t_out")" "not the pinned tree"
t_contains "$(grep '^ctx7 ' <<<"$t_out")" "DIFFERS"
t_lacks "$(grep '^pi ' <<<"$t_out")" "DIFFERS"
pin_trees_absent
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^parallel-cli ' <<<"$t_out")" "not the pinned tree"
pin_trees_in_place

t_case "check reports a missing tool and exits 1"
all_present
rm "$T/bin/vipune"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^vipune ' <<<"$t_out")" "MISSING"

t_case "rust without the static target the MCP server is built for is reported missing"
all_present
t_stub rustup 'echo x86_64-unknown-linux-gnu'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^rust ' <<<"$t_out")" "MISSING"

t_case "user adds the static target to the pinned toolchain"
all_present
t_stub rustup 'case "$*" in
  "target add"*) touch "$T/musl" ;;
  "target list"*) echo x86_64-unknown-linux-gnu; [ ! -f "$T/musl" ] || echo x86_64-unknown-linux-musl ;;
esac'
t_stub npm
t_stub cargo
t_stub tar
rm -f "$T"/calls/* "$T/musl"
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$(t_calls rustup)" "target add x86_64-unknown-linux-musl --toolchain $RUST_VERSION"
t_run "$bootstrap check"
t_lacks "$(grep '^rust ' <<<"$t_out")" "MISSING"
all_present

t_case "pi-rukas at another commit is reported"
all_present
t_stub git 'case "$*" in *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo "git version 2.53.0" ;; esac'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "DIFFERS"

t_case "pi-rukas cloned but never installed counts as missing"
all_present
rm "$T/home/.pi/agent/extensions/pi-rukas"
t_run "$bootstrap check"
t_contains "$(grep '^pi-rukas ' <<<"$t_out")" "MISSING"

t_case "docker that needs sudo is reported"
all_present
t_stub docker 'case "$1" in info) exit 1 ;; *) echo "Docker version 29.1.3" ;; esac'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^docker without sudo' <<<"$t_out")" "MISSING"

t_case "lingering switched off is reported"
all_present
t_stub loginctl 'echo no'
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^lingering' <<<"$t_out")" "MISSING"

t_case "user with everything present downloads and installs nothing"
all_present
t_stub npm
t_stub cargo
t_stub tar
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: user part done"
t_lacks "$t_out" "installing"
t_eq "$(t_calls curl)" ""
t_eq "$(t_calls npm)" ""
t_eq "$(t_calls cargo)" ""

t_case "user stops when a just-installed tool is still at the wrong version"
all_present
t_stub ctx7 'echo 0.0.1'
t_stub_npm_ci
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "installing ctx7 $CTX7_VERSION (replacing 0.0.1)"
t_contains "$(t_calls npm)" "ci --no-fund --no-audit --ignore-scripts"
t_contains "$t_out" "ctx7 is 0.0.1 after installing, expected $CTX7_VERSION"

# --- npm pins: the tree beside versions.env --------------------------------
# A scratch copy of bootstrap.sh and what it reads, so a case can break a pin
# without touching this checkout.
pin_checkout() { # -> the directory holding the copy
  local dir=$T/checkout
  rm -rf "$dir"
  mkdir -p "$dir/host" "$dir/lib"
  cp "$bootstrap" "$dir/bootstrap.sh"
  cp "$T_ROOT/lib/rukas.sh" "$dir/lib/rukas.sh"
  cp "$T_ROOT/lib/paths.sh" "$dir/lib/paths.sh"
  cp "$T_ROOT/host/versions.env" "$dir/host/versions.env"
  cp -R "$T_ROOT/host/npm" "$dir/host/npm"
  echo "$dir"
}

t_case "the npm pins in this checkout match versions.env"
all_present
dir=$(pin_checkout)
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 0
t_lacks "$t_out" "npm pin"
t_eq "$(t_calls npm)" ""

t_case "a lockfile that is not the one recorded in versions.env stops user before anything is installed"
all_present
dir=$(pin_checkout)
printf ' ' >>"$dir/host/npm/ctx7/package-lock.json"
rm -rf "$T/home/tehas"
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 1
t_contains "$t_out" "checksum mismatch for the npm pin ctx7"
t_eq "$(t_calls npm)$(t_calls curl)" ""
t_eq "$([ -e "$T/home/tehas/tools/npm/ctx7" ] && echo there)" ""

t_case "a version that the pinned tree does not name stops user before anything is installed"
all_present
dir=$(pin_checkout)
sed 's/^PI_VERSION=.*/PI_VERSION=9.9.9/' "$dir/host/versions.env" >"$dir/host/v2"
mv "$dir/host/v2" "$dir/host/versions.env"
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 1
t_contains "$t_out" "the npm pin pi says $PI_VERSION/$PI_VERSION/$PI_VERSION for @earendil-works/pi-coding-agent; host/versions.env says 9.9.9"
t_eq "$(t_calls npm)$(t_calls curl)" ""

# The lockfile and the version are one pin: a tree regenerated on its own is a
# pin nobody chose, so it is refused even though npm ci would have installed it.
t_case "a pinned tree that names another version than its lockfile is refused"
all_present
dir=$(pin_checkout)
sed 's/"ctx7": "0\.5\.14"/"ctx7": "0.5.13"/' "$dir/host/npm/ctx7/package.json" >"$dir/p2"
mv "$dir/p2" "$dir/host/npm/ctx7/package.json"
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 1
t_contains "$t_out" "the npm pin ctx7 says 0.5.13/$CTX7_VERSION/$CTX7_VERSION for ctx7; host/versions.env says $CTX7_VERSION"
t_eq "$(t_calls npm)$(t_calls curl)" ""

t_case "a pin missing from the checkout is refused"
all_present
dir=$(pin_checkout)
rm -rf "$dir/host/npm/parallel-web-cli"
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 1
t_contains "$t_out" "host/npm/parallel-web-cli is missing from this checkout: cannot install parallel-web-cli $PARALLEL_CLI_VERSION"
t_eq "$(t_calls npm)$(t_calls curl)" ""

# The tool directories come from lib/paths.sh. A copy without it names the file
# that is gone, rather than reporting the host's tools as missing ones.
t_case "a checkout without lib/paths.sh names the missing file"
all_present
dir=$(pin_checkout)
rm -f "$dir/lib/paths.sh"
t_run "$dir/bootstrap.sh check"
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap: lib/paths.sh is missing from this checkout"
t_lacks "$t_out" "MISSING"

t_case "an npm tool is installed from the pinned tree, not from a registry range"
all_present
t_stub pi 'echo 0.0.1'
t_stub_npm_ci
# What an unpinned bootstrap left in npm's global tree is dropped.
mkdir -p "$T/home/.local/lib/node_modules/@earendil-works/pi-coding-agent"
printf '{}\n' >"$T/home/.local/lib/node_modules/@earendil-works/pi-coding-agent/package.json"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$(t_calls npm)" "ci --no-fund --no-audit --ignore-scripts"
t_lacks "$(t_calls npm)" "pi@"
t_eq "$(find "$T/home/tehas/tools/npm/pi" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "package-lock.json package.json "
t_contains "$t_out" "pi is 0.0.1 after installing, expected $PI_VERSION"
# The command on PATH comes from the pinned tree, not from npm's global tree.
t_eq "$(readlink "$T/home/.local/bin/pi")" "$T/home/tehas/tools/npm/pi/node_modules/.bin/pi"
t_eq "$([ -e "$T/home/.local/lib/node_modules/@earendil-works/pi-coding-agent" ] && echo there)" ""

# --- what counts as present -------------------------------------------------
# The version alone is not the pin: a host that got these tools from an earlier
# bootstrap that installed them into npm's global tree has to get the tree.
t_case "a host that already has an npm tool at its version still gets the pinned tree"
all_present
pin_trees_absent
t_stub_npm_ci
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "reinstalling pi $PI_VERSION from host/npm/pi: $T/home/tehas/tools/npm/pi is not the pinned tree"
t_contains "$(t_calls npm)" "ci --no-fund --no-audit --ignore-scripts"
t_eq "$(find "$T/home/tehas/tools/npm" -maxdepth 1 -mindepth 1 -type d -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "ctx7 parallel-web-cli pi "
t_eq "$(readlink "$T/home/.local/bin/ctx7")" "$T/home/tehas/tools/npm/ctx7/node_modules/.bin/ctx7"
t_eq "$(readlink "$T/home/.local/bin/parallel-cli")" "$T/home/tehas/tools/npm/parallel-web-cli/node_modules/.bin/parallel-cli"

t_case "an installed tree that is not the pinned one is reinstalled"
all_present
printf ' ' >>"$T/home/tehas/tools/npm/ctx7/package-lock.json"
t_stub_npm_ci
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_contains "$t_out" "reinstalling ctx7 $CTX7_VERSION from host/npm/ctx7"
t_lacks "$t_out" "reinstalling pi"
t_contains "$(t_calls npm)" "ci --no-fund --no-audit --ignore-scripts"

# --- a pin that does not provide its command --------------------------------
# The tree an earlier bootstrap left in npm's global tree is removed only once
# the pinned commands are linked: a broken pin must not leave a link to nothing.
t_case "a pinned tree that provides no command stops before the old global install is removed"
all_present
pin_trees_absent
global=$T/home/.local/lib/node_modules/@earendil-works/pi-coding-agent
mkdir -p "$global" "$T/home/.local/bin"
printf '{"name":"@earendil-works/pi-coding-agent"}\n' >"$global/package.json"
ln -sfn "$global/cli.js" "$T/home/.local/bin/pi"
t_stub npm
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "npm ci left no package.json for @earendil-works/pi-coding-agent"
t_eq "$([ -e "$global/package.json" ] && echo there)" "there"
t_eq "$(readlink "$T/home/.local/bin/pi")" "$global/cli.js"

t_case "a command name that would write outside the bin directory is refused"
all_present
pin_trees_absent
t_stub npm 'case "$1" in
ci)
  dep=$(jq -r ".dependencies | keys | .[0]" "$PWD/package.json")
  mkdir -p "$PWD/node_modules/$dep"
  printf "{\"bin\":{\"../evil\":\"cli.js\"}}" >"$PWD/node_modules/$dep/package.json"
  ;;
esac'
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "@earendil-works/pi-coding-agent names an unusable command: ../evil"
t_eq "$(find "$T/home/.local/bin" -mindepth 1 -maxdepth 1 -printf '%f\n' | tr '\n' ' ')" ""

t_case "a command name that names a directory instead of a file is refused"
all_present
pin_trees_absent
t_stub npm 'case "$1" in
ci)
  dep=$(jq -r ".dependencies | keys | .[0]" "$PWD/package.json")
  mkdir -p "$PWD/node_modules/$dep"
  printf "{\"bin\":{\"..\":\"cli.js\"}}" >"$PWD/node_modules/$dep/package.json"
  ;;
esac'
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "@earendil-works/pi-coding-agent names an unusable command: .."
t_eq "$(find "$T/home/.local/bin" -mindepth 1 -maxdepth 1 -printf '%f\n' | tr '\n' ' ')" ""

# --- an install that fails ---------------------------------------------------
# npm ci is a network step. The tree it replaces holds the commands PATH points
# at, so a run that fails leaves that tree where it was.
t_case "an npm install that fails leaves the tree and its commands that worked before"
all_present
pin_trees_absent
t_stub_npm_ci
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 0
t_stub pi 'echo 0.0.1'
t_stub npm 'echo "npm ERR! a package did not answer" >&2; exit 1'
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "npm ci installed nothing for @earendil-works/pi-coding-agent from $T_ROOT/host/npm/pi"
t_contains "$(t_calls npm)" "ci --no-fund --no-audit --ignore-scripts"
t_eq "$([ -x "$T/home/tehas/tools/npm/pi/node_modules/.bin/pi" ] && echo there)" "there"
t_eq "$(readlink "$T/home/.local/bin/pi")" "$T/home/tehas/tools/npm/pi/node_modules/.bin/pi"
t_eq "$(sha256sum "$T/home/tehas/tools/npm/pi/package-lock.json" | cut -d ' ' -f 1)" "$PI_LOCK_SHA256"
# No staging directory is left beside the installed trees.
t_eq "$(find "$T/home/tehas/tools/npm" -maxdepth 1 -mindepth 1 -type d -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "ctx7 parallel-web-cli pi "

t_case "a pin missing its lockfile hash in versions.env is refused"
all_present
dir=$(pin_checkout)
grep -v '^PI_LOCK_SHA256=' "$dir/host/versions.env" >"$dir/v2"
mv "$dir/v2" "$dir/host/versions.env"
rm -f "$T"/calls/*
t_run "$dir/bootstrap.sh user"
t_eq "$t_rc" 1
t_contains "$t_out" "host/versions.env has no PI_LOCK_SHA256: an npm pin is its version and its lockfile hash together"
t_eq "$(t_calls npm)$(t_calls curl)" ""

# --- the Playwright pin the sandbox image builds from -----------------------
# The build stops when the tree beside sandbox/Dockerfile is not the one
# versions.env records, but the gate never builds an image. The agreement is
# checked here, as the three host/npm pins are.
pw=$T_ROOT/host/sandbox/npm/playwright

t_case "the Playwright pin in this checkout matches versions.env"
t_eq "$(grep -cE '^PLAYWRIGHT_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^PLAYWRIGHT_LOCK_SHA256=[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1
t_eq "$(sha256sum "$pw/package-lock.json" | cut -d ' ' -f 1)" "$PLAYWRIGHT_LOCK_SHA256"
t_eq "$(jq -r '.dependencies.playwright' "$pw/package.json")" "$PLAYWRIGHT_VERSION"
t_eq "$(jq -r '.packages[""].dependencies.playwright,
  .packages["node_modules/playwright"].version' "$pw/package-lock.json" | tr '\n' ' ')" \
  "$PLAYWRIGHT_VERSION $PLAYWRIGHT_VERSION "

t_case "the sandbox build passes the Playwright pin and the image checks it before npm ci"
t_contains "$(cat "$T_ROOT/host/sandbox/build.sh")" '--build-arg PLAYWRIGHT_VERSION="$PLAYWRIGHT_VERSION"'
t_contains "$(cat "$T_ROOT/host/sandbox/build.sh")" '--build-arg PLAYWRIGHT_LOCK_SHA256="$PLAYWRIGHT_LOCK_SHA256"'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'COPY npm/playwright/package.json npm/playwright/package-lock.json /opt/pin/playwright/'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'echo "${PLAYWRIGHT_LOCK_SHA256}  package-lock.json" | sha256sum -c -'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'npm ci --ignore-scripts --no-fund --no-audit'
# Nothing in the recipe takes Playwright from a registry by name and version.
t_lacks "$(cat "$T_ROOT/host/sandbox/Dockerfile")" "playwright@"

# The stub git of the pi-rukas cases: it answers for the address in
# PI_RUKAS_REPO with $rukas_head as the commit of the newest release tag, and
# with $rukas_stub_behind as the commit the clone is at, so a run of `user` has
# something to move it to.
behind_head() {
  t_stub git "case \"\$*\" in
  *ls-remote*) printf '%s\trefs/tags/v0.12.81\n' $rukas_head ;;
  *rev-parse*) echo $rukas_stub_behind ;;
  *) echo 'git version 2.53.0' ;;
esac"
}
rukas_stub_behind=$(printf '0%.0s' {1..40})

t_case "an existing pi-rukas clone is fetched from the repository in versions.env"
all_present
behind_head
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$(t_calls git)" "remote set-url origin $PI_RUKAS_REPO"

t_case "pi-rukas behind its newest release is moved to it"
t_contains "$(t_calls git)" "checkout -q $rukas_head"

# The clone is only a copy of upstream, so its fetch takes the tags by force,
# which is what lets a release tag upstream re-points be taken at all: without
# --force git refuses it as one that would clobber a tag the clone has, and the
# run stopped right there and said nothing, because the fetch asked for quiet.
t_case "the pi-rukas fetch forces the tags it takes"
t_contains "$(t_calls git)" "fetch --tags --force origin"

# A fetch that cannot be made says so, with git's own words: this is the run
# that used to leave nothing behind after the line naming the commit it wanted.
t_case "a pi-rukas fetch that fails names the step and git's reason"
all_present
behind_head
t_stub git "case \"\$*\" in
  *fetch*) echo 'fatal: the remote did not answer' >&2; exit 128 ;;
  *ls-remote*) printf '%s\trefs/tags/v0.12.81\n' $rukas_head ;;
  *rev-parse*) echo $rukas_stub_behind ;;
  *) echo 'git version 2.53.0' ;;
esac"
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "git fetch --tags --force failed for the pi-rukas clone"
t_contains "$t_out" "the remote did not answer"
rm -rf "$T/home/tehas/repos"
behind_head

t_case "check reports a pi-rukas behind its newest release"
all_present
behind_head
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$t_out" "$rukas_head"
t_contains "$t_out" "DIFFERS"

t_case "without a commit to follow the user part stops before pi-rukas"
t_stub git 'case "$*" in *ls-remote*) exit 2 ;; *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo "git version 2.53.0" ;; esac'
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "no pi-rukas commit for PI_RUKAS_FOLLOW=$PI_RUKAS_FOLLOW in $PI_RUKAS_REPO"
t_lacks "$(t_calls git)" "fetch"
behind_head

t_case "pi-rukas's installer is not given the sandbox image's name"
# It would pull that name, fail, and build upstream's image over the tag.
printf '#!/bin/sh\necho "image=${PI_ENSEMBLE_IMAGE-unset}" >"$T/installer-env"\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
T_ENV+=(PI_ENSEMBLE_IMAGE=tehas/pi-rukas:uid1000)
t_run "$bootstrap user"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_eq "$(cat "$T/installer-env")" "image=unset"
rm -rf "$T/home/tehas/repos/pi-rukas"

t_case "check reports a jq at another version than versions.env pins"
all_present
t_stub jq "[ \"\${1:-}\" = --version ] && { echo jq-1.7.1; exit 0; }; exec $jq_real \"\$@\""
t_run "$bootstrap check"
t_eq "$t_rc" 1
t_contains "$(grep '^jq ' <<<"$t_out")" "1.7.1"
t_contains "$(grep '^jq ' <<<"$t_out")" "$JQ_VERSION"
t_contains "$(grep '^jq ' <<<"$t_out")" "DIFFERS"

t_case "user fetches the pinned jq from its release and refuses one that is not it"
all_present
t_stub jq "[ \"\${1:-}\" = --version ] && { echo jq-1.7.1; exit 0; }; exec $jq_real \"\$@\""
t_stub curl 'while [ $# -gt 0 ]; do [ "$1" = -o ] && { echo tampered >"$2"; exit 0; }; shift; done'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap: installing jq $JQ_VERSION (replacing 1.7.1)"
t_contains "$(t_calls curl)" "https://github.com/jqlang/jq/releases/download/jq-$JQ_VERSION/jq-linux-amd64"
t_contains "$t_out" "checksum mismatch for jq"
t_eq "$([ -e "$T/home/tehas/bin/jq" ] && echo installed)" ""
t_stub curl 'echo "curl 8.0.0"'

t_case "the jq pin in this checkout is a version and a sum, and the image and CI take it"
t_eq "$(grep -cE '^JQ_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^JQ_SHA256=[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1
t_contains "$(cat "$T_ROOT/host/sandbox/build.sh")" '--build-arg JQ_VERSION="$JQ_VERSION"'
t_contains "$(cat "$T_ROOT/host/sandbox/build.sh")" '--build-arg JQ_SHA256="$JQ_SHA256"'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'echo "${JQ_SHA256}  /tmp/jq" | sha256sum -c -'
t_contains "$(cat "$T_ROOT/host/sandbox/Dockerfile")" 'if [ "$(jq --version)" != "jq-${JQ_VERSION}" ]'
t_contains "$(cat "$T_ROOT/.github/workflows/verify.yml")" 'echo "$JQ_SHA256  $RUNNER_TEMP/jq/jq" | sha256sum -c -'
t_contains "$(cat "$T_ROOT/.github/workflows/verify.yml")" 'echo "$RUNNER_TEMP/jq" >> "$GITHUB_PATH"'

t_case "a download that does not match its checksum is refused"
all_present
t_stub gh 'echo "gh version 1.0.0"'
t_stub curl 'while [ $# -gt 0 ]; do [ "$1" = -o ] && { echo tampered >"$2"; exit 0; }; shift; done'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "checksum mismatch for gh.tar.gz"
t_eq "$([ -e "$T/home/tehas/bin/gh" ] && echo installed)" ""
t_stub curl 'echo "curl 8.0.0"'

t_case "user refuses an unsupported system before touching anything"
all_present
rm -rf "$T/home/tehas"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-debian" PATH="$T_PATH")
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "this host is debian 13 on x86_64; bootstrap.sh supports Ubuntu 26.04 on x86_64 only"
t_eq "$([ -e "$T/home/tehas" ] && echo created)" ""

t_case "another architecture is refused"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
t_stub uname 'echo aarch64'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "on aarch64"
t_stub uname 'echo x86_64'

t_case "root refuses to run without root"
t_run "$bootstrap root someone"
t_eq "$t_rc" 1
t_contains "$t_out" "must run as root"

# --- a tag upstream moved ---------------------------------------------------
# The two cases above check what the script asks of git and what it says when
# git refuses. These check the thing only git decides: they use a remote that
# is a real repository, a clone of it standing for the host, and upstream having
# re-pointed the release tag in between. The question is which fetch gets the
# clone onto the commit the release names now.

# The commits and tags these three cases make in the scratch directory need a
# name of their own and a branch of the same name, so the test reads the same
# whatever the person who runs it has set for themselves.
# shellcheck disable=SC2329  # the three cases below call it
t_git() {
  git -c user.name=t -c user.email=t@example.com -c init.defaultBranch=main "$@"
}

# A bare repository standing for pi-rukas's, its upstream, and the host's clone
# made from it before upstream moved the tag. Two commits land on top of what
# the clone was made on, and the annotated release tag v0.12.80 is pointed at
# the first of them while v0.12.81 names the second. The clone therefore holds
# the tag at a commit upstream no longer calls a release, and the newest release
# is a tag the clone has never seen. Everything is under $T/rukas-moved.
# shellcheck disable=SC2329  # the three cases below call it
rukas_moved() {
  local r=$T/rukas-moved ref
  rm -rf "$r"
  mkdir -p "$r"
  t_git init --quiet --bare "$r/origin.git"
  t_git clone --quiet "$r/origin.git" "$r/up" 2>/dev/null
  printf 'first\n' >"$r/up/one"
  t_git -C "$r/up" add one
  t_git -C "$r/up" commit --quiet -m first
  t_git -C "$r/up" tag -a -m 'Release v0.12.80 - first' v0.12.80
  t_git -C "$r/up" push --quiet origin main v0.12.80
  t_git clone --quiet "$r/origin.git" "$r/host" 2>/dev/null
  t_git -C "$r/up" checkout --quiet -b release
  printf 'second\n' >"$r/up/two"
  t_git -C "$r/up" add two
  t_git -C "$r/up" commit --quiet -m second
  t_git -C "$r/up" tag --force -a -m 'Release v0.12.80 - moved' v0.12.80
  printf 'third\n' >"$r/up/three"
  t_git -C "$r/up" add three
  t_git -C "$r/up" commit --quiet -m third
  t_git -C "$r/up" tag v0.12.81
  t_git -C "$r/up" push --quiet --force origin release
  # The remote answers as upstream does now: its head, and the two tags as
  # upstream has them. The host's clone keeps what it read before the move.
  t_git -C "$r/origin.git" update-ref refs/heads/main "$(t_git -C "$r/up" rev-parse HEAD)"
  for ref in refs/tags/v0.12.80 refs/tags/v0.12.81; do
    t_git -C "$r/origin.git" fetch --quiet --force "$r/up" "+$ref:$ref"
  done
}

t_case "a clone of a remote whose tag moved holds the tag upstream moved"
rukas_moved
# The clone was made from the one commit the remote had then, and still has the
# tag where that left it, with no sign of the release that came after.
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)" "$(t_git -C "$T/rukas-moved/up" rev-parse "HEAD~2^{commit}")"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.12.80^{commit}")" "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)"
t_eq "$(t_git -C "$T/rukas-moved/up" rev-parse "v0.12.80^{commit}")" "$(t_git -C "$T/rukas-moved/up" rev-parse HEAD~1)"
t_lacks "$(t_git -C "$T/rukas-moved/host" tag --list)" v0.12.81

# The fetch the script used, the same arguments bar --force: git refuses the
# moved tag and exits 1, which is the failure that stopped the run.
t_case "a fetch without --force refuses the moved tag and fails"
rukas_moved
git -C "$T/rukas-moved/host" fetch -q --tags origin >"$T/fetch.out" 2>&1
t_eq "$?" 1
t_lacks "$(cat "$T/fetch.out")" "clobber"
git -C "$T/rukas-moved/host" fetch --tags origin >"$T/fetch.out" 2>&1
t_eq "$?" 1
t_contains "$(cat "$T/fetch.out")" "would clobber existing tag"

# What the script does now, on the same remote and the same clone: the moved tag
# is taken, the newer release is there, and the checkout gets onto it. This is
# the acceptance criterion: a clone whose tag upstream moved updates without a
# person fetching by hand.
t_case "the fetch the script makes takes the moved tag and the release on it"
rukas_moved
newest=$(t_git -C "$T/rukas-moved/up" rev-parse HEAD)
git -C "$T/rukas-moved/host" fetch --tags --force origin >"$T/fetch.out" 2>&1
t_eq "$?" 0
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.12.80^{commit}")" "$(t_git -C "$T/rukas-moved/up" rev-parse "v0.12.80^{commit}")"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse "v0.12.81^{commit}")" "$newest"
git -C "$T/rukas-moved/host" checkout -q "$newest"
t_eq "$(t_git -C "$T/rukas-moved/host" rev-parse HEAD)" "$newest"

# --- dev: let other accounts start a session ---------------------------------
# Stubs for everything that would change the system. The files go to scratch
# directories through the same two hooks the real paths have.
etc=$T/etc
ubin=$T/usr-local-bin
mkdir -p "$etc/sudoers.d" "$ubin" "$T/boss"
dev_stubs() { # <uid the caller has>
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss | alice | bob) ;; *) exit 1 ;; esac"
  t_stub getent 'case "$*" in "passwd boss") echo "boss:x:1001:1001::$T/boss:/bin/bash" ;; "group tehas") [ -e "$T/group-made" ] ;; *) exit 2 ;; esac'
  t_stub groupadd ': >"$T/group-made"'
  t_stub usermod
  t_stub visudo 'case "$1" in -cf) grep -q "^%tehas " "$2" ;; *) exit 1 ;; esac'
  t_stub sudo
}
dev_stubs 0
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_BIN="$ubin" PATH="$T_PATH")
rule=$etc/sudoers.d/tehas-dev
written() { find "$etc" "$ubin" -type f | wc -l; }

t_case "dev lets the group run one script as the tehas user, and nothing else"
rm -f "$T"/calls/*
t_run "$bootstrap dev boss alice bob"
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^Defaults' "$rule")" "%tehas ALL=(boss) NOPASSWD: $T/boss/tehas/libexec/dev-login"
t_eq "$(grep -c '' "$rule")" "$(grep -c -e '^#' -e '^Defaults!' -e '^%tehas ' "$rule")"
t_eq "$(grep '^Defaults' "$rule")" "Defaults!$T/boss/tehas/libexec/dev-login env_keep += \"COLORTERM\""
t_eq "$(stat -c %a "$rule")" 440
t_lacks "$(cat "$rule")" "ALL=(ALL"
t_lacks "$(cat "$rule")" "(root)"

t_case "dev names the tehas user for the command, and installs the command for everyone"
t_eq "$(cat "$etc/tehas/manager")" boss
t_eq "$(cmp -s "$ubin/tehas" "$T_ROOT/host/login/tehas" && echo same)" same
t_eq "$(stat -c %a "$ubin/tehas")" 755

t_case "dev makes the group once and adds the accounts named"
t_eq "$(t_calls groupadd)" "tehas"
t_eq "$(t_calls usermod)" "-aG tehas alice
-aG tehas bob"
rm -f "$T"/calls/*
t_run "$bootstrap dev boss"
t_eq "$t_rc" 0
t_eq "$(t_calls groupadd)" ""
t_eq "$(t_calls usermod)" ""

t_case "dev never puts an account in the docker group"
t_lacks "$(t_calls usermod)" "docker"

fresh_dev() { rm -rf "$etc" "$ubin" "$T"/calls/* "$T/group-made"; mkdir -p "$etc/sudoers.d" "$ubin"; }

t_case "dev changes nothing when an account does not exist"
fresh_dev
t_run "$bootstrap dev boss alice mallory"
t_eq "$t_rc" 1
t_contains "$t_out" "no such user: mallory"
t_eq "$(written)" 0
t_eq "$(t_calls usermod)$(t_calls groupadd)" ""

t_case "dev refuses the tehas user as one of the accounts, and names that are not plain"
for args in "boss boss" "boss 'a b'" "'bo ss'" "boss ALL" ""; do
  t_run "$bootstrap dev $args"
  [ "$t_rc" != 0 ] || t_bad "accepted: $args"
done
t_eq "$(written)" 0
t_eq "$(t_calls usermod)$(t_calls groupadd)" ""

t_case "dev installs no rule that visudo does not accept"
t_stub visudo 'exit 1'
t_run "$bootstrap dev boss alice"
t_eq "$t_rc" 1
t_contains "$t_out" "visudo"
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$([ -e "$ubin/tehas" ] && echo there)" ""
t_eq "$(t_calls usermod)" ""

t_case "dev refuses to run without root"
dev_stubs 1000
t_run "$bootstrap dev boss alice"
t_eq "$t_rc" 1
t_contains "$t_out" "must run as root"
t_eq "$(written)" 0
rm -f "$T/bin/id" "$T/bin/getent" "$T/bin/sudo" "$T/bin/visudo" "$T/bin/usermod" "$T/bin/groupadd"
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# --- slice: the one system unit every sandbox container runs under -----------
# systemctl answers as a host does: an unloaded slice has no file and no
# limit, and it is loaded from the file once it has been started.
shipped=$T_ROOT/host/slice/tehas.slice
rule_shipped=$T_ROOT/host/slice/96-tehas-memory-online.rules
reload_shipped=$T_ROOT/host/slice/tehas-slice-reload.service
slice=$etc/systemd/system/tehas.slice
rule=$etc/udev/rules.d/96-tehas-memory-online.rules
reload=$etc/systemd/system/tehas-slice-reload.service
slice_stubs() { # <uid the caller has> [<what docker info answers>] [<systemd-oomd's state>]
  # Names the stubs of this section make, so the clock section can take them
  # away again: a systemctl that only knows about chrony cannot answer the root
  # part's questions about the slice, and the root cases come after both.
  slice_tools="id docker systemctl "
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  t_stub docker "case \"\$1\" in info) echo '${2-systemd 2}' ;; *) echo 'Docker version 29.1.3' ;; esac"
  # The step asks udevd to re-read its rules after writing the new one; the
  # stub records the call and answers like any other command here.
  t_stub udevadm
  # slice-stale: the file on disk is newer than what systemd read, as systemd
  # reports it, until a daemon-reload.
  t_stub systemctl 'case "$*" in
  "show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload")
    if [ -e "$T/slice-loaded" ]; then printf "MemoryMax=21990232555\nFragmentPath=/etc/systemd/system/tehas.slice\n"
    else printf "MemoryMax=infinity\nFragmentPath=\n"; fi
    if [ -e "$T/slice-stale" ]; then echo NeedDaemonReload=yes; else echo NeedDaemonReload=no; fi ;;
  "daemon-reload") rm -f "$T/slice-stale" ;;
  "start tehas.slice") : >"$T/slice-loaded" ;;
  "is-active systemd-oomd") echo '"${3-inactive}"'; [ '"${3-inactive}"' = active ] ;;
esac'
}
fresh_slice() { rm -rf "$etc" "$T"/calls/* "$T/slice-loaded"; mkdir -p "$etc/systemd/system"; }
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
slice_tools=

t_case "slice installs the shipped file, reloads and starts"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(stat -c %a "$slice")" 644
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
# daemon-reload does not read /etc/udev/rules.d, so the step has to ask udevd
# for its own reload or the rule sits on disk until udevd next restarts.
t_eq "$(t_calls udevadm)" "control --reload-rules"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "21990232555"

# systemd reads a percentage MemoryMax when it loads the unit, so memory added
# to a running host reaches the slice only through a reload: the rule asks
# systemd-udevd for one, and the oneshot does it.
t_case "slice installs the memory-online rule and the reload unit beside it"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same
t_eq "$(cmp -s "$reload" "$reload_shipped" && echo same)" same
t_eq "$(stat -c %a "$rule")$(stat -c %a "$reload")" "644644"
# The rule names the unit and nothing else does the reload: RUN+= is killed when
# it runs too long, and many blocks come online at once. One read of the file,
# comments away, for every assertion on it: a rule may name RUN+= in its prose
# and still hold no RUN+=, and this case claims the second thing.
rule_code=$(grep -v -e '^#' -e '^$' "$rule")
t_eq "$rule_code" 'SUBSYSTEM=="memory", ACTION=="online", TAG+="systemd", ENV{SYSTEMD_WANTS}+="tehas-slice-reload.service"'
t_lacks "$rule_code" 'ACTION=="offline"'
t_lacks "$rule_code" "RUN+="
# The unit udevd is told to start is the one that ships beside the rule: both
# names are read from the files, so a rename of one side alone fails here.
t_eq "$(sed -n 's/.*SYSTEMD_WANTS}+="\([^"]*\)".*/\1/p' <<<"$rule_code")" "$(basename "$reload")"
t_contains "$(cat "$reload")" "Type=oneshot"
t_contains "$(cat "$reload")" "ExecStart=/usr/bin/systemctl daemon-reload"
# The reload is bounded, as every unit of host/systemd bounds its run: a hung
# daemon-reload would swallow every memory-online event that came after it.
t_contains "$(cat "$reload")" "TimeoutStartSec="

t_case "the shipped slice has one memory limit, a CPU weight and nothing else"
t_eq "$(grep -v -e '^#' -e '^$' "$shipped")" "[Unit]
Description=The containers tehas starts, together
[Slice]
MemoryMax=75%
CPUWeight=100"

t_case "slice run twice with the unit loaded from the file changes nothing"
rm -f "$T"/calls/*
before=$(stat -c %Y "$slice")
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: slice part already in place"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload"
t_eq "$(stat -c %Y "$slice")" "$before"

t_case "slice run twice with a file systemd has not re-read reloads and starts"
# An update changed the file and the reload after it failed: the unit is still
# the old file's, with a limit, so the file alone says nothing.
rm -f "$T"/calls/*
: >"$T/slice-stale"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_lacks "$t_out" "already in place"
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_eq "$([ -e "$T/slice-stale" ] && echo stale)" ""

t_case "slice run twice with a file but a limitless unit reloads and starts"
# The file is there but systemd never read it, or holds a slice it made up
# before the file existed.
rm -f "$T"/calls/* "$T/slice-loaded"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_lacks "$t_out" "already in place"
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"

t_case "slice stops when the unit has no limit after it was started"
fresh_slice
slice_stubs 0
t_stub systemctl 'case "$1" in show) printf "MemoryMax=infinity\nFragmentPath=\n" ;; esac'
slice_tools="systemctl "
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "no memory limit"
t_lacks "$t_out" "slice part done"

t_case "slice with arguments is refused (64)"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice boss"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: sudo ./bootstrap.sh slice"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses a cgroupfs driver and writes nothing"
slice_stubs 0 "cgroupfs 2"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "cgroupfs cgroup driver on cgroup v2"
t_contains "$t_out" "/etc/docker/daemon.json"
t_contains "$t_out" '"exec-opts": ["native.cgroupdriver=systemd"]'
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses cgroup v1 and writes nothing"
slice_stubs 0 "systemd 1"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd cgroup driver on cgroup v1"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice refuses a Docker that does not answer and writes nothing"
slice_stubs 0
t_stub docker 'exit 1'
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "Docker gave no answer"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "slice must run as root"
slice_stubs 1000
rm -f "$T"/calls/*
t_run "$bootstrap slice"
t_eq "$t_rc" 64
t_contains "$t_out" "sudo ./bootstrap.sh slice"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

t_case "slice says whether systemd-oomd is active and installs either way"
for oomd in active inactive; do
  fresh_slice
  slice_stubs 0 "systemd 2" "$oomd"
  t_run "$bootstrap slice"
  t_eq "$t_rc" 0
  t_contains "$t_out" "systemd-oomd: $oomd"
  t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
done
t_lacks "$t_out" "systemd-oomd: active"

# --- clock --------------------------------------------------------------------
# The clock part: qemu-guest-agent on a VM, the chrony drop-in under
# BOOTSTRAP_ETC. The stubs name what the host is (systemd-detect-virt), which
# time service it runs (timedatectl) and what the distribution offers (apt-get).
conf=$etc/chrony/conf.d/tehas.conf
# Where the part writes, and a decoy at a path the script cannot guess: a run
# that wrote outside BOOTSTRAP_ETC would leave its file there.
conf_other=$T/not-etc/chrony/conf.d/tehas.conf
mkdir -p "${conf_other%/*}"
# The three lines the part writes, and the one line a case that does not care
# about the comments asks for.
conf_body='# Written by bootstrap.sh clock. A restored VM starts with the clock
# of the moment it was saved; stepping is the only way back within seconds.
makestep 1 -1'
# The clock section must leave nothing behind for the sections after it: the
# root part's own cases ask systemctl about the slice, and a systemctl that only
# knows about chrony cannot answer them. Every stub this section makes is named
# in clock_tools as it is made, and the list is emptied at the end of the
# section, so a case that stubs one more tool cannot leave it for the next.
clock_tools=
clock_stubs() { # <uid> <what systemd-detect-virt --vm answers> [<timedatectl's NTP>]
  # The three questions the part asks, all answered by the case: who is running
  # it, what the host is, and which time service systemd has. The probe is
  # always stubbed: a host that has the real program would answer for the case.
  # The third argument is timedatectl's answer, and 'none' is one of them (a
  # host that runs no time service), so it has no default that can be written
  # into the body: no third argument at all means chrony.
  local clock_tool
  for clock_tool in id systemd-detect-virt timedatectl apt-get systemctl; do
    clock_tools+="$clock_tool "
  done
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  case $2 in
    '') t_stub systemd-detect-virt 'exit 1' ;;
    *) t_stub systemd-detect-virt "echo '$2'" ;;
  esac
  case ${3+set} in
    set) t_stub timedatectl "echo '$3'" ;;
    *) t_stub timedatectl 'echo chrony' ;;
  esac
  t_stub apt-get
  t_stub systemctl
}
fresh_clock() { rm -rf "$etc/chrony" "$T"/calls/*; }
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "clock on a VM installs and enables the guest agent and writes the drop-in"
fresh_clock
clock_stubs 0 kvm
rm -f "$T"/calls/*
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(t_calls systemd-detect-virt)" "--vm"
t_eq "$(t_calls timedatectl)" "show -p NTP --value"
t_eq "$(t_calls apt-get | tr '\n' '|')" "update -qq|install -y -qq qemu-guest-agent|"
t_eq "$(t_calls systemctl)" "enable --now qemu-guest-agent
restart chrony"
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(stat -c %a "$conf")" 644
t_eq "$(stat -c %a "${conf%/*}")" 755
t_contains "$t_out" "qemu-guest-agent: enabled on kvm"
t_contains "$t_out" "makestep 1 -1, chrony restarted"

t_case "clock run again keeps the file it wrote as .prev and writes the same bytes"
rm -f "$T"/calls/*
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$conf" "$conf.prev" && echo same)" same
t_eq "$(cat "$conf")" "$conf_body"

# The modes of the file and its directory: what a daemon must be able to read.
t_case "clock keeps the drop-in of an earlier run as .prev, and its mode"
fresh_clock
clock_stubs 0 kvm
rm -f "$T"/calls/*
# The file of the earlier run is put in place by the case itself: two copies of
# the script in one line of t_run would not stand for two runs, because the part
# names its file from BOOTSTRAP_ETC when the script is loaded, so both would
# write one path. What is kept is a file of another host's making, in the shape
# chrony itself ships, and the part must not write over it.
install -d -m 755 "${conf%/*}"
printf 'makestep 1 3\n' >"$conf"
chmod 600 "$conf"
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(cat "$conf.prev")" "makestep 1 3"
# The mode of the file the part wrote, and of the one it only moved aside.
t_eq "$(stat -c %a "$conf")" 644
t_eq "$(stat -c %a "$conf.prev")" 600
t_eq "$(stat -c %a "${conf%/*}")" 755
t_eq "$(find "$etc/chrony" -type f | wc -l)" 2

t_case "clock on bare metal installs no agent and still steps the clock"
fresh_clock
clock_stubs 0 ''
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(t_calls systemd-detect-virt)" "--vm"
t_eq "$(t_calls apt-get)$(t_calls systemctl | grep -v '^restart chrony$')" ""
t_contains "$t_out" "qemu-guest-agent: none on bare metal"
t_eq "$(cat "$conf")" "$conf_body"
t_eq "$(find "$etc/chrony" -type f | wc -l)" 1

t_case "clock writes no drop-in when the time service is not chrony, and says what it found"
for service in systemd-timesyncd none; do
  fresh_clock
  clock_stubs 0 kvm "$service"
  t_run "$bootstrap clock"
  t_eq "$t_rc" 0
  t_eq "$([ -e "$conf" ] && echo there)" ""
  # Nothing of the part's own left under /etc; tehas.slice stands there from the
  # slice cases above, as it does on a host that ran them.
  t_eq "$(find "$etc/chrony" 2>/dev/null | wc -l)" 0
  t_contains "$t_out" "the time service is $service, not chrony"
  t_contains "$t_out" "No drop-in was written"
  # The agent is the hypervisor's way to set the clock and does not depend on
  # which time service the host runs.
  t_contains "$(t_calls apt-get)" "install -y -qq qemu-guest-agent"
  t_eq "$(t_calls systemctl)" "enable --now qemu-guest-agent"
done

# The two files the part can own, and the modes of both: the earlier file is
# kept as .prev rather than written over, and a person who wants the host's own
# makestep back takes the drop-in away and reads the file that was kept.
t_case "clock keeps the file of an earlier run as .prev, in the same run of the script"
stat -c "%n %a" "${conf}.prev" "$conf" "${conf%/*}" | LC_ALL=C sort

# A host with no systemd-detect-virt to ask: the part takes that as bare metal
# and goes on, rather than stopping without a word about the clock. The probe is
# stubbed by every case above, because the host running the tests may well have
# the real program and would answer for the case otherwise; here it is taken
# away again.
t_case "clock on a host without the probe goes on as on bare metal"
for answer in '' none; do
  fresh_clock
  clock_stubs 0 "$answer"
  # rm, and not t_stub: no systemd-detect-virt on the PATH at all is the host
  # this case is about, and the host running the tests may well have one.
  rm -f "$T/bin/systemd-detect-virt"
  t_run "$bootstrap clock"
  t_eq "$t_rc" 0
  t_contains "$t_out" "qemu-guest-agent: none on bare metal"
  t_eq "$(t_calls apt-get)" ""
  t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"
done

# timedatectl that does not answer at all: not a host that has no chrony, but
# one whose time service nobody can name. The drop-in is refused either way.
fresh_clock
clock_stubs 0 kvm ''
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$([ -e "$conf" ] && echo there)" ""
t_contains "$t_out" "the time service is unknown, not chrony"

t_case "clock must run as root, and changes nothing when it is not"
fresh_clock
clock_stubs 1000 kvm 'systemd-timesyncd'
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "sudo ./bootstrap.sh clock"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemctl)" ""

t_case "clock with an argument is refused (64)"
fresh_clock
clock_stubs 0 kvm
t_run "$bootstrap clock boss"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: sudo ./bootstrap.sh clock"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemctl)" ""

t_case "clock on an unsupported host stops before it installs anything"
fresh_clock
clock_stubs 0 kvm
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-debian" PATH="$T_PATH")
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "supports Ubuntu 26.04 on x86_64 only"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls apt-get)$(t_calls systemd-detect-virt)" ""
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "clock that cannot install the package stops before it enables anything"
fresh_clock
clock_stubs 0 kvm
t_stub apt-get 'case "$1" in install) exit 100 ;; esac'
clock_tools+="apt-get "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "could not install qemu-guest-agent"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$(t_calls systemctl)" ""

t_case "clock whose agent will not start still writes the drop-in and exits 1"
fresh_clock
clock_stubs 0 kvm
t_stub systemctl 'case "$*" in "enable --now qemu-guest-agent") exit 1 ;; esac'
clock_tools+="systemctl "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "qemu-guest-agent: not enabled"
t_eq "$(grep -c makestep "$conf")" 1
t_contains "$(t_calls systemctl)" "restart chrony"

t_case "clock that wrote the drop-in but cannot restart chrony says so and exits 1"
fresh_clock
clock_stubs 0 kvm
t_stub systemctl 'case "$*" in "restart chrony") exit 1 ;; esac'
clock_tools+="systemctl "
t_run "$bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "chrony did not restart"
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"

# BOOTSTRAP_ETC is read when the script is loaded, so a run of the whole script
# that does not name it would write under the real /etc. The clock part owns one
# file there and its directory; this case asks what it says when it cannot make
# them. t_run builds the run's environment with env -i, so the name has to be
# taken away inside the line the run reads, and not only from T_ENV.
t_case "clock on a host that gives no BOOTSTRAP_ETC says it cannot make the file"
fresh_clock
clock_stubs 0 ''
t_run "unset BOOTSTRAP_ETC; $bootstrap clock"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot create directory '/etc/chrony"
t_eq "$([ -e "$conf" ] && echo there)" ""
t_eq "$([ -e "${conf}.prev" ] && echo there)" ""

t_case "clock writes under BOOTSTRAP_ETC and nowhere else"
fresh_clock
rm -f "$conf_other"
clock_stubs 0 ''
t_run "$bootstrap clock"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 "$conf")" "makestep 1 -1"
t_eq "$([ -e "$conf_other" ] && echo there)" ""

for clock_tool in $clock_tools $slice_tools; do rm -f "$T/bin/$clock_tool"; done
unset clock_tool clock_tools slice_tools
# BOOTSTRAP_ETC stays set for every later run of this file, as it was before
# this section: the slice file that the root and check cases write goes under
# the scratch /etc, and not the host's.
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# The root part's own stubs: apt-get, usermod and loginctl record, useradd
# makes the account, runuser runs nothing and records how it was asked.
root_stubs() {
  for tool in apt-get usermod loginctl; do t_stub "$tool"; done
  t_stub useradd ': >"$T/user-made"'
  t_stub id "case \"\$*\" in -u) echo 0 ;; boss) ;; carol) [ -e \"\$T/user-made\" ] ;; *) exit 1 ;; esac"
  t_stub runuser
  t_stub uname 'echo x86_64'
}

t_case "root installs the slice, then runs the user part as the user in the same run"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$(t_calls systemctl)" "start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "bootstrap: root part done"
t_eq "$(t_calls runuser)" "-l boss -c $T_ROOT/bootstrap.sh user"
t_contains "$(t_calls apt-get)" "minisign"
t_eq "$(t_calls useradd)" ""
t_lacks "$t_out" "must log in again"

t_case "root with --root-only stops before the user part"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root boss --root-only"
t_eq "$t_rc" 0
t_contains "$t_out" "bootstrap: root part done"
t_eq "$(t_calls runuser)" ""
t_contains "$t_out" "./bootstrap.sh user"

t_case "root fails when the user part fails"
fresh_slice
slice_stubs 0
root_stubs
t_stub runuser 'exit 1'
t_run "$bootstrap root boss"
t_eq "$t_rc" 1
t_contains "$t_out" "the user part failed"

t_case "root makes a user that is missing"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root carol"
t_eq "$t_rc" 0
t_eq "$(t_calls useradd)" "-m -s /bin/bash carol"
t_contains "$t_out" "made the user carol"
t_contains "$(t_calls usermod)" "-aG docker carol"

t_case "root refuses a name that is not a plain account name and changes nothing"
fresh_slice
slice_stubs 0
root_stubs
t_run "$bootstrap root 'bo ss'"
t_eq "$t_rc" 1
t_contains "$t_out" "not a plain account name"
t_eq "$(t_calls apt-get)$(t_calls useradd)" ""

# A Docker on cgroupfs answers systemd once daemon.json names the systemd
# driver and it was restarted.
docker_driver_stub() {
  t_stub docker "case \"\$1\" in
info) if grep -q native.cgroupdriver=systemd \"$etc/docker/daemon.json\" 2>/dev/null && [ -e \"\$T/docker-restarted\" ]; then echo 'systemd 2'; else echo 'cgroupfs 2'; fi ;;
*) echo 'Docker version 29.1.3' ;;
esac"
  t_stub systemctl "case \"\$*\" in
  'restart docker') : >\"\$T/docker-restarted\" ;;
  'show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload')
    if [ -e \"\$T/slice-loaded\" ]; then printf 'MemoryMax=21990232555\nFragmentPath=/etc/systemd/system/tehas.slice\nNeedDaemonReload=no\n'
    else printf 'MemoryMax=infinity\nFragmentPath=\nNeedDaemonReload=no\n'; fi ;;
  'start tehas.slice') : >\"\$T/slice-loaded\" ;;
esac"
}

t_case "root sets the systemd cgroup driver in a missing daemon.json, restarts Docker and installs the slice"
fresh_slice
rm -f "$T/docker-restarted"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$etc/docker/daemon.json")" '{"exec-opts":["native.cgroupdriver=systemd"]}'
t_contains "$(t_calls systemctl)" "restart docker"
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$t_out" "bootstrap: root part done"

t_case "root merges the driver into a daemon.json that has other settings and keeps them"
fresh_slice
rm -f "$T/docker-restarted"
mkdir -p "$etc/docker"
printf '%s\n' '{"log-driver":"local","exec-opts":["native.cgroupdriver=cgroupfs","other=1"]}' >"$etc/docker/daemon.json"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$etc/docker/daemon.json")" '{"log-driver":"local","exec-opts":["other=1","native.cgroupdriver=systemd"]}'
t_eq "$(jq -c . "$etc/docker/daemon.json.prev")" '{"log-driver":"local","exec-opts":["native.cgroupdriver=cgroupfs","other=1"]}'

t_case "root leaves a daemon.json it cannot read as JSON, writes no slice and says so"
fresh_slice
rm -f "$T/docker-restarted"
mkdir -p "$etc/docker"
printf '{ not json\n' >"$etc/docker/daemon.json"
slice_stubs 0
root_stubs
docker_driver_stub
t_run "$bootstrap root boss"
t_eq "$t_rc" 1
t_eq "$(cat "$etc/docker/daemon.json")" "{ not json"
t_contains "$t_out" "is not JSON"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$(t_calls runuser)" ""

t_case "root on cgroup v1 finishes, writes no slice and no daemon.json, and says what to run"
fresh_slice
slice_stubs 0 "systemd 1"
root_stubs
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$etc/docker/daemon.json" ] && echo there)" ""
t_contains "$t_out" "run sudo ./bootstrap.sh slice once Docker uses the systemd driver"
t_contains "$t_out" "bootstrap: root part done"
rm -f "$T/bin/id" "$T/bin/apt-get" "$T/bin/usermod" "$T/bin/systemctl" "$T/bin/runuser" "$T/bin/useradd"

t_case "check reports the slice and does not fail on it"
all_present
fresh_slice
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: missing (sudo ./bootstrap.sh slice)"
install -m 644 "$shipped" "$slice"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: in place"
echo "MemoryMax=10%" >>"$slice"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: differs (sudo ./bootstrap.sh slice)"
t_lacks "$t_out" "tehas.slice: in place"
t_contains "$t_out" "96-tehas-memory-online.rules: missing (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "tehas-slice-reload.service: missing (sudo ./bootstrap.sh slice)"
install -D -m 644 "$rule_shipped" "$rule"
install -D -m 644 "$reload_shipped" "$reload"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas.slice: differs"
t_contains "$t_out" "96-tehas-memory-online.rules: in place"
t_contains "$t_out" "tehas-slice-reload.service: in place"
echo "SUBSYSTEM==\"memory\", ACTION==\"offline\"" >>"$rule"
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_contains "$t_out" "96-tehas-memory-online.rules: differs (sudo ./bootstrap.sh slice)"
t_eq "$(find "$etc" -type f | wc -l)" 3
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# The write path has three installs and each can fail on its own (an unreadable
# source, a full or unwritable /etc). Two of the three on disk is not in place,
# and the next run has to repair it: a rule beside no unit is the failure this
# step exists to avoid.
t_case "slice says so when a shipped file cannot be read, and writes nothing"
fresh_slice
slice_stubs 0
# A shipped file the step cannot read is reported as the write that failed, and
# nothing is left on disk: the pre-flight test of -f cannot tell that file from
# one that is there and readable.
chmod 000 "$reload_shipped"
t_run "$bootstrap slice"
chmod 644 "$reload_shipped"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$([ -e "$reload" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" ""

T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# Each of the three installs can fail on its own (a full or unwritable target
# directory, EACCES). Two of the three on disk is not in place, and the next run
# has to repair it: a rule beside no unit is the failure this step exists to
# avoid. The case writes the unit by hand and stubs install to fail at the rule.
t_case "slice reports the file it could not write and the retry writes what is missing"
fresh_slice
slice_stubs 0
# The unit is written first, so a run that fails at the rule leaves the shipped
# unit beside no rule and no slice: what a guard on the slice alone would report
# as "already in place". The directory of the rule is made unwritable to bring
# that about, as EACCES or a full disk would.
mkdir -p "$etc/udev"
chmod 555 "$etc/udev"
t_run "$bootstrap slice"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write $rule"
t_eq "$([ -e "$slice" ] && echo there)" ""
t_eq "$([ -e "$rule" ] && echo there)" ""
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" ""
t_eq "$(t_calls udevadm)" ""
# The retry writes the three and reloads udevd once.
chmod 755 "$etc/udev"
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(t_calls udevadm)" "control --reload-rules"
t_contains "$t_out" "bootstrap: slice part done"

t_case "slice says so when udevd's rules could not be reloaded, and still installs"
T_ENV=(BOOTSTRAP_ETC="$etc" BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")
fresh_slice
slice_stubs 0
t_stub udevadm 'exit 1'
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_contains "$t_out" "udevadm control --reload-rules failed"
t_contains "$t_out" "bootstrap: slice part done"
t_eq "$(cmp -s "$rule" "$rule_shipped" && echo same)" same

t_case "no arguments prints the usage"
t_run "$bootstrap"
t_eq "$t_rc" 64
t_contains "$t_out" "bootstrap.sh check"
t_contains "$t_out" "bootstrap.sh slice"

t_done
