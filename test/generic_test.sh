#!/usr/bin/env bash
# Tests for scripts/check-generic.sh: a private address is reported in every
# file, and the list of names is data, used only when it is there and only
# for names. Each case runs the lint in a scratch repository.
# shellcheck disable=SC2016  # one case feeds the lint a line of shell, unexpanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

# Built here, not written out: this file is linted too.
addr="100.$((60 + 40)).2.3"

repo=$T/repo
fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/bin" "$repo/docs"
  cp "$T_ROOT/scripts/check-generic.sh" "$repo/scripts/"
  git -C "$repo" init -q
  echo "echo hello" >"$repo/bin/tool"
}
lint="cd '$repo' && scripts/check-generic.sh"

t_case "a clean tree passes"
fresh
t_run "$lint"
t_eq "$t_rc" 0
t_contains "$t_out" "check-generic: ok"

t_case "a private address in a platform file is reported"
fresh
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:ADDR=$addr"

t_case "a documentation address and a longer number are not private addresses"
fresh
printf '%s\n' "ADDR=192.0.2.7" "VERSION=1$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "every private range is an address, however it is written into a line"
tld=net
for a in "10.$((1 + 1)).3.4" "192.$((160 + 8)).1.1" "172.$((10 + 6)).0.1" "172.$((30 + 1)).9.9" "169.$((250 + 4)).1.1" \
  "100.$((60 + 4)).0.1" "100.$((120 + 7)).255.1" "host_$addr" "x=$addr:8080" "fd$((6 + 1))a:115c:a1e0::1" "node.tail1234.ts.$tld"; do
  fresh
  echo "see $a here" >"$repo/bin/tool"
  t_run "$lint"
  t_eq "$t_rc" 1
  t_contains "$t_out" "bin/tool:1:"
done

t_case "addresses just outside the private ranges, and versions, pass"
fresh
printf '%s\n' "172.$((30 + 2)).0.1" "100.$((120 + 8)).0.1" "100.$((60 + 3)).0.1" "8.8.8.8" "v1.10.$((1 + 1)).3.4" "127.0.0.1" "::1" "abcd:ef01" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "an address is reported in docs/ too: every commit is public"
fresh
echo "the host was at $addr" >"$repo/docs/log.md"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "docs/log.md:1:"

t_case "an address is found in a file that is not plain text"
fresh
printf 'ADDR=%s\0binary\n' "$addr" >"$repo/bin/blob"
t_run "$lint"
t_eq "$t_rc" 1

t_case "without a list of names only addresses are checked"
fresh
echo "# for the widgetshop product" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "a list of names adds words, and docs/ may use them"
printf '%s\n' "# names of one installation" "widgetshop" "gizmo-[0-9]+" >"$repo/scripts/check-generic.names"
echo "widgetshop went live" >"$repo/docs/log.md"
echo "gizmo-7" >"$repo/bin/other"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:# for the widgetshop product"
t_contains "$t_out" "bin/other:1:gizmo-7"
t_lacks "$t_out" "check-generic.names:"
t_lacks "$t_out" "docs/log.md"

t_case "the list can allow a path for names"
printf '%s\n' "widgetshop" "allow bin/tool" >"$repo/scripts/check-generic.names"
rm "$repo/bin/other"
t_run "$lint"
t_eq "$t_rc" 0

t_case "the list cannot exempt a file from the address rule"
printf '%s\n' "widgetshop" "allow bin/" "allow " >"$repo/scripts/check-generic.names"
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:ADDR=$addr"

t_case "the list is data: a line of shell is refused, not run"
for line in 'exceptions+=("")' "words='x'" 'touch "$T/ran-names"' 'a;b'; do
  fresh
  printf '%s\n' "$line" >"$repo/scripts/check-generic.names"
  t_run "$lint"
  t_eq "$t_rc" 1
  t_contains "$t_out" "not a name or an allow line"
done
t_eq "$(find "$T" -maxdepth 1 -name 'ran-names' | wc -l)" 0

t_case "the list of names is ignored by git in this repository"
t_eq "$(git -C "$T_ROOT" check-ignore -q scripts/check-generic.names && echo ignored)" ignored

t_done
