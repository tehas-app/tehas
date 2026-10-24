# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # snippets are expanded by the shell under test; the parts read the variables
# Shared setup of the dispatch_*_test.sh files: libexec/dispatch against a stub
# gh and a scratch checkout whose default branch is main. Sourced, not run.
# The part has sourced test/lib.sh before this.

dispatch="$T_ROOT/libexec/dispatch"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget

# gh answers from fixture files; every call is recorded by t_stub.
#
# t_stub writes this body through printf, so the shell expands it as it is
# written: every $ and every backslash in it is consumed then, not when the
# stub runs. So a case pattern matches what $1, $2 and $* hold at run time but
# may not name them itself, and the arguments past those are read through the
# names assigned here.
t_stub gh '
# rest_verb, rest_path: the verb and the path of a call of the form
# gh api -X <verb> <path> ... The words of the call reach this body only
# through a name given here: the plain positional parameters of the body are
# filled in when t_stub writes the stub, so only $1, $2 and $* can be read
# directly.
rest_verb=$3
rest_path=$4
# fails <fixture> [<fixture of a countdown>]: exit 1 when the first names an
# outage, or while the second holds a count above 0, which it lowers.
fails() {
  local left
  if [ -s "$T/fx/${2:-none}" ]; then
    left=$(cat "$T/fx/$2")
    [ "$left" -gt 0 ] || return 0
    echo $((left - 1)) >"$T/fx/$2"
    exit 1
  fi
  [ ! -e "$T/fx/$1" ] || exit 1
}
# not_found: say what GitHub says for a path it does not have, and fail.
not_found() { echo "message: Not Found" >&2; exit 1; }
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
      *tehas:refine*) cat "$T/fx/refines.json" 2>/dev/null || echo "[]" ;;
      *) echo "[]" ;;
    esac ;;
  "api user") cat "$T/fx/user.json" || exit 1 ;;
  # The label of a pull request, through the REST issues endpoint: a mark adds
  # it, an unmark takes it off. A mark fails while $fx/mark-fails-times counts
  # down, or at once when the file is there but holds nothing, so a case can
  # say the first two marks fail and the third works. An unmark fails on
  # $fx/unmark-fails. $fx/label-missing makes either answer 404, as GitHub
  # does for a label the repository has not got.
  # A call of gh api -X. The path of a label call is the fourth argument and
  # the verb the third, both read through the names assigned above: $1 and $2
  # of the case hold only "api" and "-X", and a pattern may not name them.
  # A mark adds tehas:refreshing to a pull request and an unmark takes it off,
  # both through the REST issues endpoint. A mark fails while
  # $fx/mark-fails-times counts down, or at once when that file is there but
  # holds nothing, so a case can say the first two marks fail and the third
  # works. An unmark fails on $fx/unmark-fails. $fx/label-missing makes either
  # answer 404, as GitHub does for a label the repository has not got.
  "api -X")
    case "$rest_verb ${rest_path%%:*}" in
      "POST repos/acme/widget/issues/"*/labels)
        [ ! -e "$T/fx/label-missing" ] || not_found
        fails mark-fails mark-fails-times ;;
      "DELETE repos/acme/widget/issues/"*/labels/*)
        [ ! -e "$T/fx/label-missing" ] || not_found
        fails unmark-fails ;;
      *) fails patch-fails ;;
    esac ;;
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
  # The check runs of a name on the head commit of the branch.
  "api repos/acme/widget/commits/"*)
    [ ! -e "$T/fx/checks-fails" ] || exit 1
    [ ! -e "$T/fx/checks-hangs" ] || exec sleep 20
    cat "$T/fx/checks.json" 2>/dev/null || echo "{\"total_count\":0,\"check_runs\":[]}" ;;
  "api graphql")
    n=$(printf "%s\n" "$@" | sed -n "s/^number=//p")
    case "$*" in
      *"comments(last"*) cat "$T/fx/comments-$n.json" || exit 1 ;;
      # The states of the issues a tehas:blocked comment names.
      # One pull request, as a refresh reads it at the start.
      *"pullRequest(number"*) cat "$T/fx/pull-$n.json" || exit 1 ;;
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
    # And the approval on record at the moment an issue was marked as running,
    # as tehas_approval_view shapes it: .approval the one waiting, .current the
    # one of the last start.
    case "$*" in *"--add-label tehas:running"*) TEHAS_HOME=$T/home/tehas "$T/store-bin" approval-current "$3" >"$T/calls/approved.at-mark" 2>/dev/null ;; esac
    [ ! -e "$T/fx/edit-fails" ] || exit 1
    # An edit whose arguments contain the text in edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(cat "$T/fx/edit-fails-on")"*) exit 1 ;; esac
    fi ;;
  "pr list") [ ! -e "$T/fx/pr-fails" ] || exit 1
    # The pull requests marked tehas:refreshing.
    case "$*" in *tehas:refreshing*) cat "$T/fx/refreshing.json" 2>/dev/null || echo "[]"; exit 0 ;; esac
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
# cycle says it holds its lock by starting its record on the state store,
# under the lock: this one does the same with the real store, and lets go.
# It fails or stays silent when a case asks it to, and with start-refused
# ends the record as a cycle that refused its number does.
default_start() {
  t_stub fake-start '[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ ! -e "$T/fx/start-silent" ] || exit 0
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >>"$T/calls/cycle-start" 2>&1 || exit 0
[ ! -e "$T/fx/start-refused" ] ||
  flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-end "$1" --attempt "$(TEHAS_HOME=$TEHAS_HOME "$T/store-bin" cycle-show "$1" | jq .cycle.attempt)" --refused >/dev/null 2>&1'
}
default_start
t_stub fake-status
# Stands in for libexec/tidy, and notes whether a cycle had started before it.
t_stub fake-tidy '[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/tidy.after-start"'
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
# Runs the index at once, as a unit that ends before the poll looks again,
# unless the fixture leaves it running: then it starts nothing.
t_stub fake-index-start '[ ! -e "$T/fx/index-runs" ] || exit 0
"$@"'
# The slice as systemd and Docker describe it, from fixture files. Its use is
# the scratch cgroup tree of test/lib.sh.
systemctl_stub='case "$1 $2" in
  "show tehas.slice")
    [ ! -e "$T/fx/slice-hangs" ] || exec sleep 20
    cat "$T/fx/slice.show" ;;
  "--user show")
    if [ -e "$T/fx/plan-runs" ] && [ "${6:-}" = "tehas-plan-$(cat "$T/fx/plan-runs").service" ]; then echo active; else echo inactive; fi ;;
esac'
t_stub systemctl "$systemctl_stub"
t_stub docker 'case "$1" in
  info) case "$*" in
      # The two questions the host asks Docker, both answered from
      # $T/fx/docker.info: lib/slice.sh wants the cgroup driver and version,
      # the first line, and the disk hold wants the directory of the images,
      # the second line when a case wrote one. With $T/fx/docker-down the
      # question is not answered at all, which is a host without Docker: df is
      # then pointed at the default, /var/lib/docker, and a figure is still
      # read or not read from there.
      *DockerRootDir*)
        [ ! -e "$T/fx/docker-down" ] || exit 1
        [ -e "$T/fx/docker-dir" ] || sed -n 2p <"$T/fx/docker.info" 2>/dev/null ;;
      *) cat "$T/fx/docker.info" ;;
    esac ;;
  image) [ -e "$T/fx/image.label" ] || exit 1; cat "$T/fx/image.label" ;;
  rm) [ ! -e "$T/fx/orphan-$3" ] || echo "$3" ;;
  run) [ -e "$T/fx/caps.json" ] || exit 1; cat "$T/fx/caps.json" ;;
esac'
# The free kilobytes of the host, as the disk hold reads it through
# lib/disk.sh, whose df and docker info calls run under timeout. The file
# system asked about answers with the kilobytes named for it: those of Docker's
# own come from $T/fx/free-docker and the ones of every other path from
# $T/fx/free, so a case can tell the two file systems from each other. With
# none of them written there is 40 GB free everywhere. A case makes a reading
# fail with $T/fx/df-fails, which takes both file systems with it, names an
# unreadable one in $T/fx/df-fails-of, asks that Docker not answer the
# directory question at all in $T/fx/docker-down, or puts a word that is no
# number in a file: a figure that is not a number is no reading either, and
# neither is one that does not fit a 64-bit number. docker info answers from
# $T/fx/docker.info: one line with Docker's cgroup driver and version, or a
# second line with the directory of its images. A figure is checked with a
# pattern match rather than a case pattern, because t_stub expands the body as
# it writes the stub and would eat the quotes an empty pattern needs.
kb() { # <gibibytes>: the kilobytes of that many GiB
  printf '%s\n' "$((10#${1} * 1048576))"
}
# One df of the file system named by its second argument.
t_stub df '
[ ! -e "$T/fx/df-fails" ] || exit 1
[ ! -e "$T/fx/df-fails-of" ] || [ "${2%/}" = "$(cat "$T/fx/df-fails-of")" ] || exit 1
docker_dir=$(sed -n 2p <"$T/fx/docker.info" 2>/dev/null)
if [ -e "$T/fx/docker-dir" ]; then docker_dir=""; fi
answer=$(cat "$T/fx/free" 2>/dev/null || echo 41943040)
if [ -n "$docker_dir" ] && [ "${2%/}" = "$docker_dir" ]; then
  answer=$(cat "$T/fx/free-docker" 2>/dev/null)
fi
[[ $answer =~ ^(0|[1-9][0-9]{0,17})$ ]] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $answer 1% /"'
df_reset() {
  rm -f "$fx/free" "$fx/free-docker" "$fx/df-fails" "$fx/df-fails-of" "$fx/docker-dir" "$fx/docker-down"
  # The directory of the images goes back with the figures: a case that names
  # one with df_free_of leaves it to no later case.
  echo "systemd 2" >"$fx/docker.info"
}
df_free() { # <gibibytes>: the same figure on every file system
  df_reset
  kb "$1" >"$fx/free"
}
# df_free_of <gibibytes of ~/tehas> <gibibytes of Docker's data>: the two file
# systems asked about answer differently. The lesser of them is the one that
# holds a cycle back. Docker names the directory of its images and df is then
# pointed at it, /srv/docker, which puts the two on file systems of their own:
# an answer that is no figure is no reading for either of them.
df_free_of() {
  df_reset
  printf '%s\n' "systemd 2" /srv/docker >"$fx/docker.info"
  kb "$1" >"$fx/free"
  kb "$2" >"$fx/free-docker"
}
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
# No case leaves the old file of a record that is on the store.
t_after='t_no_old_state "$T/home/tehas"'

reset() {
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-* "$fx"/events-* "$fx"/comments-* "$fx"/icomments-* "$fx"/prs.json "$fx"/prlist.json "$fx"/pr-fails "$fx"/issue-* "$fx"/work-* "$fx"/post-fails "$fx"/patch-fails "$fx"/plan-fails "$fx"/plan-runs "$fx"/slice-* "$fx"/orphan-* "$fx"/prs-* "$fx"/blockers* "$fx"/checks* "$fx"/mark-fails "$fx"/mark-fails-times "$fx"/unmark-fails "$fx"/label-missing
  rm -rf "$T/home/tehas/libexec" "$T/home/tehas/state/cycles" "$T"/home/tehas/state/plan-* "$T"/home/tehas/state/park-* "$T/home/tehas/live" "$T/home/tehas/state/pause.json" "$T/home/tehas/state/limit.json" "$T/home/tehas/state/branch-check"
  # The state store a poll runs, with nothing on record.
  t_store_home "$T/home/tehas"
  # A slice of 20 GiB with nothing in it: room for ten cycles of the default
  # reserve, so the slots and the review cap decide unless a case says more.
  slice_show $((20 * gib))
  echo "systemd 2" >"$fx/docker.info"
  df_reset
  slice_state 0.00
  echo '[]' >"$fx/asking.json"
  echo '[]' >"$fx/plans.json"
  echo '[]' >"$fx/refines.json"
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
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_TIDY_CMD=fake-tidy TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_START_CMD=fake-plan-start TEHAS_DISK_WAIT=1 "$slice_env")
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

plan_body() { # <box 1> <box 2> <end box> [<turn> [<line of the host's own>]]
  # printf makes the blank line around the host's line: what a caller builds
  # with $( ) arrives without the line ends it held.
  local extra=
  [ -z "${5:-}" ] || extra="${5}

"
  printf 'Two ways.\n\nAdopt?\n\n%s- [%s] Adopt all\n  Apply all five\n- [%s] Change some\n- [%s] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=%s -->\n' \
    "$extra" "$1" "$2" "$3" "${4:-1}"
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
# Issue 7 carries tehas:refine from an allowed account.
to_refine() {
  to_plan
  echo '[]' >"$fx/plans.json"
  echo '[{"number":7,"labels":[{"name":"tehas:refine"}]}]' >"$fx/refines.json"
  echo '[{"event":"labeled","label":{"name":"tehas:refine"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
}
# Issue 7 is parked by a stopped cycle, and carries the ask for a planning
# session on top: what a person does when the cycle's outcome was not what they
# wanted. Its park question is on record with its boxes unticked, and it is the
# one issue with a question on it.
parked_plan() { # <plan|refine> [<login, alice by default>]
  local ask=$1 by=${2:-alice} l
  case "$ask" in plan) l=tehas:plan ;; *) l=tehas:refine ;; esac
  parked
  jq -n --arg l "$l" '[{number: 7,
    labels: [{name: "needs-human-attention"}, {name: "tehas:question"}, {name: $l}]}]' >"$fx/asking.json"
  jq -n --arg l "$l" --arg e "$by" \
    '[{event: "labeled", label: {name: $l}, actor: {login: $e}}]' >"$fx/events-7.json"
  echo '[]' >"$fx/ready.json"
  echo '[]' >"$fx/plans.json"
  echo '[]' >"$fx/refines.json"
  # As gh lists them: an issue asked to plan carries the ask beside its
  # question, and the filter of step 5 reads both from one list.
  case "$ask" in
    plan) echo '[{"number":7,"labels":[{"name":"tehas:plan"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/plans.json" ;;
    *) echo '[{"number":7,"labels":[{"name":"tehas:refine"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/refines.json" ;;
  esac
  echo '{"title":"Faster search","body":"Make it fast."}' >"$fx/issue-7.json"
  comments 7 "$(park_body ' ' ' ')" ""
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
# What the fake runner writes for the turn it is asked to run: the agent's
# words and its question, beside the issues the turn filed. With no url the
# turn filed nothing and the outcome has no filed key.
plan_files() { # <url>...
  rm -f "$fx/plan-fails" "$fx/plan-result.json"
  jq -cn --arg text "Two ways." \
    --argjson urls "$(printf '%s\n' "$@" | jq -Rs 'split("\n") | map(select(. != ""))')" '
    {session: "abc-123", text: $text,
     question: {question: "Adopt?", options: [
       {label: "Adopt all", description: "Apply all five"},
       {label: "Change some", description: ""}]}}
    + (if ($urls | length) == 0 then {} else {filed: $urls} end)' >"$fx/plan-result.json"
}
# The question of a session whose last turn filed <url>..., as the host posted
# it: plan_body with the line naming them between the question and the boxes.
plan_filed_body() { # <turn> <url>...
  local n=$1 urls=""
  [ "$#" = 1 ] || urls=$(printf '%s\n' "${@:2}" | jq -Rrs 'split("\n") | map(select(. != "")) | join(", ")')
  plan_body ' ' x ' ' "$n" "${urls:+Filed in this turn: $urls}"
}
# A product repository whose default branch is main, and the instance's clone.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

# at_mark [<jq filter>]: the approval that waited on record when the issue was
# marked running: its time as GitHub writes one, its fields and its inputs.
# Keys sorted, as the store keeps the inputs: compare with sorted().
at_mark() {
  jq -crS "(.approval | if . == null then null else {at: (.at | todate), by, how, label, body_sha256, title_sha256,
    comment_id, refresh_pr, refresh_head} + (.input // {}) end) | ${1:-.}" "$T/calls/approved.at-mark" 2>&1
}
# past <issue> <by> <how> <at> [<cycle-end args>...]: a cycle of the issue on
# record that has ended, started on <by>'s approval of that how, given at the
# time <at> (as GitHub writes one).
past() {
  local n=$1 by=$2 how=$3 at a home=$T/home/tehas
  at=$(date -u -d "$4" +%s)
  shift 4
  a=$(t_row cycle-show "$n" -- '.cycle.attempt // 0')
  t_approve "$home" "$n" "$by" "$how" "$at"
  t_started "$home" "$n"
  TEHAS_HOME=$home flock -w 5 "$home/state/cycles/$n.lock" "$T_STORE" cycle-end "$n" --attempt $((a + 1)) \
    --status "${PAST_STATUS:-handoff}" "$@" >/dev/null || t_bad "past: the store refused the end of #$n"
}
# sorted <json>: the same value with its keys sorted, on one line.
sorted() { jq -cS . <<<"$1"; }
# approval [<jq filter> [<issue>]]: what the store has of the approvals of
# issue 7 [.current, the one its last start used], in the shape
# tehas_approval_view gives the readers (at as a time, rests_on, the inputs).
approval() {
  local t_got
  t_got=$(TEHAS_HOME=$T/home/tehas bash -c '. "$1"; f=$(tehas_approval_view "$2") && cat "$f" && rm -f "$f"' _ \
    "$T_ROOT/lib/store.sh" "${2:-7}") || { echo none; return 0; }
  jq -cr "${1:-.}" <<<"$t_got"
}

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

# queue "<issue> [label...]"...: the ready list, each issue labelled ready by
# alice unless its events are written after.
queue() {
  local arg list=""
  for arg in "$@"; do
    list+=$(jq -nc --arg a "$arg" '($a | split(" ")) as $w | {number: ($w[0] | tonumber),
      labels: ([{name: "tehas:ready"}] + [$w[1:][] | {name: .}])}')$'\n'
    jq -n '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: "alice"}, created_at: "2026-10-01T00:00:00Z"}]' \
      >"$fx/events-${arg%% *}.json"
  done
  jq -s . <<<"$list" >"$fx/ready.json"
}
# label_events <issue> <label>=<login>...: tehas:ready by alice, then each
# label applied in turn, a minute apart. "-<label>=<login>" takes it off.
label_events() {
  local n=$1
  shift
  printf '%s\n' "$@" | jq -Rn '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: "alice"}, created_at: "2026-10-01T00:00:00Z"}]
    + [[inputs] | to_entries[] | .value as $v | ($v | ltrimstr("-") | split("=")) as $p
       | {event: (if ($v | startswith("-")) then "unlabeled" else "labeled" end), label: {name: $p[0]},
          actor: (if $p[1] == "" then null else {login: $p[1]} end), created_at: "2026-10-01T00:0\(.key + 1):00Z"}]' \
    >"$fx/events-$n.json"
}
started() { t_calls fake-start | tr '\n' ' '; }

# The demand: how many issues would start if there were room, written on every
# poll for tehas status.
demand_file=$T/home/tehas/live/demand.json

# The limit tehas limit writes.
limit_file=$T/home/tehas/state/limit.json
limit() { # <cycles> [<plan true|false>] [<until or "">] [<reason>]: the limit tehas limit writes
  mkdir -p "${limit_file%/*}"
  jq -nc --argjson c "$1" --argjson p "${2:-true}" --arg u "${3:-}" --arg r "${4:-}" \
    '{v: 1, cycles: $c, plan: $p, until: (if $u == "" then null else $u end), reason: $r}' >"$limit_file"
}

# Issue 7's body is "Words." in the fixer's and the refresh's cases.
WSUM=$(printf 'Words.' | sha256sum | cut -d' ' -f1)
