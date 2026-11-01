#!/usr/bin/env bash
# Tests for libexec/review: one headless lens-review of the branch since the
# last reviewed commit. A stub docker records its arguments and answers as
# pi-rukas's headless runner does: one JSON document on stdout and an exit
# status, so no model runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# What lib/sandbox.sh gives a container: the mounts, the variables and the
# security options. AGENTS.md rule 4. The review's container is pinned against
# these lists, which name what the library gives and not what this command
# happens to pass.
# The security options, as lib/sandbox.sh prints them: the same ten words for
# a cycle, a planning turn, a manual session, the push check and a review.
sandbox_security_words=("--cap-drop" "ALL" "--cap-add" "CHOWN" "--cap-add" "FOWNER" \
  "--security-opt" "no-new-privileges:true" "--tmpfs" "/tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777")

# Where lib/sandbox.sh gives a container its copy of the code index. Said here
# as well, so a test that pins a mount names the path without reading the
# library; test/sandbox_test.sh pins the two against each other.
sandbox_cache_dir_value=/home/vscode/.tehas-cache

# The variables of lib/sandbox.sh that are not the token and not one of the
# cycle's: its workspace root and code index, git's configuration and identity,
# and the prompt that must not be asked. A PI_ENSEMBLE_* of the environment is
# added to these by the library itself, so a test adds what its own run put
# into the environment.
sandbox_env_shared=(CBM_CACHE_DIR GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_KEY_1
  GIT_CONFIG_KEY_2 GIT_CONFIG_KEY_3 GIT_CONFIG_VALUE_0 GIT_CONFIG_VALUE_1
  GIT_CONFIG_VALUE_2 GIT_CONFIG_VALUE_3 GIT_TERMINAL_PROMPT PI_ENSEMBLE_SANDBOX_MODE
  PI_ENSEMBLE_WORKSPACE_ROOT)

# The names of every -e option of a docker run call, sorted and without
# duplicates. Options are read the way docker reads them, so an -e whose value
# begins with -e counts as one name and not two.
sandbox_all_env_names() { # (reads the arguments on standard input)
  awk '
    { line[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (line[i] != "-e") continue
        v = line[i + 1]; sub(/=.*/, "", v)
        if (v != "-e") print v
      }
    }' | LC_ALL=C sort | uniq
}

# The docker options of a container that take a value, as the commands pass
# them. Read by sandbox_unknown_options: anything else before the image name is
# an option no command of tehas is known to pass, so a mount or a variable
# spelled another way (--volume, --mount, --env, -eNAME) shows up there. The
# options that take nothing are --rm and --init, the two every container gets.
sandbox_valued_options="--name --cap-drop --cap-add --security-opt --user -w -v -e --cgroup-parent \
  --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cidfile --label --tmpfs"

# Every option of a docker run call that tehas does not recognise. An option
# that is not one of the two above, and is not one of the two every container
# is given, and is not an address the host aliases made, is an option no
# command of tehas passes.
sandbox_unknown_options() { # <image> (reads the arguments on standard input)
  awk -v image="$1" -v valued=" $sandbox_valued_options " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(" --rm --init ", " " $0 " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    /^PI_ENSEMBLE_[A-Z0-9_]*(=.*)?$/ { next }
    /^GIT_CONFIG_[A-Z0-9_]*=.*$/ { next }
    { print }'
}

# The mounts lib/sandbox.sh gives a container, with the four paths of its own
# filled in: <host> the workspace on the host, <inside> the path the container
# has it at, <state> the run's own directory beside it, <runs> its transcripts.
# The same eight for every kind of container, so a command that mounts one more
# or one less fails against this list rather than a copy of it. Pi's two files
# are read from <home>, named by the caller because a test's home is not the
# home of the shell that runs the test.
sandbox_mounts() { # <home> <host> <inside> <state> <runs>
  local home=$1 host=$2 inside=$3 state=$4 runs=$5
  printf '%s\n' "$host:$inside" \
    "$state/cache:$sandbox_cache_dir_value" \
    "$state/memory:/home/vscode/.vipune" \
    "$state/sessions:/home/vscode/.pi/agent/sessions" \
    "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    "$state/models.json:/home/vscode/.pi/agent/models.json:ro" \
    "$home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    "$home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
}

# The ten words of the security options, in order, as one line. Empty
# when a container is given them another way or not at all.
sandbox_security_line() {
  awk -v words="${sandbox_security_words[*]}" '
    BEGIN { n = split(words, want, " ") }
    { arg[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (seen >= n) break
        if (arg[i] != want[seen + 1]) continue
        for (j = 1; j <= n; j++) if (arg[i + j - 1] != want[j]) break
        if (j > n) { seen = n; start = i }
      }
      if (seen == n) for (i = start; i < start + n; i++) printf "%s ", arg[i]
    }'
}

# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

# The command as a host has it: beside the console, which is a compiled program
# a checkout does not hold. The two are put in one directory of the case, the
# library beside them as test/work_lib.sh does for a cycle. The console is this
# checkout's own: it reads the range and runs the container
# (console/src/review.rs), and the tests of its parts are beside its source.
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
mkdir -p "$shim/libexec"
ln -s "$T_ROOT/libexec/review" "$shim/libexec/review"
ln -s "$T_ROOT/lib" "$shim/lib"
review=$shim/libexec/review
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
cp "$T_CYCLE" "$shim/libexec/tehas-cycle"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, and the run's own state beside it.
tree=$ws-review
state=$tree.state
record=$T/home/tehas/state/review.json
# The directory the record lives in, and the lock of a run beside it.
state_dir=$T/home/tehas/state
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
  # The commit it was asked to read from, so that a case can say the range the
  # command computed is the range the runner was given. The line it writes is
  # read back with $( ), which drops its end, so the escape of one is not
  # written here: this body reaches t_stub single-quoted and the escape would
  # reach printf as a letter.
  seen=""
  for prev in "$@"; do case " $seen " in *" --since "*) printf "%s" "$prev" >"$T/calls/since.seen" ;; esac; seen=$prev; done
  ws='"$T"'/home/tehas/work/widget-review
  # The status this stub answers: recorded so a case can tell a command that
  # read the exit of the container from one that answered the number of its
  # fixture whatever the container said. The number is read back with $( ),
  # which drops the trailing line, so no escape of a newline is written here -
  # this body is carried to t_stub single-quoted, and an escape would reach
  # printf as a letter.
  printf "%s" "$(cat "$T/fx/doc.rc" 2>/dev/null || echo 0)" >"$T/calls/runner.rc"
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  # The document of the run: the console makes the file named by --doc and
  # writes into it what the container prints, so the stub makes it too, from the
  # fixture it answers with - the two cannot then disagree. A case that means a
  # run which wrote no document leaves the fixture away, and the file stays
  # empty as it does on a host whose runner died before it answered.
  seen="" ; doc=""
  for a in "$@"; do
    case "$seen" in --doc) case "$a" in /*) doc=$a ;; esac ;; esac
    seen=$a
  done
  if [ -n "$doc" ]; then cat "$T/fx/doc.json" 2>/dev/null >"$doc"; fi
  cat "$T/fx/doc.json" 2>/dev/null
  exit "$(cat "$T/fx/doc.rc" 2>/dev/null || echo 0)" ;;
esac'
}
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
  # With no document given, one that carries the status it answers with: the
  # command reads that field now, so a case that names no document agrees with
  # itself by default, and one that means a contradiction names both.
  local doc=${2-}
  [ -n "$doc" ] || doc='{"exitCode":'"$1"'}'
  printf '%s' "$doc" >"$fx/doc.json"
  printf '%s\n' "$1" >"$fx/doc.rc"
}
turn_says() { # <exit status> [document]
  printf '%s' "${2:-{\}}" >"$fx/doc_plan.json"
  printf '%s\n' "$1" >"$fx/doc_plan.rc"
}
# A token-shaped string the console's mask replaces (console/src/mask.rs).
token=ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
console_on() { cp "$T_CONSOLE" "$shim/libexec/console"; }
console_absent() { rm -f "$shim/libexec/console"; }
# The document of a planning turn that drafted a plan to be confirmed.
turn_doc='{"subcommand":"plan","mode":"turn","exitCode":12,"status":"draft",
  "session":{"id":"abc-123","turn":1},"text":"Drafted.","title":"Faster search",
  "spec":"Do it.","questions":[]}'
# What the runner answers, both kinds of turn from files of their own: the
# review reads doc and the planning turn reads doc_plan, since the two
# documents differ.
#
# The bot's calls of the filing half. The store makes them (`store
# review-file`), and gh records every call (t_stub gh) and answers the three
# the filing makes: the list of open issues of the review's label, a comment,
# and an issue. A case puts the list GitHub answers in $T/fx/gh-open.json, and
# makes $T/fx/gh-<what>.rc to refuse one call.
findings_repo=acme/backlog
findings_on() {
  t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_REVIEW_REPO="$findings_repo"
  # The filing asks the store to render a document, mask it and file it, so the
  # home has the store of this checkout in its libexec, as a host's does. The
  # filing renders a document and touches no row, so the home needs no
  # database, and none is made.
  t_store_filing
}
open_issues() { # <json>: what `gh issue list` answers for the review's label
  printf '%s\n' "$1" >"$T/fx/gh-open.json"
}
# The body of the gh stub, carried to t_stub single-quoted so no expansion of
# this file reaches it: what it names is read when the stub runs.
# The calls of GitHub, with the words of a body left out of it: a body that
# ends with the line of tehas:refine is recorded as the two words of the call
# and then its own lines, so the lines of a body are not mistaken for calls.
github_verbs() {
  awk '$0 !~ /^[#-]/ && $1 == "issue" { print $2 }' "$T/calls/gh" |
    LC_ALL=C sort -u | tr '\n' ' ' | sed 's/ $//'
}
# The stub of GitHub for the filing: the three calls the store makes, each
# recorded for a case to read, each refused by a file of the case.
t_stub_gh_filing() {
  t_stub gh 'case "$1 $2" in
  "issue list")
    [ -f "$T/fx/gh-list.rc" ] && exit 1
    # The list of the open issues of the label, as GitHub answers it: number
    # and title, which is all the filing reads. With no file of the case the
    # answer is the empty list, which is no open issue of the review.
    [ -f "$T/fx/gh-open.json" ] || echo "[]"
    [ ! -f "$T/fx/gh-open.json" ] || cat "$T/fx/gh-open.json" ;;
  "issue create")
    [ -f "$T/fx/gh-create.rc" ] && exit 1
    while [ $# -gt 0 ]; do
      case "$1" in
        --title) printf "%s\n" "$2" >"$T/calls/issue.title" ;;
        --body-file) printf "%s\n" "$(cat "$2")" >"$T/calls/issue.body" ;;
        --label) printf "%s\n" "$2" >>"$T/calls/issue.labels" ;;
      esac
      shift
    done
    echo "https://github.com/acme/backlog/issues/77" ;;
  "issue comment")
    [ -f "$T/fx/gh-comment.rc" ] && exit 1
    printf "%s\n" "$3" >"$T/calls/comment.number"
    while [ $# -gt 0 ]; do
      [ "$1" = --body-file ] && printf "%s\n" "$(cat "$2")" >"$T/calls/comment.body"
      shift
    done ;;
esac'
}
# The document of a run that found three findings, one a CRITICAL of a file
# with a line, and the words of the third holding what must not reach GitHub
# as it came: a comment marker and a token-shaped string.
findings_doc='{"exitCode":10,"verdict":"ISSUES_FOUND","noReview":false,"findings":[
  {"severity":"CRITICAL","lens":"security","path":"lib/config.sh","line":4,
   "title":"a token is printed"},
  {"severity":"MEDIUM","lens":"simplicity","path":"libexec/review","line":2,
   "title":"a branch that repeats"},
  {"severity":"HIGH","lens":"error-handling","path":"libexec/work","line":9,
   "title":"<!-- sly --> and ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "}]}'
# The findings of a run that ended with them: the status the runner gave and
# the document it left, both of which the console reads against the other.
# The store of the filing half: the binary of this checkout in the home's
# libexec, which is where the command looks for it, and nothing else. The one
# verb of the filing renders a document and touches no row, so the home needs
# no database, and none is made: a case that says what a run left in the home
# would otherwise find the store's file among it. Only the cases of the filing
# ask for it, for the same reason.
t_store_filing() {
  t_store || t_bad "no store: $t_store_why"
  mkdir -p "$T/home/tehas/libexec"
  ln "$T_STORE" "$T/home/tehas/libexec/store" 2>/dev/null ||
    cp "$T_STORE" "$T/home/tehas/libexec/store"
}
findings_says() { runner_says 10 "$findings_doc"; }
reset() {
  # One directory for the calls of docker: the two commands of the case that
  # compares them write files named for themselves, every other run by the
  # process that made it.
  # TEHAS_REVIEW=1 in the instance every case starts from: the command exits at
  # once when the loop is off, and the cases below are of a review that ran.
  # The one case of the gate itself writes the instance again without it.
  rm -rf "$T"/calls/* "$T/home/tehas" "$fx/doc.json" "$fx/doc.rc" \
    "$fx/doc_plan.json" "$fx/doc_plan.rc" "$fx/session" \
    "$T/caps.plan" "$T/container.sh" "$T"/caps.call.* "$T/caps.n" \
    "$shim/libexec/console"
  checkout_host
  docker_run
  caps_review
  t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  console_on
  t_stub_gh_filing
  runner_says 0 '{"verdict":"APPROVED","exitCode":0}'
}

# The arguments of the review's container: the call of the run named for it,
# so that a second command a case runs writes its own file and is never read
# for the review's.
run_args() { cat "$T/calls/docker.args.review"; }
after() { run_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
tail_of() { run_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# Every option that is not one this command is known to pass, up to the image
# name: the account of the library above.
unknown_options() { sandbox_unknown_options "$1"; }
# What docker is given before the image, one argument per line, with the
# container's name replaced and the cid file dropped: what a planning turn is
# given, and what this command must be given, modulo the names the two cannot
# share.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}
# The files of a run's own under the home, of one kind, named by this test
# alone: the lock of a run is the home's too, and is not what these cases ask
# about. records_held <directory>: the files of that directory that are not a
# lock.
records_held() {
  find "$1" -type f -not -name '*.lock' 2>/dev/null | LC_ALL=C sort | tr '\n' ' '
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
# TEHAS_REVIEW=1 is part of the instance here: the gate below the load is the
# reason a run stops at 0, and an instance whose settings are malformed is
# refused 78 whatever the gate says.
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_REVIEW=1
t_run "$review"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$(find "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0

t_case "with the loop off nothing is called"
# TEHAS_REVIEW is the timer's gate: tehas-review.timer starts this command every
# week, and a host that never turned the loop on pays nothing for it. The
# instance is written without the name, which is how the loader leaves it.
reset
t_instance "${T_MINIMAL[@]}"
t_run "$review"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$T" -maxdepth 1 -name 'caps.call.*' | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0

t_case "with the loop on the same instance reviews"
reset
# The other side of the gate: the same instance, the one line added, and the
# run goes on to the container.
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "

t_case "with the loop off the settings the command needs are still checked"
# The gate sits below tehas_require_instance_settings, so a host whose instance
# could not run a review is told 78 by the weekly unit and not only by a command a
# person typed. The name below is one the review requires, and invalid rather
# than missing: tehas_require stops on the first name that is not set, so a
# missing one would answer 78 wherever the gate sat.
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CONTROL_ISSUE=abc
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CONTROL_ISSUE is not valid"
t_eq "$(t_calls docker)" ""
t_run "'$review' --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Review what the default branch gained"

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

# A docker that will not answer the probe is not an image that runs no review:
# an operator told to rebuild an image that is fine is worse off than one told
# the truth. The two answers are said apart, and both refuse the run.
t_case "a --capabilities probe that fails is named as a probe that failed, not as a missing verb"
for answer in 1 125 127; do
  t_case "docker answered $answer"
  reset
  t_run "CAPS_RC=$answer '$review'"
  t_eq "$t_rc" 78
  t_contains "$t_out" "gave no --capabilities answer (exit $answer)"
  t_lacks "$t_out" "runs no review"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0
done

# A name of the PI_ENSEMBLE_* family that the image does not list as an input a
# host may set is a lever of the runner this run was never vetted for, and it
# arrives by the environment of whoever started the review. The run is refused
# rather than started with it.
t_case "a PI_ENSEMBLE_* lever the image does not list stops the run before anything is cloned"
reset
# An instance line, which is how a PI_ENSEMBLE_* reaches a container: settings
# of the environment are dropped by tehas_load, so the line of the instance is
# the adversary here - the same file a host edits to point at its model.
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 PI_ENSEMBLE_PROTECTED_PATHS= PI_ENSEMBLE_POLICY_JUDGE=off
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "does not take what this review would be given"
t_contains "$t_out" "PI_ENSEMBLE_PROTECTED_PATHS"
t_contains "$t_out" "PI_ENSEMBLE_POLICY_JUDGE"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0

t_case "a PI_ENSEMBLE_* the image does list as an input a host may set reaches the container"
reset
jq '.inputs += [{name: "PI_ENSEMBLE_LENS_NOTE", scope: "cycle"}]' "$T/caps.json" >"$T/caps.new" &&
  mv "$T/caps.new" "$T/caps.json"
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 PI_ENSEMBLE_LENS_NOTE=short
t_run "$review"
t_eq "$t_rc" 0
t_contains "$(after -e)" "PI_ENSEMBLE_LENS_NOTE"

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
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_BRANCH=young
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
for keep in 'not json' '{"commit":null}' '{"commit":"zzzz"}' '{"exit":10}' \
  '{"commit":"abc1"}' '{"commit":"0123456789abcdef0123456789abcdef01234567"}'; do
  t_case "a record of '$keep'"
  reset
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$keep" >"$record"
  t_run "$review"
  t_eq "$t_rc" 0
  t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "
done

# The two shapes that would otherwise review a range nobody asked for and then
# record the tip as read: a prefix of a commit that resolves to something else,
# and a whole commit the branch no longer reaches - a force-push, a reset, or a
# hand that wrote the file. Both fall back to the window, and say so.
t_case "a record whose commit the branch no longer reaches reads from 7 days back, and says the record was refused"
reset
mkdir -p "$T/home/tehas/state"
# A commit of another branch of the same object database: it resolves, and is
# no ancestor of the tip.
up_other=$T/git/acme/other
rm -rf "$up_other"
git init -q -b main "$up_other"
GIT_COMMITTER_DATE="$old" GIT_AUTHOR_DATE="$old" git -C "$up_other" \
  -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m other
other=$(git -C "$up_other" rev-parse HEAD)
git -C "$checkout" fetch -q "$up_other" '+refs/heads/main:refs/remotes/origin/other'
jq -n --arg c "$other" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 0}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_contains "$t_out" "which origin/main no longer reaches"
t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "

t_case "a record naming an abbreviation of a real commit is refused, not resolved"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "${c2:0:12}" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 0}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner lens-review --since $c1 --json "

t_case "a run whose record already names the tip says there is nothing new and starts nothing"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$head" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 0}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_contains "$t_out" "nothing new since ${head:0:12}"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" 2>/dev/null | wc -l)" 0

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
  # The document carries its own exitCode, and the stub answers with the
  # status it was told to: a case of 10 or 20 therefore only passes when the
  # command read the status the container actually returned, which is what the
  # case of the file before this one could not tell from a no-op.
  runner_says "$rc" "{\"verdict\":\"x\",\"exitCode\":$rc}"
  t_run "$review"
  t_eq "$t_rc" "$rc"
  t_eq "$(cat "$T/calls/runner.rc")" "$rc"
  t_eq "$(jq -c 'keys | sort' "$record")" '["at","commit","exit"]'
  t_eq "$(jq -r .commit "$record")" "$head"
  t_eq "$(jq -r .exit "$record")" "$rc"
  t_contains "$(jq -r .at "$record")" "T"
done

# The document and the process have to agree before either is believed, as
# libexec/work reads its result and libexec/plan reads its answer before it
# decides. A container that exits 0 while its own document says the review was
# incomplete has not read the range, and the record does not move on.
t_case "a document that contradicts the container's exit records nothing and says which two disagreed"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
cp "$record" "$T/record.before"
runner_says 0 '{"verdict":"REVIEW_INCOMPLETE","exitCode":30}'
t_run "$review"
t_eq "$t_rc" 2
t_contains "$t_out" "runner_exit=0 document_exit=30 recorded=no"
t_contains "$t_out" "the two disagree"
t_eq "$(cmp "$T/record.before" "$record" && echo same)" same

t_case "a container that exits 30 with a document that says 0 records nothing"
reset
runner_says 30 '{"verdict":"APPROVED","exitCode":0}'
t_run "$review"
t_eq "$t_rc" 2
t_contains "$t_out" "runner_exit=30 document_exit=0 recorded=no"
t_eq "$([ -e "$record" ] && echo made)" ""

# A document with no exitCode of its own is no contradiction: the container's
# status is all there is, and it is what every tehas caller trusts then.
t_case "a document with no exitCode leaves the container's status as the only answer"
reset
runner_says 10 '{"verdict":"ISSUES_FOUND"}'
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(jq -r .exit "$record")" 10

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
  t_eq "$(records_held "$T/home/tehas/state")" "$record "
done

t_case "a run that failed on a host with no record makes none"
reset
runner_says 30 '{}'
t_run "$review"
t_eq "$t_rc" 2
t_eq "$([ -e "$record" ] && echo made)" ""
# Nothing but the lock, which the run opens to make sure it is the only one
# going and leaves behind as every lock of tehas does.
t_eq "$(find "$T/home/tehas/state" -type f 2>/dev/null | LC_ALL=C sort | tr '\n' ' ')" \
  "$state_dir/review.lock "

t_case "the record of the review replaces the one before, whole"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10, extra: "left over"}' >"$record"
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(jq -c 'keys | sort' "$record")" '["at","commit","exit"]'
t_eq "$(records_held "$T/home/tehas/state")" "$record "

t_case "a record that cannot be written is exit 2 and leaves the old one"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
# The making of the record is what fails here: a directory where the new
# record is written first. The old record is a file the run reads and never
# touches again, so it stays exactly as it was, and the range is read again
# next run.
mkdir "$record.new"
t_run "$review"
t_eq "$t_rc" 2
t_contains "$t_out" "could not write $record"
t_eq "$(jq -r .commit "$record")" "$c2"

# --- The container -----------------------------------------------------------------

t_case "the review container gets the mounts of a planning turn, no more"
reset
t_run "$review"
t_eq "$t_rc" 0
# The eight mounts of lib/sandbox.sh, from the lists above, with this run's
# four paths in them. A mount the library adds now fails this case.
t_eq "$(after -v | LC_ALL=C sort)" "$(sandbox_mounts "$T/home" "$tree" "$ws" "$state" \
  "$T/home/.pi/agent/ensemble-runs/review" | LC_ALL=C sort)"

t_case "the review gets no token: GH_TOKEN is not among its variables"
reset
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c -x -e GH_TOKEN)" 0
t_lacks "$(run_args)" "test-token"
# What the sandbox library gives every container (the lists above), less
# the token it keeps back and the cycle's two settings of its own, which no
# run that writes no code and runs no gate is given. A name the library adds,
# or one the environment carries that the image does not list as an input a
# host may set, fails this case: the run is then refused before it starts.
want="$(printf '%s\n' "${sandbox_env_shared[@]}" PI_ENSEMBLE_SUBAGENT_MODEL \
  PI_ENSEMBLE_SUBAGENT_PROVIDER | LC_ALL=C sort)"
# The arguments after each -e, read from the call itself: two -e in a row is
# how a name with no value is passed, and the second of the two is the name.
env_names() { run_args | sandbox_all_env_names; }
t_eq "$(env_names)" "$want"

t_case "with a token in the environment and instance lines for a plan, the container gets neither"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 PI_ENSEMBLE_PLAN_SESSION_DIR=/elsewhere PI_ENSEMBLE_PLAN_ISSUE=99 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP" PI_ENSEMBLE_PLAN_CACHE_TTL_MS=60000)
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c -e '^GH_TOKEN' -e '^PI_ENSEMBLE_PLAN_SESSION_DIR' -e '^PI_ENSEMBLE_PLAN_ISSUE' -e '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS' -e '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_lacks "$(run_args)" "/elsewhere"

t_case "the container has the slice, a cycle's limits, no privileges, the host aliases and no cid file"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_CYCLE_RESERVE=1g PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$review"
t_eq "$t_rc" 0
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "1g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_eq "$(after --cpus)" "15.00"
t_eq "$(after --user)" "$(id -u):$(id -g)"
t_eq "$(after --name)" "tehas-review"
t_eq "$(after -w)" "$ws"
# The ten words that take privileges away, in order, from the lists above:
# the same as a cycle's, a turn's and a session's container is given.
t_eq "$(run_args | sandbox_security_line)" "${sandbox_security_words[*]} "
t_lacks "$(run_args)" "--cidfile"
t_contains "$(run_args)" "--add-host=models:192.0.2.9"
t_eq "$(run_args | unknown_options "$image")" ""
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
ln -sf "$T_ROOT/libexec/plan" "$shim/libexec/plan"
plan=$shim/libexec/plan
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
# What the review is not given: the token, and the three settings of its own
# that lib/sandbox.sh gives a cycle, which no run that writes no code and runs
# no gate asks for. What the planning turn is not given: nothing. Everything
# else docker is handed is the same, which is the point of the case: the two
# containers are one container, and a fourth copy of what a sandbox is given
# would not be caught by anything else.
t_eq "$(comm -23 <(read_options "$T/review.options" | LC_ALL=C sort) \
  <(read_options "$T/plan.options" | LC_ALL=C sort) | tr '\n' ' ')" ""
t_eq "$(comm -13 <(read_options "$T/review.options" | LC_ALL=C sort) \
  <(read_options "$T/plan.options" | LC_ALL=C sort) | tr '\n' ' ')" \
  "env GH_TOKEN env PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS env PI_ENSEMBLE_MAX_WORKSTREAMS env PI_ENSEMBLE_VERIFY_TIMEOUT_MS "
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
t_eq "$(cat "$T/home/tehas/logs"/review-*.json)" '{"verdict":"APPROVED","exitCode":0}'

# What the mask is for: the token of the host in a file an agent can read.
# The mask itself is tested in the console (console/src/mask.rs); what is
# tested here is that the run's files go through it.
t_case "the run's files are masked: what the mask knows does not reach them"
reset
runner_says 0 "{\"verdict\":\"x\",\"exitCode\":0,\"note\":\"$token\"}"
t_run "$review"
t_eq "$t_rc" 0
t_lacks "$(cat "$T/home/tehas/logs"/review-*.json)" "$token"
t_eq "$(cat "$T/home/tehas/logs"/review-*.json | grep -c -o 'ghp_\*\*\*')" 1

t_case "a host with no console beside the command starts nothing: 78, and says to install"
reset
console_absent
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "no console beside this command"
t_contains "$t_out" "run install.sh"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$record" 2>/dev/null | wc -l)" 0

t_case "a run that ran out of time is stopped, exits 2 and records nothing"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
cp "$record" "$T/record.before"
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_PLAN_TIMEOUT=1
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

# A docker that does not answer must not be the last thing a run does. The two
# stops of the timeout path and the sweep of a name left behind are all
# bounded, as libexec/work bounds its own: a wedged dockerd is then reported
# and the run ends, which is what "it ran out of time" has to mean.
t_case "a docker stop that never ends is bounded, and the run still ends with 2"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_PLAN_TIMEOUT=1 TEHAS_STOP_WAIT=1
cat >"$T/container.sh" <<'EOF'
nap=$(command -pv sleep)
"$nap" 5 >/dev/null 2>&1 &
sleeper=$!
# A container that ignores the stop entirely: the stop's own bound has to end
# the wait, and then docker kill is asked, also bounded. The run waits for the
# container to end, so it lives past both bounds (2s), not a minute.
"$nap" 5
exit 137
EOF
printf '%s\n' 'if [ "$1" = stop ] || [ "$1" = kill ]; then "$(command -pv sleep)" 60; exit 0; fi' >>"$T/bin/docker"
t_run "$review"
t_eq "$t_rc" 2
t_contains "$t_out" "did not end within 1s, killing it"
t_eq "$([ -e "$tree" ] && echo left)" ""

# The other half of a wedged dockerd: the container is gone, and its client is
# not. Here the client answers to nothing but KILL, so the bound of the wait on
# it is what ends the run. The grace is TEHAS_STOP_WAIT, as for the stops.
t_case "a docker run client that ignores TERM is killed after the stop's grace, and the run ends with 2"
reset
mkdir -p "$T/home/tehas/state"
jq -n --arg c "$c2" '{commit: $c, at: "2026-10-01T00:00:00Z", exit: 10}' >"$record"
cp "$record" "$T/record.before"
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_PLAN_TIMEOUT=1 TEHAS_STOP_WAIT=1
cat >"$T/container.sh" <<'BODY'
nap=$(command -pv sleep)
# Bash runs the trap only once the sleep in front of it ends, so TERM does
# not end this client; KILL does.
trap 'exit 143' TERM
while [ ! -e "$T/calls/stopped" ]; do "$nap" 0.1; done
"$nap" 120 >/dev/null 2>&1
exit 137
BODY
t_start
t_run "$review"
t_fast 9000
t_eq "$t_rc" 2
t_contains "$t_out" "the docker client of the review would not end, killing it"
t_contains "$t_out" "ran out of time"
t_eq "$(cmp "$T/record.before" "$record" && echo same)" same
t_eq "$([ -e "$tree" ] && echo left)" ""

# --- One review at a time ---------------------------------------------------------

# The container, the workspace and the record are each one per host, so a
# second run must not start beside the first: it would remove the other's
# container and both would rm -rf the same workspace, and the record would then
# move on for a range neither finished.
t_case "a second review while one holds the lock says so, exits 0, and starts no container"
reset
mkdir -p "$T/home/tehas/state"
# The lock of a run still going: held by another process, as the flock of the
# command holds it, so the case holds it with a child that will not let go.
flock "$T/home/tehas/state/review.lock" -c '"$(command -pv sleep)" 30' &
holder=$!
while flock -n "$T/home/tehas/state/review.lock" true; do command -p sleep 0.02; done
t_run "$review"
t_eq "$t_rc" 0
t_contains "$t_out" "another review is going, nothing was reviewed"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
kill "$holder" 2>/dev/null
wait "$holder" 2>/dev/null

# The two directories a run that never started made are removed by the trap the
# command sets once it owns them, as libexec/index cleans its tree.
t_case "a run that could not clone leaves no workspace and no state behind"
reset
mkdir -p "$T/bin"
real_git=$(command -v git)
printf '%s\n' '#!/usr/bin/env bash' 'for a in "$@"; do' '  [ "$a" = fetch ] && { echo "git: cannot clone" >&2; exit 128; }' 'done' \
  'exec '"$real_git"' "$@"' >"$T/bin/git"
chmod +x "$T/bin/git"
t_run "$review"
t_eq "$t_rc" 78
t_eq "$([ -e "$tree" ] && echo left)" ""
t_eq "$([ -e "$state" ] && echo left)" ""

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
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_CYCLE_RESERVE=3g
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

t_case "with a GitHub App the review is given neither the app's token nor its directory"
# tehas plan and tehas work mount the directory the host keeps the app's token
# in. A review is given no GH_TOKEN, so it is not given that directory either.
reset
# The git of the case that could not clone is still on PATH.
rm -f "$T/bin/git"
t_instance "${T_MINIMAL[@]}" TEHAS_REVIEW=1 TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"
mkdir -p "$state_dir/github"
printf 'ghs_app\n' >"$state_dir/github/token"
t_run "$review"
t_eq "$t_rc" 0
t_lacks "$(run_args)" "/run/tehas/github"
t_lacks "$(run_args)" "$state_dir/github"
t_lacks "$(run_args)" "ghs_app"
t_eq "$(after -e | grep -c -x -e GH_TOKEN)" 0

t_case "security options that are not the documented ones start no review"
# The guard of lib/sandbox.sh, asked of a library whose list drifted: the eight
# words a cycle was given before /tmp was bounded.
drift=$T/drift
mkdir -p "$drift/libexec" "$drift/lib"
cp "$T_ROOT"/lib/*.sh "$drift/lib/"
ln -s "$review" "$drift/libexec/review"
printf '%s\n' "tehas_sandbox_security() { printf '%s\\0' --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true; }" \
  >>"$drift/lib/sandbox.sh"
reset
t_run "$drift/libexec/review"
t_eq "$t_rc" 78
t_lacks "$(t_calls docker)" "lens-review"
t_contains "$t_out" "tehas review: tehas_sandbox_security gave 8 words, wants 8: not starting a review"
t_eq "$([ -e "$tree" ] && echo made)" ""

t_case "a workspace directory tehas did not make is not touched"
reset
mkdir -p "$T/home/tehas/work"
t_run "$review"
t_eq "$t_rc" 78
t_contains "$t_out" "was not made by tehas, not touching it"
t_eq "$([ -e "$tree" ] && echo made)" ""

# --- The findings of a run, filed for a person to triage ----------------------
#
# The filing of one run is the store's work whole (`store review-file`, whose
# rule of the issue is tehas_core::review and whose masking is the pair of
# functions in tehas_core that the console's log of the same run goes through).
# What is tested here is the glue: that a run that ended with findings asks the
# store and files what it answered, that a run that ended without any files
# nothing, that
# an open issue of findings is commented on rather than copied, and that the
# words of a finding are masked on the way out.
t_case "a run that exited 10 files one issue with the line that asks for tehas:refine"
reset
findings_on
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 1
t_eq "$(cat "$T/calls/issue.title")" "Six-lens review: 3 findings on main since ${head:0:7}"
t_eq "$(cat "$T/calls/issue.labels")" "tehas:improvement"
t_contains "$(cat "$T/calls/issue.body")" "### Critical"
t_contains "$(cat "$T/calls/issue.body")" "[security] lib/config.sh:4: a token is printed"
t_eq "$(tail -n 1 "$T/calls/issue.body")" \
  'Apply `tehas:refine` to plan this into parts sized for one cycle.'
t_lacks "$(cat "$T/calls/issue.body")" "what the finding says at length"
t_contains "$t_out" "3 findings are filed in $findings_repo"
# The record moved on as it does with no filing at all.
t_eq "$(jq -r .exit "$record")" 10

t_case "a run that exited 20 files its findings the same way"
reset
findings_on
runner_says 20 "${findings_doc/\"exitCode\":10/\"exitCode\":20}"
t_run "$review"
t_eq "$t_rc" 20
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 1

t_case "a run that exited 0 files nothing"
reset
findings_on
runner_says 0 '{"verdict":"APPROVED","exitCode":0}'
t_run "$review"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "issue create"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$t_out" "are filed"

t_case "a review that names no repository files nothing and exits as it would have"
reset
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(t_calls gh)" ""
t_contains "$t_out" "TEHAS_REVIEW_REPO is not set"
t_eq "$(jq -r .exit "$record")" 10

t_case "an open issue of findings is commented on, and no second one is filed"
reset
findings_on
open_issues '[{"number":41,"title":"Six-lens review: 2 findings on main since 1111111"}]'
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 0
t_eq "$(grep -c '^issue comment' <<<"$(t_calls gh)")" 1
t_eq "$(cat "$T/calls/comment.number")" 41
t_contains "$(cat "$T/calls/comment.body")" "[security] lib/config.sh:4: a token is printed"
t_eq "$(tail -n 1 "$T/calls/comment.body")" \
  'Apply `tehas:refine` to plan this into parts sized for one cycle.'
t_contains "$t_out" "a comment on #41 of $findings_repo"

t_case "an open issue of another kind is no reason to comment instead of filing"
reset
findings_on
open_issues '[{"number":41,"title":"Slow tests"}]'
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 1
t_eq "$(grep -c '^issue comment' <<<"$(t_calls gh)")" 0

t_case "a GitHub that will not answer the question of what is open files the findings anyway"
reset
findings_on
: >"$T/fx/gh-list.rc"
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 1

t_case "what a finding holds is escaped and masked, as q_clean does"
reset
findings_on
findings_says
t_run "$review"
t_eq "$t_rc" 10
body=$(cat "$T/calls/issue.body")
t_lacks "$body" "<!-- sly -->"
# The marker left as q_clean leaves it, with its two backslashes. The expected
# line is a file the case made, so the shape of the answer is compared rather
# than its escaping: the masking of a token-shaped string below is the one that
# proves the text went through the mask of tehas_core.
t_contains "$body" sly
t_lacks "$body" "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
t_contains "$body" "ghp_***"

t_case "a filing GitHub refused is said, does not change the run's status, and keeps the record"
reset
findings_on
: >"$T/fx/gh-create.rc"
findings_says
t_run "$review"
t_eq "$t_rc" 10
t_contains "$t_out" "the findings were not filed"
t_contains "$t_out" "GitHub refused the issue in $findings_repo"
t_eq "$(jq -r .exit "$record")" 10

t_case "a run whose runner wrote no document files nothing and says so"
reset
findings_on
# The container answered the status and wrote nothing into its document: the
# file the run made stays empty, and the store has nothing to render. The two
# cases below are the same shape seen from each side: no file at all, and a
# file the store will not read as a review document. Both leave no findings.
runner_says 10 '{"exitCode":10}'
rm -f "$fx/doc.json"
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 0
t_contains "$t_out" "nothing to file"
t_eq "$(jq -r .exit "$record")" 10

t_case "a document the store refuses is said about and does not change the status"
reset
findings_on
# The document of the run is what the container wrote and the console masked
# into logs/. A document that is no review document at all - its findings are
# not an array - is refused by the store, which answers a body of no findings:
# the record moves on, GitHub is asked nothing and the line says so.
runner_says 10 '{"exitCode":10,"findings":"not an array"}'
t_run "$review"
t_eq "$t_rc" 10
t_eq "$(grep -c '^issue create' <<<"$(t_calls gh)")" 0
t_contains "$t_out" "nothing to file"
t_contains "$t_out" "left no findings"

t_case "the findings of a run are the only GitHub calls the review makes"
reset
findings_on
findings_says
t_run "$review"
t_eq "$(github_verbs)" "create list"
t_eq "$(grep -c -e 'GH_TOKEN' <<<"$(run_args)")" 0

t_done
