#!/usr/bin/env bash
# Tests for tehas panel: setting, showing, replacing and clearing the pages the
# dashboard links, what is refused, and the two files that hold the list.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

panel="$T_ROOT/bin/tehas panel"
kept=$T/home/tehas/state/panels.json
shown=$T/home/tehas/live/panels.json
t_stub gh

# The list is read and written by the console (console/src/panel.rs). A command
# looks for it beside itself, where install.sh puts it, which a test has none
# of: TEHAS_CONSOLE names the console of this checkout instead.
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
}
there() { [ -e "$1" ] && echo there; }

t_case "no panels: said, and an empty list as JSON"
reset
t_run "$panel"
t_eq "$t_rc" 0
t_contains "$t_out" "no panels"
t_run "$panel --json"
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"panels":[]}'
t_eq "$(there "$kept")$(there "$shown")" ""

t_case "set keeps the panel in state/ and publishes the same object to live/"
reset
t_run "$panel set support --label 'Ask us' --url https://support.example.org/f/acme"
t_eq "$t_rc" 0
t_contains "$t_out" "support"
t_run "$panel --json"
t_eq "$t_out" '{"api":1,"panels":[{"id":"support","label":"Ask us","url":"https://support.example.org/f/acme"}]}'
t_eq "$(jq -c . "$shown")" "$t_out"
t_eq "$(cat "$kept")" "$(cat "$shown")"
t_eq "$(stat -c %a "$kept")" 600
t_eq "$(stat -c %a "$shown")" 644
t_eq "$(t_calls gh)" ""

t_case "set again is the same list, and set on an id that is there replaces it in its place"
reset
t_run "$panel set a --label A --url https://a.example.net/"
t_run "$panel set b --label B --url https://b.example.net/"
t_run "$panel set b --label B --url https://b.example.net/"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.panels[].id] | join(",")' "$shown")" "a,b"
t_run "$panel set a --label 'A again' --url http://a.example.net:8080/x"
t_eq "$t_rc" 0
t_eq "$(jq -c '.panels[0]' "$shown")" '{"id":"a","label":"A again","url":"http://a.example.net:8080/x"}'
t_eq "$(jq -r '[.panels[].id] | join(",")' "$shown")" "a,b"

t_case "a control character in a label becomes a space"
reset
t_run "$panel set a --label \$'Ask\\nus\\t' --url https://a.example.net/"
t_eq "$t_rc" 0
t_eq "$(jq -r '.panels[0].label' "$shown")" "Ask us"

t_case "clear takes a panel away, clear of one that is not there is fine, and the last takes the files"
reset
t_run "$panel set a --label A --url https://a.example.net/"
t_run "$panel set b --label B --url https://b.example.net/"
t_run "$panel clear a"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.panels[].id] | join(",")' "$shown")" "b"
t_run "$panel clear nothing-here"
t_eq "$t_rc" 0
t_run "$panel clear b"
t_eq "$t_rc" 0
t_eq "$(there "$kept")$(there "$shown")" ""
t_run "$panel clear b"
t_eq "$t_rc" 0

t_case "a fifth panel is refused and the four are kept"
reset
for id in a b c d; do t_run "$panel set $id --label $id --url https://$id.example.net/"; done
before=$(cat "$shown")
t_run "$panel set e --label e --url https://e.example.net/"
t_eq "$t_rc" 64
t_contains "$t_out" "at most 4"
t_eq "$(cat "$shown")" "$before"

t_case "each bad id, label and url is 64 and writes nothing"
long_id=$(printf 'x%.0s' {1..33})
long_label=$(printf 'x%.0s' {1..25})
long_url="https://a.example.net/$(printf 'x%.0s' {1..491})"
for args in \
  "set Support --label A --url https://a.example.net/" \
  "set a_b --label A --url https://a.example.net/" \
  "set $long_id --label A --url https://a.example.net/" \
  "set '' --label A --url https://a.example.net/" \
  "set a --label '' --url https://a.example.net/" \
  "set a --label '  ' --url https://a.example.net/" \
  "set a --label $long_label --url https://a.example.net/" \
  "set a --label A --url ftp://a.example.net/" \
  "set a --label A --url 'javascript:alert(1)'" \
  "set a --label A --url a.example.net" \
  "set a --label A --url https://" \
  "set a --label A --url https://user:pw@a.example.net/" \
  "set a --label A --url 'https://a.example.net/ x'" \
  "set a --label A --url $long_url" \
  "set a --label A --url https://example.org/" \
  "set a --label A --url https://EXAMPLE.org./x" \
  "set a --label A" \
  "set a --url https://a.example.net/" \
  "set a --label A --url https://a.example.net/ --label B" \
  "clear Bad" \
  "clear" \
  "show" \
  "--json x"; do
  reset
  t_run "$panel $args"
  t_eq "$t_rc" 64
  t_eq "$(there "$kept")$(there "$shown")$(there "$T/home/tehas/state")" ""
done

t_case "a url of 512 characters is taken, and a sibling of the factory's domain"
reset
t_run "$panel set a --label A --url https://a.example.net/$(printf 'x%.0s' {1..490})"
t_eq "$t_rc" 0
t_run "$panel set b --label B --url https://support.example.org/"
t_eq "$t_rc" 0
t_eq "$(jq -r '.panels | length' "$shown")" 2

t_case "without an instance nothing is written, exit 78"
reset
t_no_instance
t_run "$panel set a --label A --url https://a.example.net/"
t_eq "$t_rc" 78
t_eq "$(there "$T/home/tehas/state")$(there "$shown")" ""

t_case "the help text names the command"
reset
t_run "$T_ROOT/bin/tehas panel --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: tehas panel"

t_done
