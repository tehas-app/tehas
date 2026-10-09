#!/usr/bin/env bash
# Tests for scripts/check-generic.sh: a private address in a platform file is
# reported, docs/ is exempt, and the list of names is used only when it is
# there. Each case runs the lint in a scratch repository.
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

t_case "docs/ may name what happened"
fresh
echo "the host was at $addr" >"$repo/docs/log.md"
t_run "$lint"
t_eq "$t_rc" 0

t_case "without a list of names only addresses are checked"
fresh
echo "# for the widgetshop product" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "a list of names adds words and exceptions"
printf '%s\n' "words+='|widgetshop'" 'exceptions+=("scripts/check-generic.names")' >"$repo/scripts/check-generic.names"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:# for the widgetshop product"
t_lacks "$t_out" "check-generic.names:"

t_done
