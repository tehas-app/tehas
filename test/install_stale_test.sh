#!/usr/bin/env bash
# Tests for install.sh: the Protolab server's files of before the edge
# router and the libraries no command sources are removed, and what the
# console finds left over reaches the run. The markers a cycle
# used to leave and the copy a sandbox build staged are the console's
# (console/ops/src/update/leftovers.rs).
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "the Protolab server's files of before the edge router are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
echo "# old" >"$th/edge/protolab.Caddyfile"
echo "# old" >"$th/edge/protolab.compose.yaml"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/edge/protolab.Caddyfile"
t_contains "$t_out" "stale: $th/edge/protolab.compose.yaml"
t_eq "$([ -e "$th/edge/protolab.Caddyfile" ] && echo there)" "there"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/edge/protolab.Caddyfile"
t_contains "$t_out" "removed: $th/edge/protolab.compose.yaml"
t_eq "$([ -e "$th/edge/protolab.compose.yaml" ] && echo there)" ""
t_eq "$(cat "$th/state/update-pending")" "edge --force-recreate"

t_case "the script and the jq programs of tehas history of before the console are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
gone="$th/libexec/history $th/lib/history-listing.jq $th/lib/history-repeats.jq"
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

t_case "the jq program of the bash report's repeat park is named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
f=$th/lib/park-fields.jq
echo "# old" >"$f"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $f"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $f"
t_eq "$([ -e "$f" ] && echo there)" ""

t_case "the libraries whose last callers moved into the console are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
gone=""
for f in attachments.sh cycles.sh labels.sh millwright.sh questions.sh rukas.sh sandbox.sh slice.sh store.sh history-lines.jq release.sh; do
  gone+=" $th/lib/$f"
done
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
for f in config.sh paths.sh github-app.sh release.list; do
  t_eq "$([ -e "$th/lib/$f" ] && echo there)" "there"
done

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
# tehasbot top of before this version runs with-settings at each mark, so the
# unit has to start again.
t_contains "$(cat "$th/state/update-pending")" "tehas-top.service"
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
