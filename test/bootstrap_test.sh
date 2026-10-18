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
  t_stub vipune "echo 'vipune $VIPUNE_VERSION'"
  t_stub oo "echo 'oo $OO_VERSION'"
  t_stub codebase-memory-mcp "echo 'codebase-memory-mcp $CBM_VERSION'"
  t_stub git "case \"\$*\" in *ls-remote*) printf '%s\trefs/tags/v0.12.80\n' $rukas_head ;; *rev-parse*) echo $rukas_head ;; *) echo 'git version 2.53.0' ;; esac"
  t_stub docker 'echo "Docker version 29.1.3"'
  t_stub loginctl 'echo yes'
  t_stub curl 'echo "curl 8.0.0"'
  t_stub uname 'echo x86_64'
  for tool in jq openssl flock shellcheck unzip; do t_stub "$tool" 'echo "1.2.3"'; done
  t_stub jq "exec $jq_real \"\$@\""
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

t_case "an existing pi-rukas clone is fetched from the repository in versions.env"
all_present
behind_head() {
  t_stub git "case \"\$*\" in *ls-remote*) printf '%s\trefs/tags/v0.12.80\n' $rukas_head ;; *rev-parse*) echo 0000000000000000000000000000000000000000 ;; *) echo 'git version 2.53.0' ;; esac"
}
behind_head
mkdir -p "$T/home/tehas/repos/pi-rukas/.git"
printf '#!/bin/sh\n' >"$T/home/tehas/repos/pi-rukas/install.sh"
chmod +x "$T/home/tehas/repos/pi-rukas/install.sh"
rm -f "$T"/calls/*
t_run "$bootstrap user"
t_contains "$(t_calls git)" "remote set-url origin $PI_RUKAS_REPO"

t_case "pi-rukas behind its newest release is moved to it"
t_contains "$(t_calls git)" "checkout -q $rukas_head"

t_case "check reports a pi-rukas behind its newest release"
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
slice=$etc/systemd/system/tehas.slice
slice_stubs() { # <uid the caller has> [<what docker info answers>] [<systemd-oomd's state>]
  t_stub id "case \"\$*\" in -u) echo $1 ;; boss) ;; *) exit 1 ;; esac"
  t_stub docker "case \"\$1\" in info) echo '${2-systemd 2}' ;; *) echo 'Docker version 29.1.3' ;; esac"
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

t_case "slice installs the shipped file, reloads and starts"
fresh_slice
slice_stubs 0
t_run "$bootstrap slice"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_eq "$(stat -c %a "$slice")" 644
t_eq "$(t_calls systemctl | grep -v -e '^show ' -e '^is-active ')" "daemon-reload
start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "21990232555"

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

t_case "root installs the slice too"
fresh_slice
slice_stubs 0
t_stub uname 'echo x86_64'
for tool in apt-get usermod loginctl; do t_stub "$tool"; done
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$slice" "$shipped" && echo same)" same
t_contains "$(t_calls systemctl)" "start tehas.slice"
t_contains "$t_out" "bootstrap: slice part done"
t_contains "$t_out" "bootstrap: root part done"

t_case "root with a cgroupfs driver finishes, writes no slice and says what to run"
fresh_slice
slice_stubs 0 "cgroupfs 2"
t_run "$bootstrap root boss"
t_eq "$t_rc" 0
t_eq "$([ -e "$slice" ] && echo there)" ""
t_contains "$t_out" "run sudo ./bootstrap.sh slice once Docker uses the systemd driver"
t_contains "$t_out" "bootstrap: root part done"
rm -f "$T/bin/id" "$T/bin/apt-get" "$T/bin/usermod" "$T/bin/systemctl"

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
t_eq "$(find "$etc" -type f | wc -l)" 1
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

t_case "no arguments prints the usage"
t_run "$bootstrap"
t_eq "$t_rc" 64
t_contains "$t_out" "bootstrap.sh check"
t_contains "$t_out" "bootstrap.sh slice"

t_done
