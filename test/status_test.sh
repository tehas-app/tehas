#!/usr/bin/env bash
# Tests for the three scripts around the factory's state: libexec/status
# writes the queue snapshot and runs libexec/health beside it, and
# libexec/summary prints the health file for `tehas status`.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

status="$T_ROOT/libexec/status"
health="$T_ROOT/libexec/health"
summary="$T_ROOT/libexec/summary"
live=$T/home/tehas/live

t_stub gh '
[ ! -e "$T/gh-down" ] || exit 1
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) [ ! -e "$T/all-down" ] || exit 1; cat "$T/all.json" ;;
  "issue view"*) cat "$T/paused" ;;
esac'
issue() { # number label state
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}' "$1" "${3:-OPEN}" "$2"
}
reset() {
  rm -rf "$T"/calls/* "$T/gh-down" "$T/all-down" "$live"
  echo "[]" >"$T/all.json"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
  echo false >"$T/paused"
}
cycle() { # running status step
  mkdir -p "$live"
  printf '{"issue":12,"running":%s,"status":"%s","step":"%s","startedAt":%s,"updatedAt":%s}\n' \
    "$1" "$2" "$3" "$(($(date +%s) - 600))" "$(($(date +%s) - 180))" >"$live/running.json"
}

t_case "status writes the snapshot and the health file, without a command on PATH"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.review "$live/health.json")" 1

t_case "the breakroom's words are counted over every issue, labelled for the factory or not"
reset
cat >"$T/all.json" <<'EOF2'
[{"title": "Dashboard shows the Queue", "labels": [{"name": "tehas:ready"}]},
 {"title": "A side-project: migrate the dashboard to v2", "labels": [{"name": "sidework"}, {"name": "Help Wanted"}]},
 {"title": "queue, QUEUE; dashboard!", "labels": []},
 {"title": null}, 7]
EOF2
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '.buzzwords[:2]' "$live/status.json")" '[{"word":"dashboard","count":3},{"word":"queue","count":3}]'
# Whole words in lower case, a hyphen kept, two letters too few; no title and
# no issue number is published with them.
t_eq "$(jq -r '[.buzzwords[].word] | sort | join(" ")' "$live/status.json")" \
  "dashboard help migrate queue ready shows side-project sidework tehas the wanted"
t_eq "$(jq -r '.buzzwords[] | select(.word == "sidework") | .count' "$live/status.json")" 1
t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --limit 1000 --json title,labels"
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2

t_case "at most a hundred words are published, the most used first"
reset
jq -n '[range(150) | {title: ("word\(.) " * (150 - .)), labels: []}]' >"$T/all.json"
t_run "$status"
t_eq "$(jq -r '.buzzwords | length' "$live/status.json")" 100
t_eq "$(jq -c '.buzzwords[0]' "$live/status.json")" '{"word":"word0","count":150}'

t_case "without the list of every issue the snapshot is written with no words"
reset
: >"$T/all-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r 'has("buzzwords")' "$live/status.json")" false
echo '{"message": "not a list"}' >"$T/all.json"
rm "$T/all-down"
t_run "$status"
t_eq "$(jq -r '[(.queued | length), has("buzzwords")] | join(" ")' "$live/status.json")" "2 false"

t_case "with GitHub down the old snapshot stays and health is still refreshed"
reset
t_run "$status"
before=$(cksum <"$live/status.json")
rm "$live/health.json"
: >"$T/gh-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(cksum <"$live/status.json")" "$before"
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "status with invalid settings writes nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$status"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$live" ] && echo there)" ""

t_case "summary: an idle factory in three lines, exit 0"
reset
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 3
t_contains "$t_out" "factory   pass: Idle, taking work. ("
t_contains "$t_out" "s ago)"
t_contains "$t_out" "queue     2 in line, 0 building, 1 in review, 1 stopped"
t_contains "$t_out" "cycle     none yet"

t_case "summary: a running cycle"
reset
cycle true running develop
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12."
t_contains "$t_out" "cycle     #12 running, step develop, started 10 min ago"

t_case "summary: a parked cycle is not a failure"
reset
cycle false handoff review
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "cycle     #12 handoff at step review, 3 min ago"

t_case "summary: a paused factory says so, exit 0"
reset
echo true >"$T/paused"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   warn: The factory is paused."
t_contains "$t_out" "1 stopped, paused"

t_case "summary: a failed cycle is exit 1, and still printed"
reset
cycle false aborted develop
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."
t_contains "$t_out" "cycle     #12 aborted at step develop"

t_case "summary: a health file older than ten minutes is not believed, exit 1"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   stale: the dispatcher last reported 3 h ago"
t_contains "$t_out" "It said pass: Idle, taking work."
t_contains "$t_out" "queue     2 in line"

t_case "summary: nine minutes old is still believed"
jq --arg t "$(date -u -d '-9 minutes' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Idle, taking work. (9 min ago)"

t_case "summary: a time that cannot be read counts as old"
jq '.time = "yesterday"' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   stale: the dispatcher last reported at an unknown time"

t_case "summary: no snapshot yet is a failure with the reason"
reset
t_run "$health; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: No queue snapshot yet"
t_contains "$t_out" "queue     no snapshot yet"

t_case "summary: no health file is exit 1 with one line, and nothing is written"
reset
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas status: no health file at $live/health.json yet"
t_eq "$([ -e "$live" ] && echo there)" ""

t_case "summary: a health file that is not JSON is the same"
reset
mkdir -p "$live"
echo "not json" >"$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "no health file"

t_case "summary reads nothing but the health file: no settings, no gh, no write"
reset
t_run "$status"
rm -f "$T"/calls/*
t_instance 'echo "settings were read" >&2; exit 9'
before=$(cd "$live" && cksum ./*)
t_run "$summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls gh)" ""
t_eq "$(cd "$live" && cksum ./*)" "$before"

t_case "summary takes no argument"
reset
t_run "$summary --json"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas status"

t_done
