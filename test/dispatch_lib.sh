# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # snippets are expanded by the shell under test; the parts read the variables
# Shared setup of the dispatch_*_test.sh files: libexec/dispatch against a stub
# gh and a scratch checkout whose default branch is main. Sourced, not run.
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
        # One number alone, for a poll that cannot read that issue.
        [ ! -e "$T/fx/work-$n-fails" ] || exit 1
        if [ -e "$T/fx/work-$n.json" ]; then cat "$T/fx/work-$n.json"; else
          jq -c --argjson n "$n" "{number: \$n, state: \"open\", labels: (if any(.[]?; type == \"object\" and .number == \$n)
            then [{name: \"tehas:running\"}] else [] end)}" "$T/fx/marked.json" 2>/dev/null || echo "{\"number\":$n,\"state\":\"open\"}"
        fi ;;
    esac ;;
  "api graphql")
    n=$(printf "%s\n" "$@" | sed -n "s/^number=//p")
    case "$*" in
      *"comments(last"*) cat "$T/fx/comments-$n.json" || exit 1 ;;
      # The states of the issues a tehas:blocked comment names.
      *issueOrPullRequest*) [ ! -e "$T/fx/blockers-fail" ] || exit 1
        cat "$T/fx/blockers.json" || exit 1 ;;
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
  "pr list") [ ! -e "$T/fx/pr-fails" ] || exit 1
    # gh filters by --author before it prints, and what --json lists names no
    # author at all. A fixture that does name one stands for a pull request
    # GitHub left out: one of another account, or one with no account behind
    # it (a deleted one), which --author matches no name of.
    if [ -e "$T/fx/prlist.json" ]; then
      au=$(printf "%s\n" "$@" | sed -n "/^--author$/{n;p;}")
      jq -c --arg au "$au" "[.[] | select((has(\"author\") | not)
        or ((.author.login // \"\") == \$au))]" "$T/fx/prlist.json"
    else echo "[]"; fi ;;
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
# Runs the turn at once, as a unit that ends before the poll looks again, unless
# the fixture leaves it running: then it starts nothing, and systemctl says the
# unit is active.
t_stub fake-plan-start 'n=$1; shift
[ ! -e "$T/fx/plan-runs" ] || exit 0
"$@"'
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
  "--user show")
    if [ -e "$T/fx/plan-runs" ] && [ "${6:-}" = "tehas-plan-$(cat "$T/fx/plan-runs").service" ]; then echo active; else echo inactive; fi ;;
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
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-* "$fx"/events-* "$fx"/comments-* "$fx"/icomments-* "$fx"/prs.json "$fx"/prlist.json "$fx"/pr-fails "$fx"/issue-* "$fx"/work-* "$fx"/post-fails "$fx"/patch-fails "$fx"/plan-fails "$fx"/plan-runs "$fx"/slice-* "$fx"/orphan-* "$fx"/prs-* "$fx"/blockers*
  rm -rf "$T/home/tehas/state/cycles" "$T"/home/tehas/state/plan-* "$T"/home/tehas/state/park-* "$T/home/tehas/live" "$T/home/tehas/state/pause.json"
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
  # The control issue has never been paused: no tehas:pause event to read.
  echo '[]' >"$fx/events-1.json"
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_START_CMD=fake-plan-start "$slice_env")
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

# The approval record of a start on issue 7.
approved_file=$T/home/tehas/state/cycles/7.approved.json

# icomment <id> <login> <created> <updated> <body>: one comment as REST lists it.
icomment() {
  jq -n -c --argjson id "$1" --arg who "$2" --arg c "$3" --arg u "$4" --arg body "$5" \
    '{id: $id, user: {login: $who}, created_at: $c, updated_at: $u, body: $body}'
}

# The flag tehas pause writes on the host. It decides on its own, and GitHub is
# asked nothing about it.
pause_flag() { # <json>: what to write where tehas pause writes it
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$1" >"$T/home/tehas/state/pause.json"
}

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
mesh_curl() {
  t_stub curl 'case "$*" in
  *"/chat/completions") cat >"$T/calls/curl-body"; [ -e "$T/fx/reads-images" ] ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac'
}

# Where a cycle's files are kept.
cycles_dir=$T/home/tehas/state/cycles
