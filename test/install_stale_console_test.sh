#!/usr/bin/env bash
# Tests for install.sh, split from install_stale_test.sh: the scripts the console
# replaced are named, then removed, and what the console finds left over reaches
# the run. The markers a cycle used to leave are the console's
# (console/ops/src/update/leftovers.rs).
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "the settings wrappers of before console run are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
gone=""
for f in clean init-product preview protolab refresh with-settings; do gone+=" $th/libexec/$f"; done
gone+=" $th/lib/disk.sh"
for f in $gone; do echo "# old" >"$f"; done
t_run "$install --check"
t_eq "$t_rc" 1
for f in $gone; do t_contains "$t_out" "stale: $f"; done
t_run "$install"
t_eq "$t_rc" 0
for f in $gone; do
  t_contains "$t_out" "removed: $f"
  t_eq "$([ -e "$f" ] && echo there)" ""
done
# The host panel's publisher, which ran with-settings at each mark, left core
# in 2.0: no unit restarts for it.
t_lacks "$(cat "$th/state/update-pending" 2>/dev/null)" "tehas-top.service"
for f in dev-login setup update; do
  t_eq "$([ -x "$th/libexec/$f" ] && echo there)" "there"
done

t_case "the scripts that only exec'd the console are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
gone=""
for f in check-product dev dispatch github-token improve plan review; do gone+=" $th/libexec/$f"; done
for f in $gone; do echo "# old" >"$f"; done
t_run "$install --check"
t_eq "$t_rc" 1
for f in $gone; do t_contains "$t_out" "stale: $f"; done
t_run "$install"
t_eq "$t_rc" 0
for f in $gone; do
  t_contains "$t_out" "removed: $f"
  t_eq "$([ -e "$f" ] && echo there)" ""
done

t_case "the scripts that took a lock for tehas-cycle are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
gone=""
for f in cycle index work; do gone+=" $th/libexec/$f"; done
for f in $gone; do echo "# old" >"$f"; done
t_run "$install --check"
t_eq "$t_rc" 1
for f in $gone; do t_contains "$t_out" "stale: $f"; done
t_run "$install"
t_eq "$t_rc" 0
for f in $gone; do
  t_contains "$t_out" "removed: $f"
  t_eq "$([ -e "$f" ] && echo there)" ""
done

t_case "the script of tehas export of before the console is named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
echo "# old" >"$th/libexec/export"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/libexec/export"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/libexec/export"
t_eq "$([ -e "$th/libexec/export" ] && echo there)" ""

t_case "what the console finds left over is drift for --check, and a real run says what it removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
: >"$th/sandbox/sandbox-uid.Dockerfile"
mkdir -p "$th/state/cycles"
: >"$th/state/cycles/5"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/sandbox/sandbox-uid.Dockerfile"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/sandbox/sandbox-uid.Dockerfile"
t_contains "$t_out" "removed: $th/state/cycles/5"
t_eq "$(ls -A "$th/state/cycles")" ""

t_done
