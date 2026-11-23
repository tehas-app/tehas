#!/usr/bin/env bash
# Tests for install.sh: the commands of the earlier layout are named by
# --check and removed by a real run, and when they stay.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

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
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"
t_contains "$t_out" "removed: $th/bin/tehas-test"
# The old sampler left with energy use, so no running loop is the old file
# calling itself and no unit is restarted for it.
t_lacks "$t_out" "next: units"
# systemd reloaded before the files its old units name were removed.
t_eq "$(t_calls systemctl | head -n 1)" "--user daemon-reload"
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

t_case "the libraries only the bash poll sourced go the same way"
echo "# old" >"$th/lib/planning.sh"
echo "# old" >"$th/lib/live.sh"
echo "# old" >"$th/lib/refresh.sh"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/lib/planning.sh"
t_contains "$t_out" "stale: $th/lib/live.sh"
t_contains "$t_out" "stale: $th/lib/refresh.sh"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/lib/planning.sh"
t_contains "$t_out" "removed: $th/lib/live.sh"
t_contains "$t_out" "removed: $th/lib/refresh.sh"
t_eq "$([ -e "$th/lib/planning.sh" ] || [ -e "$th/lib/live.sh" ] || [ -e "$th/lib/refresh.sh" ] || echo gone)" "gone"
t_case "the tidy script of before the console ran it goes the same way"
echo "# old" >"$th/libexec/tidy"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/libexec/tidy"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/libexec/tidy"
t_eq "$([ -e "$th/libexec/tidy" ] || echo gone)" "gone"

t_case "the library of before the console did the edge's work goes the same way"
echo "# old" >"$th/lib/edge.sh"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/lib/edge.sh"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/lib/edge.sh"
t_eq "$([ -e "$th/lib/edge.sh" ] || echo gone)" "gone"

t_case "the units of the timed refresh scan are named, then stopped and removed"
units=$home/.config/systemd/user
echo "[Timer]" >"$units/tehas-refresh.timer"
echo "[Service]" >"$units/tehas-refresh.service"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $units/tehas-refresh.timer"
t_contains "$t_out" "stale: $units/tehas-refresh.service"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $units/tehas-refresh.timer"
t_contains "$t_out" "removed: $units/tehas-refresh.service"
t_eq "$(t_calls systemctl | head -n 2)" "--user disable --now tehas-refresh.timer
--user daemon-reload"
t_eq "$([ -e "$units/tehas-refresh.timer" ] || [ -e "$units/tehas-refresh.service" ] || echo gone)" "gone"
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "they stay while systemd cannot disable the timer"
echo "[Timer]" >"$units/tehas-refresh.timer"
t_stub systemctl 'exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again   (systemctl --user disable --now tehas-refresh.timer failed"
t_eq "$([ -e "$units/tehas-refresh.timer" ] && echo there)" "there"
t_stub systemctl
t_run "$install"
t_contains "$t_out" "removed: $units/tehas-refresh.timer"

# The improvement loop, and the Protolab, parked (docs/specs/parked-protolab.md).
for old in improve protolab; do
  t_case "the units of tehas-$old are named, then stopped and removed"
  echo "[Timer]" >"$units/tehas-$old.timer"
  echo "[Service]" >"$units/tehas-$old.service"
  t_run "$install --check"
  t_eq "$t_rc" 1
  t_contains "$t_out" "stale: $units/tehas-$old.timer"
  t_contains "$t_out" "stale: $units/tehas-$old.service"
  rm -f "$T/calls/systemctl"
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "removed: $units/tehas-$old.timer"
  t_contains "$t_out" "removed: $units/tehas-$old.service"
  t_eq "$(t_calls systemctl | head -n 2)" "--user disable --now tehas-$old.timer
--user daemon-reload"
  t_eq "$([ -e "$units/tehas-$old.timer" ] || [ -e "$units/tehas-$old.service" ] || echo gone)" "gone"
done

t_case "the units of the review loop are named, then stopped and removed"
echo "[Timer]" >"$units/tehas-review.timer"
echo "[Service]" >"$units/tehas-review.service"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $units/tehas-review.timer"
t_contains "$t_out" "stale: $units/tehas-review.service"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $units/tehas-review.timer"
t_contains "$t_out" "removed: $units/tehas-review.service"
t_eq "$(t_calls systemctl | head -n 2)" "--user disable --now tehas-review.timer
--user daemon-reload"
t_eq "$([ -e "$units/tehas-review.timer" ] || [ -e "$units/tehas-review.service" ] || echo gone)" "gone"

for unit in tehas-energy.service tehas-top.service; do
  t_case "the retired $unit, a service with no timer, is stopped and removed"
  echo "[Service]" >"$units/$unit"
  t_run "$install --check"
  t_contains "$t_out" "stale: $units/$unit"
  rm -f "$T/calls/systemctl"
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "removed: $units/$unit"
  t_eq "$(t_calls systemctl | head -n 2)" "--user disable --now $unit
--user daemon-reload"
done

t_done
