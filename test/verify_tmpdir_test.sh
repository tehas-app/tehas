#!/usr/bin/env bash
# Tests for the check scripts/verify.sh makes after the workspace's tests: they
# run with a TMPDIR of their own, and anything they leave in it fails the gate.
# Each case runs the gate in a scratch repository whose steps are stubs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() {
  rm -rf "$tree" "$T/cargo-tmp"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  for s in check-generic check-bash-lines test select-tests; do
    printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/$s.sh"
  done
  chmod +x "$tree/scripts/"*.sh
  g init -q -b main
  g commit -q --allow-empty -m base
  g update-ref refs/remotes/origin/main HEAD
}
gate="cd '$tree' && TEHAS_VERIFY_FRESH=1 TEHAS_VERIFY_LOGS='$T/logs' scripts/verify.sh"
# The stub notes the TMPDIR it was given, and leaves a directory there when
# the case asks it to.
# shellcheck disable=SC2016  # the stub body is expanded when it runs
t_stub cargo 'echo "$TMPDIR" >"$T/cargo-tmp"; [ ! -e "$T/leak" ] || mkdir "$TMPDIR/tehas-cargo-4242"; exit 0'
t_stub node 'exit 0'

t_case "the tests get a TMPDIR of their own and a clean run passes"
fresh
rm -f "$T/leak"
t_run "$gate"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    nothing left in the TMPDIR of cargo test"
t_contains "$(cat "$T/cargo-tmp")" "/cargo-tmp"
t_eq "$(test -d "$(cat "$T/cargo-tmp")" && echo kept)" ""

t_case "a directory the tests leave fails the gate and is named"
fresh
touch "$T/leak"
t_run "$gate"
t_eq "$t_rc" 1
t_contains "$t_out" "FAIL  nothing left in the TMPDIR of cargo test"
t_contains "$t_out" "tehas-cargo-4242"

t_done
