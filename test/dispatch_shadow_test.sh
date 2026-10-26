#!/usr/bin/env bash
# Tests for tehas-dispatch (console/dispatch): its snapshot of a world, and its
# decision on it beside the bash poll's. Each world is one the dispatch tests
# build; a dry run of libexec/dispatch and of `tehas-dispatch decide` on it must
# log the same lines. And tehas-dispatch writes nothing: it makes no call of gh
# that changes anything, and no file under the home changes.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"

# The open issues as tehas-dispatch's one GraphQL page asks for them, made from
# the fixtures the bash poll's REST calls answer from: the lists name the
# issues and their labels, events-<n>.json their label events, work-<n>.json
# and edit-<n>.json (or edit.json) their text, and prs-<n>.json (or prs.json)
# the pull requests that close them. An issue marked running and CLOSED is a
# closed one.
cat >"$T/page-stub" <<'STUB'
#!/usr/bin/env bash
fx=$T/fx
read_or() { # <file> <second file or ""> <default>
  cat "$1" 2>/dev/null && return
  [ -n "$2" ] && cat "$2" 2>/dev/null && return
  printf '%s' "$3"
}
lists=$(for f in ready blocked asking plans refines awaiting marked; do read_or "$fx/$f.json" "" "[]"; echo; done | jq -cs 'map(. // [])')
nums=$(jq -r '[.[][] | select((.state // "OPEN") != "CLOSED") | .number | tonumber? // empty] + [1] | unique | .[]' <<<"$lists")
items=$(for n in $nums; do
  jq -nc --argjson n "$n" --argjson ev "$(read_or "$fx/events-$n.json" "" "[]")" \
    --argjson w "$(read_or "$fx/work-$n.json" "" null)" \
    --argjson e "$(read_or "$fx/edit-$n.json" "$fx/edit.json" "{}")" \
    --argjson p "$(read_or "$fx/prs-$n.json" "$fx/prs.json" "{}")" \
    '{n: $n, ev: $ev, w: $w, e: ($e.data.repository.issue // {}), p: $p}'
done | jq -cs .)
jq -n --argjson lists "$lists" --argjson items "$items" --argjson control "$(read_or "$fx/control.json" "" '{"labels":[]}')" \
  --argjson repo "$(read_or "$fx/repo.json" "" '{}')" '
  def labels($n): [$lists[][] | select((.number | tonumber?) == $n) | (.labels // [])[].name]
    + (if $n == 1 then [$control.labels[].name] else [] end)
    # Being on a list is carrying its label, as gh issue list --label has it.
    + [range(7) as $i | select(any($lists[$i][]; (.number | tonumber?) == $n))
       | ["tehas:ready", "tehas:blocked", "tehas:question", "tehas:plan", "tehas:refine",
          "tehas:awaiting-merge", "tehas:running"][$i]] | unique;
  def actor: if .actor == null then null else {login: .actor.login} end;
  {data: {repository: {nameWithOwner: $repo.nameWithOwner,
    issues: {pageInfo: {hasNextPage: false, endCursor: null},
      nodes: [$items[] | select((.w.state // "open") | ascii_downcase == "open") | . as $i | {
        number: .n, title: (.e.title // .w.title // ""), body: (.e.body // .w.body),
        lastEditedAt: .e.lastEditedAt, editor: .e.editor,
        labels: {nodes: [labels($i.n)[] | {name: .}]},
        subIssuesSummary: (.w.sub_issues_summary // null),
        timelineItems: {nodes: [.ev[] | {__typename: (if .event == "labeled" then "LabeledEvent"
            elif .event == "unlabeled" then "UnlabeledEvent" else "RenamedTitleEvent" end),
          createdAt: .created_at, actor: actor, label: .label}]},
        closedByPullRequestsReferences: {nodes: (.p.data.repository.issue.closedByPullRequestsReferences.nodes // [])}}]},
    closedRunning: {nodes: [$lists[][] | select(.state == "CLOSED") | {number}]}}}}'
STUB
chmod +x "$T/page-stub"

# What each logged, without the bash's time: the lines of a dry run.
bash_lines() { t_run "$dispatch --dry-run"; sed -E 's/^[0-9T:-]+Z //' <<<"$t_out"; }
rust_lines() {
  t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" decide --dry-run"
  jq -r '.log[]' <<<"$t_out" 2>/dev/null || printf 'not a decision: %s\n' "$t_out"
}
# The two agree, line for line; a difference is shown as diff shows it.
agree() {
  local b r
  b=$(bash_lines)
  r=$(rust_lines)
  if [ "$b" != "$r" ]; then
    t_bad "the decisions differ:
$(diff <(printf '%s\n' "$b") <(printf '%s\n' "$r") | sed 's/^/  /')"
  fi
}

t_case "an issue an allowed account labelled: both would run its cycle"
reset
agree

t_case "a label of an account outside the list: both skip it and would take it off"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
agree

t_case "every slot taken: both hold"
reset
hold 9
agree
release

t_case "the urgent issue first, by an allowed account's label"
reset
slots 2
ready 7 8 9!
agree

t_case "the pause label on the control issue: both are paused"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
agree

t_case "an issue marked running with no cycle: both would park it"
reset
echo '[{"number":9}]' >"$fx/marked.json"
agree

t_case "a slice with no room: both hold, with the same numbers"
reset
other $((19 * gib + gib / 2))
agree

t_case "an epic: both skip it"
reset
jq -cn '{number: 7, state: "open", labels: [{name: "tehas:ready"}], sub_issues_summary: {total: 3, completed: 1}}' >"$fx/work-7.json"
agree

t_case "a plan label of an allowed account: both would start a session"
to_plan
agree

t_case "a retry ticked on a parked issue: both would run its cycle"
parked
comments 7 "$(park_body x ' ')" alice
agree

t_case "a pull request open on the issue: both skip it"
reset
echo '{"data":{"repository":{"issue":{"closedByPullRequestsReferences":{"nodes":[{"url":"https://github.com/acme/widget/pull/3","state":"OPEN"}]}}}}}' >"$fx/prs.json"
agree

t_case "a planning answer: both would run the next turn with the same words"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
agree

t_case "a planning turn of another issue in flight: both wait"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_plan "$T/home/tehas" 9 '{"session":"xyz","turns":1,"running":true}'
echo 9 >"$fx/plan-runs"
mkdir -p "$T/home/tehas/state"
exec {plan_fd}>"$T/home/tehas/state/plan-9.json.lock"
flock -n "$plan_fd"
agree
exec {plan_fd}>&-
rm -f "$fx/plan-runs"

t_case "a stranger's tick on a parked question: both would undo it"
parked
comments 7 "$(park_body x ' ')" mallory
agree

t_case "a limit of no cycles: both hold"
reset
limit 0
agree

t_case "a full disk: both hold"
reset
df_free 1
agree

t_case "an outage park owed its retry: both would start it again"
parked provider-outage
comments 7 "$(park_body ' ' ' ')" ""
echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
agree

t_case "a closed control issue that still carries the pause: both are paused"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
echo '{"number":1,"state":"closed"}' >"$fx/work-1.json"
agree

t_case "a cycle whose issue is closed: both would stop it"
reset
slots 2
hold 9
TEHAS_HOME=$T/home/tehas "$T_STORE" cycle-start 9 --by-hand >/dev/null
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
agree
release

# The model endpoints, as the dispatch tests stand them in: curl answers
# mesh.test from mesh.json and backup.test from backup.json, and an image
# request while reads-images is there.
t_stub curl 'case "$*" in
  *"/chat/completions") cat >/dev/null; [ -e "$T/fx/reads-images" ] ;;
  *backup.test*) cat "$T/fx/backup.json" 2>/dev/null ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac'

t_case "a model endpoint that does not answer: both hold on no resources"
reset
mesh
agree

t_case "a model endpoint that does not offer the model: both hold, and say which"
reset
mesh '{"data":[{"id":"other"}]}'
agree

t_case "a main endpoint down and its backup up: both say so and would run the cycle"
reset
mesh
jq -n '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1", backupBaseUrl: "http://backup.test/v1"}}}' >"$T/home/.pi/agent/models.json"
echo '{"data":[{"id":"test-model"}]}' >"$fx/backup.json"
agree
rm -f "$fx/backup.json"

t_case "an issue that links an image, a model that does not declare image input: both hold it"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
jq -n '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1", models: [{id: "test-model", input: ["text"]}]}}}' >"$T/home/.pi/agent/models.json"
jq -nc '{number: 7, state: "open", body: "It looks like this:\n\n![shot](https://github.com/user-attachments/assets/0f3e-41)"}' >"$fx/work-7.json"
agree

t_case "the same issue and a model that reads images: both would run the cycle"
: >"$fx/reads-images"
jq -n '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1", models: [{id: "test-model", input: ["text", "image"]}]}}}' >"$T/home/.pi/agent/models.json"
agree
rm -f "$fx/reads-images" "$T/home/.pi/agent/models.json" "$fx/mesh.json"

t_case "a workspace no cycle has used for seven days: both would remove it"
reset
works=$T/home/tehas/work
mkdir -p "$works/widget-5" "$T/home/tehas/state/cycles"
: >"$works/.tehas-workspaces"
touch -d "8 days ago" "$works/widget-5" "$T/home/tehas/state/cycles/5.lock"
agree
rm -rf "$works/widget-5" "$T/home/tehas/state/cycles/5.lock"

t_case "a cycle not heard from for 20 minutes: both would stop it"
reset
slots 2
# Its issue is open and marked, as a running cycle's is: the page lists it.
echo '[{"number":9}]' >"$fx/marked.json"
hold 9
t_at "$(($(date +%s) - 3600))" cycle-start 9 --by-hand >/dev/null
t_at "$(($(date +%s) - 1300))" cycle-heartbeat 9 --running --status running --step develop >/dev/null
agree
release

# tehas-dispatch is given a gh that refuses every call that is no read: it
# must not make one, whatever it decides. The calls it makes are recorded.
t_case "tehas-dispatch makes no call that writes, and changes no file of the home"
reset
echo '[{"number":9}]' >"$fx/marked.json"
rm -f "$T/calls/gh"
# Every file of the home with its sum. SQLite's own -shm and -wal files are
# left out: any reader of the store makes them, the bash poll's reads too.
files() { find "$T/home" -type f ! -name '*.db-shm' ! -name '*.db-wal' | sort | xargs -r md5sum; }
before=$(files)
t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" decide"
t_eq "$(jq -r .exit <<<"$t_out")" 0
after=$(files)
t_eq "$(diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | grep '^[<>]')" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_contains "$(t_calls gh)" "api graphql"

# The shadow beside a real poll, as its unit runs it: shadow before the poll,
# the poll's lines in a file, compare after it.
shadow_log=$T/home/tehas/logs/dispatch-shadow.log
beside() { # <what the poll is given>: one shadowed poll
  mkdir -p "$T/home/tehas/logs"
  : >"$T/home/tehas/state/dispatch-shadow.on"
  rm -f "$shadow_log"
  t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" shadow"
  t_run "$dispatch"
  printf '%s\n' "$t_out" >"$T/poll.lines"
  poll_rc=$t_rc
  t_run "\"$T_DISPATCH\" compare \"$T/poll.lines\" --exit $poll_rc"
}

t_case "a poll that holds: the shadow agrees, and says so in one line"
reset
hold 9
beside
release
t_eq "$(wc -l <"$shadow_log")" 1
t_eq "$(jq -c '[.same, .bash_only, .rust_only, .exit.bash, .exit.rust]' "$shadow_log")" '[true,[],[],0,0]'
t_eq "$(jq -r '.gh_requests | type' "$shadow_log")" number
t_eq "$([ -e "$T/home/tehas/state/dispatch-shadow.json" ] && echo kept)" ""

t_case "a poll that starts a cycle: the lines of the start are the poll's own"
reset
beside
t_eq "$(jq -r .same "$shadow_log")" false
t_contains "$(jq -r '.bash_only[]' "$shadow_log")" "start #7 (labelled by alice)"
t_eq "$(jq -c .rust_only "$shadow_log")" '[]'
t_eq "$(jq -c .picked "$shadow_log")" '[7]'

t_case "a world that changed between the shadow and the poll is a difference"
reset
hold 9
mkdir -p "$T/home/tehas/logs"
: >"$T/home/tehas/state/dispatch-shadow.on"
rm -f "$shadow_log"
t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" shadow"
release
t_run "$dispatch"
printf '%s\n' "$t_out" >"$T/poll.lines"
t_run "\"$T_DISPATCH\" compare \"$T/poll.lines\" --exit 0"
t_eq "$(jq -r .same "$shadow_log")" false
t_contains "$(jq -r '.rust_only[]' "$shadow_log")" "holding: 1 cycles are running (limit 1)"

t_case "a compare with no decision kept says so, and a shadow not turned on does nothing"
reset
mkdir -p "$T/home/tehas/logs"
: >"$T/home/tehas/state/dispatch-shadow.on"
rm -f "$shadow_log" "$T/home/tehas/state/dispatch-shadow.json"
: >"$T/poll.lines"
t_run "\"$T_DISPATCH\" compare \"$T/poll.lines\""
t_contains "$(jq -r '.rust_only[]' "$shadow_log")" "the shadow decided nothing before this poll"
rm -f "$shadow_log" "$T/home/tehas/state/dispatch-shadow.on"
t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" shadow"
t_run "\"$T_DISPATCH\" compare \"$T/poll.lines\""
t_eq "$([ -e "$shadow_log" ] || [ -e "$T/home/tehas/state/dispatch-shadow.json" ] && echo written)" ""

t_case "compare finds the home as env.sh gives it, with TEHAS_HOME unset"
reset
mkdir -p "$T/home/tehas/logs"
: >"$T/home/tehas/state/dispatch-shadow.on"
rm -f "$shadow_log"
: >"$T/poll.lines"
t_run "env -u TEHAS_HOME \"$T_DISPATCH\" compare \"$T/poll.lines\""
t_eq "$(wc -l <"$shadow_log" 2>/dev/null)" 1

t_case "a snapshot is JSON in the shape the decision reads"
reset
t_run ". \"$T_ROOT/lib/config.sh\" && tehas_load && \"$T_DISPATCH\" snapshot"
t_eq "$(jq -c '[.repo, (.ready | map(.number)), .issues["7"].events[0].actor.login]' <<<"$t_out")" '["acme/widget",[7],"alice"]'

t_case "an unknown argument is refused, and --help prints the usage"
t_run "\"$T_DISPATCH\" frobnicate"
t_eq "$t_rc" 64
t_run "\"$T_DISPATCH\" --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas-dispatch decide [--dry-run]"

t_done
