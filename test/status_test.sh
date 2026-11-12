#!/usr/bin/env bash
# Tests for the three commands around the factory's state: tehasbot status
# writes the queue snapshot and runs tehasbot health beside it, and
# `tehas status` prints the health file.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/status_lib.sh
. "$(dirname "$0")/status_lib.sh" || exit 1
t_case "status writes the snapshot and the health file, without a command on PATH"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.review "$live/health.json")" 1

t_case "status with a setting not given stops with 78, before GitHub and before a write, however it is started"
reset
rm -rf "$live"
all=("TEHAS_HOME=$T/home/tehas" TEHAS_REPO=acme/widget TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=boss)
for name in TEHAS_HOME TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS; do
  given=()
  for word in "${all[@]}"; do [[ $word == "$name="* ]] || given+=("$word"); done
  t_run "env -u $name ${given[*]} '$shim/libexec/console' tehasbot status"
  t_eq "$t_rc" 78
  t_contains "$t_out" "$name is not set"
  t_eq "$(t_calls gh)" ""
  [ ! -e "$live" ] || t_bad "$name: status wrote $live"
done

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

t_case "an open issue labelled epic with no sub-issues is listed apart and the health file warns of it"
reset
epics_page '{"number":7,"title":"An epic","url":"u7","labels":{"nodes":[{"name":"epic"}]},"subIssuesSummary":{"total":1,"completed":0},
  "subIssues":{"nodes":[{"number":8,"title":"To do","url":"u8","state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}]}}' \
  '{"number":5,"title":"EPIC: Unlinked","url":"u5","labels":{"nodes":[{"name":"epic"}]},"subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}' \
  '{"number":3,"title":"Plain","url":"u3","labels":{"nodes":[]},"subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}'
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[(.epics | map(.number)), .unlinked]' "$live/epics.json")" '[[7],[5]]'
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -c .checks.epics.unlinked "$live/health.json")" '[5]'
t_contains "$(jq -r .output "$live/health.json")" "Labelled epic with no sub-issues: #5."

t_case "a read of the epics that fails leaves the file as it was and the snapshot is still written"
reset
mkdir -p "$live"
echo '{"epics":"before"}' >"$live/epics.json"
touch "$T/epics-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c .epics "$live/epics.json")" '"before"'
t_eq "$(jq -c .checks.epics "$live/health.json")" null
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(find "$live" -name 'epics.json.*' | wc -l)" 0

t_case "no failed tehas unit is no warning"
reset
: >"$T/failed-units"
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
: >"$T/failed-units"

t_case "systemd not answering the failed units is no check and no warning"
reset
T_ENV=(SYSTEMCTL_RC=1)
t_run "$status"
t_eq "$(jq -c .checks.units "$live/health.json")" 'null'
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$(t_calls systemctl)" "--user --failed --plain --no-legend tehas-*"

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

# Of three shipped issues the poll edits the two that lack the label and the one
# whose labels are no list (issue #1083).
t_case "the issues to label are the ones the read says lack the label"
reset
shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}' tehas:shipped)" \
  "$(closed 21 2026-01-03T00:00:00Z '{"merged":true}')" \
  "$(no_labels 22 2026-01-04T00:00:00Z '{"merged":true}')"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.completed[].number] | sort' "$live/status.json")" "[20,21,22]"
t_eq "$(t_calls gh | grep -c 'issue edit')" 2
t_contains "$(t_calls gh)" "issue edit 21 --repo acme/widget --add-label tehas:shipped"
t_contains "$(t_calls gh)" "issue edit 22 --repo acme/widget --add-label tehas:shipped"
t_lacks "$(t_calls gh)" "issue edit 20 --repo"

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
# It holds the pause switch and is not work: tehasbot status leaves the number
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

# The parts of the snapshot of the run before, for a case to compare: the lists
# of work, and the completed work.
lists() { jq -c '[.running, .queued, .awaiting, .stopped, .planned, .planning]' "$live/status.json"; }
done_part() { jq -c '[.completedAt, .completed, .completedCount]' "$live/status.json"; }

t_case "without the search for shipped work the completed part is kept and said stale"
reset
t_run "$status"
lists_before=$(lists)
done_before=$(done_part)
: >"$T/shipped-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(lists)" "$lists_before"
t_eq "$(done_part)" "$done_before"
t_eq "$(jq -c .stale "$live/status.json")" '["completed"]'
t_contains "$t_out" "WARN status: gh api graphql failed"
echo '{"errors": [{"message": "rate limited"}]}' >"$T/shipped.json"
rm "$T/shipped-down"
t_run "$status"
t_eq "$(done_part)" "$done_before"
t_eq "$(jq -c .stale "$live/status.json")" '["completed"]'

t_case "a list of issues that cannot be read keeps the lists of work and says why"
# The read of the list of issues in work is the one the lists rest on: they
# are the file's before, said stale, and the rest of the snapshot is this run's.
reset
t_run "$status"
lists_before=$(lists)
read_at=$(jq -r .queueAt "$live/status.json")
rm "$live/health.json"
# The stub answers the search of the issues in work from $T/issues.json and the
# word cloud of every issue from $T/all.json: only the first file is gone here,
# so the read of the lists fails and the one it can do without does not. t_case
# clears $T/calls, so the line below is this case's call.
rm "$T/issues.json"
echo true >"$T/paused"
t_run "$status"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --limit 200"
t_contains "$t_out" "WARN status: gh issue list failed"
t_eq "$(lists)" "$lists_before"
t_eq "$(jq -c .stale "$live/status.json")" '["queue"]'
t_eq "$(jq -r .queueAt "$live/status.json")" "$read_at"
# What was read is fresh: the pause on the control issue.
t_eq "$(jq -r .paused "$live/status.json")" true
t_eq "$(cut -f2 "$T/home/tehas/state/github-failures")" "WARN status: gh issue list failed"
t_eq "$(jq -r .checks.github.failed "$live/health.json")" 1

t_case "an answer of what closed these issues that is no list keeps the completed part"
reset
t_run "$status"
done_before=$(done_part)
printf '%s\n' '{"data":{}}' >"$T/shipped.json"
t_run "$status"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN status: the answer of what closed these issues was no list, its part of the snapshot is stale"
t_eq "$(done_part)" "$done_before"
t_eq "$(jq -c .stale "$live/status.json")" '["completed"]'
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "a read on a bad minute is tried once more and the snapshot is whole"
reset
t_run "$status"
rm "$live/health.json"
t_gh_fail 1 1 "HTTP 502: Bad Gateway"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -- '--limit 200')" 2
t_eq "$(jq -c .stale "$live/status.json")" null
t_lacks "$t_out" "WARN status"
t_eq "$([ -e "$T/home/tehas/state/github-failures" ] && echo kept)" ""

t_case "with GitHub down the lists are kept, and health names the failed calls"
reset
t_run "$status"
lists_before=$(lists)
rm "$live/health.json"
: >"$T/gh-down"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(lists)" "$lists_before"
t_eq "$(jq -c .stale "$live/status.json")" '["queue","completed"]'
t_eq "$(jq -r .status "$live/health.json")" warn
t_contains "$(jq -r .output "$live/health.json")" "GitHub failed 6 calls in the last 10 min, the last: WARN status: gh "
t_contains "$t_out" "queue     stale: queue and completed kept from an earlier read, GitHub did not answer"
t_contains "$t_out" "github    6 failed calls in the last 10 min"

t_case "given the poll's name, status leaves the health file to the poll"
reset
T_ENV=(TEHAS_STATUS_CMD="$status") t_run "unset TEHAS_FROM_POLL; $status"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
rm "$live/health.json"
T_ENV=(TEHAS_STATUS_CMD="$status") t_run "export TEHAS_FROM_POLL=1; $status"
t_eq "$t_rc" 0
t_eq "$([ -e "$live/health.json" ] && echo written)" ""
T_ENV=(TEHAS_STATUS_CMD="$status") t_run "export TEHAS_FROM_POLL=; $status"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass

# The name of the token is not the name of the poll: a host whose poll holds
# no App token, or whose token read failed, still has health to refresh.
t_case "the poll's token alone does not leave the health file to the poll"
reset
rm "$live/health.json"
T_ENV=(TEHAS_STATUS_CMD="$status") t_run "export TEHAS_GH_TOKEN=ghs_poll; $status"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "the poll's name leaves the analytics of the trap alone"
reset
t_stub_analytics 'printf "%s\n" "{\"report\":1}"'
T_ENV=(TEHAS_STATUS_CMD="$status" TEHAS_ANALYTICS=on) t_run "export TEHAS_FROM_POLL=1; $status"
t_eq "$t_rc" 0
t_eq "$(wc -l <"$T/calls/analytics")" 4

t_case "with GitHub down for three hours, health warns and the queue line says how old it is"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours -5 minutes' +%FT%TZ)" '.generatedAt = $t | .queueAt = $t' "$live/status.json" >"$T/old.json"
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
t_at "$(date +%s)" poll-record --started "$(($(date +%s) - 30))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/no-such.log" >/dev/null || t_bad "no poll recorded"
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
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/bin/tehasbot health"
t_eq "$t_rc" 1
t_contains "$t_out" "the planning sessions could not be read from the state store"
t_eq "$([ -e "$live/health.json" ] && echo written)" ""
t_store_answers plan-show 78 'store: corrupt: the file is not a database'
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/bin/tehasbot health"
t_eq "$t_rc" 78
t_eq "$([ -e "$live/health.json" ] && echo written)" ""
rm -f "$T/home/tehas/libexec/store"
t_run "TEHAS_HOME=$T/home/tehas $T_ROOT/bin/tehasbot health"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"

t_case "health: no console beside the command is 78 and says to install"
# The wrapper's own answer, as its siblings give it: the file would otherwise
# age in silence, which is the failure the manual names for a stopped
# dispatcher.
reset
# The host where install.sh put no console beside the command: no console in
# the directory of the copy, and no name in the environment. The console every
# other run of this file is given is named by the file's T_ENV_ALWAYS, which
# the case cannot empty, so the wrapper of this case is given the empty name by
# its own command line and the run is called with env -i through t_run's PATH.
mkdir -p "$T/no-console/bin" "$T/no-console/libexec"
cp "$T_ROOT/bin/tehasbot" "$T/no-console/bin/tehasbot"
rm -f "$T/bin/console"
T_ENV=(TEHAS_CONSOLE= TEHAS_CONSOLE= PATH=/usr/bin:/bin)
t_run "unset TEHAS_CONSOLE; TEHAS_HOME=$T/home/tehas $T/no-console/bin/tehasbot health"
t_eq "$t_rc" 78
t_contains "$t_out" "tehasbot: no console at $T/no-console/libexec/console: run install.sh"
t_eq "$([ -e "$live/health.json" ] && echo written)" ""

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


t_done
