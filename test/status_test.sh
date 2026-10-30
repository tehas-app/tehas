#!/usr/bin/env bash
# Tests for the three scripts around the factory's state: libexec/status
# writes the queue snapshot and runs libexec/health beside it, and
# libexec/summary prints the health file for `tehas status`.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The limit is read and written by the console (console flags).
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

# The command as a host has it: beside the console, which decides what the read
# of what closed an issue means (console/src/shipped.rs) and is this checkout's
# own build. The copy is a real file and not a symlink, because the command finds
# its siblings - health, analytics, store - beside itself by the directory it was
# run from, and a symlink there would name the checkout instead.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
mkdir -p "$shim/libexec"
cp "$T_ROOT/libexec/status" "$shim/libexec/status"
ln -s "$T_ROOT/lib" "$shim/lib"
cp "$T_CONSOLE" "$shim/libexec/console"
for sibling in health summary analytics; do
  ln -s "$T_ROOT/libexec/$sibling" "$shim/libexec/$sibling"
done
status=$shim/libexec/status
health="$T_ROOT/libexec/health"
summary="$T_ROOT/libexec/summary"
live=$T/home/tehas/live

t_stub gh '
# One answer per search. The count is the only call that asks for a total and
# no nodes, so that shape tells the two apart; the count of the label under
# test is never the thing that routes the answer.
gh_graphql() {
  case "$1" in
    *subIssues*) [ ! -e "$T/epics-down" ] || exit 1; cat "$T/epics-page.json" ;;
    *"first: 0) { issueCount }"*) [ ! -e "$T/count-down" ] || exit 1
      cat "$T/count.json" ;;
    *) cat "$T/shipped.json" ;;
  esac
}
[ ! -e "$T/gh-down" ] || exit 1
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) [ ! -e "$T/all-down" ] || exit 1; cat "$T/all.json" ;;
  # The names of the labels of the control issue, as --jq joins them.
  "issue view"*) [ ! -e "$T/control-down" ] || exit 1
    { [ "$(cat "$T/paused")" != true ] || echo tehas:pause; [ ! -e "$T/allhands" ] || echo tehas:all-hands; } | paste -sd " " ;;
  "issue edit"*) [ -z "${GH_EDIT_FAILS:-}" ] || exit 1 ;;
  # gh is signed in, which is how the releases of a private repository are read.
  "auth status") exit "${GH_AUTH:-0}" ;;
  "api repos/acme/widget/issues/"*"/events"*) cat "$T/events.json" 2>/dev/null || exit 1 ;;
  "api graphql"*) [ ! -e "$T/shipped-down" ] || exit 1; gh_graphql "$*" ;;
  # The published releases of the platform, for the platform line of a release
  # host; nothing until a case puts the answer there.
  "api repos/"*"/releases"*) [ ! -e "$T/rel.json" ] || cat "$T/rel.json" ;;
esac'
# A release host reads the list with gh when it is signed in and with curl
# otherwise; CURL_RC is what curl answers with, and 22 is no answer at all.
t_stub curl '[ ! -e "${CURL_RC:-}" ] && exit "${CURL_RC:-0}"; exit 0'
# The failed tehas units, as systemctl --user --failed --plain --no-legend lists
# them: none until a case writes $T/failed-units.
t_stub systemctl 'cat "$T/failed-units" 2>/dev/null'
closed() { # number closedAt closer [label]: a closed issue as the search for shipped work returns it
  local label=""
  [ -z "${4:-}" ] || label="{\"name\":\"$4\"}"
  printf '{"number":%s,"title":"t","url":"u","createdAt":"2025-12-01T00:00:00Z","updatedAt":"%s","closedAt":"%s","labels":{"nodes":[%s]},"timelineItems":{"nodes":[{"closer":%s}]}}' \
    "$1" "$2" "$2" "$label" "$3"
}
no_labels() { # number closedAt closer: as closed, with no labels field at all - a read that said nothing of them
  printf '{"number":%s,"title":"t","url":"u","updatedAt":"%s","closedAt":"%s","timelineItems":{"nodes":[{"closer":%s}]}}' \
    "$1" "$2" "$2" "$3"
}
shipped() { # closed issues, as the GraphQL answer
  local IFS=,
  printf '{"data":{"search":{"nodes":[%s]}}}' "$*" >"$T/shipped.json"
}
epics_page() { # issue nodes: the one page of open issues the read of the epics answers
  local IFS=,
  printf '{"data":{"repository":{"issues":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}}}}' "$*" >"$T/epics-page.json"
}
shipped_count() { # total of the closed issues labelled tehas:shipped, as GraphQL answers it
  printf '{"data":{"search":{"issueCount":%s}}}' "$1" >"$T/count.json"
}
# The state the count cases share: one closed issue a merged pull request closed,
# and the count answer of reset still saying 0.
one_shipped() {
  reset
  shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}')"
}
issue() { # number label state
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"createdAt":"2025-12-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","closedAt":null}' "$1" "${3:-OPEN}" "$2"
}
reset() {
  rm -rf "$T"/calls/* "$T/gh-down" "$T/all-down" "$T/shipped-down" "$T/count-down" "$T/control-down" "$T/allhands" "$T/epics-down" "$live" \
    "$T/home/tehas/state" "$T/home/tehas/current" "$T/home/tehas/versions" "$T/home/tehas/versions.env" "$T/rel.json"
  echo "[]" >"$T/all.json"
  epics_page
  shipped
  shipped_count 0
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  # The state store the health file reads the planning turns from, with
  # today's backup.
  t_store_home "$T/home/tehas"
  TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "reset: the store made no backup"
  echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
  echo false >"$T/paused"
  echo '[]' >"$T/events.json"
}
one() { # issue running status step [started [updated]]: one cycle on the store, as the live view shows it; times are seconds ago
  t_live "$T/home/tehas" "$@"
}
cycle() { # running status step: issue 12, running or the last one
  one 12 "$@"
}

t_case "status writes the snapshot and the health file, without a command on PATH"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.review "$live/health.json")" 1

t_case "status writes the open epics and their sub-issues, and not an issue without any"
reset
epics_page '{"number":7,"title":"An epic","url":"u7","subIssuesSummary":{"total":2,"completed":1},
  "subIssues":{"nodes":[{"number":8,"title":"Done","url":"u8","state":"CLOSED","repository":{"nameWithOwner":"acme/widget"}},
  {"number":9,"title":"To do","url":"u9","state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}]}}' \
  '{"number":3,"title":"Plain","url":"u3","subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}'
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.repo, (.epics | map(.number))]' "$live/epics.json")" '["acme/widget",[7]]'
t_eq "$(jq -c '.epics[0].subIssues | map([.number, .state])' "$live/epics.json")" '[[8,"closed"],[9,"open"]]'
t_eq "$(jq -c '.epics[0] | [.total, .completed, .more]' "$live/epics.json")" '[2,1,0]'

t_case "a read of the epics that fails leaves the file as it was and the snapshot is still written"
reset
mkdir -p "$live"
echo '{"epics":"before"}' >"$live/epics.json"
touch "$T/epics-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c .epics "$live/epics.json")" '"before"'
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(find "$live" -name 'epics.json.*' | wc -l)" 0

t_case "no failed tehas unit is no warning"
reset
rm -f "$T/failed-units"
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c .checks.units "$live/health.json")" '[]'
t_contains "$(t_calls systemctl)" "--user --failed --plain --no-legend tehas-*"

t_case "a failed tehas-dispatch.path is a warning that names it"
reset
echo "tehas-dispatch.path loaded failed failed Tehas: a poll as soon as a cycle has ended" >"$T/failed-units"
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -c .checks.units "$live/health.json")" '["tehas-dispatch.path"]'
t_contains "$(jq -r .output "$live/health.json")" "systemd gave up on tehas-dispatch.path"
rm -f "$T/failed-units"

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

# The control issue holds the pause switch and no outcome: whoever labels it,
# it is not work, so it is not counted in line, worked on, stopped or awaiting.
t_case "the control issue is not counted in line, worked on, stopped or awaiting, whatever label it carries"
reset
echo "[$(issue 1 tehas:ready),$(issue 3 tehas:ready),$(issue 1 tehas:running),$(issue 1 needs-human-attention),$(issue 1 tehas:awaiting-merge),$(issue 3 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.queued, .running, .stopped, .awaiting] | map(length)' "$live/status.json")" "[1,0,1,1]"
t_eq "$(jq -r '[.running[]?.number] | join(",")' "$live/status.json")" ""
t_eq "$(jq -r '[.queued[].number] | join(",")' "$live/status.json")" "3"
t_eq "$(jq -r '[.stopped[].number] | join(",")' "$live/status.json")" "6"
t_eq "$(jq -r '[.awaiting[].number] | join(",")' "$live/status.json")" "3"

# The same numbers with the switch labelled tehas:pause, which is how the
# control issue is normally seen: the pause is reported, the issue is not.
t_case "and it stays out of the counts while the factory is paused by its label"
reset
echo "[$(issue 1 tehas:ready),$(issue 3 tehas:ready)]" >"$T/issues.json"
echo true >"$T/paused"
t_run "$status"
t_eq "$(jq -r '.paused' "$live/status.json")" true
t_eq "$(jq -c '[.queued, .running, .stopped, .awaiting] | map(length)' "$live/status.json")" "[1,0,0,0]"
t_eq "$(jq -r '[.queued[].number] | join(",")' "$live/status.json")" "3"

# A cycle starts with --restart and pi-rukas takes needs-human-attention off
# once it runs: until then the issue carries both labels.
t_case "an issue a cycle is running on is not stopped, whatever else it carries"
reset
echo '[{"number":6,"title":"t","url":"u","state":"OPEN","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}]' >"$T/issues.json"
t_run "$status"
t_eq "$(jq -r '.running | length' "$live/status.json")" 1
t_eq "$(jq -r '.stopped | length' "$live/status.json")" 0

t_case "planning lists and counts the open issues labelled tehas:plan or tehas:planning, from the same search"
reset
echo "[$(issue 3 tehas:ready),$(issue 7 tehas:plan),$(issue 8 tehas:planning),$(issue 9 tehas:plan CLOSED),$(issue 1 tehas:planning),$(issue 10 tehas:question)]" >"$T/issues.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.planning' "$live/status.json")" 2
t_eq "$(jq -c '.planned | map(.number)' "$live/status.json")" '[7,8]' # 1 is the control issue
t_eq "$(jq -r '.queued | length' "$live/status.json")" 1
t_eq "$(t_calls gh | grep -c -- '--search')" 1
t_contains "$(t_calls gh | grep -- '--search')" 'label:"tehas:ready","tehas:running","tehas:awaiting-merge","needs-human-attention","tehas:plan","tehas:planning" '
reset
t_run "$status"
t_eq "$(jq -r '.planning' "$live/status.json")" 0
t_eq "$(jq -c '.planned' "$live/status.json")" '[]'

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

t_case "a list of every issue past the limit of one argument still gives a snapshot"
reset
# 1500 titles of 100 characters: over 128 KiB, which no one argument can hold.
jq -n '[range(1500) | {title: ("word\(.) " + ("x" * 100)), labels: [{name: "bug"}]}]' >"$T/all.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.buzzwords | length' "$live/status.json")" 100
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2

t_case "a queue snapshot past the limit of one argument still gives a health file"
reset
# 800 issues in line with titles of 200 characters: a status.json over 128 KiB.
jq -n '[range(800) | {number: (. + 100), title: ("t\(.) " + ("x" * 200)), state: "OPEN",
  url: "https://github.com/acme/widget/issues/\(. + 100)", updatedAt: "2026-10-22T00:00:00Z",
  closedAt: null, labels: [{name: "tehas:ready"}]}]' >"$T/issues.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(( $(wc -c <"$live/status.json") > 131072 ))" 1
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.inLine "$live/health.json")" 800

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
t_eq "$(jq -c '.completed[0] | keys' "$live/status.json")" '["closedAt","createdAt","number","title","updatedAt","url"]'
t_eq "$(jq -c '.completed[0] | [.title, .url, .createdAt]' "$live/status.json")" '["t","u","2025-12-01T00:00:00Z"]'
# The stub answers whatever it is asked, so the fields must be asked for.
t_contains "$(t_calls gh)" "number title url createdAt updatedAt closedAt labels("
t_contains "$(t_calls gh)" "--json number,title,url,state,labels,createdAt,updatedAt,closedAt"
t_contains "$(jq -r .links.completed "$live/status.json")" "label%3Atehas%3Ashipped"
t_contains "$(t_calls gh)" "q=repo:acme/widget is:issue is:closed linked:pr sort:updated-desc"
# The count comes from the search for the label, whose answer says 0 here.
t_contains "$(t_calls gh)" "q=repo:acme/widget is:issue is:closed label:tehas:shipped"
t_contains "$(t_calls gh)" "first: 0) { issueCount }"
t_eq "$(jq -r .completedCount "$live/status.json")" 0

t_case "completedCount is every closed issue labelled tehas:shipped, though the list holds only the latest hundred"
reset
jq -n '{data: {search: {nodes: [range(50) | {number: (. + 100), title: "t",
  url: "u", updatedAt: "2026-01-02T00:00:00Z", closedAt: "2026-01-02T00:00:00Z",
  labels: {nodes: []}, timelineItems: {nodes: [{closer: {merged: true}}]}}]}}}' >"$T/shipped.json"
shipped_count 138
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.completed | length' "$live/status.json")" 50
t_eq "$(jq -r .completedCount "$live/status.json")" 138
# The two reads of shipped work, and the read of the epics beside them.
t_eq "$(t_calls gh | grep -c 'api graphql -f q=')" 2
t_eq "$(t_calls gh | grep -c 'api graphql')" 3

# The figure is published as a whole number of issues, or no key at all: an
# answer that is not one is dropped rather than published for a reader to judge,
# and 0 is a published answer, not an absent one. The bounds are the ones
# host/fun/business-view.js accepts, so both sides of the pair are pinned here.
t_case "completedCount is published only as a whole number of issues"
one_shipped
for answer in 0 1 9007199254740991; do
  shipped_count "$answer"
  t_run "$status"
  t_eq "$t_rc" 0
  t_eq "$(jq -r '.completedCount | type' "$live/status.json")" number
  t_eq "$(jq -r .completedCount "$live/status.json")" "$answer"
done
# -138 and 1e400 are numbers GitHub could answer and no reader can use.
for answer in -138 -0.5 1e400 1E+999 9007199254740992; do
  shipped_count "$answer"
  t_run "$status"
  t_eq "$t_rc" 0
  t_eq "$(jq -r 'has("completedCount")' "$live/status.json")" false
done

# A gh that fails the call outright, with an answer that would read well above
# every bound: the figure is left out as if the search had given no number.
t_case "a count call that fails leaves the field out"
one_shipped
shipped_count 138
: >"$T/count-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r 'has("completedCount") ' "$live/status.json")" false
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"

t_case "a count that cannot be read leaves the field out, and the rest of the snapshot is written"
one_shipped
# The count that cannot be read, however gh answers: the snapshot is written as
# it was before the field, and the list stands as it is.
for answer in '' 'not json' '{"errors": [{"message": "rate limited"}]}' '{"data": {}}' \
  '{"data": {"search": {}}}' '{"data": {"search": {"issueCount": null}}}' \
  '{"data": {"search": {"issueCount": "138"}}}' '{"data": {"search": {"nodes": []}}}'; do
  printf '%s\n' "$answer" >"$T/count.json"
  t_run "$status"
  t_eq "$t_rc" 0
  t_eq "$(jq -r 'has("completedCount")' "$live/status.json")" false
  t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"
  t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
done

t_case "a shipped issue without the label is given it, once"
one_shipped
t_run "$status"
t_eq "$(t_calls gh | grep -c 'issue edit')" 1
t_contains "$(t_calls gh)" "issue edit 20 --repo acme/widget --add-label tehas:shipped"

t_case "a shipped issue is given the label whatever else it carries, and once"
# The read answers by the closer and not by the labels, so an issue that carries
# another label is shipped and edited like one that carries none: three issues,
# three edits, the label itself absent from all of them.
reset
jq -n '{data: {search: {nodes: [range(6; 9) | {number: ., title: "t", url: "u",
  updatedAt: "2026-01-02T00:00:00Z", closedAt: "2026-01-02T00:00:00Z",
  labels: {nodes: [{name: "bug"}]},
  timelineItems: {nodes: [{closer: {merged: true}}]}}]}}}' >"$T/shipped.json"
t_run "$status"
t_eq "$(jq -c '[.completed[].number] | sort' "$live/status.json")" "[6,7,8]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 3

t_case "an issue already labelled tehas:shipped is not asked for it, whatever the poll"
# The read names the labels of every issue it lists, so the edit is asked for
# only an issue that does not carry the label. gh issue edit is no no-op: it
# reads the issue and sends a mutation every time, and one poll used to pay 45
# of them for issues that all held the label already (#731).
reset
shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}' tehas:shipped)"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 0

# One of three that shipped without it is the issue the poll edits, and the only
# one: the two that hold the label are left alone.
t_case "of three shipped issues, only the one that lacks the label is edited"
reset
shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}' tehas:shipped)" \
  "$(closed 21 2026-01-03T00:00:00Z '{"merged":true}')" \
  "$(closed 22 2026-01-04T00:00:00Z '{"merged":true}' tehas:shipped)"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number] | sort' "$live/status.json")" "[20,21,22]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 1
t_contains "$(t_calls gh)" "issue edit 21 --repo acme/widget --add-label tehas:shipped"
t_lacks "$(t_calls gh)" "issue edit 20 --repo"
t_lacks "$(t_calls gh)" "issue edit 22 --repo"

# A label the read could not read is a missing label: the edit is sent, as it
# was before the query read the labels at all.
t_case "a shipped issue whose labels the read did not answer is edited"
reset
shipped "$(no_labels 20 2026-01-02T00:00:00Z '{"merged":true}')"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 1
t_contains "$(t_calls gh)" "issue edit 20 --repo acme/widget --add-label tehas:shipped"

t_case "the control issue is not listed as shipped and not given the label, whatever the read answers"
# It holds the pause switch and is not work: libexec/status leaves the number
# TEHAS_CONTROL_ISSUE names out of every list and every count, and tehas tidy
# follows the same rule when it puts the label on.
reset
jq -n '{data: {search: {nodes: [
  {number: 1, title: "t", url: "u", updatedAt: "2026-01-02T00:00:00Z",
   closedAt: "2026-01-02T00:00:00Z", timelineItems: {nodes: [{closer: {merged: true}}]}},
  {number: 20, title: "t", url: "u", updatedAt: "2026-01-02T00:00:00Z",
   closedAt: "2026-01-02T00:00:00Z", timelineItems: {nodes: [{closer: {merged: true}}]}}]}}}' >"$T/shipped.json"
t_run "$status"
t_eq "$(jq -c '[.completed[].number]' "$live/status.json")" "[20]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 1
t_lacks "$(t_calls gh)" "issue edit 1 --repo"

t_case "an answer that carries errors is no snapshot, even where it holds a merged closer"
# The same refusal tehas tidy makes of an answer with errors beside its data:
# the two commands read one acceptance rule, so neither may publish what the
# other refuses. The snapshot of the run before is the thing that must stay.
reset
t_run "$status"
before=$(jq -c .completed "$live/status.json")
jq -n '{errors: [{message: "declined"}], data: {search: {nodes: [
  {number: 20, title: "t", url: "u", updatedAt: "2026-01-02T00:00:00Z",
   closedAt: "2026-01-02T00:00:00Z", timelineItems: {nodes: [{closer: {merged: true}}]}}]}}}' >"$T/shipped.json"
t_run "$status"
t_eq "$(jq -c .completed "$live/status.json")" "$before"
t_eq "$(t_calls gh | grep -c 'issue edit')" 0

t_case "an issue closed with its label still on is not counted as shipped for the label alone"
reset
echo "[$(issue 5 tehas:awaiting-merge CLOSED)]" >"$T/issues.json"
t_run "$status"
t_eq "$(jq -c '.completed' "$live/status.json")" "[]"
t_eq "$(jq -r .completedCount "$live/status.json")" 0

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

t_case "a list of issues that cannot be read warns and leaves the old snapshot"
# The header of the command promises a WARN line whenever it writes no snapshot.
# The read of the list of issues in work is one of the two reads that end the
# run with nothing written, and used to do so in silence: now it says so, as the
# read of the closers has always done, and exit stays 0.
reset
t_run "$status"
before=$(cksum <"$live/status.json")
rm "$live/health.json"
# The stub answers the search of the issues in work from $T/issues.json and the
# word cloud of every issue from $T/all.json: only the first file is gone here,
# so the read the run cannot do without fails and the one it can do without does
# not. t_case clears $T/calls, so the two lines below are this case's calls.
rm "$T/issues.json"
t_run "$status"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --limit 200"
t_lacks "$t_out" "WARN status: the answer of what closed these issues was no list"
t_contains "$t_out" "WARN status: the list of issues could not be read, no snapshot was written"
t_eq "$(cksum <"$live/status.json")" "$before"
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "an answer of what closed these issues that is no list warns and leaves the old snapshot"
# The read answers 0 with a reply that holds no list of nodes: the console says
# it is no answer at all, so the run writes no snapshot and says why.
reset
t_run "$status"
before=$(cksum <"$live/status.json")
rm "$live/health.json"
printf '%s\n' '{"data":{}}' >"$T/shipped.json"
t_run "$status"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN status: the answer of what closed these issues was no list, no snapshot was written"
t_eq "$(cksum <"$live/status.json")" "$before"
t_eq "$(jq -r .status "$live/health.json")" pass

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

t_case "summary: the poll line under the factory line, from the last poll on record"
reset
t_run "$status && $summary"
t_lacks "$t_out" "poll      "
printf '0m1.000s 0m0.500s\n0m1.000s 0m0.600s\n' >"$T/poll.times"
printf '\n\n\n' >"$T/poll.tally"
t_at "$(date +%s)" poll-record --started "$(($(date +%s) - 30))" --times "$T/poll.times" \
  --tally "$T/poll.tally" --writes-log "$T/no-such.log" >/dev/null || t_bad "no poll recorded"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$(sed -n 2p <<<"$t_out")" "poll      last 3"
t_contains "$t_out" " s wall, 3.1 s CPU, 3 gh; last hour 1 poll, 3.1 s CPU"
t_contains "$(sed -n 3p <<<"$t_out")" "queue     "

t_case "summary: the last tehas check-product, when one ran"
reset
t_run "$status && $summary"
t_lacks "$t_out" "product"
mkdir -p "$T/home/tehas/state"
at=$(date -u -d @$(($(date +%s) - 300)) +%Y-%m-%dT%H:%M:%SZ)
printf '{"v":1,"at":"%s","ok":true,"failures":[],"sandbox":null}\n' "$at" >"$T/home/tehas/state/check-product.json"
t_run "$summary"
t_contains "$t_out" "product   keeps the contract, checked 5 min ago"
printf '{"v":1,"at":"%s","ok":false,"failures":["no AGENTS.md","no gate"],"sandbox":null}\n' "$at" >"$T/home/tehas/state/check-product.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "product   2 to fix: no AGENTS.md; no gate, checked 5 min ago"

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

t_case "status writes the pause switch: the flag with its time and reason, never who wrote it"
reset
mkdir -p "$T/home/tehas/state"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "waiting for the gate"}' >"$T/home/tehas/state/pause.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .paused "$live/status.json")" "true"
t_eq "$(jq -rc '.pause.flag | keys | join(",")' "$live/status.json")" "at,reason"
t_eq "$(jq -r .pause.flag.reason "$live/status.json")" "waiting for the gate"
t_eq "$(jq -r .pause.label "$live/status.json")" "false"
t_lacks "$(jq -c .pause "$live/status.json")" "boss"
t_eq "$(jq -r .status "$live/health.json")" "warn"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
t_eq "$(jq -rc '.checks.pause.flag | keys | join(",")' "$live/health.json")" "at,reason"
t_contains "$t_out" "pause     flag, written"
t_contains "$t_out" "waiting for the gate"
t_contains "$t_out" "1 stopped, paused"

t_case "the label alone is a pause the health file names, with no flag"
reset
echo true >"$T/paused"
t_run "$status && $summary"
t_eq "$(jq -rc '.pause.flag' "$live/status.json")" "null"
t_eq "$(jq -r .pause.label "$live/status.json")" "true"
t_eq "$(jq -rc '.checks.pause.flag' "$live/health.json")" "null"
t_contains "$t_out" "pause     label: tehas:pause on the control issue"
t_lacks "$t_out" "pause     flag"

t_case "neither switch set: no pause line, and a pause object that says so"
reset
t_run "$status && $summary"
t_eq "$(jq -r .paused "$live/status.json")" "false"
t_eq "$(jq -rc .pause "$live/status.json")" '{"flag":null,"label":false,"lifted":false}'
t_eq "$(jq -rc .checks.pause "$live/health.json")" '{"flag":null,"label":false,"lifted":false}'
t_lacks "$t_out" "pause     "
t_eq "$(jq -r .status "$live/health.json")" "pass"

# The label works one way: its absence is a resume only when an allowed account
# took it off. The reporting says what the poll acts on.
pause_events() { # <event> <login or "">: one tehas:pause event on the control issue
  local actor
  [ -z "$2" ] && actor="null" || actor="{\"login\":\"$2\"}"
  jq -nc --arg e "$1" --argjson a "$actor" \
    '[{event: $e, label: {name: "tehas:pause"}, actor: $a, created_at: "2026-01-02T00:00:00Z"}]' >"$T/events.json"
}

t_case "a pause a stranger took off is published as paused, with a line of its own"
reset
pause_events unlabeled tehas-bot
t_run "$status && $summary"
t_eq "$(jq -r .paused "$live/status.json")" "true"
t_eq "$(jq -rc .pause "$live/status.json")" '{"flag":null,"label":false,"lifted":true}'
t_eq "$(jq -rc .checks.pause "$live/health.json")" '{"flag":null,"label":false,"lifted":true}'
t_eq "$(jq -r .status "$live/health.json")" "warn"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
t_contains "$t_out" "pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off"
t_lacks "$t_out" "pause     label: tehas:pause on the control issue"

t_case "a pause an allowed account took off is not a pause, and a pause event with no account is"
reset
pause_events unlabeled alice
t_run "$status"
t_eq "$(jq -r .pause.lifted "$live/status.json")" "false"
for actor in "" "mallory"; do
  pause_events unlabeled "$actor"
  t_run "$status"
  t_eq "$(jq -r .pause.lifted "$live/status.json")" "true"
done

# The label is gone and the last pause event only applied it: the label object
# was deleted or renamed, and no account was asked to lift the pause.
t_case "a pause no event took off is published as paused"
reset
pause_events labeled mallory
t_run "$status && $summary"
t_eq "$(jq -r .paused "$live/status.json")" "true"
t_eq "$(jq -r .pause.lifted "$live/status.json")" "true"
t_eq "$(jq -r .status "$live/health.json")" "warn"
t_contains "$t_out" "pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off"

t_case "a flag that cannot be read still holds the factory paused"
reset
mkdir -p "$T/home/tehas/state"
echo 'not a flag' >"$T/home/tehas/state/pause.json"
t_run "$status && $summary"
t_eq "$(jq -r .paused "$live/status.json")" "true"
t_eq "$(jq -rc .pause.flag "$live/status.json")" '{"at":null,"reason":null}'
t_eq "$(jq -r .status "$live/health.json")" "warn"
t_contains "$t_out" "pause     flag, written at an unknown time"

t_case "summary: a failed cycle is exit 1, and still printed"
reset
cycle false aborted develop
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."
t_contains "$t_out" "cycle     #12 aborted at step develop"

t_case "summary: two running cycles are both named, each on a line"
reset
one 12 true running develop
one 14 true running plan
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12, #14."
t_contains "$t_out" "cycle     #12 running, step develop"
t_contains "$t_out" "cycle     #14 running, step plan"
t_eq "$(jq -c '[.checks.cycle.issue, (.checks.cycles | map(.issue))]' "$live/health.json")" "[12,[12,14]]"

# A planning turn in flight: its session running on the state store, and its
# lock held by this shell until let_go, as the turn's unit holds it.
turn_fd=""
turn() { # <issue> <turns before> <started seconds ago>
  local st=$T/home/tehas/state/plan-$1.json
  t_plan "$T/home/tehas" "$1" "$(jq -nc --argjson t "$2" --arg at "$(date -u -d "-$3 seconds" +%FT%TZ)" \
    '{session: "abc", turns: $t, running: true, startedAt: $at}')"
  exec {turn_fd}>>"$st.lock"
  flock -n "$turn_fd"
}
let_go() { [ -z "$turn_fd" ] || exec {turn_fd}>&-; turn_fd=""; }

t_case "summary: a planning turn in flight has its own line"
reset
turn 7 1 720
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "plan      #7 turn 2 running, started 12 min ago"
t_eq "$(jq -c '.checks.plans | map([.issue, .turn])' "$live/health.json")" "[[7,2]]"
t_run "$summary --json"
t_eq "$(jq -c '.planning | map([.issue, .turn, (.since | type)])' <<<"$t_out")" '[[7,2,"string"]]'
let_go

t_case "summary: a turn whose lock is free, or that left its status, is not shown as running"
reset
turn 7 1 720
let_go
t_run "$status && $summary"
t_lacks "$t_out" "plan      "
t_run "$summary --json"
t_eq "$(jq -c .planning <<<"$t_out")" "[]"
hold_turn() { exec {turn_fd}>>"$T/home/tehas/state/plan-7.json.lock" && flock -n "$turn_fd"; }
hold_turn
echo 0 >"$T/home/tehas/state/plan-7.json.rc"
t_run "$status && $summary"
t_lacks "$t_out" "plan      "
let_go
# A session that runs no turn on record, whatever holds the lock.
TEHAS_HOME=$T/home/tehas "$T_STORE" plan-end 7 --turn 2 --outcome finished --rc 0 --session abc >/dev/null
rm -f "$T/home/tehas/state/plan-7.json.rc"
hold_turn
t_run "$status && $summary"
t_lacks "$t_out" "plan      "
let_go

t_case "health: a store that cannot list the planning sessions writes no health file"
reset
t_run "$status"
t_eq "$([ -e "$live/health.json" ] && echo written)" written
rm -f "$live/health.json"
t_store_answers plan-show 1 'store: busy #0: the database is locked'
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/libexec/health"
t_eq "$t_rc" 1
t_contains "$t_out" "the planning sessions could not be read from the state store"
t_eq "$([ -e "$live/health.json" ] && echo written)" ""
t_store_answers plan-show 78 'store: corrupt: the file is not a database'
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/libexec/health"
t_eq "$t_rc" 78
t_eq "$([ -e "$live/health.json" ] && echo written)" ""
rm -f "$T/home/tehas/libexec/store"
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/libexec/health"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"

t_case "summary: the last cycle failed and the cycle beside it started earlier: still failed"
reset
one 14 true running plan 600 5
one 12 false aborted develop 700 180
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."

t_case "summary: a cycle that started after the last one failed clears the failure"
reset
one 14 true running plan 60 5
one 12 false aborted develop 700 180
t_run "$status; $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #14."

t_case "a cycle that stopped writing is lost, and the factory says so"
reset
one 9 true running develop 900 300
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[(.cycles | length), .last.status]' "$live/running.json")" '[0,"lost"]'

t_case "a cycle that stopped writing while its lock is held stays running; a free lock is lost"
reset
one 9 true running develop 900 300
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
ended() { # issue status started-ago updated-ago
  one "$1" false "$2" develop "$3" "$4"
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
ended 12 killed 900 20
ended 13 merged 800 10
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c '[.checks.failed[].issue]' "$live/health.json")" '[12]'
t_eq "$(jq -c '[.[].issue]' "$live/endings.json")" '[12,13]'

t_case "the endings keep the last ten, and a failure eleven endings ago is gone"
reset
t_run "$status"
ended 20 aborted 900 300
t_run "$health"
for n in 21 22 23 24 25 26 27 28 29 30; do
  ended "$n" merged 800 $((300 - n))
  t_run "$health"
done
t_eq "$(jq length "$live/endings.json")" 10
t_eq "$(jq -c .checks.failed "$live/health.json")" '[]'
t_eq "$(ls "$live/cycles")" "30.json"

t_case "an endings file that is not a list is written again"
reset
t_run "$status"
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

t_case "platform: the first poll, with no record of a fetch, prints nothing"
reset
platform_setup
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""

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

# A host that runs a release: ~/tehas/current is a link to a version with a
# VERSION, and the releases are what the gh stub answers from $T/rel.json.
# The releases of the platform come from the gh stub, which answers the list
# with nothing until a case puts it in $T/rel.json.
release_setup() { # <installed> [<installed> ...]
  rm -rf "$T/home/tehas/current" "$T/home/tehas/versions" "$T/home/tehas/platform" \
    "$T/home/tehas/state" "$T/home/tehas/versions.env" "$T/rel.json"
  mkdir -p "$T/home/tehas/versions" "$T/home/tehas/state"
  # The release repository, as the host's versions.env gives it.
  cp "$T_ROOT/host/versions.env" "$T/home/tehas/versions.env"
  local v
  for v in "$@"; do
    mkdir -p "$T/home/tehas/versions/$v"
    echo "$v" >"$T/home/tehas/versions/$v/VERSION"
  done
  ln -s "versions/$1" "$T/home/tehas/current"
  t_store_db "$T/home/tehas"
  TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "release_setup: the store made no backup"
}
# The published releases, newest first, as GitHub answers the list.
releases() { # <tag> [...]
  local line="" v
  for v in "$@"; do
    [ -n "$line" ] && line=$line,
    line=$line"{\"tag_name\":\"$v\",\"draft\":false,\"prerelease\":false}"
  done
  printf '[%s]\n' "$line" >"$T/rel.json"
}
# A release host runs from current and names the release, not a checkout.
t_case "platform: a release host names the release and the newest, and passes"
reset
release_setup v0.9.0
releases v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c 'del(.generatedAt, .fetchedAt)' "$live/platform.json")" \
  '{"release":"v0.9.0","newest":"v0.9.0","behind":0,"upstream":"releases"}'
t_contains "$t_out" "platform  v0.9.0, newest v0.9.0, fetched "
t_eq "$(jq -r .checks.platform.release "$live/health.json")" v0.9.0
t_eq "$(jq -r .checks.platform.commit "$live/health.json")" null

t_case "platform: a newer release is said as one that is out"
reset
release_setup v0.9.0
releases v0.9.1 v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "
t_eq "$(jq -r .checks.platform.newest "$live/health.json")" v0.9.1

t_case "platform: releases that cannot be read leave the newest unknown, and nothing else changes"
reset
release_setup v0.9.0
# Neither gh nor curl answers: the list has no newest.
T_ENV=(GH_AUTH=1 CURL_RC=22)
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c 'del(.generatedAt, .fetchedAt)' "$live/platform.json")" \
  '{"release":"v0.9.0","newest":null,"behind":0,"upstream":"releases"}'
t_contains "$t_out" "platform  v0.9.0, newest unknown, never fetched"

t_case "platform: the releases are read at most once an hour"
reset
release_setup v0.9.0
releases v0.9.0
t_run "$status"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
releases v0.9.1 v0.9.0
t_run "$status"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.0
t_eq "$(cat "$T/home/tehas/state/platform-newest")" v0.9.0
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
t_run "$status && $summary"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 2
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "

t_case "platform: a release host with no record of a read says it was never fetched"
reset
release_setup v0.9.0
t_run "$status && $summary"
t_contains "$t_out" "platform  v0.9.0, newest unknown, never fetched"
t_eq "$(jq -r .fetchedAt "$live/platform.json")" null

t_case "platform: a release host keeps the newest of the last read when GitHub goes down"
reset
release_setup v0.9.0 v0.9.1
releases v0.9.1 v0.9.0
t_run "$status"
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
T_ENV=(GH_AUTH=1)
rm -f "$T/rel.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "

# A cached newest is an answer from an earlier read, so it is only believed
# while it is not behind what the host runs.
t_case "platform: a cached newest older than the installed release is not said"
reset
release_setup v0.10.0 v0.9.0
releases v0.9.0
# The read of an hour ago, before the host moved to v0.10.0.
mkdir -p "$T/home/tehas/state"
now=$(date +%s)
echo "$((now - 7200)) $((now - 7200))" >"$T/home/tehas/state/platform-fetch"
echo v0.9.0 >"$T/home/tehas/state/platform-newest"
T_ENV=(GH_AUTH=1 CURL_RC=22)
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" null
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest unknown, fetched "

t_case "platform: a cached newest older than the installed release is read again"
reset
release_setup v0.10.0 v0.9.0
releases v0.10.0
mkdir -p "$T/home/tehas/state"
now=$(date +%s)
echo "$((now - 7200)) $((now - 7200))" >"$T/home/tehas/state/platform-fetch"
echo v0.9.0 >"$T/home/tehas/state/platform-newest"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.10.0
t_eq "$(cat "$T/home/tehas/state/platform-newest")" v0.10.0
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.10.0, fetched "

# The words of the line, whatever the two versions are: only a newest greater
# than the installed release is out.
t_case "platform: the same release installed and newest says no 'is out'"
reset
release_setup v0.10.0
releases v0.10.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.10.0, fetched "

t_case "platform: a newest published behind the installed release says no 'is out'"
reset
release_setup v0.10.0
releases v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.9.0, fetched "

# A VERSION the host wrote without the v is shown as it is: it is no triple
# beside v0.10.0, so it is never said to be out.
t_case "platform: an installed version with no v is shown as it is, never as one that is out"
reset
release_setup 0.10.0
releases v0.10.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  0.10.0, newest v0.10.0, fetched "

t_case "platform: a checkout host with a current that is not a release stays on the checkout"
reset
platform_setup
# tehas_release_installed reads a link with a VERSION and nothing else, which
# is what keeps a host whose current is not such a link on its checkout.
mkdir -p "$T/home/tehas/versions/v0.9.0"
echo v0.9.0 >"$T/home/tehas/versions/v0.9.0/VERSION"
touch "$T/home/tehas/current"
t_run "$status && $summary"
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), up to date with origin/main, fetched "
t_eq "$(jq -r .checks.platform.release "$live/health.json")" null

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

t_case "summary takes no argument but --json"
reset
t_run "$summary --jsn"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas status [--json]"
t_run "$summary --json --json"
t_eq "$t_rc" 64

# What the dispatcher's last poll wrote: the demand, and what held cycles back.
demand() { # <waiting> [<seconds since the oldest was accepted> [<age of the file>]]
  mkdir -p "$live"
  jq -n --argjson w "$1" --arg at "$(date -u -d "-${3:-30} seconds" +%FT%TZ)" \
    --arg oldest "$(date -u -d "-${2:-0} seconds" +%FT%TZ)" \
    '{time: $at, waiting: $w} + (if $w > 0 then {oldest: $oldest} else {} end)' >"$live/demand.json"
}
held() { # <why> <text> <free> <await room>
  mkdir -p "$live"
  jq -n --arg why "$1" --arg hold "$2" --argjson free "$3" --argjson aw "$4" --arg at "$(date -u -d '-30 seconds' +%FT%TZ)" \
    '{time: $at, why: $why, hold: $hold, count: 1, room: 3, committed: 2147483648, max: 21474836480, reserve: "2g",
      free: $free, awaitRoom: $aw}' >"$live/dispatch.json"
}
# The types status --json promises, for every object it prints.
shape='(.api == 1) and (.time | type) == "string" and (.age | type) == "number" and (.stale | type) == "boolean"
  and (.state | IN("pass", "warn", "fail")) and (.reasons | type) == "array" and (.reasons | all(type == "string"))
  and (.paused | (.local | type) == "boolean" and (.label | type) == "boolean")
  and (.allHands | type) == "boolean" and (.limit.state | type) == "string"
  and (.running | type) == "array"
  and (.running | all((.issue | type) == "number" and (.since | type) == "string"))
  and (.planning | type) == "array"
  and (.planning | all((.issue | type) == "number" and (.turn | type) == "number"))
  and (.demand | . == null or ((.waiting | type) == "number"))
  and (.hold | . == null or ((.why | type) == "string" and (.text | type) == "string"))
  and (.room | . == null or type == "object")'

t_case "status --json: a fresh idle factory, one object of the promised shape"
reset
demand 0
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq "$shape" <<<"$t_out")" true
t_eq "$(jq -c '{state, stale, reasons, paused, allHands, limit, running, demand, hold}' <<<"$t_out")" \
  '{"state":"pass","stale":false,"reasons":[],"paused":{"local":false,"label":false},"allHands":false,"limit":{"state":"none"},"running":[],"demand":{"waiting":0},"hold":null}'
t_eq "$(jq -r '.age < 60' <<<"$t_out")" true

t_case "status --json: the demand, with how long the oldest has waited"
reset
demand 2 5400
t_run "$status && $summary --json"
t_eq "$(jq -r .demand.waiting <<<"$t_out")" 2
t_eq "$(jq -r '.demand.oldestWait | . >= 5400 and . < 5460' <<<"$t_out")" true

t_case "status: a hold on refreshes is in the demand and in the text"
reset
demand 0
jq '. + {refreshHold: "the pi-rukas in the sandbox image does not take --continue-pr"}' "$live/demand.json" >"$live/demand.new" &&
  mv "$live/demand.new" "$live/demand.json"
t_run "$status && $summary --json"
t_eq "$(jq -r .demand.refreshHold <<<"$t_out")" "the pi-rukas in the sandbox image does not take --continue-pr"
t_eq "$(jq "$shape" <<<"$t_out")" true
t_run "$status && $summary"
t_contains "$t_out" "refresh   held: the pi-rukas in the sandbox image does not take --continue-pr"

t_case "status: no hold on refreshes, no line and no field"
reset
demand 0
t_run "$status && $summary"
t_lacks "$t_out" "refresh   "
t_run "$summary --json"
t_eq "$(jq -c '.demand | has("refreshHold")' <<<"$t_out")" false

t_case "status --json: no demand from the last ten minutes is null"
reset
demand 2 5400 700
t_run "$status && $summary --json"
t_eq "$(jq -c .demand <<<"$t_out")" null
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: a busy factory lists its running cycles"
reset
cycle true running develop
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.running | map({issue, step})' <<<"$t_out")" '[{"issue":12,"step":"develop"}]'
t_eq "$(jq -r '.running[0].since | fromdateiso8601 | type' <<<"$t_out")" number
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: a held factory says why, and the room under each cap"
reset
held count "holding: 1 cycles are running (limit 1)" 0 3
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"count","text":"holding: 1 cycles are running (limit 1)"}'
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":0,"maxAwaiting":3,"slice":3}'
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status: an unknown gate is the hold, in the text and in --json"
# A gate whose runs finished without saying they passed holds the same way as
# a red branch, under a word of its own: the one tells a person to fix the
# branch, the other to name the right check.
reset
demand 0
held unknown-gate 'holding: the check "build / verify (linux)" on main at 0123456789ab finished without saying it passed: cycles start' 1 3
t_run "$status && $summary"
t_contains "$t_out" 'slice     holding: the check "build / verify (linux)" on main at 0123456789ab'
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"unknown-gate","text":"holding: the check \"build / verify (linux)\" on main at 0123456789ab finished without saying it passed: cycles start"}'

t_case "status: a red branch is the hold, in the text and in --json"
reset
held red "holding: main is red at 0123456789ab" 1 3
t_run "$status && $summary"
t_contains "$t_out" "slice     holding: main is red at 0123456789ab ("
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"red","text":"holding: main is red at 0123456789ab"}'

t_case "status --json: room the last poll did not read is left out"
reset
held "" "" 1 null
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" null
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":1,"slice":3}'

t_case "status --json: a paused factory, by the flag or by the label, is held by the pause"
reset
mkdir -p "$T/home/tehas/state"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "waiting for the gate"}' >"$T/home/tehas/state/pause.json"
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.state, .paused, .hold.why]' <<<"$t_out")" '["warn",{"local":true,"label":false},"pause"]'
t_contains "$(jq -r .hold.text <<<"$t_out")" "waiting for the gate"
t_lacks "$t_out" "boss"
t_eq "$(jq -r '.reasons | length' <<<"$t_out")" 1
t_eq "$(jq "$shape" <<<"$t_out")" true
reset
echo true >"$T/paused"
t_run "$status && $summary --json"
t_eq "$(jq -c '[.paused, .hold.why]' <<<"$t_out")" '[{"local":false,"label":true},"pause"]'

t_case "status --json: a failed cycle is exit 1, as the text is"
reset
cycle false aborted develop
t_run "$status && $summary --json"
t_eq "$t_rc" 1
t_eq "$(jq -r .state <<<"$t_out")" fail
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "#12 ended as aborted"

t_case "status --json: a stale health file is exit 1 and says so"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary --json"
t_eq "$t_rc" 1
t_eq "$(jq -c '[.stale, .age >= 10800]' <<<"$t_out")" '[true,true]'
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: no health file is exit 1 with nothing on standard output"
reset
t_run "$summary --json 2>/dev/null"
t_eq "$t_rc" 1
t_eq "$t_out" ""

t_case "status --json reads nothing but the health file: no settings, no gh, no write"
reset
demand 1 60
t_run "$status"
rm -f "$T"/calls/*
t_instance 'echo "settings were read" >&2; exit 9'
before=$(cd "$live" && cksum ./*)
t_run "$summary --json"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls gh)" ""
t_eq "$(cd "$live" && cksum ./*)" "$before"

# tehas:all-hands on the control issue: reported, and nothing else.
all_hands() { # <login or ""> [labeled|unlabeled]: the label's last event, and the label on the issue
  [ "${2:-labeled}" = unlabeled ] || : >"$T/allhands"
  jq -nc --arg w "$1" --arg e "${2:-labeled}" '[{event: $e, label: {name: "tehas:all-hands"},
    actor: (if $w == "" then null else {login: $w} end), created_at: "2026-10-17T10:00:00Z"}]' >"$T/events.json"
}

t_case "all-hands: applied by an allowed account, it is reported and changes nothing else"
reset
all_hands alice
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .allHands "$live/status.json")" true
t_eq "$(jq -r .checks.allHands "$live/health.json")" true
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "all-hands tehas:all-hands is on the control issue"
t_run "$summary --json"
t_eq "$(jq -c '[.allHands, .state, .hold]' <<<"$t_out")" '[true,"pass",null]'

t_case "all-hands: applied by anyone else it is ignored, and said once"
reset
all_hands mallory
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false
t_contains "$t_out" "tehas:all-hands on #1 was applied by 'mallory', not an allowed account: ignored"
t_run "$status"
t_eq "$t_out" ""
t_run "$summary"
t_lacks "$t_out" "all-hands"

t_case "all-hands: not on the issue is off, whatever its last event says"
reset
all_hands alice unlabeled
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false
reset
all_hands alice
rm "$T/allhands"
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false

t_case "all-hands: on the issue with events that cannot be read is unknown, and left out of --json"
reset
all_hands alice
rm "$T/events.json"
t_run "$status && $summary --json"
t_eq "$(jq -r .allHands "$live/status.json")" null
t_eq "$(jq -c 'has("allHands")' <<<"$t_out")" false
reset
: >"$T/control-down"
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" null

t_case "all-hands: a snapshot older than ten minutes does not say it"
reset
all_hands alice
t_run "$status"
jq --arg t "$(date -u -d '-20 minutes' +%FT%TZ)" '.generatedAt = $t' "$live/status.json" >"$live/status.json.old"
mv "$live/status.json.old" "$live/status.json"
t_run "$health"
t_eq "$(jq -r .checks.allHands "$live/health.json")" null

t_case "all-hands: a paused factory still reports it"
reset
all_hands alice
echo true >"$T/paused"
t_run "$status && $summary --json"
t_eq "$(jq -c '[.allHands, .paused.label]' <<<"$t_out")" '[true,true]'

# The limit tehas limit sets, as health reads it from state/limit.json.
set_limit() { # <json>
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$1" >"$T/home/tehas/state/limit.json"
}

t_case "limit: a limit is reported in the lines and in --json, its reason only in the lines"
reset
set_limit '{"v":1,"cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z","reason":"held for alpha"}'
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "limit     2 cycles at once, planning off, until 2036-01-01T00:00:00Z: held for alpha"
t_eq "$(jq -r .status "$live/health.json")" pass
t_run "$summary --json"
t_eq "$(jq -c .limit <<<"$t_out")" '{"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z"}'

t_case "limit: no limit prints no line, and --json says none"
reset
t_run "$status && $summary"
t_lacks "$t_out" "limit "
t_run "$summary --json"
t_eq "$(jq -c .limit <<<"$t_out")" '{"state":"none"}'

t_case "limit: expired or unreadable is a warning that says nothing starts"
reset
set_limit '{"v":1,"cycles":2,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}'
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.state, .limit.state]' <<<"$t_out")" '["warn","expired"]'
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "Nothing starts: the limit expired at 2020-01-01T00:00:00Z"
reset
set_limit 'not a limit'
t_run "$status && $summary --json"
t_eq "$(jq -c '[.state, .limit]' <<<"$t_out")" '["warn",{"state":"unreadable"}]'
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "Nothing starts: the limit cannot be read"

t_case "limit: the room under it, from the last poll"
reset
held limit "holding: the limit allows 1 cycle at once and 1 is running" 1 2
jq '.limitRoom = 0' "$live/dispatch.json" >"$live/dispatch.json.new" && mv "$live/dispatch.json.new" "$live/dispatch.json"
t_run "$status && $summary --json"
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":1,"maxAwaiting":2,"slice":3,"limit":0}'
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"limit","text":"holding: the limit allows 1 cycle at once and 1 is running"}'

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

t_case "health: a hold on the disk is a warning with the hold as output"
# A full disk, and a disk whose figure cannot be read, both hold every start,
# so /health warns and tehas status carries the line the poll logged. The two
# share the word: neither names a number a person can fix.
for hold in "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts" "holding: the free disk space could not be read: no cycle starts"; do
  t_case "health: the disk hold '$hold' is a warning"
  reset
  poll disk "$hold"
  t_run "$status; $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" warn
  t_eq "$(jq -r .output "$live/health.json")" "$hold"
  t_eq "$(jq -r .checks.slice.why "$live/health.json")" disk
  t_eq "$(find "$live" -name health.down | wc -l)" 0
  t_contains "$t_out" "factory   warn: $hold"
  t_contains "$t_out" "slice     $hold (4"
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
# No checkout on this host, so the platform line stays out and the slice line
# is the last of the four.
rm -rf "$T/home/tehas/platform"
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

t_case "summary: with a refresh lane the slice line says how much of it is free"
reset
rm -rf "$T/home/tehas/platform"
poll "" ""
jq '.refreshRoom = 1' "$live/dispatch.json" >"$live/x" && mv "$live/x" "$live/dispatch.json"
t_run "$status && $summary"
t_contains "$t_out" "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, refresh lane 1 free, 4"
t_eq "$(jq -r .checks.slice.refreshRoom "$live/health.json")" 1

t_case "summary: a health file written before the slice was known says unknown"
reset
t_run "$status"
jq 'del(.checks.slice)' "$live/health.json" >"$live/x" && mv "$live/x" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

t_case "health and summary read files only, and ask systemd only for the failed units"
reset
t_stub systemctl
t_stub docker
poll room "holding: the reason"
t_run "$status && $summary"
t_eq "$(t_calls systemctl)" "--user --failed --plain --no-legend tehas-*"
t_eq "$(t_calls docker)" ""

t_case "a lost cycle ends two minutes after its last heartbeat"
reset
one 9 true running develop 900 300
one 12 false merged review 600 200
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[.last.issue, .last.status]' "$live/running.json")" '[9,"lost"]'
t_eq "$(find "$live/cycles" -name '*.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "9.json "

t_case "health warns when the state store has no backup, or only one more than two days old"
reset
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r '.checks.backup.ageSeconds < 60' "$live/health.json")" true
rm -rf "$T/home/tehas/state/backup"
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "There is no backup of the state store: tehasbot clean makes one a day."
t_eq "$(jq -c .checks.backup "$live/health.json")" null
TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null
touch -d '72 hours ago' "$T"/home/tehas/state/backup/tehas-*.db
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The newest backup of the state store is 72 h old."

# The cost of the polls that started nothing, read from the state store beside
# the backup. A poll of the factory the plan of #730 describes costs about 3 s
# of CPU and 20 s of wall, so the default budget of cpu=5,wall=30,writes=0
# leaves it alone and warns at one that costs more than that.
poll_record() { # <cpu_seconds> <wall_seconds> [WRITES] [STARTS]: one poll of
# the hour on record, whose CPU time is the four figures bash's `times` writes
# (so a quarter of the figure each, which the store adds back) and whose wall
# time is the second it names, asked of the store's own clock. WRITES left out
# is no write log at all, which leaves the figure off the answer; 0 is a log of
# none, which the store says as none and no budget of writes can be over.
  local cpu=$1 wall=$2 writes=${3-} starts=${4:-0} q ms n now
  now=$(date +%s)
  q=$((cpu * 250))
  printf '0m%s.%03ds 0m%s.%03ds\n0m%s.%03ds 0m%s.%03ds\n' \
    "$((q / 1000))" "$((q % 1000))" "$((q / 1000))" "$((q % 1000))" \
    "$((q / 1000))" "$((q % 1000))" "$((q / 1000))" "$((q % 1000))" >"$T/poll.times"
  : >"$T/poll.tally"
  rm -f "$T/poll.writes"
  if [ -n "$writes" ]; then
    : >"$T/poll.writes"
    ms=$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)
    for ((n = 0; n < writes; n++)); do
      printf '%s\tissue edit\t\n' "$ms" >>"$T/poll.writes"
    done
  fi
  # The wall time is the difference of the two times the store reads from its
  # own clock, so both name the same second and the figure is the one asked for.
  t_at "$now" poll-record --started "$((now - wall))" --times "$T/poll.times" \
    --tally "$T/poll.tally" --writes-log "$([ -n "$writes" ] && printf %s "$T/poll.writes" || printf %s "$T/no-such.log")" \
    --starts "$starts" >/dev/null || t_bad "no poll recorded"
}

t_case "an idle poll within the budget is no warning, and the check is on record"
reset
poll_record 3 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc .checks.poll_cost "$live/health.json")" '{"over":[],"budget":"cpu=5,wall=30,writes=0"}'

t_case "an idle poll over its CPU budget warns and names only that figure"
reset
poll_record 6 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["cpu"]'
t_eq "$(jq -r .output "$live/health.json")" \
  "An idle poll is over its budget on cpu (TEHAS_POLL_BUDGET=cpu=5,wall=30,writes=0)."

t_case "an idle poll over all three budgets names them in the order of the spec"
reset
poll_record 6 40 2
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["cpu","wall","writes"]'

t_case "a poll of a write log that holds none passes the writes of the default budget"
reset
poll_record 3 20 0
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '[]'

t_case "one write is over the writes the default budget allows, which are none"
reset
poll_record 3 20 1
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["writes"]'

# The budget the instance set is the one the store is asked to measure against,
# and the health file carries it so a reader sees the figures beside their
# ceiling. The stub of the verb gives an over figure so the case does not lean
# on the store's arithmetic, which the store's own tests cover.
t_case "the budget of the instance is what the store is asked for, and the health file says it"
reset
t_store_answers poll-idle 0 '{"api":1,"ok":true,"verb":"poll-idle","polls":1,"cpu_ms":6000,"wall_ms":20000,"writes":0,"over":["cpu"]}'
T_ENV=(TEHAS_INSTANCE_DIR="$T/budget-instance")
mkdir -p "$T/budget-instance"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_POLL_BUDGET=cpu=10,wall=10" >"$T/budget-instance/instance.env"
poll_record 3 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc .checks.poll_cost "$live/health.json")" '{"over":["cpu"],"budget":"cpu=10,wall=10"}'
t_contains "$(t_calls store)" "poll-idle --since 3600 --budget cpu=10,wall=10"
T_ENV=()

t_case "a poll that started a cycle is not an idle poll and costs no warning"
reset
poll_record 60 60 3 1
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '[]'
t_eq "$(jq -rc .checks.poll_cost.budget "$live/health.json")" cpu=5,wall=30,writes=0

t_case "the store's refusal is no warning, and the health file says there was no check"
reset
poll_record 6 60
t_store_answers poll-idle 1 'store: busy: the store is held'
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c .checks.poll_cost "$live/health.json")" null

t_done
