#!/usr/bin/env bash
# Tests for install.sh: the commands of the earlier layout are named by
# --check and removed by a real run, and when they stay.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

# Before bin/tehas there was one file for each command in ~/tehas/bin.
old_layout() {
  for f in tehas-console.ts tehas-dispatch tehas-energy tehas-health tehas-index tehas-protolab \
    tehas-status tehas-test tehas-work; do echo "# old" >"$th/bin/$f"; done
  echo "# older" >"$th/bin/tehas-dispatch.prev"
}

t_case "--check names the commands of the earlier layout and removes nothing"
fresh
instance
t_run "$install"
old_layout
echo "# a tool bootstrap.sh put there" >"$th/bin/gh"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (9 files)"
t_eq "$(printf '%s\n' "$t_out" | grep -c "stale: $th/bin/tehas-")" 9
t_contains "$t_out" "stale: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"

t_case "a real run removes them, and nothing else in bin"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"
t_contains "$t_out" "removed: $th/bin/tehas-test"
# A running sampler is the old file calling itself. The units step restarts it,
# so the step is asked for and owed although no unit file changed.
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer"
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-energy.service"
# systemd reloaded before the files its old units name were removed.
t_eq "$(t_calls systemctl)" "--user daemon-reload"
t_eq "$(cd "$th/bin" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./gh ./tehas ./tehasbot "
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "the console script of before it was compiled goes the same way"
echo "// old" >"$th/libexec/console.ts"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/libexec/console.ts"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/libexec/console.ts"
t_eq "$([ -e "$th/libexec/console.ts" ] || echo gone)" "gone"
t_eq "$([ -x "$th/libexec/console" ] && echo there)" "there"

t_case "while a cycle runs they stay, and the install says to come back"
old_layout
mkdir -p "$th/state"
lock=$th/state/dispatch.lock
bash -c 'exec 8>"$1"; flock 8; exec sleep 60' _ "$lock" &
holder=$!
for _ in $(seq 50); do flock -n "$lock" true || break; sleep 0.1; done
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again when the running cycle has ended"
# Not a step: `tehas update` must not take it for one it does not know.
t_lacks "$t_out" "next: ./install.sh"
t_lacks "$t_out" "removed: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-status" ] && echo there)" "there"
kill "$holder" 2>/dev/null
wait "$holder" 2>/dev/null
t_run "$install"
t_contains "$t_out" "removed: $th/bin/tehas-status"
t_eq "$([ -e "$th/bin/tehas-status" ] || echo gone)" "gone"
t_eq "$(flock -n "$lock" true && echo free)" "free"

t_case "they stay while systemd has not reloaded its units"
old_layout
rm -f "$T"/calls/systemctl
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again once systemctl --user daemon-reload works"
t_lacks "$t_out" "removed: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"
# A wrapper that prints an error and still exits 0 did not reload either.
t_stub systemctl 'echo "System has not been booted with systemd"'
t_run "$install"
t_contains "$t_out" "later: ./install.sh again once systemctl --user daemon-reload works"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"
t_stub systemctl
t_run "$install"
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"

t_case "an old command that cannot be removed fails the install"
old_layout
chmod a-w "$th/bin"
t_run "$install"
chmod u+w "$th/bin"
if [ "$(id -u)" = 0 ]; then
  t_eq "$t_rc" 0 # root removes it anyway
else
  t_eq "$([ "$t_rc" != 0 ] && echo failed)" "failed"
  t_eq "$([ -e "$th/bin/tehas-energy" ] && echo there)" "there"
fi
t_run "$install"
t_eq "$t_rc" 0


t_done
