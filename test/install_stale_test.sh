#!/usr/bin/env bash
# Tests for install.sh: the Protolab server's files of before the edge
# router, the markers a cycle used to leave, and the copy a sandbox build staged
# beside the installed script, are removed.
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
t_eq "$([ -e "$th/lib/history-lines.jq" ] && echo there)" "there"
t_eq "$([ -e "$th/lib/park-fields.jq" ] && echo there)" "there"

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

t_case "the copy a build left in the sandbox directory is named, then removed"
fresh
instance
t_run "$install"
rm -f "$T"/calls/*
# The two names a sandbox build stages beside the script, found here as the
# leftovers of a run that never reached its cleanup. The recipe of the image and
# the build script themselves are installed files and stay.
printf 'FROM nowhere\n' >"$th/sandbox/sandbox-uid.Dockerfile"
printf 'FROM nowhere\n' >"$th/sandbox/sandbox-uid.Dockerfile.abcdef"
printf 'toolong\n' >"$th/sandbox/sandbox-uid.Dockerfile.toolong"
printf 'x\n' >"$th/sandbox/sandbox-uid.Dockerfilex"
printf 'x\n' >"$T/outside-named"
ln -s "$T/outside-named" "$th/sandbox/sandbox-uid.Dockerfile.link01"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/sandbox/sandbox-uid.Dockerfile"
t_contains "$t_out" "stale: $th/sandbox/sandbox-uid.Dockerfile.abcdef"
t_eq "$([ -e "$th/sandbox/sandbox-uid.Dockerfile" ] && echo there)" "there"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/sandbox/sandbox-uid.Dockerfile"
t_contains "$t_out" "removed: $th/sandbox/sandbox-uid.Dockerfile.abcdef"
t_eq "$([ -e "$th/sandbox/sandbox-uid.Dockerfile" ] && echo there)" ""
t_eq "$([ -e "$th/sandbox/sandbox-uid.Dockerfile.abcdef" ] && echo there)" ""
# The shapes that are not a build's two names are left for their owner, as the
# installed files beside them are: an installer that retires a name it did not
# ship has to name the shape it means.
t_eq "$([ -e "$th/sandbox/sandbox-uid.Dockerfile.toolong" ] && echo there)" "there"
t_eq "$([ -e "$th/sandbox/sandbox-uid.Dockerfilex" ] && echo there)" "there"
t_eq "$([ -L "$th/sandbox/sandbox-uid.Dockerfile.link01" ] && echo there)" "there"
t_eq "$([ -e "$T/outside-named" ] && echo there)" "there"
t_eq "$([ -s "$th/sandbox/Dockerfile" ] && echo there)" "there"
t_eq "$([ -s "$th/sandbox/build.sh" ] && echo there)" "there"


t_done
