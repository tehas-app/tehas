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
  "issue edit"*) [ -z "${GH_EDIT_FAILS:-}" ] || exit 1 ;;
  "api graphql"*) [ ! -e "$T/shipped-down" ] || exit 1; cat "$T/shipped.json" ;;
esac'
closed() { # number closedAt closer [label]: a closed issue as the search for shipped work returns it
  local label=""
  [ -z "${4:-}" ] || label="{\"name\":\"$4\"}"
  printf '{"number":%s,"title":"t","url":"u","updatedAt":"%s","closedAt":"%s","labels":{"nodes":[%s]},"timelineItems":{"nodes":[{"closer":%s}]}}' \
    "$1" "$2" "$2" "$label" "$3"
}
shipped() { # closed issues, as the GraphQL answer
  local IFS=,
  printf '{"data":{"search":{"nodes":[%s]}}}' "$*" >"$T/shipped.json"
}
issue() { # number label state
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}' "$1" "${3:-OPEN}" "$2"
}
reset() {
  rm -rf "$T"/calls/* "$T/gh-down" "$T/all-down" "$T/shipped-down" "$live"
  echo "[]" >"$T/all.json"
  shipped
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
  echo false >"$T/paused"
}
one() { # issue running status step [started [updated]]: one cycle as the work script writes it; times are seconds ago
  printf '{"issue":%s,"running":%s,"status":"%s","step":"%s","startedAt":%s,"updatedAt":%s}' \
    "$1" "$2" "$3" "$4" "$(($(date +%s) - ${5:-600}))" "$(($(date +%s) - ${6:-180}))"
}
cycle() { # running status step: issue 12, running or the last one
  mkdir -p "$live"
  if [ "$1" = true ]; then
    printf '{"cycles":[%s],"last":null}\n' "$(one 12 "$@")" >"$live/running.json"
  else
    printf '{"cycles":[],"last":%s}\n' "$(one 12 "$@")" >"$live/running.json"
  fi
}

t_case "status writes the snapshot and the health file, without a command on PATH"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.review "$live/health.json")" 1

t_case "a closed issue is not in line, being worked on or stopped, whatever label it carries"
reset
echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready CLOSED),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention),$(issue 8 tehas:running CLOSED),$(issue 9 needs-human-attention CLOSED),$(issue 10 tehas:question CLOSED)]" >"$T/issues.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.queued | length' "$live/status.json")" 1
t_eq "$(jq -r '.running | length' "$live/status.json")" 0
t_eq "$(jq -r '.stopped | length' "$live/status.json")" 1
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 1
t_eq "$(jq -r '[.queued[].number] | join(",")' "$live/status.json")" "3"
t_eq "$(jq -r .status "$live/health.json")" pass

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

t_case "completed is the closed issues a merged pull request closed, newest first"
reset
shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}')" \
  "$(closed 21 2026-01-03T00:00:00Z '{"merged":true}' tehas:shipped)" \
  "$(closed 22 2026-01-04T00:00:00Z '{"merged":false}')" \
  "$(closed 23 2026-01-05T00:00:00Z null)" \
  "$(closed 24 2026-01-06T00:00:00Z '{}')"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[21,20]"
t_eq "$(jq -c '.completed[0] | keys' "$live/status.json")" '["closedAt","number","title","updatedAt","url"]'
t_contains "$(jq -r .links.completed "$live/status.json")" "label%3Atehas%3Ashipped"
t_contains "$(t_calls gh)" "q=repo:acme/widget is:issue is:closed linked:pr sort:updated-desc"

t_case "a shipped issue without the label is given it, once"
t_eq "$(t_calls gh | grep -c 'issue edit')" 1
t_contains "$(t_calls gh)" "issue edit 20 --repo acme/widget --add-label tehas:shipped"

t_case "an issue closed with its label still on is not counted as shipped for the label alone"
reset
echo "[$(issue 5 tehas:awaiting-merge CLOSED)]" >"$T/issues.json"
t_run "$status"
t_eq "$(jq -c '.completed' "$live/status.json")" "[]"

t_case "a label that cannot be given leaves the snapshot written"
reset
shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}')"
T_ENV=(GH_EDIT_FAILS=1)
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"

t_case "without the search for shipped work the old snapshot stays"
reset
t_run "$status"
before=$(cksum <"$live/status.json")
: >"$T/shipped-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(cksum <"$live/status.json")" "$before"
echo '{"errors": [{"message": "rate limited"}]}' >"$T/shipped.json"
rm "$T/shipped-down"
t_run "$status"
t_eq "$(cksum <"$live/status.json")" "$before"

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

t_case "with GitHub down for three hours, health warns and the queue line says how old it is"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours -5 minutes' +%FT%TZ)" '.generatedAt = $t' "$live/status.json" >"$T/old.json"
mv "$T/old.json" "$live/status.json"
: >"$T/gh-down"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The queue snapshot is 3 h old: GitHub did not answer the last polls."
t_contains "$t_out" "factory   warn: The queue snapshot is 3 h old: GitHub did not answer the last polls."
t_contains "$t_out" "queue     2 in line, 0 building, 1 in review, 1 stopped (3 h ago)"

t_case "a snapshot of nine minutes passes, and a failed cycle is still said first"
reset
t_run "$status"
jq --arg t "$(date -u -d '-9 minutes' +%FT%TZ)" '.generatedAt = $t' "$live/status.json" >"$T/old.json"
mv "$T/old.json" "$live/status.json"
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" pass
jq --arg t "$(date -u -d '-11 minutes' +%FT%TZ)" '.generatedAt = $t' "$live/status.json" >"$T/old.json"
mv "$T/old.json" "$live/status.json"
t_run "$health"
t_eq "$(jq -r .output "$live/health.json")" "The queue snapshot is 11 min old: GitHub did not answer the last polls."
cycle false aborted develop
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" fail

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
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 4
t_contains "$t_out" "factory   pass: Idle, taking work. ("
t_contains "$t_out" "s ago)"
t_contains "$t_out" "queue     2 in line, 0 building, 1 in review, 1 stopped"
t_contains "$t_out" "cycle     none yet"
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

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

t_case "summary: two running cycles are both named, each on a line"
reset
mkdir -p "$live"
printf '{"cycles":[%s,%s],"last":null}\n' "$(one 12 true running develop)" "$(one 14 true running plan)" >"$live/running.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12, #14."
t_contains "$t_out" "cycle     #12 running, step develop"
t_contains "$t_out" "cycle     #14 running, step plan"
t_eq "$(jq -c '[.checks.cycle.issue, (.checks.cycles | map(.issue))]' "$live/health.json")" "[12,[12,14]]"

t_case "summary: the last cycle failed and the cycle beside it started earlier: still failed"
reset
mkdir -p "$live"
printf '{"cycles":[%s],"last":%s}\n' "$(one 14 true running plan 600 5)" "$(one 12 false aborted develop 700 180)" >"$live/running.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."

t_case "summary: a cycle that started after the last one failed clears the failure"
reset
mkdir -p "$live"
printf '{"cycles":[%s],"last":%s}\n' "$(one 14 true running plan 60 5)" "$(one 12 false aborted develop 700 180)" >"$live/running.json"
t_run "$status; $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #14."

t_case "health reads the one object an earlier version wrote: a running cycle"
reset
mkdir -p "$live"
one 12 true running develop >"$live/running.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12."

t_case "health reads the one object an earlier version wrote: a failed cycle stays failed"
reset
mkdir -p "$live"
one 12 false aborted develop >"$live/running.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #12 ended as aborted."

t_case "a cycle that stopped writing is lost, and the factory says so"
reset
mkdir -p "$live/cycles"
printf '{"issue":9,"running":true,"status":"running","step":"develop","startedAt":%s,"updatedAt":%s}\n' \
  "$(($(date +%s) - 900))" "$(($(date +%s) - 300))" >"$live/cycles/9.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[(.cycles | length), .last.status]' "$live/running.json")" '[0,"lost"]'

t_case "a cycle that stopped writing while its lock is held stays running; a free lock is lost"
reset
mkdir -p "$live/cycles" "$T/home/tehas/state/cycles"
printf '{"issue":9,"running":true,"status":"running","step":"develop","startedAt":%s,"updatedAt":%s}\n' \
  "$(($(date +%s) - 900))" "$(($(date +%s) - 300))" >"$live/cycles/9.json"
exec 8>"$T/home/tehas/state/cycles/9.lock"
flock -n 8
t_run "$status; $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #9."
t_eq "$(jq -r .status "$live/cycles/9.json")" running
exec 8>&-
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."

t_case "an abort a merge overtook ten seconds later passes, and checks.failed still names it"
reset
t_run "$status"
mkdir -p "$live/cycles"
ended() { # issue status started-ago updated-ago
  printf '{"issue":%s,"running":false,"status":"%s","step":"develop","startedAt":%s,"updatedAt":%s}\n' \
    "$1" "$2" "$(($(date +%s) - $3))" "$(($(date +%s) - $4))" >"$live/cycles/$1.json"
}
ended 12 aborted 900 20
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" fail
ended 13 merged 800 10
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$([ -e "$live/cycles/12.json" ] && echo kept)" ""
t_eq "$(jq -c '[.checks.failed[] | [.issue, .status, .step]]' "$live/health.json")" '[[12,"aborted","develop"]]'
t_eq "$(jq -r '.checks.failed[0].time | fromdateiso8601 | (now - .) < 60' "$live/health.json")" true

t_case "two endings that arrive together are both recorded"
reset
t_run "$status"
mkdir -p "$live/cycles"
ended 12 killed 900 20
ended 13 merged 800 10
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c '[.checks.failed[].issue]' "$live/health.json")" '[12]'
t_eq "$(jq -c '[.[].issue]' "$live/endings.json")" '[12,13]'

t_case "the endings keep the last ten, and a failure eleven endings ago is gone"
reset
t_run "$status"
mkdir -p "$live/cycles"
ended 20 aborted 900 300
t_run "$health"
for n in 21 22 23 24 25 26 27 28 29 30; do
  ended "$n" merged 800 $((300 - n))
  t_run "$health"
done
t_eq "$(jq length "$live/endings.json")" 10
t_eq "$(jq -c .checks.failed "$live/health.json")" '[]'
t_eq "$(ls "$live/cycles")" "30.json"

t_case "an endings file that is not a list is started again"
reset
t_run "$status"
mkdir -p "$live/cycles"
echo '{"not":"a list"}' >"$live/endings.json"
ended 12 aborted 900 20
t_run "$health"
t_eq "$(jq -c '[.[].issue]' "$live/endings.json")" '[12]'
t_eq "$(jq -c '[.checks.failed[].issue]' "$live/health.json")" '[12]'

# The platform checkout: a clone of a local origin, which a case moves ahead.
g() { git -c user.name=t -c user.email=t@example.invalid -c init.defaultBranch=main "$@" >/dev/null 2>&1; }
platform_setup() {
  rm -rf "$T/origin" "$T/origin.git" "$T/home/tehas/platform" "$T/home/tehas/state/platform-fetch"
  g init -q "$T/origin"
  g -C "$T/origin" commit -q --allow-empty -m one
  g clone -q --bare "$T/origin" "$T/origin.git"
  g clone -q "$T/origin.git" "$T/home/tehas/platform"
}
origin_ahead() { # <commits> <date>
  local i
  for ((i = 0; i < $1; i++)); do
    GIT_COMMITTER_DATE=$2 GIT_AUTHOR_DATE=$2 g -C "$T/origin" commit -q --allow-empty -m "ahead $i"
  done
  g -C "$T/origin" push -q "$T/origin.git" main
}

t_case "platform: an up to date checkout is said, and passes"
reset
platform_setup
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), up to date with origin/main, fetched "
t_eq "$(jq -r .checks.platform.behind "$live/health.json")" 0

t_case "platform: behind for more than a day is a warning that says how far"
platform_setup
origin_ahead 2 "$(date -u -d '-2 days' +%FT%TZ)"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The platform is 2 commits behind origin/main since $(date -u -d '-2 days' +%F): run tehas update."
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), 2 commits behind origin/main since $(date -u -d '-2 days' +%F)"

t_case "platform: behind for less than a day is said and passes"
platform_setup
origin_ahead 1 "$(date -u -d '-2 hours' +%FT%TZ)"
t_run "$status && $summary"
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "1 commits behind origin/main"

t_case "platform: the checkout is fetched at most once an hour"
platform_setup
t_run "$status"
origin_ahead 1 "$(date -u -d '-2 days' +%FT%TZ)"
t_run "$status"
t_eq "$(jq -r .behind "$live/platform.json")" 0
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
t_run "$status"
t_eq "$(jq -r .behind "$live/platform.json")" 1

t_case "platform: a fetch that fails is no error, and is not tried again for an hour"
platform_setup
mv "$T/origin.git" "$T/origin.gone"
echo "1 1" >"$T/home/tehas/state/platform-fetch"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r .fetchedAt "$live/platform.json")" "1970-01-01T00:00:01Z"
read -r last _ <"$T/home/tehas/state/platform-fetch"
t_eq "$(( $(date +%s) - last < 60 ))" 1
mv "$T/origin.gone" "$T/origin.git"

t_case "platform: with GitHub down the checkout is still read"
platform_setup
: >"$T/gh-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r .behind "$live/platform.json")" 0
rm -f "$T/gh-down"

t_case "platform: no checkout, no file and no line"
reset
rm -rf "$T/home/tehas/platform"
mkdir -p "$live"
echo '{}' >"$live/platform.json"
t_run "$status && $summary"
t_eq "$([ -e "$live/platform.json" ] && echo there)" ""
t_lacks "$t_out" "platform"
t_eq "$(jq -r .checks.platform "$live/health.json")" null

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

disk() { # low [age in seconds]
  mkdir -p "$live"
  jq -n --argjson low "$1" --arg at "$(date -u -d "@$(($(date +%s) - ${2:-60}))" +%FT%TZ)" \
    '{generatedAt: $at, disk: {freeBytes: 1073741824, minFreeBytes: 5368709120, low: $low}, previews: []}' \
    >"$live/previews.json"
}

t_case "health: a host with no previews has no disk check"
reset
t_run "$status"
t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"

t_case "health: low disk is a warning, with the figures"
reset
disk true
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "warn"
t_contains "$(jq -r '.output' "$live/health.json")" "low on disk: previews are not being built"
t_eq "$(jq -c '.checks.disk | {freeBytes, minFreeBytes, low}' "$live/health.json")" \
  '{"freeBytes":1073741824,"minFreeBytes":5368709120,"low":true}'
t_eq "$(find "$live" -name health.down | wc -l)" 0

t_case "health: free space that could not be read is a warning that says so"
reset
disk true
jq '.disk.freeBytes = null' "$live/previews.json" >"$live/x" && mv "$live/x" "$live/previews.json"
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "warn"
t_eq "$(jq -r '.output' "$live/health.json")" "The host's free disk space could not be read: previews are not being built."
t_eq "$(jq -c '.checks.disk | {freeBytes, minFreeBytes, low}' "$live/health.json")" \
  '{"freeBytes":null,"minFreeBytes":5368709120,"low":true}'

t_case "health: a figure with low disk keeps the sentence about low disk"
reset
disk true
t_run "$status"
t_eq "$(jq -r '.output' "$live/health.json")" "The host is low on disk: previews are not being built."

t_case "health: no figure and not low is not a warning"
reset
disk false
jq '.disk.freeBytes = null' "$live/previews.json" >"$live/x" && mv "$live/x" "$live/previews.json"
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"

t_case "health: enough disk passes"
reset
disk false
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"

t_case "health: a figure older than half an hour is not believed"
reset
disk true 3600
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"
t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"

for bad in 'not json' '{"generatedAt":"yesterday","disk":{"low":true}}' '{"disk":{"low":true}}' '[]' '{"generatedAt":7,"disk":"full"}'; do
  t_case "health: a previews.json of '$bad' gives no disk check and no warning"
  reset
  mkdir -p "$live"
  printf '%s\n' "$bad" >"$live/previews.json"
  t_run "$status"
  t_eq "$(jq -r '.status' "$live/health.json")" "pass"
  t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"
  t_lacks "$(jq -r '.output' "$live/health.json")" "disk"
  t_eq "$(find "$live" -name health.down | wc -l)" 0
done

t_case "health: a failed cycle still wins over low disk"
reset
disk true
cycle false died verify
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "fail"

# What the dispatcher's last poll wrote about the slice, as libexec/dispatch
# writes live/dispatch.json. 4 GiB of 20 GiB are promised.
poll() { # <why> <hold> [<age in seconds>]
  mkdir -p "$live"
  jq -n --arg why "$1" --arg hold "$2" --arg at "$(date -u -d "@$(($(date +%s) - ${3:-40}))" +%FT%TZ)" \
    '{time: $at, why: $why, hold: $hold, count: 1, room: 8, committed: 4294967296, max: 21474836480, reserve: "2g"}' \
    >"$live/dispatch.json"
}

t_case "health carries checks.slice from live/dispatch.json"
reset
poll "" ""
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c '.checks.slice | del(.time)' "$live/health.json")" \
  '{"why":"","hold":"","count":1,"room":8,"committed":4294967296,"max":21474836480,"reserve":"2g"}'
t_eq "$(jq -r .checks.slice.time "$live/health.json")" "$(jq -r .time "$live/dispatch.json")"

for why in slice unreadable; do
  t_case "health: a hold for '$why' is a warning with the hold as output"
  reset
  poll "$why" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_run "$status; $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" warn
  t_eq "$(jq -r .output "$live/health.json")" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_eq "$(find "$live" -name health.down | wc -l)" 0
  t_contains "$t_out" "factory   warn: holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_contains "$t_out" "slice     holding: no slice file on this host (sudo ./bootstrap.sh slice) (4"
done

for why in count awaiting pressure room; do
  t_case "health: a hold for '$why' is not a warning, and tehas status still says it"
  reset
  poll "$why" "holding: the reason"
  t_run "$status && $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" pass
  t_contains "$t_out" "factory   pass: Idle, taking work."
  t_contains "$t_out" "slice     holding: the reason (4"
done

t_case "health: no resources is a warning, said by the factory line, and the slice keeps its numbers"
reset
poll resources "holding: no resources: the model endpoint does not answer"
t_run "$status; $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "No resources: the model endpoint does not answer."
t_eq "$(jq -r .checks.slice.why "$live/health.json")" resources
t_eq "$([ -e "$live/health.down" ] && echo down)" ""
t_eq "$(jq -r .noResources "$live/status.json")" true
t_contains "$t_out" "factory   warn: No resources: the model endpoint does not answer."
t_contains "$t_out" "slice     room for 8 cycles"

t_case "status: no resources is not said from a poll older than ten minutes, or from any other hold"
reset
poll resources "holding: no resources: the model endpoint does not answer" 700
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false
t_eq "$(jq -r .status "$live/health.json")" pass
poll room "holding: tehas.slice has no room for another cycle"
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false
rm -f "$live/dispatch.json"
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false

t_case "health: a paused factory is said before no resources"
reset
poll resources "holding: no resources: the model endpoint does not answer"
echo true >"$T/paused"
t_run "$status"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
echo false >"$T/paused"

t_case "health: a paused factory and a failed cycle are said before a slice hold"
reset
poll slice "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
echo true >"$T/paused"
t_run "$status"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
echo false >"$T/paused"
cycle false aborted develop
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" fail

t_case "health: a dispatch.json older than ten minutes gives no slice check and no warning"
reset
poll slice "holding: no slice file on this host (sudo ./bootstrap.sh slice)" 660
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -c .checks.slice "$live/health.json")" null
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

for bad in '{"time":' 'not json' '[]' '{"why":"slice","hold":"x"}' '{"time":"yesterday","why":"slice","hold":"x"}' \
  '{"time":7,"why":"slice"}' "{} {}"; do
  t_case "health: a dispatch.json of '$bad' does not break the health write"
  reset
  mkdir -p "$live"
  printf '%s\n' "$bad" >"$live/dispatch.json"
  t_run "$status && $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" pass
  t_eq "$(jq -c .checks.slice "$live/health.json")" null
  t_contains "$t_out" "slice     unknown"
done

t_case "summary: the slice line says the room, what is promised, the reserve and its age"
reset
poll "" ""
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 4
t_contains "$t_out" "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, 4"
t_contains "$(printf '%s\n' "$t_out" | tail -n 1)" " s ago)"
poll "" "" 300
jq '.committed = 4831838208' "$live/dispatch.json" >"$live/x" && mv "$live/x" "$live/dispatch.json"
t_run "$status && $summary"
t_contains "$t_out" "slice     room for 8 cycles (4.5 GiB of 20 GiB promised, reserve 2g, 5 min ago)"

t_case "summary: a health file written before the slice was known says unknown"
reset
t_run "$status"
jq 'del(.checks.slice)' "$live/health.json" >"$live/x" && mv "$live/x" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

t_case "health and summary read files only: no systemctl, no docker"
reset
t_stub systemctl
t_stub docker
poll room "holding: the reason"
t_run "$status && $summary"
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

t_case "a lost cycle counts from when it was found, not from its last heartbeat"
reset
mkdir -p "$live/cycles"
printf '{"issue":9,"running":true,"status":"running","step":"develop","startedAt":%s,"updatedAt":%s}\n' \
  "$(($(date +%s) - 900))" "$(($(date +%s) - 300))" >"$live/cycles/9.json"
printf '{"issue":12,"running":false,"status":"merged","step":"review","startedAt":%s,"updatedAt":%s}\n' \
  "$(($(date +%s) - 600))" "$(($(date +%s) - 200))" >"$live/cycles/12.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[.last.issue, .last.status]' "$live/running.json")" '[9,"lost"]'
t_eq "$(find "$live/cycles" -name '*.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "9.json "

t_done
