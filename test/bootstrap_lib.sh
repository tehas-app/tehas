# shellcheck shell=bash
# What the tests of bootstrap.sh share: stub tools at the pinned versions, the
# pinned npm trees and a scratch copy of the checkout. Source this from a
# bootstrap_*_test.sh file, after test/lib.sh.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2034  # the variables here are read by the test files

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
# What a jailed run of the clock part really asks for and does not have stubbed:
# the base system it runs its tests, its copies and its redirect with. A case
# that jails its run is given these and nothing else out there.
T_PLANT=(bash cat chmod chown cp date dirname grep head install ln mkdir mv rm sed stat uname)

# The commit of pi-rukas's newest release tag, as git ls-remote answers it,
# asked through the console install.sh puts in the home.
rukas_head=$(printf 'e%.0s' {1..40})
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"

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
  t_stub git "case \"\$*\" in *ls-remote*) printf '%s\trefs/tags/v0.15.81\n' $rukas_head ;; *rev-parse*) echo $rukas_head ;; *) echo 'git version 2.53.0' ;; esac"
  t_stub docker 'echo "Docker version 29.1.3"'
  t_stub loginctl 'echo yes'
  t_stub curl 'echo "curl 8.0.0"'
  t_stub uname 'echo x86_64'
  for tool in jq openssl flock minisign shellcheck unzip; do t_stub "$tool" 'echo "1.2.3"'; done
  t_stub jq "[ \"\${1:-}\" = --version ] && { echo jq-$JQ_VERSION; exit 0; }; exec $jq_real \"\$@\""
  t_stub tsc "echo 'Version $TSC_VERSION'"
  mkdir -p "$T/home/.pi/agent/extensions"
  ln -sfn "$T" "$T/home/.pi/agent/extensions/pi-rukas"
  mkdir -p "$T/home/tehas/libexec"
  cp "$T_CONSOLE" "$T/home/tehas/libexec/console"
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

# A scratch copy of bootstrap.sh and what it reads, so a case can break a pin
# without touching this checkout.
pin_checkout() { # -> the directory holding the copy
  local dir=$T/checkout
  rm -rf "$dir"
  mkdir -p "$dir/host" "$dir/lib"
  cp "$bootstrap" "$dir/bootstrap.sh"
  cp "$T_ROOT/lib/paths.sh" "$dir/lib/paths.sh"
  cp "$T_ROOT/host/versions.env" "$dir/host/versions.env"
  cp -R "$T_ROOT/host/npm" "$dir/host/npm"
  echo "$dir"
}
