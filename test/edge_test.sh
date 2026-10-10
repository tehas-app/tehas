#!/usr/bin/env bash
# Tests for lib/edge.sh, what tehasbot protolab and tehasbot preview share,
# and for the router's files in host/edge.
# shellcheck disable=SC2016  # snippets and patterns are expanded later, or never
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

lib='. "$T_ROOT/lib/edge.sh"'
d=$T/home/tehas
mkdir -p "$d"

t_case "edge_put writes the whole file or leaves the old one"
echo old >"$d/list.json"
t_run "$lib; edge_put '$d/list.json' echo new; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(cat "$d/list.json")" "new"
t_run "$lib; edge_put '$d/list.json' bash -c 'echo half; exit 3'; echo rc=\$?"
t_eq "$t_out" "rc=1"
t_eq "$(cat "$d/list.json")" "new"
t_eq "$(find "$d" -mindepth 1 -maxdepth 1 -printf '%f ')" "list.json "

t_case "edge_place puts a directory in place of the old one"
mkdir -p "$d/site/.work/7" "$d/site/7"
echo fresh >"$d/site/.work/7/index.html"
echo stale >"$d/site/7/index.html"
t_run "$lib; edge_place '$d/site/.work/7' '$d/site/7'; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(cat "$d/site/7/index.html")" "fresh"
t_eq "$(find "$d/site" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" ".work 7 "

t_case "edge_numbers lists the numbered directories only"
mkdir -p "$d/site/12" "$d/site/x1" "$d/site/3a" "$d/site/repo.git"
: >"$d/site/99"
t_run "$lib; edge_numbers '$d/site' | sort -n | tr '\n' ' '"
t_eq "$t_out" "7 12 "
t_run "$lib; edge_numbers '$d/nowhere'; echo rc=\$?"
t_eq "$t_out" "rc=0"

t_case "edge_lock is held by one process at a time"
t_run "$lib; edge_lock demo || exit 9; ( exec 9>&-; $lib; edge_lock demo; echo second=\$? ); echo first=ok"
t_eq "$t_out" "second=1
first=ok"
t_run "$lib; edge_lock demo; echo again=\$?"
t_eq "$t_out" "again=0"

t_done
