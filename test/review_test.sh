#!/usr/bin/env bash
# Tests for libexec/review: one headless lens-review of the branch since the
# last reviewed commit. A stub docker records its arguments and answers as
# pi-rukas's headless runner does: one JSON document on stdout and an exit
# status, so no model runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

review="$T_ROOT/libexec/review"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, and the run's own state beside it.
tree=$ws-review
state=$tree.state
record=$T/home/tehas/state/review.json
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
# The commits of the branch: one older than the 7 days a host with no record
# reads from, then two since. Dated where they are made, and the clone keeps
# those dates, so the 7 days mean the same thing here as in the checkout.
day() { GIT_COMMITTER_DATE="$1" GIT_AUTHOR_DATE="$1" git -C "$upstream" \
  -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$2"; }
old=$(date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ)
recent=$(date -u -d '2 days ago' +%Y-%m-%dT%H:%M:%SZ)
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
# How far back a host with no record reads, which the run's report names.
days=7
day "$old" first
c1=$(git -C "$upstream" rev-parse HEAD)
day "$recent" second
c2=$(git -C "$upstream" rev-parse HEAD)
day "$recent" third
head=$(git -C "$upstream" rev-parse HEAD)
# The product checkout of a host: a clone of the upstream, as plan_test and
# work_lib make theirs, whose origin ends in the repository's name, which is
# what tehas_checkout checks, and whose origin/HEAD names the default branch.
checkout_host() {
  rm -rf "$checkout"
  git clone -q "$upstream" "$checkout"
  git -C "$checkout" fetch -q "$upstream" '+refs/heads/main:refs/remotes/origin/main'
  git -C "$checkout" checkout -q main
  git -C "$checkout" symbolic-ref refs/remotes/origin/HEAD refs/remotes/origin/main
}
checkout_host

# The container notes what it was handed, since the workspace is gone when the
# run is over: the commit its clone is at, and whether it is clean. What the
# runner answers is $T/fx/doc.json, its exit status in $T/fx/doc.rc.
runner=/opt/pi-rukas/bin/pi-rukas-headless
# The container of a cycle that named none: it notes what it was handed and
# answers at once, what the case left in $T/fx. A case that means a container
# still working writes its body to $T/container.sh, as test/work_lib.sh does,
# and takes it away again where it means one that says nothing.
docker_run() { # the stub of the runner, and of docker's slice questions
  t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
case " $* " in *" --capabilities "*)
  # The probe of the image is no container: recorded by itself, so that a
  # case which says no container started means that, and not no docker at all.
  printf "%s\n" "$*" >"$T/caps.call.$(cat "$T/caps.n" 2>/dev/null || echo 0)"
  printf "%s\n" "$(( $(cat "$T/caps.n" 2>/dev/null || echo 0) + 1 ))" >"$T/caps.n"
  # What the image answers, from the file the case made. A case that made
  # none is asked what an image whose runner does neither kind of turn
  # answers, which is nothing.
  case " $* " in *" $runner plan "*) [ ! -e "${PLAN_CAPS:-}" ] ||
      cat "$PLAN_CAPS" ;;
    *) [ ! -e "$T/caps.json" ] || cat "$T/caps.json" ;;
  esac
  exit "${CAPS_RC:-0}" ;;
esac
# The call of a run, under the name of its container: the call of a review and
# that of a planning turn cannot be read for one another, and each is the
# newest of its own kind. A call with no container in it, the probe of an
# image, goes by the process that made it, as every call of a case does.
# The two commands of the case that compares them take their calls from
# files of their own, named for the command; every other case reads the
# calls of its run together.
case " $* " in *" --name tehas-review "*) suffix=review ;;
  *" --name tehas-plan-"*) suffix=plan ;; *) suffix=$$ ;; esac
printf "%s\n" "$@" >"$T/calls/docker.args.$suffix"
case " $* " in
  *" stop "*) : >"$T/calls/stopped" ;;
  *" '"$runner"' plan "*)
  # A planning turn, answered as the runner of one is, for the case that
  # compares the docker calls of the two commands. It notes the brief it was
  # given and keeps the session the case named, as a turn keeps its own.
  ws='"$T"'/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  if [ -s "$fx/session" ]; then
    sid=$(cat "$fx/session")
    dir=$ws/.git/pi-plan-
    mkdir -p "$dir""sessions/$sid"
    printf "%s\n" "$sid-$sid" | tr "a-z" "-" >"$dir""sessions/$sid/session.json"
  fi
  cat "$T/fx/doc_plan.json" 2>/dev/null
  exit "$(cat "$T/fx/doc_plan.rc" 2>/dev/null || echo 11)" ;;
  *" '"$runner"' lens-review "*)
  # A container a case means to be still working reads its body from the
  # file; every other one notes what it was handed and answers at once.
  if [ -s "$T/container.sh" ]; then . "$T/container.sh"; exit $?; fi
  ws='"$T"'/home/tehas/work/widget-review
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  cat "$T/fx/doc.json" 2>/dev/null
  exit "$(cat "$T/fx/doc.rc" 2>/dev/null || echo 0)" ;;
esac'
}
# The command looks at the container once a second. Here a look costs a tenth
# of a second, and the container of a case that is still working uses the real
# sleep, $nap, as test/work_lib.sh's does.
t_stub sleep '"$(command -pv sleep)" 0.1'
# shellcheck disable=SC2034  # the body of a container that works reads $nap
nap=$(command -pv sleep)
# The clone of the review needs a git identity, as every container's does: the
# tehas user's own on a host.
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
image="tehas/pi-rukas:uid$(id -u)"

# What an image whose runner reviews answers to --capabilities: the
# lens-review verb with the flags this command passes.
caps_review() {
  jq -n '{contract: "0.14.0", features: {planTurns: true},
    verbs: {"lens-review": {flags: ["--base", "--head", "--since", "--full",
        "--context", "--pr", "--json"]},
      plan: {flags: ["--resume", "--answer", "--json"]},
      work: {flags: ["--restart", "--json", "--timeout"]}},
    inputs: [{name: "PI_ENSEMBLE_SANDBOX_MODE", scope: "cycle"},
      {name: "PI_ENSEMBLE_WORKSPACE_ROOT", scope: "cycle"},
      {name: "PI_ENSEMBLE_MAX_WORKSTREAMS", scope: "cycle"},
      {name: "PI_ENSEMBLE_VERIFY_TIMEOUT_MS", scope: "cycle"},
      {name: "PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS", scope: "cycle"},
      {name: "PI_ENSEMBLE_SUBAGENT_MODEL", scope: "cycle"},
      {name: "PI_ENSEMBLE_SUBAGENT_PROVIDER", scope: "cycle"},
      {name: "PI_ENSEMBLE_EXTRA_IMAGE_DIRS", scope: "cycle"}]}' >"$T/caps.json"
}
# What the runner answers, both kinds of turn from files of their own: the
# review reads doc and the planning turn reads doc_plan, since the two
# documents differ.
runner_says() { # <exit status> [document]
  printf '%s' "${2:-{\}}" >"$fx/doc.json"
  printf '%s\n' "$1" >"$fx/doc.rc"
}
turn_says() { # <exit status> [document]
  printf '%s' "${2:-{\}}" >"$fx/doc_plan.json"
  printf '%s\n' "$1" >"$fx/doc_plan.rc"
}
# The document of a planning turn that drafted a plan to be confirmed.
turn_doc='{"subcommand":"plan","mode":"turn","exitCode":12,"status":"draft",
  "session":{"id":"abc-123","turn":1},"text":"Drafted.","title":"Faster search",
  "spec":"Do it.","questions":[]}'
reset() {
  # One directory for the calls of docker: the two commands of the case that
  # compares them write files named for themselves, every other run by the
  # process that made it.
  rm -rf "$T"/calls/* "$T/home/tehas" "$fx/doc.json" "$fx/doc.rc" \
    "$fx/doc_plan.json" "$fx/doc_plan.rc" "$fx/session" \
    "$T/caps.plan" "$T/container.sh" "$T"/caps.call.* "$T/caps.n"
  checkout_host
  docker_run
  caps_review
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  runner_says 0 '{"verdict":"APPROVED"}'
}

# The arguments of the review's container: the call of the run named for it,
# so that a second command a case runs writes its own file and is never read
# for the review's.
run_args() { cat "$T/calls/docker.args.review"; }
after() { run_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
tail_of() { run_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_lib.sh.
unknown_options() { # <image> <options that take a value> <options that take none>
  awk -v image="$1" -v valued=" $2 " -v plain=" $3 " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    /^PI_ENSEMBLE_[A-Z0-9_]*(=.*)?$/ { next }
    /^GIT_CONFIG_[A-Z0-9_]*=.*$/ { next }
    { print }'
}
# What docker is given before the image, one argument per line, with the
# container's name replaced and the cid file dropped: what a planning turn is
# given, and what this command must be given, modulo the names the two cannot
# share.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}
logs_named() { find "$T/home/tehas/logs" -type f 2>/dev/null | LC_ALL=C sort; }

# --- Usage and the gates before anything runs ------------------------------------
# These cases run the command as a person or a timer would, with the host
# reset() leaves it: the instance, the slice, the checkout, and an image whose
# runner reviews. Nothing before the gates below may run the command.

# The text the command prints for itself, which is not the dispatcher's usage,
# and no container started.
t_case "-h prints the usage of the command, starts no container and makes nothing"
reset
t_run "'$review' -h >'$T/help.out' 2>&1; echo \$? >'$T/help.rc'"
t_eq "$(cat "$T/help.rc")" 0
t_contains "$(cat "$T/help.out")" "Review what the default branch gained"
t_contains "$(cat "$T/help.out")" "tehasbot review"
t_lacks "$(cat "$T/help.out")" "unknown command"
t_eq "$(find "$T/home/tehas" \
  -not -path "$T/home/tehas/repos*" 2>/dev/null | wc -l)" 1
t_eq "$(t_calls docker)" ""

t_case "--help prints the same text"
reset
t_run "'$review' --help"
t_eq "$t_rc" 0
t_eq "$t_out" "$(cat "$T/help.out")"

for bad in "--help extra" "--bogus" "--dry-run" "7"; do
  t_case "an argument this command does not know ('$bad') is refused before anything runs"
  reset
  t_run "$review $bad"
  t_eq "$t_rc" 64
  t_contains "$t_out" "review: unknown argument"
  t_contains "$t_out" "Review what the default branch gained"
  t_eq "$(t_calls docker)" ""
  # The checkout of the product is the host's, put there before the command
  # ran: what the command must not make is a log, a workspace or a record.
  t_eq "$(find "$T/home/tehas" \
    -not -path "$T/home/tehas/repos*" 2>/dev/null | wc -l)" 1
done

t_case "an invalid instance starts nothing"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$review"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$(find "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0

t_case "without Pi's settings.json nothing runs"
reset
mv "$T/home/.pi/agent/settings.json" "$T/settings.kept"
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "no $T/home/.pi/agent/settings.json: run install.sh"
t_eq "$(t_calls docker)" ""
t_eq "$(find "$T" -maxdepth 1 -name 'caps.call.*' | wc -l)" 0
mv "$T/settings.kept" "$T/home/.pi/agent/settings.json"

# An image whose runner reviews nothing is refused before the clone: no
# workspace, no log, no record, and no container but the capabilities probe.
t_case "an image with no lens-review verb starts nothing: 78, named, before any clone"
for caps in '{"contract":"0.12.80","verbs":{"work":{"flags":["--json"]}}}' \
  '{"verbs":{"lens-review":{}}}' \
  '{"verbs":{"lens-review":{"flags":["--base","--head"]}}}' \
  '{"verbs":{"lens-review":{"flags":"--since"}}}' \
  '{"features":{"planTurns":true}}' 'not json' ''; do
  t_case "a --capabilities answer without both --since and --json of lens-review ('$caps')"
  printf '%s\n' "$caps" >"$T/caps.json"
  reset
  printf '%s\n' "$caps" >"$T/caps.json"
  t_run "$review"
  t_eq "$t_rc" 78
  t_contains "$t_out" "runs no review (its --capabilities has no verbs lens-review)"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0
done

t_case "without the git identity of the tehas user nothing runs"
reset
mv "$T/home/.gitconfig" "$T/gitconfig.kept"
t_run "$review"
t_eq "$t_rc" 64
t_contains "$t_out" "git user.name and user.email are not set"
t_eq "$(grep -l -x -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" 2>/dev/null | wc -l)" 0
mv "$T/gitconfig.kept" "$T/home/.gitconfig"

# --- Where the review starts from --------------------------------------------------

t_case "a host with no record reads from the commit 7 days back, and says where it started from"
reset
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "
t_contains "$t_out" "main from ${c1:0:12} ($days days)"

t_case "a branch younger than 7 days reads from its root, and says so"
reset
# A branch that is one commit, made after the window opened: no commit of it
# is 7 days back, so the run reads it from its root, which is that commit.
up_young=$T/git/acme/young
rm -rf "$up_young"
git init -q -b main "$up_young"
GIT_COMMITTER_DATE="$now" GIT_AUTHOR_DATE="$now" git -C "$up_young" \
  -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m only
young=$(git -C "$up_young" rev-parse HEAD)
git -C "$checkout" fetch -q "$up_young" '+refs/heads/main:refs/remotes/origin/young'
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=young
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $young --json "
t_contains "$t_out" "young from ${young:0:12} (root)"

t_case "a host with a record reads from the commit it holds, not from 7 days back"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $c2 --json "

t_case "a record that is unreadable or names no commit reads from 7 days back"
for keep in 'not json' '{"commit":null}' '{"commit":"zzzz"}' '{"exit":10}'; do
  t_case "a record of '$keep'"
  reset
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$keep" >"$record"
  t_run "$review"
  t_eq "$t_rc" 0
  t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "
done

t_case "the checkout's own worktree decides nothing: the review is at the fetched tip"
reset
printf 'untracked of the person who works here\n' >"$checkout/scratch.txt"
git -C "$checkout" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m local-only
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "
t_eq "$(cat "$T/calls/head.seen")" "$head"

# --- What the record becomes -------------------------------------------------------

t_case "the runner's 0, 10 and 20 all record the head that was reviewed, and the command exits with it"
for rc in 0 10 20; do
  t_case "a review that exited $rc"
  reset
  runner_says "$rc" '{"verdict":"x"}'
  t_run "$review"
  t_eq "$t_rc" "$rc"
  t_eq "$(jq -c 'keys | sort' "$record")" '["at","commit","exit"]'
  t_eq "$(jq -r .commit "$record")" "$head"
  t_eq "$(jq -r .exit "$record")" "$rc"
  t_contains "$(jq -r .at "$record")" "T"
done

t_case "any other exit leaves the record exactly as it was, and the command exits 2"
for rc in 30 40 70 1; do
  t_case "a review that exited $rc"
  reset
  mkdir -p "$T/home/tehas/state"
  jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
  cp "$record" "$T/record.before"
  runner_says "$rc" '{}'
  t_run "$review"
  t_eq "$t_rc" 2
  t_contains "$t_out" "runner_exit=$rc recorded=no"
  t_eq "$(cmp "$T/record.before" "$record" && echo same)" same
  t_eq "$(find "$T/home/tehas/state" -type f | LC_ALL=C sort | tr '\n' ' ')" "$record "
done

t_case "a run that failed on a host with no record makes none"
reset
runner_says 30 '{}'
t_run "$review"
t_eq "$t_rc" 2
t_eq "$([ -e "$record" ] && echo made)" ""
t_eq "$(find "$T/home/tehas/state" 2>/dev/null | wc -l)" 0

t_case "the record of the review replaces the one before, whole"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10, extra: "left over"}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(jq -c 'keys | sort' "$record")" '["at","commit","exit"]'
t_eq "$(find "$T/home/tehas/state" -type f | LC_ALL=C sort | tr '\n' ' ')" "$record "

t_case "a record that cannot be written is exit 2 and leaves the old one"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
chmod 500 "$T/home/tehas/state"
t_run "$review"
chmod 700 "$T/home/tehas/state"
t_eq "$t_rc" 2
t_contains "$t_out" "could not write $record"
t_eq "$(jq -r .commit "$record")" "$c2"

# --- The container -----------------------------------------------------------------

t_case "the review container gets the mounts of a planning turn, no more"
reset
t_run "$review"
t_eq "$t_rc" 0
want="$tree:$ws
$T/home/.pi/agent/ensemble-runs/review:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the review gets no token: GH_TOKEN is not among its variables"
reset
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c -x -e GH_TOKEN)" 0
t_lacks "$(run_args)" "test-token"
# The names the sandbox library always gives: the code index, the git
# credential helper and identity, and the prompt that must not be asked; plus
# the two of its own, and every PI_ENSEMBLE_* of the environment the run was
# given. The token is the name the review is given no value for, and so no
# variable at all, which is what the two lines above check.
want="$(printf '%s\n' CBM_CACHE_DIR GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 \
  GIT_CONFIG_KEY_1 GIT_CONFIG_KEY_2 GIT_CONFIG_KEY_3 GIT_CONFIG_VALUE_0 \
  GIT_CONFIG_VALUE_1 GIT_CONFIG_VALUE_2 GIT_CONFIG_VALUE_3 GIT_TERMINAL_PROMPT \
  PI_ENSEMBLE_SANDBOX_MODE PI_ENSEMBLE_WORKSPACE_ROOT \
  PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS PI_ENSEMBLE_MAX_WORKSTREAMS \
  PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER \
  PI_ENSEMBLE_VERIFY_TIMEOUT_MS | LC_ALL=C sort)"
# The arguments after each -e, read from the call itself: two -e in a row is
# how a name with no value is passed, and the second of the two is the name.
env_names() {
  run_args | awk -v image="$image" '
    $0 == image { last = NR }
    { line[NR] = $0 }
    END {
      for (i = 1; i <= last; i++) {
        if (line[i] != "-e") continue
        v = line[i + 1]; sub(/=.*/, "", v)
        if (v != "-e") print v
      }
    }' | LC_ALL=C sort | uniq
}
t_eq "$(env_names)" "$want"

t_case "with a token in the environment and instance lines for a plan, the container gets neither"
reset
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_PLAN_SESSION_DIR=/elsewhere PI_ENSEMBLE_PLAN_ISSUE=99 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP" PI_ENSEMBLE_PLAN_CACHE_TTL_MS=60000)
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c -e '^GH_TOKEN' -e '^PI_ENSEMBLE_PLAN_SESSION_DIR' -e '^PI_ENSEMBLE_PLAN_ISSUE' -e '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS' -e '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_lacks "$(run_args)" "/elsewhere"

t_case "the container has the slice, a cycle's limits, no privileges, the host aliases and no cid file"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=1g PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "1g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_eq "$(after --cpus)" "3.75"
t_eq "$(after --user)" "$(id -u):$(id -g)"
t_eq "$(after --name)" "tehas-review"
t_eq "$(after -w)" "$ws"
t_lacks "$(run_args)" "--cidfile"
t_contains "$(run_args)" "--add-host=models:192.0.2.9"
t_eq "$(run_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm --init")" ""
t_eq "$(run_args | grep -c -x -e --init)" 1

t_case "nothing that controls the host is reachable, and the capabilities were asked before the run"
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"
t_lacks "$args" "$checkout"
caps_at=$(t_calls docker | grep -n -F -e "run --rm --init $image $runner --capabilities" | cut -d: -f1 | head -n 1)
run_at=$(t_calls docker | grep -n -e " $runner lens-review " | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$caps_at" ] && [ -n "$run_at" ] && [ "$caps_at" -lt "$run_at" ] && echo before)" before
rm_at=$(t_calls docker | grep -n -x -e 'rm -f tehas-review' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$rm_at" ] && [ -n "$run_at" ] && [ "$rm_at" -lt "$run_at" ] && echo before)" before

t_case "docker is given what tehas plan gives a planning turn: only the name, the three directories of its own, and what is run differ"
# The two runs go through the same stubbed docker with the same settings, and
# their options are compared after only the names the two cannot share are
# rewritten. A run given a cid file would keep one here and fail the compare.
plan=$T_ROOT/libexec/plan
reset
t_run "$review"
t_eq "$t_rc" 0
options <"$T/calls/docker.args.review" |
  sed -e "s|^$tree:|$ws-review:|" >"$T/review.options"
printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
# The instance of the comparison leaves the memory floor as lib/config.sh
# leaves it, so that the two containers are given the same limits.
# What an image whose runner takes both planning turns and reviews answers:
# one answer that carries the lens-review verb, the planning-turn feature and
# the inputs a host may set, which is what both runs read of it.
# What the image answers a planning turn's probe: the same answer, with the
# feature that says its runner takes planning turns, which caps_review's
# answer for a reviewer alone leaves out.
plan_caps=$T/caps.plan
jq '.features = {planTurns: true}' "$T/caps.json" >"$plan_caps"
T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP"
  "PLAN_CAPS=$plan_caps")
# A planning turn answers what its runner answers: the document of a drafted
# plan and the exit that means it was shown.
turn_says 12 "$turn_doc"
# The record of the session the turn starts, which its container leaves in
# the clone as the runner of a turn leaves one: the id alone in a file of the
# case's, so that no quote of any kind has to survive the making of the stub.
turn_id=abc-123
printf '%s\n' "$turn_id" >"$fx/session"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"

t_eq "$t_rc" 0
options <"$T/calls/docker.args.plan" |
  sed -e "s|^$T/home/tehas/work/widget-plan-7:|$ws-review:|" -e 's|/ensemble-runs/plan-7:|/ensemble-runs/review:|' -e 's|widget-plan-7\.state|widget-review.state|' >"$T/plan.options"
# The options docker is given, read the way docker reads them, with the
# container's name and the three directories of each run's own rewritten:
# what is left is what the two commands were given. An -e whose name is only
# in one list is the token a planning turn is given and a review is not,
# which is the one difference this case is about, so the names are compared
# with the -e that carries them kept: a name and its -e are one option.
read_options() { # <the file of the run>
  awk -v image="$image" '$0 == image { exit }
    $0 == "-e" { name = "yes"; next }
    name { name = ""; printf "env %s\n", $0; next }
    { print }' <"$1"
}
# What the review is not given: the token. What the planning turn is not
# given: nothing. Everything else docker is handed is the same.
t_eq "$(comm -23 <(read_options "$T/review.options" | LC_ALL=C sort) \
  <(read_options "$T/plan.options" | LC_ALL=C sort) | tr '\n' ' ')" ""
t_eq "$(comm -13 <(read_options "$T/review.options" | LC_ALL=C sort) \
  <(read_options "$T/plan.options" | LC_ALL=C sort) | tr '\n' ' ')" "env GH_TOKEN "
# One file per case: the review's run and the planning turn's each have their
# own, so neither is read for the other.
t_eq "$(grep -l -x -e GH_TOKEN "$T"/calls/docker.args.review \
  "$T"/calls/docker.args.plan 2>/dev/null)" "$T/calls/docker.args.plan"

t_case "the workspace is the run's own, and it is gone when the run is over"
t_eq "$([ -e "$tree" ] && echo left)" ""
t_eq "$([ -e "$state" ] && echo left)" ""
t_eq "$([ -e "$ws" ] && echo made)" ""
t_eq "$(cat "$T/calls/tree.status")" ""

t_case "the runner's document and its progress are written beside each other"
reset
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(logs_named | wc -l)" 2
t_eq "$(logs_named | grep -c -x "$T/home/tehas/logs/review-[0-9][0-9]*-[0-9][0-9]*\.json")" 1
t_eq "$(logs_named | grep -c -x "$T/home/tehas/logs/review-[0-9][0-9]*-[0-9][0-9]*\.jsonl")" 1
t_eq "$(cat "$T/home/tehas/logs"/review-*.json)" '{"verdict":"APPROVED"}'

t_case "a run that ran out of time is stopped, exits 2 and records nothing"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
cp "$record" "$T/record.before"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=1
# The container lives until docker stop is called for it, as one does: it is
# killed for the stop and ends as one does, with 137.
cat >"$T/container.sh" <<'EOF'
nap=$(command -pv sleep)
"$nap" 60 >/dev/null 2>&1 &
sleeper=$!
while [ ! -e "$T/calls/stopped" ]; do "$nap" 0.1; done
kill "$sleeper" 2>/dev/null
wait "$sleeper" 2>/dev/null
exit 137
EOF
t_run "$review"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_contains "$t_out" "ran out of time"
t_eq "$(cmp "$T/record.before" "$record" && echo same)" same
t_eq "$(grep -l -x stop "$T"/calls/docker.args.* | wc -l)" 1
t_eq "$([ -e "$tree" ] && echo left)" ""

# --- Without a usable slice -------------------------------------------------------

t_case "refuses with 78 without the slice, before any docker run"
reset
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$review"
t_eq "$t_rc" 78
t_lacks "$(t_calls docker)" "run "
t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0
t_contains "$t_out" "tehas review: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  t_case "docker info answered '$answer'"
  reset
  echo "$answer" >"$T/docker.info"
  t_run "$review"
  t_eq "$t_rc" 78
  t_lacks "$(t_calls docker)" "run "
  t_contains "$t_out" "tehas review: "
done

t_case "a reserve above the ceiling is refused"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$review"
t_eq "$t_rc" 78
t_lacks "$(t_calls docker)" "run "
t_contains "$t_out" "TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"

t_case "the fourteen words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$review" "$short/libexec/review"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
  t_case "the limits were '$words'"
  reset
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" >"$short/lib/slice.sh"
  t_run "$short/libexec/review"
  t_eq "$t_rc" 78
  t_lacks "$(t_calls docker)" "run "
  t_contains "$t_out" "tehas review: the container limits could not be made, not starting"
done

t_case "a workspace directory tehas did not make is not touched"
reset
mkdir -p "$T/home/tehas/work"
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "was not made by tehas, not touching it"
t_eq "$([ -e "$tree" ] && echo made)" ""

t_done
