#!/usr/bin/env bash
# Tests for install.sh: the commands of the earlier layout are named by
# --check and removed by a real run, and when they stay.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

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
rm -f "$T/calls/systemctl"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"
t_contains "$t_out" "removed: $th/bin/tehas-test"
# A running sampler is the old file calling itself. The units step restarts it,
# so the step is asked for and owed although no unit file changed.
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-clean.timer"
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

t_case "while a poll holds the dispatcher's lock nothing is installed, and the next run removes them"
old_layout
mkdir -p "$th/state"
lock=$th/state/dispatch.lock
bash -c 'exec 8>"$1"; flock 8; exec sleep 60' _ "$lock" &
holder=$!
for _ in $(seq 50); do flock -n "$lock" true || break; sleep 0.1; done
rm -f "$th/bin/tehas"
t_run "${no_wait}$install"
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

# A tree may carry a polyfill of a pi-rukas feature (scripts/polyfills.sh);
# install.sh has no name for one, so a case makes the tree rather than the
# repository holding one: the same tree twice, once with the file in its lib/
# and once without. Nothing under $T_ROOT is written, and the checkout's lib/
# is a copy, so the polyfill is never sourced from the tree that has it.
make_checkout() { # <dir> <polyfill?>
  local d=$1 with=${2:-}
  rm -rf "$d"
  mkdir -p "$d"
  cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" \
    "$T_ROOT/libexec" "$d/"
  # An install of a checkout reads the instance's models.json before it copies
  # anything, so the copy carries the file this file's instance() writes.
  cp "$TEHAS_INSTANCE_DIR/models.json" "$d/host/"
  t_console_source "$d"
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
# The host answers the way the checkout does: a feature the image lacks and the
# tree carries is `polyfill`, not `off`. The file the function reads for it is
# the one beside the headless.sh it was sourced from, so the checkout's answer
# is read from a copy of the checkout's own file: sourced from $T_ROOT/lib it
# would look for the polyfill beside that headless.sh, and the rule of these
# cases is that the repository carries none.
t_run ". '$th/lib/headless.sh' && tehas_headless_mode '{}' demo"
t_eq "$t_out" "polyfill"
t_run "cp '$T_ROOT/lib/headless.sh' '$checkout/lib/headless.sh' && . '$checkout/lib/headless.sh' && tehas_headless_mode '{}' demo"
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
t_run ". '$th/lib/headless.sh' && tehas_headless_mode '{}' demo"
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
