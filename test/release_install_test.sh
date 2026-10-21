#!/usr/bin/env bash
# Tests for host/release/install, the installer a release publishes: the key it
# checks with is the repository's, a download that fails a check puts nothing
# in the user's home, and a good one is placed as versions/<version> with
# current pointing at it after bootstrap.sh root ran. id, getent and chown are
# stubs, so nothing runs as root; minisign is a stub that accepts a signature
# file saying "signed".
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

installer=$T_ROOT/host/release/install
rel=$T/release
bob=$T/home/bob
name=tehas-v1.0.0-linux-x86_64.tar.gz

make_release() {
  local src=$T/src/tehas-v1.0.0
  rm -rf "$T/src" "$rel"
  mkdir -p "$src/bin" "$rel"
  echo v1.0.0 >"$src/VERSION"
  printf '#!/bin/sh\nprintf "bootstrap %%s\\n" "$*" >>"%s/calls/bootstrap"\nexit "${BOOT_RC:-0}"\n' "$T" >"$src/bootstrap.sh"
  chmod +x "$src/bootstrap.sh"
  echo tehas >"$src/bin/tehas"
  tar -czf "$rel/$name" -C "$T/src" tehas-v1.0.0
  (cd "$rel" && sha256sum "$name" >"$name.sha256")
  echo signed >"$rel/$name.minisig"
}

stubs() {
  rm -rf "$bob" "$T/calls"/*
  mkdir -p "$bob"
  t_stub id 'case "$1" in -u) echo "${UID_IS:-0}" ;; -gn) echo staff ;; esac'
  t_stub getent "echo \"bob:x:1001:1001::$bob:/bin/bash\""
  t_stub chown
  t_stub minisign 'x=""; prev=""
for a in "$@"; do [ "$prev" != -x ] || x=$a; prev=$a; done
[ "$(cat "$x")" = signed ]'
}

t_case "the key the installer checks with is host/release/minisign.pub"
embedded=$(sed -n "/^pubkey='/,/'\$/p" "$installer" | sed "s/^pubkey='//; s/'\$//")
t_eq "$embedded" "$(cat "$T_ROOT/host/release/minisign.pub")"

t_case "a good release is placed in the user's home after bootstrap.sh root"
make_release
stubs
t_run "sh '$installer' --from '$rel' --version v1.0.0 bob"
t_eq "$t_rc" 0
t_eq "$(t_calls bootstrap)" "bootstrap root bob"
t_eq "$(readlink "$bob/tehas/current")" versions/v1.0.0
t_eq "$(cat "$bob/tehas/versions/v1.0.0/VERSION")" v1.0.0
t_eq "$(cat "$bob/tehas/state/release-history")" v1.0.0
t_contains "$(t_calls chown)" "-h bob:staff $bob/tehas/current"
t_contains "$(t_calls minisign)" "-m "
t_contains "$t_out" "now run, as bob: ~/tehas/current/bin/tehas setup"

t_case "--no-bootstrap places the release and runs no bootstrap.sh"
stubs
t_run "sh '$installer' --from '$rel' --version v1.0.0 --no-bootstrap bob"
t_eq "$t_rc" 0
t_eq "$(t_calls bootstrap)" ""
t_eq "$(readlink "$bob/tehas/current")" versions/v1.0.0

t_case "a bad signature puts nothing in the home and runs no bootstrap.sh"
stubs
echo forged >"$rel/$name.minisig"
t_run "sh '$installer' --from '$rel' --version v1.0.0 bob"
t_eq "$t_rc" 1
t_contains "$t_out" "is not signed by the tehas release key"
t_eq "$(t_calls bootstrap)" ""
t_eq "$(ls -A "$bob")" ""

t_case "a tarball that does not match its .sha256 is refused"
make_release
stubs
echo tampered >>"$rel/$name"
t_run "sh '$installer' --from '$rel' --version v1.0.0 bob"
t_eq "$t_rc" 1
t_contains "$t_out" "does not match its .sha256"
t_eq "$(ls -A "$bob")" ""

t_case "a failed bootstrap.sh puts nothing in the home"
make_release
stubs
t_run "BOOT_RC=1 sh '$installer' --from '$rel' --version v1.0.0 bob"
t_eq "$t_rc" 1
t_contains "$t_out" "bootstrap.sh root bob failed"
t_eq "$(ls -A "$bob")" ""

t_case "usage: not root, no user, --from without --version, a bad version"
stubs
t_run "UID_IS=1000 sh '$installer' --from '$rel' --version v1.0.0 bob"
t_eq "$t_rc" 64
t_contains "$t_out" "run this as root"
t_run "sh '$installer'"
t_eq "$t_rc" 64
t_run "sh '$installer' --from '$rel' bob"
t_eq "$t_rc" 64
t_contains "$t_out" "--from needs --version"
t_run "sh '$installer' --version latest bob"
t_eq "$t_rc" 64
t_run "sh '$installer' --from '$rel' --version v1.0.0 root"
t_eq "$t_rc" 64

t_done
