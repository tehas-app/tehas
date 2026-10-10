#!/usr/bin/env bash
# Tests for bootstrap.sh, against stub tools that print a version: what
# `check` reports, that `user` skips what is present, and the platform guard.
# No download and no install happens here.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

bootstrap="$T_ROOT/bootstrap.sh"
# shellcheck source=host/versions.env
. "$T_ROOT/host/versions.env"

printf 'ID=ubuntu\nVERSION_ID="26.04"\n' >"$T/os-ubuntu"
printf 'ID=debian\nVERSION_ID="13"\n' >"$T/os-debian"
# Only the stubs and the base system: tools installed for the person running
# the tests must not be found.
T_PATH=$T/bin:/usr/bin:/bin
T_ENV=(BOOTSTRAP_OS_RELEASE="$T/os-ubuntu" PATH="$T_PATH")

# Every tool at its expected version.
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
  t_stub git "case \"\$*\" in *rev-parse*) echo $PI_RUKAS_COMMIT ;; *) echo 'git version 2.53.0' ;; esac"
  t_stub docker 'echo "Docker version 29.1.3"'
  t_stub loginctl 'echo yes'
  t_stub curl 'echo "curl 8.0.0"'
  t_stub uname 'echo x86_64'
  for tool in jq openssl flock shellcheck unzip; do t_stub "$tool" 'echo "1.2.3"'; done
  mkdir -p "$T/home/.pi/agent/extensions"
  ln -sfn "$T" "$T/home/.pi/agent/extensions/pi-rukas"
}

t_case "check passes when every tool is at its version"
all_present
t_run "$bootstrap check"
t_eq "$t_rc" 0
t_lacks "$t_out" "MISSING"
t_lacks "$t_out" "DIFFERS"
t_contains "$t_out" "node"
t_contains "$t_out" "$PI_RUKAS_COMMIT"

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
t_stub ctx7 'echo 0.0.1'
t_run "$bootstrap user"
t_eq "$t_rc" 1
t_contains "$t_out" "installing ctx7 $CTX7_VERSION (replacing 0.0.1)"
t_contains "$(t_calls npm)" "ctx7@$CTX7_VERSION"
t_contains "$t_out" "ctx7 is 0.0.1 after installing, expected $CTX7_VERSION"

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

t_case "no arguments prints the usage"
t_run "$bootstrap"
t_eq "$t_rc" 64
t_contains "$t_out" "bootstrap.sh check"

t_done
