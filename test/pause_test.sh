#!/usr/bin/env bash
# Tests for libexec/pause and libexec/resume against a stub gh: what the flag
# holds and who can read it, what a second pause keeps, what resume removes,
# and what resume says about the other switch, tehas:pause on the control issue.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

pause="$T_ROOT/libexec/pause"
resume="$T_ROOT/libexec/resume"
flag=$T/home/tehas/state/pause.json

t_stub gh '
case "$1 $2" in
  "issue view") cat "$T/labels.json" ;;
  "api repos/acme/widget/issues/1/events?per_page=100") cat "$T/events.json" ;;
  "issue edit") exit 1 ;;
esac'

reset() {
  rm -f "$T"/calls/* "$flag"
  rm -rf "$T/home/tehas/state"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo false >"$T/labels.json"
  echo '[]' >"$T/events.json"
}
# The control issue's pause events, newest last.
pause_events() { # <event> <login or "">
  local actor
  [ -z "$2" ] && actor="null" || actor="{\"login\":\"$2\"}"
  jq -nc --arg e "$1" --argjson a "$actor" \
    '[{event: $e, label: {name: "tehas:pause"}, actor: $a, created_at: "2026-01-02T00:00:00Z"}]' >"$T/events.json"
}

t_case "pause writes the flag: who set it, when, and the reason"
reset
t_run "$pause 'waiting for the gate'"
t_eq "$t_rc" 0
t_contains "$t_out" "paused at "
t_contains "$t_out" "waiting for the gate"
t_eq "$(jq -r '.reason' "$flag")" "waiting for the gate"
t_eq "$(jq -r '[(.at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")), (.by | type == "string")] | map(tostring) | join(",")' "$flag")" "true,true"
t_eq "$(jq -rc 'keys | join(",")' "$flag")" "at,by,reason"
t_eq "$(t_calls gh)" ""

t_case "the flag is readable by this account alone"
t_eq "$(stat -c %a "$flag")" "600"

t_case "pause with no argument records no reason"
reset
t_run "$pause"
t_eq "$t_rc" 0
t_eq "$(jq -r .reason "$flag")" ""

t_case "the reason is kept to 200 characters"
reset
t_run "$pause $(printf 'x%.0s' {1..400})"
t_eq "$(jq -r .reason "$flag" | wc -c)" 201

t_case "pause again succeeds and keeps the first flag"
reset
t_run "$pause first"
t_run "$pause second"
t_eq "$t_rc" 0
t_contains "$t_out" "already paused"
t_contains "$t_out" "the flag is unchanged"
t_eq "$(jq -r .reason "$flag")" "first"

t_case "a flag that cannot be read is replaced, and said"
reset
mkdir -p "${flag%/*}"
echo '{"at": 7}' >"$flag"
chmod 600 "$flag"
t_run "$pause fresh"
t_eq "$t_rc" 0
t_contains "$t_out" "is not a pause flag"
t_eq "$(jq -r .reason "$flag")" "fresh"

t_case "resume removes the flag, and says so"
reset
t_run "$pause x"
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "resumed: the pause flag is gone"
t_eq "$([ -e "$flag" ] && echo there)" ""

t_case "resume with no flag succeeds"
reset
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "no pause flag on this host"

t_case "resume changes no label"
reset
t_run "$resume"
t_lacks "$(t_calls gh)" "issue edit"

t_case "resume says when the label still holds the host"
reset
echo true >"$T/labels.json"
t_run "$pause x"
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "the factory stays paused: tehas:pause is still on issue #1"
t_eq "$([ -e "$flag" ] && echo there)" ""

t_case "resume says when a stranger lifted the label, and stays silent when an allowed account did"
reset
pause_events unlabeled tehas-bot
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "taken off #1 by 'tehas-bot'"
t_contains "$t_out" "stays paused"
pause_events unlabeled alice
t_run "$resume"
t_eq "$t_rc" 0
t_lacks "$t_out" "stays paused"
pause_events unlabeled ""
t_run "$resume"
t_contains "$t_out" "taken off #1 by 'unknown'"
t_eq "$(t_calls gh | grep -c 'events')" 3

t_case "resume says when nothing took the label off, only applied it"
reset
pause_events labeled mallory
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "its last tehas:pause event only applied it"
t_contains "$t_out" "stays paused"

t_case "resume with no pause event on the control issue says nothing extra"
reset
t_run "$resume"
t_lacks "$t_out" "stays paused"

t_case "resume that cannot read the control issue says so and still removes the flag"
reset
rm -f "$T/labels.json"
t_run "$pause x"
t_run "$resume"
t_eq "$t_rc" 0
t_contains "$t_out" "the labels of issue #1 could not be read"
t_eq "$([ -e "$flag" ] && echo there)" ""

t_case "an argument is a usage error, before the settings are read"
reset
t_run "$pause too many arguments here"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas pause [reason]"
t_run "$resume 7"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas resume"
t_eq "$([ -e "$flag" ] && echo there)" ""

t_case "help prints the text of each command and writes nothing"
reset
for name in pause resume; do
  t_run "$T_ROOT/libexec/$name --help"
  t_eq "$t_rc" 0
  t_contains "$t_out" "Usage: tehas $name"
  t_lacks "$t_out" "set -uo pipefail"
done
t_eq "$([ -e "$flag" ] && echo there)" ""

t_case "without instance settings nothing is written, exit 78"
reset
rm -rf "$TEHAS_INSTANCE_DIR"
t_run "$pause why"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$([ -e "$T/home/tehas/state" ] && echo there)" ""
t_run "$resume"
t_eq "$t_rc" 78
t_eq "$([ -e "$flag" ] && echo there)" ""
t_eq "$(t_calls gh)" ""

t_case "resume without the settings it reads the control issue with stops at 78"
reset
t_instance TEHAS_REPO=acme/widget TEHAS_CONTROL_ISSUE=1
t_run "$resume"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS is not set"
t_eq "$([ -e "$flag" ] && echo there)" ""

t_done
