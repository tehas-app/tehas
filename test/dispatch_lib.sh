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
    left=$(<"$T/fx/$2")
    [ "$left" -gt 0 ] || return 0
    echo $((left - 1)) >"$T/fx/$2"
    exit 1
  fi
  [ ! -e "$T/fx/$1" ] || exit 1
}
# The write log of the host shim (host/shim/gh), when a case keeps one: a line for
# each write, in the format of the shim.
if [ -e "$T/home/tehas/state/github-writes.log" ]; then
  case "$1 $2" in
    "issue edit" | "issue comment" | "api -X") printf "%s\t%s %s\tacme/widget\t-\t0\t-\n" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2" >>"$T/home/tehas/state/github-writes.log" ;;
  esac
fi
# not_found: say what GitHub says for a path it does not have, and fail.
not_found() { echo "message: Not Found" >&2; exit 1; }
case "$1 $2" in
  "repo view") if [ -e "$T/fx/gh-stderr-fails" ]; then
      cat "$T/fx/gh-stderr" >&2 2>/dev/null; exit 1; fi
    cat "$T/fx/repo.json" || exit 1 ;;
  "issue view") cat "$T/fx/issue-$3.json" 2>/dev/null || cat "$T/fx/control.json" || exit 1 ;;
  "issue list")
    case "$*" in
      *awaiting-merge*) cat "$T/fx/awaiting.json" || exit 1 ;;
      *tehas:ready*) cat "$T/fx/ready.json" || exit 1 ;;
      *tehas:blocked*) cat "$T/fx/blocked.json" 2>/dev/null || echo "[]" ;;
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
      # A question as the Rust poll posts it, with its method said.
      "POST repos/acme/widget/issues/"*/comments) [ ! -e "$T/fx/post-fails" ] || exit 1; echo 4002 ;;
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
      # The open issues in one page, as tehas-dispatch asks for them.
      *"issues(states: OPEN"*) if [ -e "$T/fx/gh-stderr-fails" ]; then
          cat "$T/fx/gh-stderr" >&2 2>/dev/null; exit 1; fi
        "$T/page-stub" || exit 1 ;;
      # Or the REST answer a case wrote, as GraphQL gives it.
      *"comments(last"*) cat "$T/fx/comments-$n.json" 2>/dev/null ||
        jq "{data: {repository: {issue: {comments: {nodes: [.[] | {databaseId: .id, author: {login: .user.login},
          body, createdAt: .created_at, lastEditedAt: (if .updated_at == .created_at then null else .updated_at end),
          editor: (if .updated_at == .created_at then null else {login: .user.login} end)}]}}}}}" "$T/fx/icomments-$n.json" || exit 1 ;;
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
    # What the host had on record at the moment an issue was marked as planning:
    # the session as the store gives it.
    case "$*" in *"--add-label tehas:planning"*) TEHAS_HOME=$T/home/tehas "$T/store-bin" plan-show "$3" >"$T/calls/record.at-mark" 2>/dev/null ;; esac
    # And the approval on record at the moment an issue was marked as running,
    # as tehas_approval_view shapes it: .approval the one waiting, .current the
    # one of the last start.
    case "$*" in *"--add-label tehas:running"*) TEHAS_HOME=$T/home/tehas "$T/store-bin" approval-current "$3" >"$T/calls/approved.at-mark" 2>/dev/null ;; esac
    [ ! -e "$T/fx/edit-fails" ] || exit 1
    # An edit whose arguments contain the text in edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(<"$T/fx/edit-fails-on")"*) exit 1 ;; esac
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
# Stands in for libexec/health, as these two commands are stood in for: a call
# of health is recorded and it answers 0. The poll runs health once at its
# start, and the real one is about 45 processes - five calls of the store and
# about eighteen of jq - which no case of these files reads. A case that looks
# at the health file, or at what health prints, names the real command with
# real_health.
t_stub fake-health
real_health() { T_ENV_ALWAYS+=(TEHAS_HEALTH_CMD="$T_ROOT/libexec/health"); }
# The dispatcher waits for a planning turn in steps of ten seconds. No case
# here needs that time to
# pass: the wait for a turn returns at once. Every other sleep is real, the
# stand-ins for a cycle among them: every hang fixture of this suite hangs by
# exec'ing sleep, so a bound is what keeps a poll from waiting for real. A case
# that wants a real wait to answer at once stubs sleep over, and names the
# command it hangs in $T/fx/timeout-hangs so its bound still holds.
t_stub sleep '[ "$1" = 10 ] || PATH=/usr/bin:/bin exec sleep "$@"'
# The real timeout, named once, for the stub of it below.
t_real_timeout=$(command -v timeout || true)
# Stands in for the real timeout, which sits around the calls of lib/store.sh,
# lib/disk.sh, lib/slice.sh, lib/analytics.sh, lib/attachments.sh and
# libexec/status, and around the poll's own git fetch, gh call, stop of a unit
# and run of the low-priority gate. The commands a poll of these tests runs are
# stubs, and a stub answers from a fixture: it runs no network, no daemon and
# nothing that waits, so a bound around it guards nothing a test can hang on,
# and the process that pays for that bound is the test's. Such a call is execed
# at once.
#
# A command that can hang is bounded, and the list below is what may hang: a
# name it keeps goes through the real timeout, with the seconds it was given,
# as on a host. Sleep is in it by its bare word because every hang fixture of
# this suite hangs by exec'ing sleep: the checks of the branch, the slice
# probe, the store that never answers and the gate that stays up. None of them
# stubs sleep, so each of those bounds holds. The others are the commands of
# the host a poll reaches for under timeout - df and docker through lib/disk.sh
# and lib/slice.sh, systemctl through lib/slice.sh, gh through libexec/tidy and
# the poll's own branch check, the fetch of the checkout, the gate of rule 7,
# the analytics program, the attachment checker and the store - which a case
# would hang by some means other than sleep.
#
# $T/fx/timeout-hangs adds a name to that list, one line each, for the case of
# a command the case stubs to hang which the list does not name: were sleep
# itself stubbed to answer at once, a bound checked through it would go
# unbounded. Production keeps every bound: this is a stub of a test.
t_timeout_kept=(df docker systemctl gh git curl sleep gate-run analytics attach)
# Built as one string so the list reaches the stub's body when the stub is
# written, not when a test calls it: the body is one single-quoted argument.
t_timeout_kept_str=" ${t_timeout_kept[*]} "
t_stub timeout '
name=${2##*/}
[ "$1" = -k ] && name=${4##*/}
hang=""
[ ! -s "$T/fx/timeout-hangs" ] || hang=$(<"$T/fx/timeout-hangs")
case "'"${t_timeout_kept_str}"' $hang " in
  *" $name "*) exec '"$t_real_timeout"' "$@" ;;
esac
[ "$1" != -k ] || shift 2
shift
exec "$@"'
# Writes a recorded outcome where the dispatcher asked for it. A failing turn
# leaves what the file plan-fails holds, if anything.
t_stub fake-plan 'TEHAS_HOME=$T/home/tehas "$T/store-bin" plan-show "$1" >"$T/calls/record.at-turn" 2>/dev/null
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
    if [ -e "$T/fx/plan-runs" ] && [ "${6:-}" = "tehas-plan-$(<"$T/fx/plan-runs").service" ]; then echo active; else echo inactive; fi ;;
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
[ ! -e "$T/fx/df-fails-of" ] || [ "${2%/}" = "$(<"$T/fx/df-fails-of")" ] || exit 1
docker_dir=$(sed -n 2p <"$T/fx/docker.info" 2>/dev/null)
if [ -e "$T/fx/docker-dir" ]; then docker_dir=""; fi
if [ -e "$T/fx/free" ]; then answer=$(<"$T/fx/free"); else answer=41943040; fi
if [ -n "$docker_dir" ] && [ "${2%/}" = "$docker_dir" ]; then
  answer=$(<"$T/fx/free-docker")
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
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-* "$fx"/events-* "$fx"/comments-* "$fx"/icomments-* "$fx"/prs.json "$fx"/prlist.json "$fx"/pr-fails "$fx"/issue-* "$fx"/work-* "$fx"/post-fails "$fx"/patch-fails "$fx"/plan-fails "$fx"/plan-runs "$fx"/slice-* "$fx"/orphan-* "$fx"/prs-* "$fx"/blockers* "$fx"/blocked.json "$fx"/checks* "$fx"/mark-fails "$fx"/mark-fails-times "$fx"/unmark-fails "$fx"/label-missing "$fx"/gh-stderr "$fx"/gh-stderr-fails
  rm -rf "$T/home/tehas/libexec" "$T/home/tehas/state/cycles" "$T"/home/tehas/state/plan-* "$T/home/tehas/live" "$T/home/tehas/state/pause.json" "$T/home/tehas/state/limit.json" "$T/home/tehas/state/branch-check" \
    "$T"/home/tehas/work/widget-plan-* "$T"/home/tehas/tmp/plan-*
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
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_TIDY_CMD=fake-tidy TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_START_CMD=fake-plan-start "$slice_env")
  # Every poll of the case runs in Rust, as on a host: libexec/dispatch hands
  # it to tehas-dispatch. T_BASH_POLL=1 runs the bash poll instead, as
  # TEHAS_DISPATCHER=bash does.
  if [ -n "${T_BASH_POLL:-}" ]; then
    case " ${T_MINIMAL[*]} " in *" TEHAS_DISPATCHER=bash "*) ;; *) T_MINIMAL+=(TEHAS_DISPATCHER=bash) ;; esac
  else
    # shellcheck disable=SC2154  # t_dispatch_why is set by t_dispatch (test/lib.sh)
    t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"
    # No TEHAS_HEALTH_CMD: health is the stub t_stub made above. The Rust poll
    # names its own health beside its own binary, as the bash poll names the one
    # beside libexec/dispatch, so the bare word of a command on PATH is never
    # asked for: the stub has to be named. It records its call and answers 0;
    # the WARN of a health file that could not be written is read here as no
    # command having been given to the poll at all.
    T_ENV_ALWAYS=(TEHAS_DISPATCH_CMD="$T_DISPATCH" TEHAS_HEALTH_CMD=fake-health)
  fi
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
# comments_of <issue> <id> <question body> <editor or "">: the same, the bot's
# question being the comment <id>, as a poll of this run posted it.
comments_of() {
  comments "$1" "$3" "$4"
  jq --argjson id "$2" '.data.repository.issue.comments.nodes[0].databaseId = $id' "$fx/comments-$1.json" >"$fx/comments-$1.new" &&
    mv "$fx/comments-$1.new" "$fx/comments-$1.json"
}
# The body of a park comment, as q_render of lib/questions.sh writes it: the
# three boxes of a park of this factory, the two retries between them and the
# one to leave it, their text the pair of names in that file, which the polls
# of both dispatchers read. It is sourced for those two names alone: nothing
# here calls GitHub, as nothing of that file does.
# shellcheck source=lib/questions.sh
. "$T_ROOT/lib/questions.sh"
# park_body <clean-start box> <alone box> [<leave box>]: each is x or a space.
# With no alone box the body is the shape a comment had before that box
# existed: it holds no such line at all, so a poll that reads the old comments
# still finds its boxes. A comment of that shape has no box to leave it,
# either, so the second argument is the leave box there, as it was before the
# third box was added. A box of a three-box body that no argument names is
# unticked, never guessed from the number of arguments.
park_body() {
  local clean=${1:-' '} alone=${2:-} leave=${3:-} b
  # The shape of a comment written before the third box is named by the number
  # of arguments, and its second one is the leave box. Nothing decides the
  # shape from an empty argument, which would give one argument two meanings.
  if [ "$#" -le 2 ]; then
    leave=$alone
    alone=
  fi
  b='Tehas cycle stopped.'
  # One box line: the text lib/questions.sh names, ticked when its box is x.
  b+=$'\n\n'$(park_box "$clean" "$Q_BOX_CLEAN")
  # A comment written before the box for the issue alone existed holds no such
  # line at all, which is what an empty second argument stands for.
  [ -z "$alone" ] || b+=$'\n'$(park_box "$alone" "$Q_BOX_ALONE")
  b+=$'\n'$(park_box "$leave" 'Leave it parked')
  b+=$'\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n'
  printf '%s' "$b"
}

# park_box <x or anything else> <text>: one line of a park's body, its box
# ticked for x alone.
park_box() {
  local mark=' '
  [ "$1" = x ] && mark=x
  printf -- '- [%s] %s\n' "$mark" "$2"
}

# park_q [<issue>]: the question of the issue's current park on record [#7],
# none without one.
park_q() { t_row park-show "${1:-7}" -- '.park.question // "none"'; }
# park_of [<jq of the park>]: #7's current park on record.
park_of() { t_row park-show 7 -- ".park | ${1:-.} | tostring"; }
# Issue 7 is parked with a question the host has on record, and nothing else
# is queued. Its last cycle started on alice's label of "Words." (or by hand)
# and parked on the cap given [failed]; the park's question is the comment 4001.
parked() { # [<reason> [<who started the cycle: alice, or - for by hand>]]
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
  t_ended "$T/home/tehas" 7 "${2:-alice}" label --parked
  t_park "$T/home/tehas" 7 "${1:-failed}" 4001
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
# The files a planning turn of issue 7 passes on are named after this; its
# record is on the store.
record=$T/home/tehas/state/plan-7.json
# What the session carries from one turn to the next, beside the workspace.
keep=$T/home/tehas/work/widget-plan-7.keep
# rec: issue 7's session as the store has it, in the shape its record file
# had (tehas_plan_shape); null when there is none. rec_open: "open" while one
# is open, else "none".
plan_shape=$(bash -c '. "$1"; printf "%s" "$tehas_plan_shape"' _ "$T_ROOT/lib/store.sh")
rec() { t_row plan-show 7 -- "$plan_shape" | jq -c .; }
rec_open() { t_row plan-show 7 -- 'if .planning != null and .planning.ended_at == null then "open" else "none" end'; }
# The session's brief, and the answer the turn running was given, on record.
rec_brief() { t_row plan-show 7 -- '.planning.brief // "" | rtrimstr("\n")'; }
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
  comments 7 "$(park_body ' ' ' ' ' ')" ""
}
# Issue 7 has a planning question open after one turn.
planning() {
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7}]' >"$fx/asking.json"
  t_plan "$T/home/tehas" 7 '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"comment":4001}'
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
# curl_heard: the opening of a curl stub. What curl was given is kept the same
# way from either poll: the bash poll's config in a /dev/fd file and its body on
# standard input, the Rust poll's config on standard input with the body as its
# `data` line. The config's lines go to calls/curl-config, the body to
# calls/curl-body.
curl_heard='conf=""
case " $* " in *" -K - "*) conf=$(cat) ;; esac
for a; do case "$a" in /dev/fd/*) cat "$a" >>"$T/calls/curl-config" ;; esac; done
if [ -n "$conf" ]; then
  printf "%s\n" "$conf" | grep -v "^data = " >>"$T/calls/curl-config"
  body=$(printf "%s\n" "$conf" | sed -n "s/^data = //p")
  [ -z "$body" ] || jq -r . <<<"$body" >"$T/calls/curl-body"
fi
heard() { [ -n "$conf" ] || cat >"$T/calls/curl-body"; }
'
# A request with an image is answered when the file reads-images is there; what
# was sent is kept.
mesh_curl() {
  t_stub curl "$curl_heard"'case "$*" in
  *"/chat/completions") heard; [ -e "$T/fx/reads-images" ] ;;
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

# store_refuses <args> <rc> <line>: the installed store answers <rc> with
# <line> on stderr for each call whose arguments are <args> or begin with
# <args> and a space, and runs the real store for every other call. Each call
# is noted in t_calls store.
store_refuses() {
  local bin=$T/home/tehas/libexec/store
  rm -f "$bin"
  printf '%s\n' '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$*\" >>$(t_q "$T/calls/store")" \
    "a=\"\$*\" p=$(t_q "$1")" \
    'if [ "$a" = "$p" ] || [ "${a#"$p "}" != "$a" ]; then' \
    "  printf '%s\\n' $(t_q "$3") >&2; exit $(t_q "$2")" \
    'fi' \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
  chmod 755 "$bin"
}
# open_read_fails_again: the open planning sessions are read once, and every
# read of them after that answers busy, as after a planning write of the poll.
# With a session on #9 whose issue no longer asks, the poll ends it, which is
# such a write.
open_read_fails_again() {
  local bin=$T/home/tehas/libexec/store
  t_plan "$T/home/tehas" 9 '{"turns":1,"comment":4009}'
  rm -f "$bin"
  printf '%s\n' '#!/usr/bin/env bash' \
    "if [ \"\$*\" = 'plan-show --open' ] && [ -e $(t_q "$T/calls/open-read") ]; then" \
    "  echo 'store: busy #0: the database is locked' >&2; exit 1" \
    'fi' \
    "[ \"\$*\" != 'plan-show --open' ] || : >$(t_q "$T/calls/open-read")" \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
  chmod 755 "$bin"
}

# The open issues as tehas-dispatch's one GraphQL page asks for them, made from
# the fixtures the bash poll's REST calls answer from: the lists name the
# issues and their labels, events-<n>.json their label events, work-<n>.json
# and edit-<n>.json (or edit.json) their text, prs-<n>.json (or prs.json)
# the pull requests that close them, and issue-<n>.json with a state their
# state and labels. An issue marked running and CLOSED is a
# closed one.
cat >"$T/page-stub" <<'STUB'
#!/usr/bin/env bash
fx=$T/fx
# Read without cat: an idle poll's count of programs is the poll's, not the
# stub's.
read_or() { # <file> <second file or ""> <default>
  [ -r "$1" ] && printf '%s' "$(<"$1")" && return
  [ -n "$2" ] && [ -r "$2" ] && printf '%s' "$(<"$2")" && return
  printf '%s' "$3"
}
lists=$(for f in ready blocked asking plans refines awaiting marked; do read_or "$fx/$f.json" "" "[]"; echo; done | jq -cs 'map((. // []) | flatten)')
nums=$(jq -r '[.[][] | select((.state // "OPEN") != "CLOSED") | .number | tonumber? // empty] + [1] | unique | .[]' <<<"$lists")
# An issue whose cycle holds a lock file is marked running, as on a host; and
# an issue a case wrote the REST answer of is on GitHub too: an open one is in
# the queue whether or not a list names it. With $fx/page-drops-events the
# page answers every timeline empty, as GitHub can for a large page. A cycle
# that holds its lock was marked running by the bot ($running).
# Held, as flock -n finds it. The system's flock, on an open file and with no
# command: a case may count the flock and true it stubs. /proc/locks does not
# do, as in a container its inodes are not the ones ls -i gives.
fl=$(command -pv flock)
locked=$(for f in "$T"/home/tehas/state/cycles/[0-9]*.lock; do
  [ -e "$f" ] || continue
  { "$fl" -n 3 || { f=${f##*/}; echo "${f%.lock}"; }; } 3<"$f"
done | jq -Rcs 'split("\n") | map(select(. != "") | tonumber)')
for n in $(jq -r '.[]' <<<"$locked"); do
  [ "$(jq -r '.state // "open" | ascii_downcase' "$fx/work-$n.json" 2>/dev/null || echo open)" = open ] && nums=$(printf '%s\n%s\n' "$nums" "$n" | sort -un)
done
for w in "$fx"/work-*.json; do
  [ -f "$w" ] || continue
  n=${w##*/work-}
  n=${n%.json}
  [ "$(jq -r '.state // "open" | ascii_downcase' "$w" 2>/dev/null)" = open ] && nums=$(printf '%s
%s
' "$nums" "$n" | sort -un)
done
items=$(for n in $nums; do
  jq -nc --argjson n "$n" --argjson ev "$(read_or "$fx/events-$n.json" "" "[]")" \
    --argjson w "$(read_or "$fx/work-$n.json" "" null)" \
    --argjson e "$(read_or "$fx/edit-$n.json" "$fx/edit.json" "{}")" \
    --argjson p "$(read_or "$fx/prs-$n.json" "$fx/prs.json" "{}")" \
    --argjson i "$(read_or "$fx/issue-$n.json" "" "{}")" \
    '{n: $n, ev: $ev, w: $w, e: ($e.data.repository.issue // {}), p: $p, i: $i}'
done | jq -cs .)
jq -n --argjson lists "$lists" --argjson items "$items" --argjson control "$(read_or "$fx/control.json" "" '{"labels":[]}')" \
  --argjson repo "$(read_or "$fx/repo.json" "" '{}')" --argjson locked "$locked" \
  --argjson dropped "$([ -e "$fx/page-drops-events" ] && echo true || echo false)" \
  --argjson running '{"__typename":"LabeledEvent","createdAt":"2026-01-01T00:00:00Z","actor":{"login":"tehas-bot"},"label":{"name":"tehas:running"}}' '
  def labels($n): [$lists[][] | select((.number | tonumber?) == $n) | (.labels // [])[].name]
    + (if $n == 1 then [$control.labels[].name] else [] end)
    # Being on a list is carrying its label, as gh issue list --label has it.
    + [range(7) as $i | select(any($lists[$i][]; (.number | tonumber?) == $n))
       | ["tehas:ready", "tehas:blocked", "tehas:question", "tehas:plan", "tehas:refine",
          "tehas:awaiting-merge", "tehas:running"][$i]] | unique;
  def actor: if .actor == null then null else {login: .actor.login} end;
  {data: {repository: {nameWithOwner: $repo.nameWithOwner,
    issues: {pageInfo: {hasNextPage: false, endCursor: null},
      nodes: [$items[] | select((.w.state // "open") | ascii_downcase == "open")
        | select((.i.state // "OPEN") != "CLOSED") | . as $i | {
        number: .n, title: (.e.title // .i.title // .w.title // ""), body: (.e.body // .i.body // .w.body),
        lastEditedAt: .e.lastEditedAt, editor: .e.editor,
        # The labels its lists give it, and those of its REST answer when a case
        # wrote one: on GitHub the two agree.
        # The answer of gh issue view, when a case wrote its state, is the truth.
        labels: {nodes: [((if $i.i.state then [($i.i.labels // [])[] | .name? // . | strings]
            else labels($i.n) + [($i.w.labels // [])[] | .name? // . | strings] end)
          + (if any($locked[]; . == $i.n) then ["tehas:running"] else [] end)) | unique | .[] | {name: .}]},
        subIssuesSummary: (.w.sub_issues_summary // null),
        # Parenthesised: jq 1.7, as in the sandbox image, takes no + in an object value.
        timelineItems: {nodes: ([.ev[] | select($dropped | not) | {__typename: (if .event == "labeled" then "LabeledEvent"
            elif .event == "unlabeled" then "UnlabeledEvent" else "RenamedTitleEvent" end),
          createdAt: .created_at, actor: actor, label: .label}] + [select(any($locked[]; . == $i.n) and ($dropped | not)) | $running])},
        closedByPullRequestsReferences: {nodes: (.p.data.repository.issue.closedByPullRequestsReferences.nodes // [])}}]},
    closedRunning: {nodes: [$lists[][] | select(.state == "CLOSED") | {number}]}}}}'
STUB
chmod +x "$T/page-stub"

# bash_reads: whether the poll under test makes the bash poll's own reads,
# its REST calls and their words (T_BASH_POLL). The Rust poll reads the queue
# in one GraphQL page instead, and ends the poll on its failure with its own
# words, so a case that checks those calls checks them for the bash alone.
bash_reads() { [ -n "${T_BASH_POLL:-}" ]; }
