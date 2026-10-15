#!/usr/bin/env bash
# Tests for libexec/dispatch against a stub gh and a scratch checkout whose
# default branch is main.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

dispatch="$T_ROOT/libexec/dispatch"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget

# gh answers from fixture files; every call is recorded by t_stub.
t_stub gh '
case "$1 $2" in
  "repo view") cat "$T/fx/repo.json" || exit 1 ;;
  "issue view") cat "$T/fx/issue-$3.json" 2>/dev/null || cat "$T/fx/control.json" || exit 1 ;;
  "issue list")
    case "$*" in
      *awaiting-merge*) cat "$T/fx/awaiting.json" || exit 1 ;;
      *tehas:ready*) cat "$T/fx/ready.json" || exit 1 ;;
      *tehas:running*) cat "$T/fx/marked.json" || exit 1 ;;
      *tehas:question*) cat "$T/fx/asking.json" || exit 1 ;;
      *tehas:plan*) cat "$T/fx/plans.json" || exit 1 ;;
      *) echo "[]" ;;
    esac ;;
  "api user") cat "$T/fx/user.json" || exit 1 ;;
  "api -X") [ ! -e "$T/fx/patch-fails" ] || exit 1 ;;
  "api repos/acme/widget/issues/"*"/comments") [ ! -e "$T/fx/post-fails" ] || exit 1; echo 4002 ;;
  "api repos/acme/widget/issues/"*"/comments?per_page=100")
    n=${2#repos/acme/widget/issues/}
    cat "$T/fx/icomments-${n%%/*}.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/"*)
    n=${2#repos/acme/widget/issues/}
    case "$n" in
      */*) cat "$T/fx/events-${n%/events*}.json" || exit 1 ;;
      # What the number is: an open issue, unless a case writes the fixture.
      # It carries tehas:running when the list of marked issues names it.
      *)
        [ ! -e "$T/fx/work-fails" ] || exit 1
        if [ -e "$T/fx/work-$n.json" ]; then cat "$T/fx/work-$n.json"; else
          jq -c --argjson n "$n" "{number: \$n, state: \"open\", labels: (if any(.[]?; type == \"object\" and .number == \$n)
            then [{name: \"tehas:running\"}] else [] end)}" "$T/fx/marked.json" 2>/dev/null || echo "{\"number\":$n,\"state\":\"open\"}"
        fi ;;
    esac ;;
  "api graphql")
    n=$(printf "%s\n" "$@" | sed -n "s/^number=//p")
    case "$*" in
      *"comments(last"*) cat "$T/fx/comments-$n.json" || exit 1 ;;
      # The pull requests that close the issue: none, unless a case writes them.
      *closedByPullRequestsReferences*)
        [ ! -e "$T/fx/prs-fails" ] || exit 1
        cat "$T/fx/prs-$n.json" 2>/dev/null || cat "$T/fx/prs.json" || exit 1 ;;
      *) cat "$T/fx/edit-$n.json" 2>/dev/null || cat "$T/fx/edit.json" || exit 1 ;;
    esac ;;
  "issue edit")
    # What the host had on record at the moment an issue was marked as planning.
    case "$*" in *"--add-label tehas:planning"*) cat "$T/home/tehas/state/plan-$3.json" >"$T/calls/record.at-mark" 2>/dev/null ;; esac
    # And the approval record at the moment an issue was marked as running.
    case "$*" in *"--add-label tehas:running"*) cat "$T/home/tehas/state/cycles/$3.approved.json" >"$T/calls/approved.at-mark" 2>/dev/null ;; esac
    [ ! -e "$T/fx/edit-fails" ] || exit 1
    # An edit whose arguments contain the text in edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(cat "$T/fx/edit-fails-on")"*) exit 1 ;; esac
    fi ;;
  "pr list") [ ! -e "$T/fx/pr-fails" ] || exit 1; cat "$T/fx/prlist.json" 2>/dev/null || echo "[]" ;;
esac'
# Stands in for systemctl --user start tehas-cycle@<issue>.service. A real
# cycle says it holds its lock by writing the .taken file. This one fails or
# stays silent when a case asks it to.
default_start() {
  t_stub fake-start '[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ -e "$T/fx/start-silent" ] || : >"$TEHAS_HOME/state/cycles/$1.taken"'
}
default_start
t_stub fake-status
# The dispatcher waits for a planning turn in steps of ten seconds. No case
# here needs that time to
# pass: the wait for a turn returns at once. Every other sleep is real, the
# stand-ins for a cycle among them.
t_stub sleep '[ "$1" = 10 ] || PATH=/usr/bin:/bin exec sleep "$@"'
# Writes a recorded outcome where the dispatcher asked for it. A failing turn
# leaves what the file plan-fails holds, if anything.
t_stub fake-plan 'cat "$T/home/tehas/state/plan-$1.json" >"$T/calls/record.at-turn" 2>/dev/null
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/plan.fd8"
out=""
while [ "$#" -gt 0 ]; do [ "$1" = --out ] && out=$2; shift; done
if [ -e "$T/fx/plan-fails" ]; then
  [ ! -s "$T/fx/plan-fails" ] || cp "$T/fx/plan-fails" "$out"
  exit 2
fi
cp "$T/fx/plan-result.json" "$out"'
# Notes what the checkout was at and whether a cycle had already started.
t_stub fake-index 'git -C "$T/home/tehas/repos/widget" rev-parse HEAD >"$T/calls/index.head"
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/index.fd8"
[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/index.late"
[ ! -e "$T/fx/index-fails" ] || exit 1'
# The slice as systemd and Docker describe it, from fixture files. Its use is
# the scratch cgroup tree of test/lib.sh.
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice")
    [ ! -e "$T/fx/slice-hangs" ] || exec sleep 20
    cat "$T/fx/slice.show" ;;
esac'
t_stub docker 'case "$1" in
  info) cat "$T/fx/docker.info" ;;
  image) [ -e "$T/fx/image.label" ] || exit 1; cat "$T/fx/image.label" ;;
  rm) [ ! -e "$T/fx/orphan-$3" ] || echo "$3" ;;
esac'
gib=1073741824
slice_env=TEHAS_CGROUP_ROOT=$T_CGROUP
slice_show() { # <MemoryMax> [<ActiveState>]: a slice loaded from its file
  printf '%s\n' "FragmentPath=/etc/systemd/system/tehas.slice" "ActiveState=${2:-active}" "MemoryMax=$1" >"$fx/slice.show"
}
# A host where the root step was never run: the slice is only a name.
no_slice() { printf '%s\n' "FragmentPath=" "ActiveState=inactive" "MemoryMax=infinity" >"$fx/slice.show"; }
# A container in the slice that is no cycle of this poll, a dev session say.
other() { slice_child "$(slice_id d)" "$1" 0 $((2 * gib)); } # <bytes in use>
# The container of a running cycle: its cid file and its place in the slice.
cid() { # <issue> <hex digit> <bytes in use>
  slice_id "$2" >"$T/home/tehas/state/cycles/$1.cid"
  slice_child "$(slice_id "$2")" "$3" 0 $((2 * gib))
}
dispatch_json=$T/home/tehas/live/dispatch.json

reset() {
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-* "$fx"/events-* "$fx"/comments-* "$fx"/icomments-* "$fx"/prs.json "$fx"/prlist.json "$fx"/pr-fails "$fx"/issue-* "$fx"/work-* "$fx"/post-fails "$fx"/patch-fails "$fx"/plan-fails "$fx"/slice-* "$fx"/orphan-* "$fx"/prs-*
  rm -rf "$T/home/tehas/state/cycles" "$T"/home/tehas/state/plan-* "$T"/home/tehas/state/park-* "$T/home/tehas/live"
  # A slice of 20 GiB with nothing in it: room for ten cycles of the default
  # reserve, so the slots and the review cap decide unless a case says more.
  slice_show $((20 * gib))
  echo "systemd 2" >"$fx/docker.info"
  slice_state 0.00
  echo '[]' >"$fx/asking.json"
  echo '[]' >"$fx/plans.json"
  echo '{"login":"tehas-bot"}' >"$fx/user.json"
  echo '{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}' >"$fx/edit.json"
  echo '{"data":{"repository":{"issue":{"closedByPullRequestsReferences":{"nodes":[]}}}}}' >"$fx/prs.json"
  echo '{"nameWithOwner":"acme/widget"}' >"$fx/repo.json"
  echo '{"labels":[]}' >"$fx/control.json"
  echo '{"session":"abc-123","text":"Two ways.","question":{"question":"Adopt?","options":[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]}}' >"$fx/plan-result.json"
  echo '[]' >"$fx/awaiting.json"
  echo '[]' >"$fx/marked.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_PLAN_CMD=fake-plan "$slice_env")
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
}
# A cycle that is running: its lock, held by this shell until release.
held=()
hold() { # issue
  local fd
  mkdir -p "$T/home/tehas/state/cycles"
  exec {fd}>"$T/home/tehas/state/cycles/$1.lock"
  flock -n "$fd"
  held+=("$fd")
}
release() {
  local fd
  for fd in "${held[@]}"; do exec {fd}>&-; done
  held=()
}

# An instance that works on at most <n> issues side by side. The host's CPU
# count is read with nproc; a stub makes every test machine the same.
t_stub nproc 'echo 8'
slots() { t_instance "${T_MINIMAL[@]}" "TEHAS_TICKET_CONCURRENCY=$1" "${@:2}"; }
# The label events of an issue: tehas:ready by alice, then tehas:urgent by
# <login>. An empty login stands for a labelling that names nobody.
urgent_by() { # <issue> <login or "">
  jq -nc --arg w "$2" '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: "alice"}},
    {event: "labeled", label: {name: "tehas:urgent"},
     actor: (if $w == "" then null else {login: $w} end)}]' >"$fx/events-$1.json"
}

ready() { # issue... : the queue, each labelled by alice; "8!" is urgent, by alice too
  local n num labels list=""
  for n in "$@"; do
    num=${n%!}
    labels='{"name":"tehas:ready"}'
    if [ "$n" = "$num" ]; then
      cp "$fx/events-7.json" "$fx/events-$num.json" 2>/dev/null || true
    else
      labels="$labels,{\"name\":\"tehas:urgent\"}"
      urgent_by "$num" alice
    fi
    list="$list,{\"number\":$num,\"labels\":[$labels]}"
  done
  echo "[${list#,}]" >"$fx/ready.json"
}

# The comments of an issue: the bot's question, who edited it last, and
# whatever was written after it.
comments() { # <issue> <question body> <editor or ""> [later comments as JSON]
  jq -n --arg body "$2" --arg ed "$3" --argjson after "${4:-[]}" '{data: {repository: {issue: {comments: {nodes: ([
    {databaseId: 4001, author: {login: "tehas-bot"}, body: $body, createdAt: "2026-01-01T00:00:00Z",
     lastEditedAt: (if $ed == "" then null else "2026-01-01T01:00:00Z" end),
     editor: (if $ed == "" then null else {login: $ed} end)}] + $after)}}}}}' >"$fx/comments-$1.json"
}
park_body() { # <retry box> <leave box>
  printf 'Tehas cycle stopped.\n\n- [%s] Retry from a clean start\n- [%s] Leave it parked\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n' "$1" "$2"
}
park_id=$T/home/tehas/state/park-7.id
# Issue 7 is parked with a question the host has on record, and nothing else
# is queued.
parked() {
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
  mkdir -p "$T/home/tehas/state"
  echo 4001 >"$park_id"
}

plan_body() { # <box 1> <box 2> <end box>
  printf 'Two ways.\n\nAdopt?\n\n- [%s] Adopt all\n  Apply all five\n- [%s] Change some\n- [%s] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->\n' "$1" "$2" "$3"
}
record=$T/home/tehas/state/plan-7.json
# Issue 7 is labelled for planning by alice and nothing else is queued.
to_plan() {
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
  echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
  echo '{"title":"Faster search","body":"Make it fast."}' >"$fx/issue-7.json"
}
# Issue 7 has a planning question open after one turn.
planning() {
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7}]' >"$fx/asking.json"
  mkdir -p "$T/home/tehas/state"
  echo '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"comment":4001}' >"$record"
}
plan_calls() { t_calls fake-plan; }

# A product repository whose default branch is main, and the instance's clone.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

t_case "a dry run picks the issue an allowed account labelled"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an issue labelled by another account is skipped"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"
t_contains "$t_out" "0 cycles running, room for 1 more"
t_contains "$t_out" "nothing to do"

# A tehas:ready from an account outside the list is no one's request for a
# cycle. Left on the issue it reads as queued, so the poll takes it off.
for other in tehas-bot mallory; do
  t_case "a tehas:ready applied by $other is taken off the issue and said once"
  reset
  echo "[{\"event\":\"labeled\",\"label\":{\"name\":\"tehas:ready\"},\"actor\":{\"login\":\"$other\"}}]" >"$fx/events-7.json"
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_contains "$t_out" "skip #7: tehas:ready was applied by '$other', not an allowed account"
  t_contains "$t_out" "clear #7: tehas:ready off, applied by '$other' who may not authorise work here"
  t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:ready')" 1
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took"
  t_eq "$(t_calls gh | grep -c -e '^issue comment ')" "1"
done

t_case "a dry run says the label would come off and changes nothing"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would take tehas:ready off #7: it was applied by 'tehas-bot'"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "while the label cannot be taken off nothing is said on the issue, and it is said once in the end"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 0
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready"
rm -f "$fx/edit-fails"
rm -f "$T"/calls/gh
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1

t_case "a stale approval from an allowed account keeps its label on the issue"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T00:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "the most recent labelling decides, not an earlier one"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"unlabeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a login that only contains an allowed one is not allowed"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice2"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a label event with no actor is not an allowed account"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'unknown'"
t_contains "$t_out" "nothing to do"

t_case "an issue with no label event at all is skipped"
reset
echo '[]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"
t_contains "$t_out" "nothing to do"

for odd in "alice  bob" "alice " " alice" " "; do
  t_case "a labeller list with stray spaces ('$odd') is refused, not guessed at"
  reset
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='$odd'" >"$TEHAS_INSTANCE_DIR/instance.env"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 78
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls gh)" ""
done

t_case "when the pause check cannot reach GitHub nothing is dispatched"
reset
rm "$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue view failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the awaiting-merge count cannot be read nothing is dispatched"
reset
rm "$fx/awaiting.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_eq "$(t_calls fake-start)" ""

t_case "when the label events cannot be read the issue is not taken"
reset
rm "$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

# Open issues only. The list a poll read can name an issue that was closed
# since, and a number can name a pull request: GitHub is asked what it is
# before a label is changed or a cycle is started.
t_case "an issue closed after the queue was read starts nothing"
reset
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_lacks "$t_out" "would run a cycle"

t_case "a pull request is refused, open, merged or closed"
for answer in '{"number":7,"state":"open","pull_request":{"url":"u"}}' '{"number":7,"state":"closed","pull_request":{"url":"u"}}'; do
  reset
  printf '%s' "$answer" >"$fx/work-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 0
  t_contains "$t_out" "skip #7: it is a pull request, not an issue the factory works on"
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls fake-start)" ""
done

t_case "an answer that cannot be read stops the poll before any label change"
for answer in '' 'not json' 'null' '[]'; do
  reset
  printf '%s' "$answer" >"$fx/work-7.json"
  t_run "$dispatch"
  t_eq "$t_rc" 1
  t_contains "$t_out" "ERROR: GitHub's answer for #7 could not be read"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done
reset
: >"$fx/work-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api repos/acme/widget/issues/7 failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a closed issue labelled for planning starts no session"
to_plan
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_lacks "$t_out" "would start a planning session"
t_eq "$(t_calls fake-plan)" ""
t_lacks "$(t_calls gh)" "issue edit"

# The ask is paid where it can change the outcome. An issue the labels already
# hold back is not asked for, and a number is asked once a poll.
t_case "an issue no allowed account labelled is not asked for"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c '^api repos/acme/widget/issues/7$')" 0
t_eq "$(t_calls fake-start)" ""

approved_file=$T/home/tehas/state/cycles/7.approved.json
A=https://github.com/user-attachments/assets
t_case "a start writes the approval record before the label changes: who, when, how, the body's hash and its links"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
jq -n --arg body "See ![a]($A/1111)"$'\r\n'"and ![b]($A/2222) and again $A/1111, not https://example.com/c.png" \
  '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: $body}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
sum=$(printf 'See ![a](%s/1111)\nand ![b](%s/2222) and again %s/1111, not https://example.com/c.png' "$A" "$A" "$A" | sha256sum | cut -d' ' -f1)
t_eq "$(jq -c . "$T/calls/approved.at-mark" 2>&1)" \
  "{\"at\":\"2026-01-01T00:00:00Z\",\"by\":\"alice\",\"how\":\"label\",\"body_sha256\":\"$sum\",\"links\":[\"$A/1111\",\"$A/2222\"]}"
t_eq "$(cmp "$approved_file" "$T/calls/approved.at-mark" && echo same)" same

t_case "without the comments setting the record names no comment and none are read"
t_eq "$(jq -c 'has("comment_links")' "$approved_file")" false
t_lacks "$(t_calls gh)" "comments?per_page"

# icomment <id> <login> <created> <updated> <body>: one comment as REST lists it.
icomment() {
  jq -n -c --argjson id "$1" --arg who "$2" --arg c "$3" --arg u "$4" --arg body "$5" \
    '{id: $id, user: {login: $who}, created_at: $c, updated_at: $u, body: $body}'
}
t_case "with comments on, the record takes the links of comments an allowed account wrote before the label"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
jq -n --arg body "![a]($A/1111)" '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: $body}}}}' >"$fx/edit-7.json"
{
  echo '['
  icomment 52 alice 2025-12-31T10:00:00Z 2025-12-31T10:00:00Z "later ![c]($A/5555) and ![a]($A/1111)"
  echo ','
  icomment 51 alice 2025-12-31T09:00:00Z 2025-12-31T09:00:00Z "first ![b]($A/4444)"
  echo ','
  icomment 53 mallory 2025-12-31T11:00:00Z 2025-12-31T11:00:00Z "an outsider's ![m]($A/6666)"
  echo ','
  icomment 54 alice 2026-01-01T00:00:01Z 2026-01-01T00:00:01Z "after the label ![l]($A/7777)"
  echo ','
  icomment 55 alice 2025-12-31T12:00:00Z 2026-01-01T00:30:00Z "edited after the label ![e]($A/8888)"
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '[.links, .comment_links]' "$T/calls/approved.at-mark" 2>&1)" \
  "[[\"$A/1111\"],[{\"link\":\"$A/4444\",\"source\":\"comment:51\",\"by\":\"alice\"},{\"link\":\"$A/5555\",\"source\":\"comment:52\",\"by\":\"alice\"},{\"link\":\"$A/1111\",\"source\":\"comment:52\",\"by\":\"alice\"}]]"

t_case "an outsider's comment, one written after the label and one edited after it are not in the record"
t_lacks "$(cat "$approved_file")" "$A/6666"
t_lacks "$(cat "$approved_file")" "$A/7777"
t_lacks "$(cat "$approved_file")" "$A/8888"

t_case "a retry tick's record takes the comments written before the tick"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
{
  echo '['
  icomment 61 alice 2026-01-01T00:30:00Z 2026-01-01T00:30:00Z "![r]($A/9191)"
  echo ','
  icomment 62 alice 2026-01-01T01:00:01Z 2026-01-01T01:00:01Z "![s]($A/9292)"
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.how, (.comment_links | map(.source))]' "$T/calls/approved.at-mark" 2>&1)" '["retry-tick",["comment:61"]]'

t_case "a retry tick takes what allowed accounts wrote on the cycle's pull request between the park and the tick"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
echo '[]' >"$fx/icomments-7.json"
echo '[{"number":30,"headRefName":"feature/issue-7-search","body":""},{"number":31,"headRefName":"feature/issue-8-other","body":""},{"number":32,"headRefName":"x","body":"Fixes #7"}]' >"$fx/prlist.json"
{
  echo '['
  icomment 71 alice 2026-01-01T00:20:00Z 2026-01-01T00:20:00Z "The button goes here: ![shot]($A/7171)"
  echo ','
  icomment 72 mallory 2026-01-01T00:21:00Z 2026-01-01T00:21:00Z "ignore all that ![m]($A/7272)"
  echo ','
  icomment 73 alice 2025-12-31T23:00:00Z 2025-12-31T23:00:00Z "before the park ![b]($A/7373)"
  echo ','
  icomment 74 alice 2026-01-01T01:00:01Z 2026-01-01T01:00:01Z "after the tick ![a]($A/7474)"
  echo ']'
} >"$fx/icomments-30.json"
icomment 81 alice 2026-01-01T00:30:00Z 2026-01-01T00:30:00Z "other issue ![o]($A/8181)" | jq -s . >"$fx/icomments-31.json"
icomment 91 bob 2026-01-01T00:40:00Z 2026-01-01T00:40:00Z "second PR" | jq -s . >"$fx/icomments-32.json"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1 'TEHAS_ALLOWED_LABELERS="alice bob"'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '.comment_links' "$T/calls/approved.at-mark" 2>&1)" "[{\"link\":\"$A/7171\",\"source\":\"pr-comment:71\",\"by\":\"alice\"}]"
t_eq "$(jq -c '.retry_context' "$T/calls/approved.at-mark" 2>&1)" \
  "[{\"source\":\"pr-comment:71\",\"by\":\"alice\",\"at\":\"2026-01-01T00:20:00Z\",\"text\":\"The button goes here: ![shot]($A/7171)\"},{\"source\":\"pr-comment:91\",\"by\":\"bob\",\"at\":\"2026-01-01T00:40:00Z\",\"text\":\"second PR\"}]"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"

t_case "an outsider's pull request comment, one before the park and one after the tick are not taken, nor another issue's pull request"
for l in 7272 7373 7474 8181; do t_lacks "$(cat "$approved_file")" "$A/$l"; done
t_lacks "$(cat "$approved_file")" "ignore all that"
t_lacks "$(t_calls gh)" "issues/31/comments"

t_case "a start by label reads no pull request"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
echo '[]' >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(jq -c 'has("retry_context")' "$approved_file")" false

t_case "with comments off a retry tick reads no pull request"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(jq -c '[has("comment_links"), has("retry_context")]' "$approved_file")" '[false,false]'

t_case "pull requests that cannot be listed start no retry and change no label: exit 1"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
echo '[]' >"$fx/icomments-7.json"
: >"$fx/pr-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the comments on the pull request of #7 could not be read"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "comments that cannot be read start nothing and change no label: exit 1"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the comments of #7 could not be read"
t_contains "$t_out" "ERROR: could not write the approval record of #7, not starting a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
echo '{"not": "a list"}' >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_lacks "$(t_calls gh)" "issue edit"

t_case "an issue with no body is approved with the hash of nothing and no links"
reset
t_run "$dispatch"
t_eq "$(jq -c '[.body_sha256, .links]' "$approved_file")" "[\"$(printf '' | sha256sum | cut -d' ' -f1)\",[]]"

t_case "a dry run writes no record"
reset
t_run "$dispatch --dry-run"
t_eq "$(find "$approved_file" 2>/dev/null | wc -l)" 0

t_case "a record that cannot be written starts nothing and changes no label: exit 1"
reset
mkdir -p "$approved_file.new"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the approval record of #7, not starting a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
rmdir "$approved_file.new"

# The seam between the two commands. A cycle the poll started can find its
# issue closed when it asks, and refuse it. It claimed its place before it
# asked, so the poll does not report a start that did happen as one that did
# not, and the issue keeps only tehas:running, which the next poll clears in
# silence.
t_case "a cycle that refused its number leaves nothing a person has to clean up"
reset
t_run "$dispatch"
t_contains "$t_out" "start #7 (labelled by alice)"
t_lacks "$t_out" "did not start"
t_lacks "$(t_calls gh)" "could not start the cycle"
# What GitHub answers for #7 once it is closed.
echo '[{"number":7,"state":"CLOSED"}]' >"$fx/marked.json"
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "clear #7: it is closed, marked tehas:running, and no cycle holds its lock"
t_lacks "$t_out" "did not start"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$(t_calls gh)" "To retry, apply"

t_case "a checkout of another repository stops everything"
reset
git -C "$checkout" remote set-url origin "$T/git/acme/other"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "is a checkout of"
t_contains "$t_out" "not of acme/widget"
t_eq "$(t_calls gh)" ""
git -C "$checkout" remote set-url origin "$upstream"

for url in "https://github.com/acme/widget.git" "https://github.com/acme/widget" "git@github.com:acme/widget.git" "ssh://git@github.com/acme/widget/"; do
  t_case "origin $url is recognised as acme/widget"
  reset
  git -C "$checkout" remote set-url origin "$url"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 0
  t_contains "$t_out" "would run a cycle for #7"
done
t_case "origin ending in a longer name is not acme/widget"
reset
git -C "$checkout" remote set-url origin "https://github.com/notacme/widget.git"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
git -C "$checkout" remote set-url origin "$upstream"

t_case "a missing checkout stops before any label changes, even with TEHAS_BRANCH set"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=main TEHAS_WORK_REPO="$T/nowhere"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "no product checkout at $T/nowhere"
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$T/nowhere" ] && echo created)" ""

t_case "a renamed repository stops the poll and says its new name"
reset
echo '{"nameWithOwner":"acme/gadget"}' >"$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: acme/widget is now acme/gadget on GitHub: set TEHAS_REPO"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue list"

t_case "a repository that cannot be looked up stops the poll"
reset
rm "$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh repo view failed"
t_lacks "$t_out" "nothing to do"

t_case "the repository's name is compared without regard to case"
reset
echo '{"nameWithOwner":"Acme/Widget"}' >"$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"

t_case "the pause label on the control issue stops dispatch"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "paused: issue #1 carries tehas:pause"

t_case "an invalid instance makes no call at all and exits 78"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an empty labeller list does not widen who may label"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS=''" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""

t_case "a real poll puts the checkout on main and fast-forwards it"
reset
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" checkout -q -b feature/left-over
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(git -C "$checkout" branch --show-current)" "main"
t_eq "$(git -C "$checkout" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_lacks "$t_out" "WARN"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "a fetch that hangs is given up after its bound, said, and the cycle still starts"
reset
printf '#!/bin/sh\nexec sleep 30 2>/dev/null\n' >"$T/slow-upload-pack"
chmod +x "$T/slow-upload-pack"
git -C "$checkout" config remote.origin.uploadpack "$T/slow-upload-pack"
T_ENV+=(TEHAS_FETCH_WAIT=1)
began=$(date +%s)
t_run "$dispatch"
git -C "$checkout" config --unset remote.origin.uploadpack
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: could not fast-forward $checkout"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(($(date +%s) - began < 20))" 1

t_case "the code index is refreshed after the fetch and before the cycle"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(cat "$T/calls/index.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "an index that cannot be refreshed is a warning, and the cycle still runs"
reset
: >"$fx/index-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not refreshed"
t_eq "$(t_calls fake-start)" "7"
rm "$fx/index-fails"

t_case "a dry run refreshes no index"
reset
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-index)" ""

t_case "a real poll does not start an issue the bot labelled, and takes that label off"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"the-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:ready')" 1
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1

t_case "every page of the label events is read, a hundred at a time"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "a cycle does not start when the issue cannot be marked as running"
reset
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not mark #7 as running"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/edit-fails"

t_case "a body edited by its author after the label was applied is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'mallory' after alice applied tehas:ready"

t_case "an edit before the label was applied is what the label approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T09:59:59Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7"

t_case "an edit after the label by an allowed account keeps the approval"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"alice"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7"

t_case "an edit after the label by nobody known is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":null}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'unknown'"

t_case "a title changed after the label by another account is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":{"login":"mallory"},"created_at":"2026-01-02T10:01:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'mallory'"

t_case "a title changed after the label by an account with no login is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":null,"created_at":"2026-01-02T10:01:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'unknown'"
t_lacks "$t_out" "would run a cycle"

# The pull requests GitHub says close an issue: <url> <state>...
closing_prs() { # <issue> [<url> <state>]...
  local n=$1 nodes="[]"
  shift
  while [ "$#" -gt 1 ]; do
    nodes=$(jq -c --arg u "$1" --arg s "$2" '. + [{url: $u, state: $s}]' <<<"$nodes")
    shift 2
  done
  jq -nc --argjson nodes "$nodes" '{data: {repository: {issue: {closedByPullRequestsReferences: {nodes: $nodes}}}}}' >"$fx/prs-$n.json"
}

t_case "an issue with an open pull request that closes it starts nothing, and loses tehas:ready"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: pull request https://github.com/acme/widget/pull/90 is open and closes it"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing: https://github.com/acme/widget/pull/90 is open and closes it."

t_case "a closed or merged pull request that closed it holds nothing back"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 CLOSED https://github.com/acme/widget/pull/91 MERGED
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "closedByPullRequestsReferences"

t_case "an open pull request does not hold up the issue behind it"
reset
ready 7 8
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"

t_case "when the label cannot be taken off, nothing is said, and the next poll tries again"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
echo "remove-label tehas:ready" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "a dry run says it would take the label off, and changes nothing"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: pull request https://github.com/acme/widget/pull/90 is open and closes it"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "when GitHub cannot say which pull requests close it, the poll ends"
reset
: >"$fx/prs-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "an answer that cannot be read ends the poll too"
reset
echo '{"data":{"repository":{"issue":null}}}' >"$fx/prs-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: unreadable pull requests of #7"
t_eq "$(t_calls fake-start)" ""

t_case "an issue no allowed account labelled costs no question about its pull requests"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "closedByPullRequestsReferences"

t_case "an edited issue does not hold up the one behind it"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
cp "$fx/events-7.json" "$fx/events-8.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #8"
rm -f "$fx/events-8.json"

t_case "when the edit record cannot be read the issue is not taken"
reset
rm -f "$fx/edit.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a pause answer that cannot be understood dispatches nothing"
reset
echo '{"labels":null}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""

t_case "TEHAS_BRANCH overrides the checkout's default branch"
reset
git -C "$checkout" branch -q release main
git -C "$upstream" branch -q release main
git -C "$checkout" fetch -q origin
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=release
t_run "$dispatch"
t_eq "$(git -C "$checkout" branch --show-current)" "release"

t_case "a checkout without origin/HEAD stops before any label changes"
reset
git -C "$checkout" remote set-head origin -d
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "default branch"
t_lacks "$(t_calls gh)" "issue edit"
git -C "$checkout" remote set-head origin main

t_case "--init-labels is libexec/labels now: here it is an unknown argument"
reset
t_run "$dispatch --init-labels"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --init-labels"
t_eq "$(t_calls gh)" ""

t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_case "the poll's lock is not passed on to the cycle it starts"
t_eq "$(find "$T/calls/start.fd8" 2>/dev/null | wc -l)" 0

t_case "the status is refreshed before the poll and after the start"
t_eq "$(t_calls fake-status | wc -l)" 2

t_case "for one issue: the running label, then the start comment, then the start"
reset
# fake-order notes what the gh calls held when the start was asked for.
t_stub fake-order 'grep -q -e "--add-label tehas:running" "$T/calls/gh" && : >"$T/calls/order.label"
grep -q "^issue comment $1 " "$T/calls/gh" && : >"$T/calls/order.comment"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-order TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls/order.label" "$T/calls/order.comment" 2>/dev/null | wc -l)" 2
label_at=$(grep -n -e "--add-label tehas:running" "$T/calls/gh" | head -n 1 | cut -d: -f1)
comment_at=$(grep -n "^issue comment 7 " "$T/calls/gh" | head -n 1 | cut -d: -f1)
t_eq "$([ "$label_at" -lt "$comment_at" ] && echo before)" "before"

t_case "the poll does not wait for the cycle: it returns while the cycle lives, with its own lock free"
reset
# The cycle outlives the poll and holds its own lock. It must not hold the
# poll's, so the next poll can run. Its output is detached from the test's pipe.
t_stub fake-live '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 8 ) >/dev/null 2>&1 &
echo $! >"$T/live.pid"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-live TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
alive=no
kill -0 "$(cat "$T/live.pid")" 2>/dev/null && alive=yes
free=$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)
kill "$(cat "$T/live.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_eq "$alive" yes
t_eq "$free" free

t_case "the poll's lock is not handed to the index"
reset
t_run "$dispatch"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.fd8" 2>/dev/null | wc -l)" 0

t_case "a real poll records when it took the lock"
t_eq "$(($(date +%s) - $(cat "$T/home/tehas/state/dispatch.taken") < 60))" 1

t_case "a poll that finds the lock held for longer than a turn may take says so once, and exits 0"
reset
taken_file=$T/home/tehas/state/dispatch.taken
rm -f "$T/home/tehas/state/dispatch.stuck"
echo $(($(date +%s) - 6000)) >"$taken_file"
exec {lockfd}>>"$T/home/tehas/state/dispatch.lock"
flock -n "$lockfd"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the dispatcher's lock has been held since"
t_contains "$t_out" "more than 5400s: a poll or tehas index hangs, and no poll runs until it ends"
t_eq "$(t_calls gh)" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "a poll that finds the lock held for a short while says nothing"
stamp=$(($(date +%s) - 60))
echo "$stamp" >"$taken_file"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "another poll, or tehas index, holds the dispatcher's lock"
t_lacks "$t_out" "WARN"

t_case "a poll that finds the lock held does not change when it was taken"
t_eq "$(cat "$taken_file")" "$stamp"
exec {lockfd}>&-

t_case "while a cycle is running nothing is started"
reset
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
release

t_case "an issue marked as running with no cycle behind it is handed to a human"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue comment 9 --repo acme/widget --body Tehas found this issue marked \`tehas:running\` with no cycle behind it"
t_contains "$(t_calls gh)" "logs/dispatch-work-9.log"
t_contains "$(t_calls gh)" "To retry, apply \`tehas:ready\`. If it is on the issue already, nothing more is needed."
t_lacks "$(t_calls gh)" "remove \`needs-human-attention\`"

t_case "and the poll goes on to the queue, and starts nothing for that issue"
t_eq "$(t_calls fake-start)" "7"

t_case "an issue whose cycle ended after the list was read is not parked"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"number":9,"state":"open","labels":[{"name":"tehas:awaiting-merge"}]}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #9: tehas:running is gone from it since the list was read"
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "the issue is read again before it is parked, and a failed read ends the poll"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/work-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/work-fails"

t_case "a closed issue marked as running with no cycle loses the mark and is not parked"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 9 --repo acme/widget --remove-label tehas:running')" 1
t_lacks "$(t_calls gh)" "issue comment 9"
t_contains "$t_out" "clear #9: it is closed, marked tehas:running, and no cycle holds its lock"

t_case "a closed issue whose cycle still holds its lock is left alone"
reset
slots 2
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 9"
release

t_case "closed issues are asked for too"
reset
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --label tehas:running"

t_case "a lock file nobody holds does not make it a running cycle"
reset
echo '[{"number":9}]' >"$fx/marked.json"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "an issue whose cycle holds its lock is left alone"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"
release

t_case "it is found while the slots are full too"
reset
echo '[{"number":8},{"number":9}]' >"$fx/marked.json"
hold 8
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 8"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""
release

t_case "when its label cannot be changed nothing is said on it, and the poll goes on"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo "issue edit 9" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on #9 failed"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "a dry run says so and changes nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would park #9: it is marked tehas:running and no cycle holds its lock"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls docker)" "rm "

t_case "a container left by a cycle whose lock nobody holds is removed, and that is logged"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_contains "$t_out" "removed the container tehas-work-9: no cycle holds the lock of #9"

t_case "with no container left the removal says nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_lacks "$t_out" "removed the container"

t_case "a closed issue's left container goes too"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_contains "$t_out" "removed the container tehas-work-9"

t_case "the container of a cycle that holds its lock is not touched"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls docker)" "rm "
release

t_case "a paused factory leaves it"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the issues marked as running cannot be read nothing is dispatched"
reset
rm -f "$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an answer that is no list of issues dispatches nothing"
reset
echo '{"oops":1}' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: unreadable list of issues marked as running"
t_eq "$(t_calls fake-start)" ""

t_case "the lock file of a cycle that ended holds nothing back"
reset
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a cycle that cannot be started is handed to a human, exit 1"
reset
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start the cycle"

t_case "a cycle that never says it holds its lock is handed to a human"
reset
: >"$fx/start-silent"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that holds its lock but wrote no marker in time is taken as started"
reset
# The holder keeps the cycle's lock for a few seconds with its output detached,
# so the stub returns at once and the test's pipe is not held open.
t_stub fake-late '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 5 ) >/dev/null 2>&1 &
echo $! >"$T/late.pid"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-late TEHAS_CYCLE_START_WAIT=1 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
kill "$(cat "$T/late.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "a start marker left by an earlier cycle does not count"
reset
: >"$fx/start-silent"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/7.taken"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"

t_case "a cycle that ended at once still counts as started"
reset
# fake-start writes the marker and is gone: no lock is held when the poll looks.
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$t_out" "did not start"

t_case "two slots: two issues are started in one poll, in order"
reset
slots 2
ready 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the index is refreshed once, before the first start"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "a ceiling of one: the second issue waits"
reset
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

# nproc is stubbed to 8 above, and reset()'s slice has room for ten.
t_case "the ceiling defaults to the stubbed host's CPUs minus one: seven"
reset
t_instance "${T_MINIMAL[@]}"
ready 7 8 9 10 11 12 13 14
for n in 20 21 22 23 24 25 26; do hold "$n"; done
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 7 cycles are running (limit 7)"
release

t_case "below the ceiling, no more cycles start than the containers' ceilings fit in the slice"
reset
t_instance "${T_MINIMAL[@]}"
ready 7 8 9 10 11 12 13 14
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 10 "
reset
t_instance "${T_MINIMAL[@]}"
for d in a b c d; do slice_child "$(slice_id "$d")" "$gib" 0 0; done
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: tehas.slice holds as many containers as their ceilings fit (4 of 5120m)"
t_eq "$(jq -c '[.why, .room]' "$dispatch_json")" '["full",0]'

t_case "an urgent issue is started first"
reset
slots 2
ready 7 8! 9
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 7 "

# The order is what an urgent label buys, so who applied it is checked as the
# labelling of tehas:ready is.
t_case "urgent from an allowed account moves an issue ahead of a lower number"
reset
slots 2
ready 3 8!
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 3 "

t_case "urgent applied by the bot is ignored, and the poll says so"
reset
slots 2
ready 3 8!
urgent_by 8 tehas-bot
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'tehas-bot', not an allowed account"

t_case "urgent applied by an account outside the list is ignored"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'mallory', not an allowed account"

t_case "an urgent label with no account in its event is ignored"
reset
slots 2
ready 3 8!
urgent_by 8 ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'unknown', not an allowed account"

t_case "an urgent label with no labelling event is ignored"
reset
slots 2
ready 3 8!
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'unknown', not an allowed account"

t_case "the most recent urgent labelling decides"
reset
slots 2
ready 3 8!
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:urgent"},"actor":{"login":"mallory"}},
       {"event":"unlabeled","label":{"name":"tehas:urgent"},"actor":{"login":"mallory"}},
       {"event":"labeled","label":{"name":"tehas:urgent"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 3 "

t_case "an ignored urgent label leaves the issue ready on its own label"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "a dry run reports the order a real poll would take"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'mallory', not an allowed account"
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/.*would run a cycle for #\([0-9]*\).*/\1/p' | tr '\n' ' ')" "3 8 "

t_case "the urgent labelling of an issue is read once, the ready labelling again at the decision"
reset
slots 2
ready 3 8!
t_run "$dispatch"
t_eq "$t_rc" 0
# Once for the order, once when #8 is picked: no start rests on the read that
# settled the order.
t_eq "$(grep -c -e 'api repos/acme/widget/issues/8/events' <<<"$(t_calls gh)")" "2"

t_case "an urgent issue this poll cannot start is not asked about"
reset
slots 2
ready 8!
hold 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(grep -c -e 'api repos/acme/widget/issues/8/events' <<<"$(t_calls gh)")" "0"
t_contains "$t_out" "wait #8: its cycle is still running"
release

t_case "urgent order survives an issue number that arrives as text"
reset
slots 2
ready 3 8
echo '[{"number":3,"labels":[{"name":"tehas:ready"}]},{"number":"8","labels":[{"name":"tehas:ready"},{"name":"tehas:urgent"}]}]' >"$fx/ready.json"
urgent_by 8 alice
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/.*would run a cycle for #\([0-9]*\).*/\1/p' | tr '\n' ' ')" "8 3 "

t_case "when the urgent label events cannot be read nothing is started and no label changes"
reset
slots 2
ready 3 8!
rm "$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api repos/acme/widget/issues/8/events?per_page=100 failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "label events that are not JSON end the poll, whatever label is being read"
reset
slots 2
ready 3 8!
echo 'not json' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: unreadable label events of #8"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "label events that answer with an object end the poll too"
reset
slots 2
ready 3 8!
echo '{"message":"Bad credentials"}' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: unreadable label events of #8"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a running cycle takes a slot"
reset
slots 2
ready 7 8
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "with every slot taken nothing is started, and no ready issue is looked at"
reset
slots 2
ready 7 8
hold 9
hold 10
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "tehas:ready"
t_lacks "$(t_calls gh)" "awaiting-merge"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 2 cycles are running (limit 2)"
release

t_case "the index is not refreshed under a running cycle"
reset
slots 2
ready 7
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls fake-index)" ""
t_contains "$t_out" "the code index stays as it is: a cycle is running"
release

t_case "running cycles count towards the cap on unreviewed work"
reset
slots 3 TEHAS_MAX_AWAITING=3
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "at the cap nothing is started, whatever slots are free"
reset
slots 3 TEHAS_MAX_AWAITING=2
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 issues await merge and 1 cycles are running (limit 2)"
release

t_case "an issue that is not approved does not use a slot"
reset
slots 2
ready 7 8 9
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 9 "
t_lacks "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"

t_case "a cycle that does not start does not stop the next one, and the poll ends with 1"
reset
slots 2
ready 7 8
# fake-start fails for issue 7 only.
t_stub fake-start '[ "$1" != 7 ] || exit 1
: >"$TEHAS_HOME/state/cycles/$1.taken"'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_lacks "$(t_calls gh)" "issue edit 8 --repo acme/widget --remove-label"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
default_start

t_case "a dry run names every issue it would start and how much room there is"
reset
slots 3
ready 7 8
hold 9
t_run "$dispatch --dry-run"
t_contains "$t_out" "1 cycles running, room for 2 more"
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_contains "$t_out" "would run a cycle for #8 (labelled by alice)"
t_eq "$(t_calls fake-start)" ""
release

# Arguments. A poll starts only from an argument this command knows, and the
# check runs before the instance settings are read.
t_case "--help prints the usage of the command and calls nothing"
reset
t_run "$dispatch --help 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_contains "$t_out" "tehas dispatch --dry-run"
t_lacks "$t_out" "set -uo pipefail"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "-h is the same as --help"
reset
t_run "$dispatch -h 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_eq "$(t_calls gh)" ""

t_case "--help writes to standard output and nothing to standard error"
reset
t_run "$dispatch --help 1>/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "--help answers on a host with no instance settings yet"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance
t_run "$dispatch --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "a mistyped argument starts no poll and creates nothing"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an unknown argument is reported on standard error, never as a poll"
reset
t_run "$dispatch --dryrun 2>/dev/null"
t_eq "$t_rc" 64
t_lacks "$t_out" "unknown argument"
t_run "$dispatch --dryrun 1>/dev/null"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"

t_case "an unknown argument is refused before the instance settings are read"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dryrun"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""

t_case "an unknown option is refused too, whatever it looks like"
reset
for arg in "-x" "--dry-run=1" "7" "--"; do
  t_run "$dispatch '$arg'"
  t_eq "$t_rc" 64
  t_contains "$t_out" "tehas dispatch: unknown argument: $arg"
done
t_eq "$(t_calls gh)" ""

t_case "an empty argument is an argument, not a poll"
reset
t_run "$dispatch ''"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument:"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "two arguments are refused, not combined"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dry-run --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --help"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "the same argument twice is refused too"
reset
t_run "$dispatch --dry-run --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "--help with another argument is still an unknown invocation"
reset
t_run "$dispatch --help --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "no argument and --dry-run still behave as before"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-start)" "7"

t_case "an unanswered question starts nothing"
parked
comments 7 "$(park_body ' ' ' ')" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a retry ticked by an allowed account starts a cycle"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label needs-human-attention --remove-label tehas:question"
t_contains "$(t_calls gh)" "authorized by @alice's retry tick"
t_contains "$t_out" "start #7 (retry ticked by alice)"

t_case "a retry tick writes the approval record with the tick's time, before the label changes"
t_eq "$(jq -c '[.by, .how, .at]' "$T/calls/approved.at-mark" 2>&1)" '["alice","retry-tick","2026-01-01T01:00:00Z"]'

t_case "the question that was answered is used up by the start"
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "a retry waits while every slot is taken, and the tick stays"
parked
comments 7 "$(park_body x ' ')" alice
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
release

t_case "a retry whose cycle does not start is handed back, exit 1"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas could not start the cycle for this issue"

t_case "the comments are read a hundred at a time"
t_contains "$(t_calls gh)" "comments(last: 100)"

t_case "a dry run says what the tick would do and changes nothing"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry ticked by alice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001

t_case "a retry ticked by the bot or a stranger starts nothing and is undone"
for other in tehas-bot mallory ""; do
  parked
  comments 7 "$(park_body x ' ')" "$other"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
  t_contains "$(t_calls gh)" "may not answer for this factory"
done

t_case "when a refused tick cannot be undone, the note is not repeated every poll"
parked
comments 7 "$(park_body x ' ')" mallory
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "may not answer for this factory"
t_contains "$t_out" "could not untick the question on #7"

t_case "both boxes ticked is no choice: nothing starts, the boxes are emptied, one note"
parked
comments 7 "$(park_body x x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "Tick one"

t_case "leave it parked takes the question away and keeps the issue parked"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "leave it parked keeps the question on record while its label cannot be removed"
parked
comments 7 "$(park_body ' ' x)" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$(cat "$park_id")" 4001
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:question"
rm -f "$fx/edit-fails"

t_case "a dry run of leave it parked changes nothing"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch --dry-run"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(cat "$park_id")" 4001

t_case "a comment from an allowed account on a parked issue is discussion, not an answer"
parked
comments 7 "$(park_body ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"retry please","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "and it does not hide a tick made after it"
parked
comments 7 "$(park_body x ' ')" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"hm","createdAt":"2026-01-01T00:30:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a retry does not approve text that somebody else changed after the tick"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its text was changed by 'mallory' after alice ticked retry; untick the box and tick it again to approve it"

t_case "a retry waits while too many pull requests await a merge"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":1},{"number":2},{"number":3}]' >"$fx/awaiting.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "the pause label stops a retry too"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "a marker in somebody else's comment is not a question"
parked
jq -n --arg b "$(park_body x ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "mallory"}, body: $b, createdAt: "t", lastEditedAt: "t", editor: {login: "alice"}}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "when the comments cannot be read nothing is decided"
parked
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""

t_case "a question on one issue does not hold up a ready issue behind it"
reset
echo '[{"number":5}]' >"$fx/asking.json"
jq -n --arg b "$(park_body ' ' ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "tehas-bot"}, body: $b, createdAt: "t", lastEditedAt: null, editor: null}]}}}}}' >"$fx/comments-5.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a retry tick is used up once the issue is marked"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
patch_line=$(t_calls gh | grep -n 'api -X PATCH' | head -1 | cut -d: -f1)
start_line=$(t_calls gh | grep -n -e '--add-label tehas:running' | head -1 | cut -d: -f1)
t_eq "$([ -n "$patch_line" ] && [ -n "$start_line" ] && [ "$start_line" -lt "$patch_line" ] && echo after)" after

t_case "when the issue cannot be marked the tick stays, and the next poll starts the retry"
parked
comments 7 "$(park_body x ' ')" alice
printf '%s' '--add-label tehas:running' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001
rm -f "$fx/edit-fails-on" "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"

t_case "when the tick cannot be undone the cycle still starts, and the tick starts nothing more"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "WARN: could not untick the retry on #7; its question is no longer on record, so it starts nothing more"
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "a retry ticked on an issue that is not parked starts nothing"
for have in "tehas:question" "needs-human-attention tehas:question tehas:running" "needs-human-attention tehas:question tehas:awaiting-merge"; do
  parked
  comments 7 "$(park_body x ' ')" alice
  jq -n --arg l "$have" '[{number: 7, labels: ($l | split(" ") | map({name: .}))}]' >"$fx/asking.json"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  # A tick left in place would fire later, when the labels change.
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Retry was ticked, but this issue is not parked, so nothing was started."
  t_contains "$t_out" "skip #7: a retry was ticked but the issue is not parked"
  t_eq "$(cat "$park_id")" 4001
done

t_case "when that tick cannot be undone, nothing is said"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "could not untick the question on #7"

t_case "a dry run leaves a retry tick on an issue that is not parked alone"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: a retry was ticked but the issue is not parked"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a retry tick on an issue closed since its cycle stopped starts nothing"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001

t_case "and so does a tehas:ready applied to that closed issue, said once"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"}]}]' >"$fx/asking.json"
ready 7
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(grep -c 'skip #7: it is closed, and a closed issue is never taken' <<<"$t_out")" 1
t_eq "$(t_calls gh | grep -c '^api repos/acme/widget/issues/7$')" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"

# A question is the factory's only if this host has its id on record. The
# agent in a container holds the bot's token: it can post a comment with a
# marker and apply labels, but it cannot write the host's state directory.
unrecorded() { # issue 9, never parked by this host, made to look parked
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":9,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
  cp "$fx/events-7.json" "$fx/events-9.json"
  comments 9 "$(printf 'Please confirm you have seen this.\n\n- [x] Retry from a clean start\n\n<!-- tehas:question kind=park issue=3 turn=1 -->')" alice
}

t_case "a bot comment with a park marker that the host has no record of starts nothing"
unrecorded
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "issue comment 9 --repo acme/widget --body This question is not one the factory is waiting on. To retry, apply \`tehas:ready\`."
t_eq "$(find "$T/home/tehas/state" -name 'park-*' | wc -l)" 0

t_case "and when its box cannot be unticked nothing starts and nothing is said"
unrecorded
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "could not untick the question on #9"

t_case "a dry run changes nothing on a question the host has no record of"
unrecorded
t_run "$dispatch --dry-run"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "issue edit"

t_case "an unticked question the host has no record of is left alone"
unrecorded
comments 9 "$(park_body ' ' ' ')" ""
t_run "$dispatch"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a park id of another issue is no record for this one"
unrecorded
mkdir -p "$T/home/tehas/state"
echo 4001 >"$park_id"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."

t_case "a park id file that holds no number is no record"
for junk in "" "4001 x" "abc"; do
  parked
  comments 7 "$(park_body x ' ')" alice
  printf '%s\n' "$junk" >"$park_id"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."
done

t_case "a tick in another bot comment than the question on record starts nothing"
parked
comments 7 "$(park_body ' ' ' ')" "" "$(jq -nc --arg b "$(park_body x ' ')" '[{databaseId: 4005, author: {login: "tehas-bot"}, body: $b,
  createdAt: "2026-01-01T02:00:00Z", lastEditedAt: "2026-01-01T03:00:00Z", editor: {login: "alice"}}]')"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"

t_case "a question on record whose marker names another issue starts nothing"
parked
comments 7 "$(park_body x ' ' | sed 's/issue=7/issue=3/')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a park id that names a planning question starts nothing"
parked
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"
t_lacks "$(t_calls gh)" "api -X"

t_case "a question on record that is not among the comments read is said in the log"
parked
jq -n '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "alice"}, body: "hm", createdAt: "t", lastEditedAt: null, editor: null}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its question is not among the last 100 comments"

t_case "retry is the ticked line's text, not its place"
swapped() { # <leave box> <retry box>
  printf 'Tehas cycle stopped.\n\n- [%s] Leave it parked\n- [%s] Retry from a clean start\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n' "$1" "$2"
}
parked
comments 7 "$(swapped x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
parked
comments 7 "$(swapped ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a plan label from an allowed account starts a session from the issue's text"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --brief $record.brief"
t_eq "$(cat "$record.brief")" '# Faster search

Make it fast.'
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:plan\` label"
t_contains "$(t_calls gh)" "- [ ] Adopt all"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"running":false,"comment":4002}'
t_eq "$([ -e "$record.out" ] && echo left)" ""
t_eq "$(t_calls fake-start)" ""

t_case "an issue with no body is planned from its title"
to_plan
echo '{"title":"Faster search","body":null}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
t_eq "$(head -n 1 "$record.brief")" "# Faster search"

t_case "a plan label from the bot or a stranger starts nothing and is taken off the issue"
for other in tehas-bot mallory; do
  to_plan
  echo "[{\"event\":\"labeled\",\"label\":{\"name\":\"tehas:plan\"},\"actor\":{\"login\":\"$other\"}}]" >"$fx/events-7.json"
  t_run "$dispatch"
  t_eq "$(plan_calls)" ""
  t_contains "$t_out" "skip #7: tehas:plan was applied by '$other', not an allowed account"
  t_contains "$t_out" "clear #7: tehas:plan off, applied by '$other' who may not authorise work here"
  t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:plan')" 1
  t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1
done

t_case "a dry run leaves a stranger's plan label where it is"
to_plan
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would take tehas:plan off #7: it was applied by 'mallory'"
t_lacks "$(t_calls gh)" "issue edit"

t_case "text changed after the plan label by somebody else is not planned"
to_plan
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T00:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

t_case "a dry run says a session would start and changes nothing"
to_plan
t_run "$dispatch --dry-run"
t_contains "$t_out" "would start a planning session for #7 (labelled by alice)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "planning is not held back by pull requests that await a merge"
to_plan
echo '[{"number":1},{"number":2},{"number":3}]' >"$fx/awaiting.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"

t_case "the poll's lock is not handed to a planning turn"
t_eq "$(find "$T/calls/plan.fd8" 2>/dev/null | wc -l)" 0

t_case "the pause label stops planning"
to_plan
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

t_case "a session does not start when the issue cannot be marked as planning"
to_plan
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "ERROR: could not mark #7 as planning, not starting a turn"
t_eq "$([ -e "$record" ] && echo kept)" ""
rm -f "$fx/edit-fails"

t_case "the turn is on record as running before the issue is marked"
to_plan
t_run "$dispatch"
t_eq "$(jq -c . "$T/calls/record.at-mark")" '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}'

t_case "a session does not start when its record cannot be written, and the issue is not marked"
to_plan
mkdir -p "$record.new"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the record of #7, not starting a turn"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
rm -rf "$record.new"

t_case "a later turn that cannot be marked leaves the record as it was, still answerable"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(cat "$record")
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_eq "$(cat "$record")" "$before"
t_eq "$(jq -r .running "$T/calls/record.at-mark")" true
rm -f "$T"/calls/* "$fx/edit-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"

t_case "while a later turn runs the record names no question"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$(jq -c . "$T/calls/record.at-turn")" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"running":true}'

t_case "a turn cut off before the issue was marked is reported like any cut-off turn"
planning
jq '.running = true | del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
# The mark never happened: the issue still asks, and the old answer is there.
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it was cut off"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":2,"options":[],"pending":false,"failed":true,"running":false,"comment":4002}'
t_eq "$(plan_calls)" ""

t_case "a first turn cut off before the issue was marked ends visibly and takes the plan label off"
to_plan
mkdir -p "$T/home/tehas/state"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
# What GitHub answers once the label is off.
echo '[]' >"$fx/plans.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""
t_eq "$(plan_calls)" ""

t_case "a first turn that fails parks the issue and keeps no record"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a first turn that fails after its session came into being keeps the session"
to_plan
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it exited with status 2."
t_contains "$(t_calls gh)" "Reply in a comment to run the turn again"
t_lacks "$(t_calls gh)" "could not start"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":1,"options":[],"pending":false,"failed":true,"running":false,"comment":4002}'

t_case "and a reply runs the turn again in that session"
rm -f "$T"/calls/* "$fx/plan-fails"
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
echo '[]' >"$fx/plans.json"
jq -n '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 4002, author: {login: "tehas-bot"}, createdAt: "2026-01-01T00:00:00Z", lastEditedAt: null, editor: null,
   body: "This planning turn did not finish.\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->"},
  {databaseId: 4003, author: {login: "alice"}, body: "go on", createdAt: "2026-01-01T02:00:00Z"}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"

t_case "a session id that is not a string in a failed turn's file is no session"
to_plan
echo '{"session":{"a":1}}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "the record lists the options that were posted, so a box and its option keep the same place"
to_plan
jq -n '{session: "abc-123", text: "Many ways.", question: {question: "Which?", options: [range(30) | {label: "Way \(.)", description: ""}]}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(jq -c '[(.options | length), .options[0], .options[19]]' "$record")" '[20,"Way 0","Way 19"]'
t_eq "$(t_calls gh | grep -c '^- \[ \] Way ')" 20
t_contains "$(t_calls gh)" "Options left out: 10."

t_case "questions posted as one have no option boxes, so a tick answers none of them"
to_plan
jq -n '{session: "abc-123", text: "Two things.", question: {question: "The agent asked 2 questions. Answer each by its number in a reply.\n\n1. Which?\n   - A\n\n2. Where?\n   - B", options: []}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(jq -c .options "$record")" "[]"
t_contains "$(t_calls gh)" "1. Which?"
t_contains "$(t_calls gh)" "2. Where?"
t_eq "$(t_calls gh | grep -c '^- \[ \] ')" 1
t_contains "$(t_calls gh)" "- [ ] End this planning session"

t_case "a ticked option goes to the session as the next message"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_eq "$(jq -r .turns "$record")" 2

t_case "a reply goes to the session as it was written"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"Adopt all, but keep `tagged-at` open.\nThen file 3 and 5.","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(cat "$record.answer")" 'The operator replied on the issue:

Adopt all, but keep `tagged-at` open.
Then file 3 and 5.'

t_case "a reply's turn is given the reply's comment id, so its attachments go with it"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer --comment 4003"

t_case "a ticked option passes no comment"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_lacks "$(plan_calls)" "--comment"

t_case "a reply wins over a tick, and the answer says what was ticked"
planning
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice bob'" >"$TEHAS_INSTANCE_DIR/instance.env"
comments 7 "$(plan_body x ' ' ' ')" alice '[{"databaseId":4003,"author":{"login":"bob"},"body":"actually, wait","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(cat "$record.answer")" 'The operator replied on the issue:

actually, wait

A box was also ticked, by alice: Adopt all'
t_contains "$t_out" "planning #7 continues (answer from bob)"

t_case "a reply with the way out ticked runs the turn and says the box was ticked"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"one more thing","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $record.answer"
t_contains "$(cat "$record.answer")" "A box was also ticked, by alice: End this planning session"

t_case "a reply alone says nothing of a box"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"just this","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_lacks "$(cat "$record.answer")" "A box was also ticked"

t_case "an answer on an issue closed since the turn ran runs no turn"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(cat "$record")
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_eq "$(plan_calls)" ""
t_eq "$(cat "$record")" "$before"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a stranger's reply and a stranger's tick run no turn"
planning
comments 7 "$(plan_body x ' ' ' ')" mallory '[{"databaseId":4003,"author":{"login":"mallory"},"body":"file everything","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "api -X PATCH"

t_case "the way out ends the session: labels off, record gone, one comment"
planning
mkdir -p "$record.keep/images"
comments 7 "$(plan_body ' ' ' ' x)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:question"
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$([ -e "$record" ] && echo kept)" ""
t_eq "$([ -e "$record.keep" ] && echo kept)" ""

t_case "the turn limit ends the session in place of another turn"
planning
jq '.turns = 20' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "reached its limit of 20 turns"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "an answer to a session whose record is gone ends it and says why"
planning
rm "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "The record of this planning session is gone"

t_case "a later turn that fails asks again and keeps the session"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish"
t_contains "$(t_calls gh)" "--add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -r '[.session, .turns, .failed] | @tsv' "$record")" "abc-123	2	true"

t_case "a later turn whose Pi session is gone ends the session and says to start again"
planning
mkdir -p "$record.keep"
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"
t_contains "$t_out" "planning #7: Pi no longer has session abc-123"
t_contains "$(t_calls gh)" "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention"
t_contains "$(t_calls gh)" "its conversation is no longer on"
t_contains "$(t_calls gh)" "Apply \`tehas:plan\` again to start a new session."
t_lacks "$(t_calls gh)" "This planning turn did not finish"
t_lacks "$(t_calls gh)" "tehas:question kind=plan issue=7 turn=2"
t_eq "$([ -e "$record" ] || [ -e "$record.keep" ] && echo kept)" ""

t_case "a lost session whose labels cannot be taken off is ended by the next poll"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
echo "--remove-label tehas:question --remove-label tehas:planning" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(jq -c '[.lost, .running]' "$record")" '[true,true]'
t_lacks "$(t_calls gh)" "its conversation is no longer on"
rm -f "$T"/calls/* "$fx/edit-fails-on" "$fx/plan-fails"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "its conversation is no longer on"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "an outcome that could not be posted is posted by the next poll, before anything else"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/post-fails"
t_run "$dispatch"
t_eq "$(jq -r .pending "$record")" true
t_eq "$([ -s "$record.out" ] && echo kept)" kept
t_lacks "$(t_calls gh)" "--add-label tehas:question"
rm -f "$T"/calls/* "$fx/post-fails"
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -r .pending "$record")" false

t_case "the record of a session whose issue no longer asks is removed"
planning
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$([ -e "$record" ] && echo kept)" ""
t_lacks "$(t_calls gh)" "issue comment"

t_case "an answered question is served before a new plan label and before a ready issue"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '[{"number":8,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"number":9,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out"
t_lacks "$(plan_calls)" "8 --out"
t_eq "$(t_calls fake-start)" ""

# One issue is in one flow at a time.
t_case "tehas:ready on an issue with an open planning session starts no cycle"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]' >"$fx/ready.json"
before=$(cat "$record")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_eq "$(cat "$record")" "$before"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_lacks "$t_out" "would run a cycle"

t_case "tehas:ready on an issue whose planning turn is marked as running starts no cycle"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:planning"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"

t_case "an issue with a planning session does not hold up the ready issue behind it"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
cp "$fx/events-7.json" "$fx/events-8.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
rm -f "$fx/events-8.json"

t_case "a parked issue retried by its labels starts a cycle, and its old question goes"
parked
comments 7 "$(park_body ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:question')" 1
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "the question label is only taken off an issue that carries it"
reset
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready')" 1

# tehas:ready from an allowed account starts a cycle whatever else the issue
# carries. Only a cycle that is alive holds it back.
has() { # issue label...: the queue is that one issue, labelled ready by alice, carrying the labels too
  local n=$1 l list='{"name":"tehas:ready"}'
  shift
  for l in "$@"; do list="$list,{\"name\":\"$l\"}"; done
  echo "[{\"number\":$n,\"labels\":[$list]}]" >"$fx/ready.json"
}

t_case "tehas:ready on a parked issue starts a cycle without needs-human-attention being removed first"
reset
has 7 needs-human-attention
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label needs-human-attention')" 1

t_case "tehas:ready on an issue that awaits a merge starts a cycle and takes that label off"
reset
has 7 tehas:awaiting-merge
echo '[]' >"$fx/awaiting.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:awaiting-merge')" 1

t_case "every label of an earlier outcome goes with the start"
reset
has 7 needs-human-attention tehas:question tehas:awaiting-merge
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label needs-human-attention --remove-label tehas:awaiting-merge --remove-label tehas:question')" 1

t_case "an issue left marked as running with no cycle is started on tehas:ready"
reset
has 7 tehas:running
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready')" 1

t_case "a parked issue labelled by a stranger starts nothing and loses that label"
reset
has 7 needs-human-attention
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"

t_case "tehas:ready applied while the issue's cycle runs waits, said in the log and not on the issue"
reset
slots 2
has 7 tehas:running
hold 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "wait #7: its cycle is still running"

t_case "and the poll after that cycle ended starts a fresh one"
release
has 7 tehas:awaiting-merge
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "--add-label tehas:running --remove-label tehas:ready --remove-label tehas:awaiting-merge"

t_case "an issue that waits behind its own cycle does not hold back the next one"
reset
slots 2
ready 7 8
hold 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
release

t_case "a retry tick takes a tehas:ready that was left on the issue with it"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"}]}]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label needs-human-attention --remove-label tehas:question --remove-label tehas:ready')" 1

t_case "every list of issues is read to a thousand"
reset
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -e '^issue list .* --limit 1000 ')" 5
t_lacks "$(t_calls gh)" "--limit 100 "

t_case "an answer on a question the session is not waiting on runs no turn"
planning
jq '.comment = 9999' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_contains "$t_out" "skip #7: its question is not among the last 100 comments"

t_case "an answer to a session whose record names no question runs no turn"
planning
jq 'del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: the question answered is not the one this session is waiting on"
t_eq "$([ -e "$record" ] && echo kept)" kept

t_case "a newer stray bot comment with a plan marker does not lock the session out"
planning
comments 7 "$(plan_body ' ' x ' ')" alice "$(jq -nc --arg b "$(plan_body ' ' ' ' ' ' | sed 's/turn=1/turn=5/')" '[{databaseId: 4009, author: {login: "tehas-bot"}, body: $b,
  createdAt: "2026-01-01T02:00:00Z", lastEditedAt: null, editor: null}]')"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"

t_case "a planning question on record whose marker is a park marker runs no turn and no cycle"
planning
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"

t_case "a question that posted but could not be labelled is not lost"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
printf '%s' '--add-label tehas:question' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(jq -r '[.comment, .pending] | @tsv' "$record")" "4002	true"
t_eq "$([ -s "$record.out" ] && echo kept)" kept
rm -f "$T"/calls/* "$fx/edit-fails-on"
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'issues/7/comments')" 0
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -r .pending "$record")" false
t_eq "$([ -e "$record.out" ] && echo left)" ""

t_case "a later turn cut off by a restart is reported and can be answered"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it was cut off"
t_contains "$(t_calls gh)" "--add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -r '[.session, .turns, .failed, .running] | @tsv' "$record")" "abc-123	2	true	false"
t_eq "$(plan_calls)" ""

t_case "the container of a cut-off turn is removed before the turn is reported"
t_contains "$(t_calls docker)" "rm -f tehas-plan-7"
t_case "a first turn cut off by a restart ends visibly"
planning
echo '[]' >"$fx/asking.json"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a first turn cut off by a restart says nothing while its label change fails"
planning
echo '[]' >"$fx/asking.json"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
: >"$fx/edit-fails"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(jq -r .running "$record")" true

t_case "a record of a turn in flight or waiting to be posted is not cleaned up"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
t_run "$dispatch --dry-run"
t_eq "$([ -e "$record" ] && echo kept)" kept
planning
echo '[]' >"$fx/asking.json"
jq '.pending = true | del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
cp "$fx/plan-result.json" "$record.out"
: >"$fx/post-fails"
t_run "$dispatch"
t_eq "$([ -e "$record" ] && echo kept)" kept
t_eq "$(jq -r .pending "$record")" true

t_case "the end of a session is said once"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice
printf '%s' '--remove-label needs-human-attention' >"$fx/edit-fails-on"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$([ -e "$record" ] && echo kept)" kept
rm -f "$T"/calls/* "$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'Planning session ended by alice.')" 1
t_contains "$(t_calls gh)" "--remove-label needs-human-attention"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a session whose record is gone is not announced while its labels cannot be removed"
planning
rm "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a dry run reports a cut-off turn and changes nothing"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
before=$(cat "$record")
t_run "$dispatch --dry-run"
t_contains "$t_out" "would report the cut-off planning turn of #7"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(cat "$record")" "$before"

t_case "a file in the state directory that is not a session record is left alone"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$T/home/tehas/state"
echo '{"running":true}' >"$T/home/tehas/state/plan-x.json"
echo 'not json' >"$T/home/tehas/state/plan-9.json"
t_run "$dispatch"
t_eq "$([ -e "$T/home/tehas/state/plan-x.json" ] && [ -e "$T/home/tehas/state/plan-9.json" ] && echo kept)" kept
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(grep -c 'planning record of #9 is not readable' <<<"$t_out")" 1

t_case "a bot account in the allowed list stops the poll before any question is read"
parked
comments 7 "$(park_body x ' ')" alice
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"

t_case "the bot is refused as an allowed labeller even with no question open"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_contains "$(t_calls gh)" "api user"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "an instance that lists the bot is refused before its slots are swept"
reset
echo '[{"number":9}]' >"$fx/marked.json"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(t_calls fake-start)" ""

t_case "an own login that GitHub answers without one ends the poll before anything is written"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"message":"Bad credentials"}' >"$fx/user.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the bot's own login could not be read"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(t_calls fake-start)" ""

# Admission by the slice. Every sandbox container runs in tehas.slice; a
# cycle starts only while the slice has one more reserve of room. reset gives
# a slice of 20 GiB and the default reserve of 2g.
t_case "no room: nothing starts, no label is edited, the tick stays, exit 0, the dry run names the room"
parked
comments 7 "$(park_body x ' ')" alice
ready 8
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(cat "$park_id")" 4001
t_contains "$t_out" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"

t_case "a cold start is limited by the reserve: with room for two, two of four start"
reset
slots 4
ready 7 8 9 10
other $((15 * gib + gib / 2))
t_run "$dispatch --dry-run"
t_contains "$t_out" "0 cycles running, room for 2 more"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$(t_calls gh)" "issue edit 9 "

t_case "running cycles with no container yet count one reserve each"
reset
slots 4
ready 7 8
other $((13 * gib))
hold 9
hold 10
t_run "$dispatch --dry-run"
t_contains "$t_out" "2 cycles running, room for 1 more"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "a running cycle's container counts at its floor or its use, whichever is more"
reset
slots 6
ready 7 8 9
other $((10 * gib))
hold 9
hold 10
# One young cycle under its floor and one grown past it: 2 and 5 GiB.
cid 9 a $((gib / 2))
cid 10 b $((5 * gib))
t_run "$dispatch --dry-run"
t_contains "$t_out" "2 cycles running, room for 1 more"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "another container's memory lowers the room while cycles are young"
reset
slots 6
ready 7 8 9 10
slice_show $((12 * gib))
hold 12
cid 12 a $((gib / 10))
# A container at 7 GiB is not hidden inside the cycle's floor: 2 and 7 GiB
# are promised, and one more reserve fits in the 3 GiB that are left.
other $((7 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 "
release

t_case "reclaimable slab and file cache do not lower the room"
reset
slots 6
ready 7 8 9 10 11 12
slice_show $((12 * gib))
SLICE_FILE=$((8 * gib)) slice_child "$(slice_id d)" "$gib" $((6 * gib)) $((2 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 "

t_case "full pressure holds over half the slice promised, some pressure does not"
reset
slice_state 12.00
other $((11 * gib))
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "holding: tehas.slice is under memory pressure (full avg60 12.00)"
reset
slice_state 0.00 55.00
other $((11 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"
# One container thrashing at its own ceiling in a slice mostly empty.
reset
slice_state 12.00
other $((5 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"

t_case "an unreadable probe holds and names the file"
reset
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/memory.pressure cannot be read"
t_eq "$(jq -r .hold "$dispatch_json")" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/memory.pressure cannot be read"
reset
other "$gib"
echo "anon many" >"$T_CGROUP/tehas.slice/docker-$(slice_id d).scope/memory.stat"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/docker-$(slice_id d).scope/memory.stat has no number for anon"
t_lacks "$t_out" "would run a cycle"
# A planning turn is held too: its container would start in a slice nobody
# can see into, after its label and comment were spent.
to_plan
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "holding: the slice's use could not be read"
t_eq "$(jq -r .why "$dispatch_json")" unreadable
planning
comments 7 "$(plan_body ' ' x ' ')" alice
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "/comments -f"

t_case "an unreadable probe is named before a full count"
# The hold on planning has to be the hold that is logged, so it comes before
# the count, which holds cycles only.
to_plan
hold 9
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "holding: the slice's use could not be read"
t_lacks "$t_out" "cycles are running"
t_eq "$(jq -r .why "$dispatch_json")" unreadable
release

t_case "a missing cgroup directory under running cycles holds while the slice is active"
reset
slots 2
rm -rf "$T_CGROUP/tehas.slice"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the slice's use could not be read"
release
# A cycle between containers in a slice that is not active is one reserve.
reset
slots 2
slice_show $((20 * gib)) inactive
rm -rf "$T_CGROUP/tehas.slice"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"
t_eq "$(jq -r '"\(.room) \(.committed)"' "$dispatch_json")" "3 $((2 * gib))"
release
# With nothing running and the slice not active, no directory is an empty slice.
reset
slice_show $((20 * gib)) inactive
rm -rf "$T_CGROUP/tehas.slice"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "no slice: the hold is logged in a real poll and nothing is labelled"
reset
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "awaiting-merge"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
t_lacks "$t_out" "would run a cycle"

t_case "a cgroupfs driver holds the same way"
reset
echo "cgroupfs 2" >"$fx/docker.info"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "no slice: a ticked retry is left as it is"
parked
comments 7 "$(park_body x ' ')" alice
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001

t_case "no slice: a waiting planning answer is left as it is, no label is edited, no comment"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
no_slice
before=$(cat "$record")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "/comments -f"
t_eq "$(cat "$record")" "$before"
t_eq "$([ -e "$record.answer" ] && echo written)" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host"
t_lacks "$t_out" "would run a planning turn"
# The same answer runs its turn once the slice is there.
slice_show $((20 * gib))
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"

t_case "no slice: the way out of a planning session still ends it"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice
no_slice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "no slice: a plan label starts no session and keeps its label"
to_plan
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$([ -e "$record" ] || [ -e "$record.brief" ] && echo written)" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host"
t_lacks "$t_out" "would start a planning session"

t_case "no room still does not hold planning"
to_plan
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
slice_state 12.00
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"

t_case "the orphan sweep still runs"
reset
no_slice
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""

t_case "a real poll logs a hold once, then released once, and writes live/dispatch.json"
reset
echo '[]' >"$fx/ready.json"
other $((19 * gib + gib / 2))
t_run "$dispatch --dry-run"
t_eq "$([ -e "$dispatch_json" ] && echo written)" ""
t_run "$dispatch"
t_eq "$(grep -c 'holding: tehas.slice has no room' <<<"$t_out")" 1
# The status is written once more when a hold begins or ends, for tehas status.
t_eq "$(t_calls fake-status | wc -l)" 2
t_eq "$(jq -c '[.why, .count, .room, .committed, .max, .reserve]' "$dispatch_json")" \
  "[\"room\",0,0,$((19 * gib + gib / 2)),$((20 * gib)),\"2g\"]"
t_eq "$(jq -r .hold "$dispatch_json")" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"
t_eq "$(jq -r '.time | fromdateiso8601 | (now - .) < 60' "$dispatch_json")" true
t_eq "$(find "$T/home/tehas/live" -name 'dispatch.json.*' | wc -l)" 0
rm -f "$T/calls/fake-status"
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_eq "$(t_calls fake-status | wc -l)" 1
other "$gib"
t_run "$dispatch"
t_eq "$(grep -c 'released: cycles may start again' <<<"$t_out")" 1
t_eq "$(t_calls fake-status | wc -l)" 3
t_lacks "$t_out" "holding:"
t_eq "$(jq -c '[.why, .hold, .room, .committed]' "$dispatch_json")" "[\"\",\"\",3,$((2 * gib))]"
t_run "$dispatch"
t_lacks "$t_out" "released"

# The model endpoint, as models.json names it for the provider. The stand-in
# for curl answers with the file mesh.json, or fails when there is none.
mesh() { # [<the endpoint's answer>]
  mkdir -p "$T/home/.pi/agent"
  echo '{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"}}}' >"$T/home/.pi/agent/models.json"
  rm -f "$fx/mesh.json"
  [ "$#" = 0 ] || echo "$1" >"$fx/mesh.json"
}
# A request with an image is answered when the file reads-images is there; what
# was sent is kept.
t_stub curl 'case "$*" in
  *"/chat/completions") cat >"$T/calls/curl-body"; [ -e "$T/fx/reads-images" ] ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac'

t_case "an endpoint that offers the model holds nothing back"
reset
mesh '{"data":[{"id":"other"},{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls curl)" "http://mesh.test/v1/models"
t_eq "$(jq -r .why "$dispatch_json")" ""

while IFS='|' read -r answer text; do
  t_case "no resources: $text"
  reset
  if [ "$answer" = none ]; then mesh; else mesh "$answer"; fi
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$(grep -c "holding: no resources: $text" <<<"$t_out")" 1
  t_eq "$(jq -c '[.why, .hold]' "$dispatch_json")" "[\"resources\",\"holding: no resources: $text\"]"
  t_lacks "$(cat "$dispatch_json")" "mesh.test"
done <<'CASES'
none|the model endpoint does not answer
not json|the model endpoint offers no models
{"data":[]}|the model endpoint offers no models
{"data":[{"id":"other"}]}|the model endpoint does not offer test-model
CASES

t_case "a role's own model the endpoint does not offer holds the start, and the endpoint is asked once"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(grep -c "holding: no resources: the model endpoint does not offer small-model" <<<"$t_out")" 1
t_eq "$(t_calls curl | grep -c '/models$')" 1
mesh '{"data":[{"id":"test-model"},{"id":"small-model"}]}'
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a role on a provider of its own is asked of that provider's endpoint"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other
mesh '{"data":[{"id":"test-model"}]}'
echo '{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"},"other":{"baseUrl":"http://other.test/v1"}}}' >"$T/home/.pi/agent/models.json"
t_run "$dispatch"
t_contains "$(t_calls curl)" "http://other.test/v1/models"
t_eq "$(grep -c "holding: no resources: the model endpoint does not offer big-model" <<<"$t_out")" 1
t_lacks "$(cat "$dispatch_json")" "other.test"

# The model as models.json declares it, with the input it takes.
declares() { # <input list as JSON>
  jq -n --argjson input "$1" '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1",
    models: [{id: "other", input: ["text", "image"]}, {id: "test-model", input: $input}]}}}' >"$T/home/.pi/agent/models.json"
}

t_case "without images for a cycle the model is not asked to read one"
reset
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$(t_calls curl)" "chat/completions"

# An instance that gives a cycle images, and a model that offers what
# models.json declares for it.
imaging() { # <input list as JSON>
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
  mesh '{"data":[{"id":"test-model"}]}'
  declares "$1"
  rm -f "$fx/reads-images"
}
# An issue whose text links an attachment in GitHub's storage.
pictured() { # <issue>
  jq -nc --argjson n "$1" '{number: $n, state: "open",
    body: "It looks like this:\n\n![shot](https://github.com/user-attachments/assets/0f3e-41)"}' >"$fx/work-$1.json"
}
image_asks() { t_calls curl | grep -c 'chat/completions'; }

t_case "with images for a cycle, an issue that links one starts when the model declares image input and answers one"
imaging '["text","image"]'
pictured 7
: >"$fx/reads-images"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
t_contains "$(t_calls curl)" "-m 30 -H Content-Type: application/json -d @- http://mesh.test/v1/chat/completions"
t_eq "$(jq -c '[.model, .max_tokens, (.messages[0].content | map(.type))]' "$T/calls/curl-body")" '["test-model",1,["text","image_url"]]'
t_eq "$(jq -r '.messages[0].content[1].image_url.url' "$T/calls/curl-body" | cut -d, -f2 | base64 -d | head -c 8 | od -An -tx1 | tr -d ' \n')" "89504e470d0a1a0a"

t_case "an issue that links no image starts while the model cannot read one, and the model is not asked"
for input in '["text"]' '["text","image"]'; do
  imaging "$input"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_eq "$(image_asks)" 0
  t_eq "$(jq -r .why "$dispatch_json")" ""
  t_lacks "$(t_calls gh)" "images are off"
done

t_case "an issue that links an image waits while the model does not declare image input, and it is not asked"
for input in '["text"]' 'null'; do
  imaging "$input"
  pictured 7
  : >"$fx/reads-images"
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not declare image input in models.json"
  t_eq "$(image_asks)" 0
  t_eq "$(jq -r .why "$dispatch_json")" ""
done

t_case "an issue that links an image waits while the model does not answer an image"
imaging '["text","image"]'
pictured 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not answer an image"
t_lacks "$t_out" "mesh.test"

t_case "an issue without images behind one that waits for them starts"
imaging '["text"]'
ready 7 8
pictured 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
t_contains "$t_out" "hold #7:"

t_case "the model is asked to read an image once a poll"
imaging '["text","image"]'
slots 3 TEHAS_WORK_IMAGES=1
ready 7 8 9
pictured 7
pictured 8
t_run "$dispatch"
t_eq "$(image_asks)" 1
t_eq "$(t_calls fake-start)" "9"
t_contains "$t_out" "hold #8:"

t_case "with every slot taken no image request is sent"
imaging '["text","image"]'
slots 1 TEHAS_WORK_IMAGES=1
pictured 7
: >"$fx/reads-images"
hold 9
t_run "$dispatch"
t_eq "$(image_asks)" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .why "$dispatch_json")" count
release

t_case "a retry of an issue that links an image waits for a model that reads one, and the tick stays"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text"]'
comments 7 "$(park_body x ' ')" alice
pictured 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_contains "$t_out" "hold #7: its text links an attachment"

t_case "a planning session is not held by the image part of the check"
imaging '["text"]'
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
pictured 7
t_run "$dispatch"
t_eq "$(t_calls fake-plan | wc -l)" 1
t_eq "$(image_asks)" 0

t_case "an issue whose text cannot be read for its links is not started"
imaging '["text","image"]'
echo '{"number":7,"state":"open","body":42}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the text of #7 could not be read for its links"

# What the sandbox image says about its own pi-rukas, as its build writes it:
# <commit>|<reads>.
img_commit=$(printf 'c%.0s' {1..40})
image_says() { # <1, 0, unknown, or none when the image carries no label>
  rm -f "$fx/image.label"
  [ "$1" = none ] || printf '%s\n' "$img_commit|$1" >"$fx/image.label"
}

t_case "an issue that links an image waits while the sandbox image says its pi-rukas does not read them, and the model is not asked"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says 0
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(image_asks)" 0
t_contains "$t_out" "hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but the pi-rukas in the sandbox image cccccccccccc does not read them"
t_eq "$(jq -r .why "$dispatch_json")" ""
image_says none

t_case "an issue that links no image starts while the sandbox image does not read them"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
image_says 0
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
image_says none

t_case "an image that reads them is held for nothing, and the model is still asked"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says 1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
t_eq "$(image_asks)" 1
image_says none

t_case "an image that says nothing is not held for it, whatever its pi-rukas really reads"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
mesh '{"data":[{"id":"test-model"}]}'
declares '["text","image"]'
: >"$fx/reads-images"
pictured 7
image_says unknown
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""
image_says none
rm -f "$fx/reads-images"

t_case "no resources is logged once, with nothing in line as well, and released when the endpoint is back"
reset
echo '[]' >"$fx/ready.json"
mesh
t_run "$dispatch"
t_eq "$(grep -c 'holding: no resources' <<<"$t_out")" 1
t_eq "$(jq -r .why "$dispatch_json")" resources
t_run "$dispatch"
t_lacks "$t_out" "holding:"
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(grep -c 'released: cycles may start again' <<<"$t_out")" 1
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "no resources holds a planning session too, and its label stays"
reset
mesh
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-plan)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a dry run says no resources and writes nothing"
reset
mesh
t_run "$dispatch --dry-run"
t_eq "$(grep -c 'holding: no resources: the model endpoint does not answer' <<<"$t_out")" 1
t_eq "$([ -e "$dispatch_json" ] && echo written)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a missing slice is said before no resources"
reset
mesh
no_slice
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" slice
rm -rf "$T/home/.pi"

t_case "a poll with room and no hold before it logs nothing about the slice"
reset
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_lacks "$t_out" "released"
t_eq "$(jq -c '[.why, .count, .room]' "$dispatch_json")" '["",0,4]'

t_case "a hold whose numbers change between polls is logged once while nothing waits"
reset
echo '[]' >"$fx/ready.json"
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice has no room"
other $((19 * gib + gib * 3 / 4))
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_eq "$(jq -r .committed "$dispatch_json")" "$((19 * gib + gib * 3 / 4))"

t_case "while an issue waits, the hold is logged on every poll"
reset
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice has no room"
t_run "$dispatch"
t_eq "$(grep -c 'holding' <<<"$t_out")" 1

t_case "a hold whose reason changes is logged again"
slice_state 12.00
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$t_out" "holding: tehas.slice is under memory pressure"
t_eq "$(jq -r .why "$dispatch_json")" pressure
no_slice
t_run "$dispatch"
t_contains "$t_out" "holding: no slice file on this host"
t_eq "$(jq -c '[.why, .room, .committed, .max]' "$dispatch_json")" '["slice",0,0,0]'

t_case "a full set of slots is a hold like any other: logged once, then released"
reset
hold 9
t_run "$dispatch"
t_eq "$(grep -c 'holding: 1 cycles are running (limit 1)' <<<"$t_out")" 1
t_eq "$(jq -c '[.why, .count]' "$dispatch_json")" '["count",1]'
t_run "$dispatch"
t_lacks "$t_out" "holding:"
release
t_run "$dispatch"
t_contains "$t_out" "released: cycles may start again"
t_eq "$(t_calls fake-start)" "7"

t_case "a dispatch.json that cannot be read does not stop the poll"
reset
mkdir -p "$T/home/tehas/live"
echo '{"time":' >"$dispatch_json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "a slice probe that hangs holds within fifteen seconds"
reset
: >"$fx/slice-hangs"
began=$SECONDS
t_run "$dispatch"
t_eq "$([ $((SECONDS - began)) -le 15 ] && echo in time)" "in time"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: systemctl gave no answer"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

# Outside the tests nothing names the scripts: the dispatcher runs the ones in
# its own directory, whatever PATH holds, and starts a cycle through systemd.
bare_tree() {
  # The cgroup root is the one seam left: the slice's use cannot be read from
  # the machine the tests run on.
  T_ENV=("$slice_env")
  tree=$T/tree
  rm -rf "$tree"
  mkdir -p "$tree/libexec" "$tree/lib"
  cp "$dispatch" "$tree/libexec/dispatch"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/cycles.sh" "$T_ROOT/lib/labels.sh" "$T_ROOT/lib/questions.sh" "$T_ROOT/lib/planning.sh" "$T_ROOT/lib/slice.sh" "$T_ROOT/lib/attachments.sh" "$tree/lib/"
  for name in status index plan; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
  for name in tehas tehasbot tehas-work tehas-status tehas-index tehas-plan; do t_stub "$name"; done
}

t_case "without the test seams it runs the scripts beside it and starts the unit"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo loaded ;;
  "--user start") n=${3#tehas-cycle@}
    : >"$TEHAS_HOME/state/cycles/${n%.service}.taken" ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax
--user show -p LoadState --value tehas-cycle@7.service
--user start tehas-cycle@7.service"
t_eq "$(t_calls docker)" "info --format {{.CgroupDriver}} {{.CgroupVersion}}"
t_contains "$(t_calls fake-index)" "--before-cycle"
t_eq "$(t_calls fake-status | wc -l)" 2
for name in tehas tehasbot tehas-work tehas-status tehas-index tehas-plan; do t_eq "$(t_calls "$name")" ""; done

t_case "without the test seams a planning turn runs the script beside it too"
to_plan
bare_tree
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --brief $record.brief"
for name in tehas tehasbot tehas-plan; do t_eq "$(t_calls "$name")" ""; done

t_case "a unit systemd cannot load changes no label and starts nothing, exit 1"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo not-found ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: systemd cannot load tehas-cycle@.service (LoadState: not-found): run install.sh and its units step, and run tehas dispatch as the tehas user's own session"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls systemctl)" "start"
t_eq "$(t_calls fake-index)" ""

t_case "a systemd that gives no answer changes no label and starts nothing, exit 1"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") exit 1 ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: systemd cannot load tehas-cycle@.service (LoadState: no answer): run install.sh"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls systemctl)" "start"

# --- The slots are filled ---------------------------------------------------
new_rukas=$(printf 'a%.0s' {1..40})

t_case "a pi-rukas no cycle has run before starts as many cycles as the slots allow"
reset
slots 4
echo "$new_rukas|1" >"$fx/image.label"
ready 7 8 9 10 11 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 10 "
t_lacks "$t_out" "is new on this host"
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
rm -f "$fx/image.label"

t_case "with one cycle running and five waiting, one poll starts three"
reset
slots 4
ready 7 8 9 10 11
hold 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 "
t_lacks "$t_out" "holding"
release

# A running cycle whose lock a process holds, which fake-stop ends as
# systemctl stop ends the unit. It took its lock now.
hold_proc() { # <issue>
  mkdir -p "$T/home/tehas/state/cycles"
  : >"$T/home/tehas/state/cycles/$1.taken"
  flock -o "$T/home/tehas/state/cycles/$1.lock" sleep 30 >/dev/null 2>&1 &
  echo "$!" >"$fx/holder-$1"
  for _ in $(seq 50); do
    flock -n "$T/home/tehas/state/cycles/$1.lock" true || break
    sleep 0.1
  done
}
stop_by_kill() {
  t_stub fake-stop 'kill "$(cat "$T/fx/holder-$1")"'
  T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
}
bot_pr() { # <issue> <state> <created at>
  jq -n --arg n "$1" --arg st "$2" --arg at "$3" '[{url: "https://github.com/acme/widget/pull/90", state: $st,
    body: "Fixes #\($n)", headRefName: "feature/issue-\($n)-x", createdAt: $at}]' >"$fx/prlist.json"
}

t_case "a running cycle whose issue is closed is stopped, and its slot is filled on the same poll"
reset
stop_by_kill
ready 7
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_contains "$t_out" "stop #9: its issue is closed"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" closed
t_eq "$(t_calls fake-start)" "7"

t_case "a running cycle whose pull request is merged is stopped, and its slot is filled on the same poll"
reset
stop_by_kill
ready 7
hold_proc 9
bot_pr 9 MERGED 2099-01-01T00:00:00Z
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_contains "$t_out" "stop #9: its pull request https://github.com/acme/widget/pull/90 is merged"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" merged
t_eq "$(t_calls fake-start)" "7"

t_case "a pull request closed without a merge stops the cycle too"
reset
stop_by_kill
echo '[]' >"$fx/ready.json"
hold_proc 9
bot_pr 9 CLOSED 2099-01-01T00:00:00Z
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" pr-closed

t_case "an open pull request, one of an earlier cycle, or one of another issue stops nothing"
reset
stop_by_kill
ready 7
hold_proc 9
for pr in "9 OPEN 2099-01-01T00:00:00Z" "9 MERGED 2000-01-01T00:00:00Z" "8 MERGED 2099-01-01T00:00:00Z"; do
  # shellcheck disable=SC2086  # three words
  bot_pr $pr
  t_run "$dispatch --dry-run"
  t_lacks "$t_out" "would stop"
done
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"
kill "$(cat "$fx/holder-9")"

t_case "a dry run says it would stop a finished cycle, and stops nothing"
reset
stop_by_kill
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: its issue is closed"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(find "$T/home/tehas/state/cycles/9.done" 2>/dev/null | wc -l)" 0
kill "$(cat "$fx/holder-9")"

t_case "with no cycle running the bot's pull requests are not asked for"
reset
t_run "$dispatch"
t_lacks "$(t_calls gh)" "pr list"

t_case "a poll that starts fewer than it could logs exactly one reason, on every such poll"
reset
slots 4 TEHAS_MAX_AWAITING=3
echo '[{"number":1}]' >"$fx/awaiting.json"
ready 7 8 9 10 11 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_eq "$(grep -c 'holding' <<<"$t_out")" 1
t_contains "$t_out" "holding: 2 of 4 free slots start: 1 issues await merge and 0 cycles are running (limit 3)"
t_eq "$(jq -r .why "$dispatch_json")" awaiting
rm -f "$T"/calls/fake-start
rm -rf "$T/home/tehas/state/cycles"
t_run "$dispatch"
t_eq "$(grep -c 'holding' <<<"$t_out")" 1

t_case "the slice's room, when it is the limit, is the one reason"
reset
slots 4
other $((15 * gib))
ready 7 8 9 10
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_eq "$(grep -c 'holding' <<<"$t_out")" 1
t_contains "$t_out" "holding: 2 of 4 free slots start: tehas.slice has room for 2 more"
t_eq "$(jq -r .why "$dispatch_json")" room

t_case "a poll that starts as many as wait says no reason"
reset
slots 4 TEHAS_MAX_AWAITING=3
echo '[{"number":1}]' >"$fx/awaiting.json"
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$t_out" "holding"
t_eq "$(jq -r .why "$dispatch_json")" ""

# --- A cycle a provider outage parked -------------------------------------------
cycles_dir=$T/home/tehas/state/cycles
# Issue 7 was parked by an outage after a cycle alice started at 02:00, its
# question unticked, and the endpoint answers again.
outage_parked() { # [<retry>]
  parked
  comments 7 "$(park_body ' ' ' ')" ""
  mesh '{"data":[{"id":"test-model"}]}'
  mkdir -p "$cycles_dir"
  jq -n --argjson s "$(date -d 2026-01-01T02:00:00Z +%s)" --argjson r "${1:-true}" \
    '{cap: "step-failed:adversarial", reason: "Connection error.", provider: true, retry: $r, started: $s}' >"$cycles_dir/7.park"
  echo alice >"$cycles_dir/7.by"
  echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
}

t_case "a cycle a provider outage parked starts again once, without a tick, and the issue is told"
outage_parked
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label needs-human-attention --remove-label tehas:question"
t_contains "$(t_calls gh)" "Tehas starts this issue again on its own: its last cycle was parked by a provider outage, and the model endpoint answers again. It does this once."
t_contains "$(t_calls gh)" "authorized by @alice's start of the cycle a provider outage parked"
t_contains "$t_out" "start #7 (requeued by alice)"
t_lacks "$(t_calls gh)" "api -X PATCH"

t_case "the park is used up, and the start is marked as the retry for its cycle"
t_eq "$(find "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
t_eq "$(find "$cycles_dir/7.requeued" 2>/dev/null | wc -l)" 1
t_eq "$(cat "$cycles_dir/7.by")" alice
t_eq "$([ -e "$park_id" ] && echo kept)" ""
t_eq "$(jq -r '"\(.how) \(.by) \(.at)"' "$cycles_dir/7.approved.json")" "outage-retry alice 2026-01-01T02:00:00Z"

t_case "a start a person asked for is not the retry, and says who asked"
reset
mkdir -p "$cycles_dir"
: >"$cycles_dir/7.requeued"
echo '{"retry":true}' >"$cycles_dir/7.park"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(find "$cycles_dir/7.requeued" "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
t_eq "$(cat "$cycles_dir/7.by")" alice

t_case "a park that asks for no retry starts nothing and stays"
outage_parked false
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1

t_case "the retry waits while the endpoint does not answer, or the slots are full, and its park stays"
outage_parked
mesh
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1
outage_parked
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1
release

t_case "a dry run says what it would do and changes nothing"
outage_parked
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry after a provider outage, started by alice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1

t_case "no retry when its starter is no longer allowed, the text was changed by another, or the issue is not parked"
for why in stranger edited closed unparked nobody; do
  outage_parked
  case "$why" in
    stranger) echo mallory >"$cycles_dir/7.by" ;;
    nobody) rm -f "$cycles_dir/7.by" ;;
    edited) echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json" ;;
    closed) echo '{"state":"CLOSED","labels":[{"name":"needs-human-attention"}]}' >"$fx/issue-7.json" ;;
    unparked) echo '{"state":"OPEN","labels":[]}' >"$fx/issue-7.json" ;;
  esac
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
  t_eq "$(find "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
done

t_case "an issue a person labelled tehas:ready is left to that label"
outage_parked
echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:ready"}]}' >"$fx/issue-7.json"
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (labelled by alice)"
t_eq "$(find "$cycles_dir/7.requeued" "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
rm -f "$T/home/.pi/agent/models.json" "$fx/mesh.json"

# Kept workspaces. Issue 5's were last used eight days ago, issue 6's today.
works=$T/home/tehas/work
kept() { # <issue> <age, as touch -d takes it> [what is beside it]
  local p
  mkdir -p "$works" "$T/home/tehas/state/cycles"
  : >"$works/.tehas-workspaces"
  for p in "$works/widget-$1" "${@:3}"; do
    mkdir -p "$p"
    touch -d "$2" "$p"
  done
  touch -d "$2" "$T/home/tehas/state/cycles/$1.lock"
}
old() { kept "$1" "8 days ago" "$works/widget-$1.before-retry" "$works/widget-$1.images" "$works/widget-$1.state"; }
left() { find "$works" -mindepth 1 -maxdepth 1 ! -name .tehas-workspaces -printf '%f\n' | sort | tr '\n' ' '; }

t_case "a workspace no cycle has used for seven days goes, with what is beside it, and that is logged"
reset
rm -rf "$works"
old 5
kept 6 "1 day ago" "$works/widget-6.before-retry"
mkdir -p "$works/widget-5x" "$works/other-5"
touch -d "8 days ago" "$works/widget-5x" "$works/other-5"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "other-5 widget-5x widget-6 widget-6.before-retry "
t_contains "$t_out" "removed the kept workspace of #5: no cycle has run on it for 7 days"

t_case "a workspace whose issue's lock is held is kept, however old"
reset
rm -rf "$works"
old 5
hold 5
touch -d "8 days ago" "$works/widget-5" "$T/home/tehas/state/cycles/5.lock"
t_run "$dispatch"
release
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_lacks "$t_out" "removed the kept workspace"

t_case "a workspace moved aside recently keeps the issue's workspaces"
reset
rm -rf "$works"
old 5
touch "$works/widget-5.before-retry"
t_run "$dispatch"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "nothing goes from a directory tehas work did not mark"
reset
rm -rf "$works"
old 5
rm "$works/.tehas-workspaces"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "a dry run says what would go and removes nothing"
reset
rm -rf "$works"
old 5
t_run "$dispatch --dry-run"
t_contains "$t_out" "would remove the kept workspace of #5: no cycle has run on it for 7 days"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "a poll removes the workspaces of three issues at most"
reset
rm -rf "$works"
for n in 2 3 4 5; do kept "$n" "8 days ago"; done
t_run "$dispatch"
t_eq "$(left)" "widget-5 "
t_run "$dispatch"
t_eq "$(left)" ""
rm -rf "$works"


# A cycle on #9 whose live file was last written <age> seconds ago, and that
# took its lock <taken> seconds ago.
quiet_cycle() { # <age> <taken>
  hold 9
  mkdir -p "$T/home/tehas/live/cycles"
  printf '{"issue":9,"running":true,"updatedAt":%s}\n' "$(($(date +%s) - $1))" >"$T/home/tehas/live/cycles/9.json"
  touch -d "@$(($(date +%s) - $2))" "$T/home/tehas/state/cycles/9.taken"
}

t_case "a cycle that holds its lock and has not been heard from for 20 minutes is reported and stopped, once"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #9 holds its lock but has not been heard from since "
t_contains "$t_out" "more than 1200s: stopping tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
t_run "$dispatch"
t_lacks "$t_out" "#9 holds its lock"
t_eq "$(t_calls fake-stop)" "9"
release

t_case "a stop that fails is said"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"; exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_contains "$t_out" "WARN: could not stop tehas-cycle@9.service"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
release

t_case "a cycle heard from lately, or one that took its lock lately, is left alone"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 20 3600
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
release
# A live file left by the issue's last cycle, and a new one still starting.
quiet_cycle 86400 60
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a dry run says it would stop a quiet cycle, every time, and stops nothing"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: the cycle for #9 holds its lock but has not been heard from since "
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" ""
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
release

t_done
