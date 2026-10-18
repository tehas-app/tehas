#!/usr/bin/env bash
# Tests for install.sh: the Protolab server's files of before the edge
# router, and the markers a cycle used to leave, are removed.
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

# A cycle used to leave one empty file per issue number in state/cycles/. It
# read nothing and the flag it fed is gone, so a host that ran cycles then has
# the files to lose. Only a name of digits alone is one of them.
t_case "the marker a cycle used to leave is removed, and its neighbours stay"
fresh
instance
t_run "$install"
mkdir -p "$th/state/cycles"
for n in 5 23 25 26; do : >"$th/state/cycles/$n"; done
for suffix in lock taken park result.json; do echo keep >"$th/state/cycles/5.$suffix"; done
echo keep >"$th/state/cycles/5x"
echo keep >"$th/state/cycles/notanumber"
echo keep >"$th/state/cycles/.hidden"
mkdir "$th/state/cycles/77"
ln -s /etc/passwd "$th/state/cycles/99"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/state/cycles/5"
t_contains "$t_out" "removed: $th/state/cycles/26"
t_eq "$(cd "$th/state/cycles" && find . | LC_ALL=C sort | tr '\n' ' ')" \
  ". ./.hidden ./5.lock ./5.park ./5.result.json ./5.taken ./5x ./77 ./99 ./notanumber "


t_done
