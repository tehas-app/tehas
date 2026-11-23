#!/usr/bin/env bash
# Tests for install.sh, split from install_layout_test.sh: what outsource left,
# the retired units, the timers of a feature that is off, and the polyfills.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
units=$home/.config/systemd/user
fresh
instance
t_run "$install"

t_case "what outsource left in the home goes: its programs, its files and its two directories of state"
left=(libexec/tehas-proxy libexec/tehas-proxy.stamp libexec/tehas-contractor libexec/tehas-contractor.stamp
  live/contractors.json sandbox/vipune-shim)
left_dirs=(state/outsource state/memory/cycles)
for f in "${left[@]}"; do mkdir -p "$th/${f%/*}" && echo "# old" >"$th/$f"; done
mkdir -p "$th/state/outsource/downloads/jobs" "$th/state/outsource/stamp" "$th/state/images" "$th/state/memory/cycles"
echo "x" >"$th/state/outsource/downloads/jobs/a" && echo "x" >"$th/state/memory/cycles/issue-7.db"
echo "kept" >"$th/state/beside" && echo "kept" >"$th/state/images/beside" && echo "kept" >"$th/state/memory/beside"
t_run "$install --check"
for f in "${left[@]}"; do t_contains "$t_out" "stale: $th/$f"; done
for d in "${left_dirs[@]}"; do t_contains "$t_out" "stale: $th/$d/"; done
t_eq "$([ -e "$th/libexec/tehas-proxy" ] && [ -d "$th/state/outsource" ] && echo there)" "there"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
for f in "${left[@]}"; do
  t_contains "$t_out" "removed: $th/$f"
  t_eq "$f: $([ -e "$th/$f" ] && echo there)" "$f: "
done
for d in "${left_dirs[@]}"; do
  t_contains "$t_out" "removed: $th/$d/"
  t_eq "$d: $([ -e "$th/$d" ] && echo there)" "$d: "
done
# What is beside them in state/ stays, and a second run finds nothing left.
t_eq "$(cat "$th/state/beside" "$th/state/images/beside" "$th/state/memory/beside" | tr '\n' ' ')" "kept kept kept "
t_run "$install --check"
t_lacks "$t_out" "stale: $th/state/"

t_case "a link where a directory of outsource was is unlinked, and what it names stays"
mkdir -p "$T/elsewhere" && echo "keep" >"$T/elsewhere/file"
ln -s "$T/elsewhere" "$th/state/outsource"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$([ -e "$th/state/outsource" ] || [ -L "$th/state/outsource" ] || echo gone)" "gone"
t_eq "$(cat "$T/elsewhere/file")" "keep"

t_case "the model proxy of outsource, a service with no timer, is stopped and removed"
echo "[Service]" >"$units/tehas-outsource-proxy.service"
t_run "$install --check"
t_contains "$t_out" "stale: $units/tehas-outsource-proxy.service"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $units/tehas-outsource-proxy.service"
t_eq "$(t_calls systemctl | head -n 2)" "--user disable --now tehas-outsource-proxy.service
--user daemon-reload"

t_case "the wiki's units go, its service only once systemd lets go of its path unit"
for u in tehas-wiki-status.timer tehas-wiki-status.service tehas-wiki.timer tehas-wiki.path tehas-wiki.service; do echo "[Unit]" >"$units/$u"; done
t_stub systemctl '[ "$4" != tehas-wiki.path ]'
t_run "$install"
t_eq "$([ -e "$units/tehas-wiki.path" ] && [ -e "$units/tehas-wiki.service" ] && [ ! -e "$units/tehas-wiki.timer" ] && echo kept)" "kept"
t_stub systemctl
t_run "$install"
t_eq "$(compgen -G "$units/tehas-wiki*" || echo gone)" "gone"

t_case "one retired timer systemd cannot disable leaves its units and not the other's"
echo "[Timer]" >"$units/tehas-refresh.timer"
echo "[Timer]" >"$units/tehas-improve.timer"
echo "[Service]" >"$units/tehas-improve.service"
t_stub systemctl '[ "$4" != tehas-refresh.timer ]'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again   (systemctl --user disable --now tehas-refresh.timer failed"
t_contains "$t_out" "removed: $units/tehas-improve.timer"
t_contains "$t_out" "removed: $units/tehas-improve.service"
t_eq "$([ -e "$units/tehas-refresh.timer" ] && echo there)" "there"
t_stub systemctl
t_run "$install"
t_contains "$t_out" "removed: $units/tehas-refresh.timer"

t_case "an enabled timer of a feature that is off is named, then stopped and disabled, and its files stay"
echo "[Timer]" >"$units/tehas-plugin.timer"
echo "[Service]" >"$units/tehas-plugin.service"
rm -f "$T/disabled"
t_stub systemctl 'case $2 in
  is-enabled) [ ! -e "$T/disabled" ] ;;
  disable) : >"$T/disabled" ;;
esac'
rm -f "$T/calls/systemctl"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "enabled: $units/tehas-plugin.timer (its feature is off; a real run disables it)"
t_lacks "$(t_calls systemctl)" "disable"
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "systemd: stopped and disabled tehas-plugin.timer: its feature is off (the unit files stay)"
t_contains "$(t_calls systemctl)" "--user disable --now tehas-plugin.timer"
t_eq "$(cat "$units/tehas-plugin.timer" "$units/tehas-plugin.service")" "[Timer]
[Service]"
t_run "$install"
t_contains "$t_out" "tehas is up to date"
t_lacks "$t_out" "tehas-plugin.timer"

t_case "a timer of a feature that is off that systemd cannot disable is left for the next run"
rm -f "$T/disabled"
t_stub systemctl '[ "$2" != disable ]'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: systemctl --user disable --now tehas-plugin.timer   (its feature is off)"
rm -f "$units/tehas-plugin.timer" "$units/tehas-plugin.service"
t_stub systemctl

t_case "while a poll holds the dispatcher's lock nothing is installed, and the next run removes them"
old_layout
mkdir -p "$th/state"
lock=$th/state/dispatch.lock
bash -c 'exec 8>"$1"; flock 8; exec sleep 60' _ "$lock" &
holder=$!
for _ in $(seq 50); do flock -n "$lock" true || break; sleep 0.1; done
rm -f "$th/bin/tehas"
t_run "export TEHAS_DISPATCH_LOCK_WAIT=0; $install"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: a poll is running ($lock still held after 0s); nothing was installed"
t_lacks "$t_out" "removed: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-status" ] && echo there)" "there"
t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
kill "$holder" 2>/dev/null
wait "$holder" 2>/dev/null
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/bin/tehas-status"
t_eq "$([ -e "$th/bin/tehas-status" ] || echo gone)" "gone"
t_eq "$([ -e "$th/bin/tehas" ] && echo there)" "there"
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

# A tree may carry a polyfill (scripts/polyfills.sh). A case makes the tree:
# a copy of the fixture, once with the file in its lib/ and once without.
make_checkout() { # <dir> <polyfill?>
  local d=$1 with=${2:-}
  rm -rf "$d"
  mkdir -p "$d"
  cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
    "$fixture/libexec" "$d/"
  # An install of a checkout reads the instance's models.json before it copies
  # anything, so the copy carries the file this file's instance() writes.
  cp "$TEHAS_INSTANCE_DIR/models.json" "$d/host/"
  t_console_source "$d" "$fixture"
  [ -z "$with" ] || echo '# polyfill of a feature the image lacks' >"$d/lib/polyfill-demo.sh"
}

t_case "a polyfill the tree carries is installed beside the other libraries"
# The host of the cases above is the one the real run of this file made, and it
# keeps what an install puts; clear it so neither case sees the other's lib/.
fresh
instance
# install.sh reads its own directory, so the tree is a copy of this checkout
# with the polyfill added to its lib/.
checkout=$T/checkout
make_checkout "$checkout" demo
t_run "cd '$checkout' && ./install.sh"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/lib/polyfill-demo.sh"
t_eq "$([ -f "$th/lib/polyfill-demo.sh" ] && echo there)" "there"
t_eq "$(stat -L -c %a "$th/lib/polyfill-demo.sh")" "644"
# A feature the image lacks and the tree carries is `polyfill`, asked of the
# tehas-cycle beside the tree's lib/; the build here is a stub, so it is put back.
mode_in() { # <tree> <feature>: what tehas-cycle in that tree answers
  local held=""
  [ ! -e "$1/libexec/tehas-cycle" ] || { held=$T/held-cycle; mv "$1/libexec/tehas-cycle" "$held"; }
  cp "$T_CYCLE" "$1/libexec/tehas-cycle"
  t_run "'$1/libexec/tehas-cycle' caps mode $2 <<<'{}'"
  rm -f "$1/libexec/tehas-cycle"
  [ -z "$held" ] || mv "$held" "$1/libexec/tehas-cycle"
}
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
mode_in "$th" demo
t_eq "$t_out" "polyfill"
mkdir -p "$checkout/libexec"
mode_in "$checkout" demo
t_eq "$t_out" "polyfill"
# The host the install just made is current, apart from the follow-ups its own
# copies asked for, which no run of this file has taken.
t_run "rm -f '$th/state/update-pending'; cd '$checkout' && ./install.sh --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "polyfill-demo.sh"

t_case "a tree with no polyfill installs no polyfill file"
fresh
instance
make_checkout "$checkout"
t_run "cd '$checkout' && ./install.sh"
t_eq "$t_rc" 0
t_eq "$(cd "$th/lib" && find . -name 'polyfill-*' | tr '\n' ' ')" ""
mode_in "$th" demo
t_eq "$t_out" "off"

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
