#!/usr/bin/env bash
# Tests for bootstrap.sh, against stub tools that print a version: what
# `check` reports, that `user` skips what is present, how an npm pin is checked
# and installed, and the Playwright pin the sandbox image builds from.
# bootstrap_rukas_test.sh and bootstrap_system_test.sh hold the other parts.
# No download and no install happens in any of them.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/bootstrap_lib.sh
. "$(dirname "$0")/bootstrap_lib.sh" || exit 1

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
jq '.dependencies.ctx7 = "0.5.13"' "$dir/host/npm/ctx7/package.json" >"$dir/p2"
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

t_done
