#!/usr/bin/env bash
# Tests for tehas limit: setting, showing and clearing the factory's limit,
# and lib/limit.sh, which reads it for everything that honours it.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

limit="$T_ROOT/bin/tehas limit"
file=$T/home/tehas/state/limit.json
t_stub gh

# The limit is read, written and printed by the console (console/src/flags.rs).
# A command looks for it beside itself, where install.sh puts it, which a test
# has none of: TEHAS_CONSOLE names the console of this checkout instead.
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
show() { t_run "$limit --json"; }
# The limit as lib/limit.sh reads it, at <now> epoch seconds or now.
read_limit() { t_run ". '$T_ROOT/lib/status.sh'; . '$T_ROOT/lib/limit.sh'; tehas_limit_read ${1:-}"; }
in_secs() { date -u -d "@$(($(date +%s) + $1))" +%FT%TZ; }

t_case "no limit: none, in words and as JSON"
reset
t_run "$limit"
t_eq "$t_rc" 0
t_eq "$t_out" "limit     none"
show
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"state":"none"}'
t_eq "$(t_calls gh)" ""

t_case "set --cycles: a limit with planning on and no expiry"
reset
t_run "$limit set --cycles 2"
t_eq "$t_rc" 0
t_eq "$t_out" "limit     2 cycles at once, planning on, until cleared"
show
t_eq "$t_out" '{"api":1,"state":"ok","cycles":2,"plan":true}'
t_eq "$(stat -c %a "$file")" 600
t_eq "$(find "$T/home/tehas/state" -name '*.new' | wc -l)" 0

t_case "set with every flag: planning off, an expiry and a reason"
reset
t_run "$limit set --cycles 1 --plan off --for 300 --reason 'held for alpha'"
t_eq "$t_rc" 0
t_contains "$t_out" "limit     1 cycle at once, planning off, until 20"
t_contains "$t_out" ": held for alpha"
show
t_eq "$(jq -c '{api, state, cycles, plan, reason}' <<<"$t_out")" '{"api":1,"state":"ok","cycles":1,"plan":false,"reason":"held for alpha"}'
t_eq "$(jq -r '(.until | fromdateiso8601) - now | . > 290 and . <= 300' <<<"$t_out")" true

t_case "set --until a time"
reset
until=$(in_secs 3600)
t_run "$limit set --cycles 0 --until $until"
t_eq "$t_rc" 0
show
t_eq "$(jq -r .until <<<"$t_out")" "$until"

t_case "setting twice with the same arguments leaves the same limit, exit 0 both times"
reset
until=$(in_secs 3600)
t_run "$limit set --cycles 3 --plan off --until $until --reason r"
first=$(cat "$file")
t_run "$limit set --cycles 3 --plan off --until $until --reason r"
t_eq "$t_rc" 0
t_eq "$(cat "$file")" "$first"

t_case "a new set replaces the whole limit"
reset
t_run "$limit set --cycles 1 --plan off --for 60 --reason old"
t_run "$limit set --cycles 4"
show
t_eq "$t_out" '{"api":1,"state":"ok","cycles":4,"plan":true}'

t_case "clear: none again, and clearing twice is fine"
reset
t_run "$limit set --cycles 1"
t_run "$limit clear"
t_eq "$t_rc" 0
t_eq "$t_out" "limit     none"
t_eq "$([ -e "$file" ] && echo there)" ""
t_run "$limit clear"
t_eq "$t_rc" 0

t_case "the reason is one line of at most 200 characters"
reset
t_run "$limit set --cycles 1 --reason \"\$(printf 'a\\tb\\nc\\001d')\""
show
t_eq "$(jq -r .reason <<<"$t_out")" "a b c d"
t_run "$limit set --cycles 1 --reason $(printf 'x%.0s' {1..300})"
show
t_eq "$(jq -r '.reason | length' <<<"$t_out")" 200

t_case "bad arguments are 64 and write nothing"
for args in "set" "set --cycles" "set --cycles -1" "set --cycles 1.5" "set --cycles x" "set --cycles 10000" \
  "set --cycles 1 --cycles 2" "set --cycles 1 --plan maybe" "set --cycles 1 --for 0" "set --cycles 1 --for 5s" \
  "set --cycles 1 --for 60 --until $(in_secs 600)" "set --cycles 1 --until tomorrow" \
  "set --cycles 1 --until $(in_secs -60)" "set --cycles 1 --bogus" "set --cycles 1 extra" \
  "clear now" "--json extra" "bogus" "--jsn"; do
  reset
  t_run "$limit $args"
  [ "$t_rc" = 64 ] || t_bad "'$args' exited $t_rc, not 64"
  [ ! -e "$file" ] || t_bad "'$args' wrote $file"
done
t_contains "$t_out" "usage: tehas limit"

t_case "an expired limit is read as expired, and says nothing starts"
reset
mkdir -p "${file%/*}"
jq -nc --arg u "$(in_secs -60)" '{v: 1, cycles: 2, plan: true, until: $u, reason: "r"}' >"$file"
show
t_eq "$(jq -c '{state, cycles, plan}' <<<"$t_out")" '{"state":"expired","cycles":2,"plan":true}'
t_run "$limit"
t_contains "$t_out" "limit     expired at "
t_contains "$t_out" "nothing starts until it is set again or cleared"

t_case "a limit that cannot be read is unreadable, never none"
for bad in 'not json' '[]' '{"v":2,"cycles":1,"plan":true,"until":null}' '{"v":1,"cycles":-1,"plan":true,"until":null}' \
  '{"v":1,"cycles":1,"plan":"yes","until":null}' '{"v":1,"cycles":1,"plan":true,"until":"soon"}' '{"v":1,"plan":true,"until":null}' ''; do
  reset
  mkdir -p "${file%/*}"
  printf '%s' "$bad" >"$file"
  show
  [ "$t_out" = '{"api":1,"state":"unreadable"}' ] || t_bad "'$bad' read as $t_out"
done
reset
mkdir -p "$file"
show
t_eq "$t_out" '{"api":1,"state":"unreadable"}'

t_case "lib/limit.sh: what a reader is given"
reset
read_limit
t_eq "$t_out" '{"state":"none"}'
t_run "$limit set --cycles 2 --plan off --until 2036-01-01T14:05:00Z --reason r"
read_limit "$(date -d 2036-01-01T14:00:00Z +%s)"
t_eq "$t_out" '{"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r"}'
read_limit "$(date -d 2036-01-01T14:05:00Z +%s)"
t_eq "$(jq -r .state <<<"$t_out")" expired

t_case "without an instance nothing is written, exit 78"
reset
t_no_instance
t_run "$limit set --cycles 1"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$([ -e "$file" ] && echo there)" ""

t_done
