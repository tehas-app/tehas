#!/usr/bin/env bash
# What tehas work says when the read of the conflict check of a refresh cannot
# be judged (#2227), and what it says when the product's install cannot open its
# preflight log. A refresh that is not judged starts no container: the cycle log
# says which part of the answer was missing, and no note is written for a
# conflict nobody saw. A git with no such read at all is the shape of the host,
# and its start runs, as before.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

caps_fresh() { rm -rf "$T"/calls/* "$ws" "$ws.state" "$ws.before-retry"; }

# The image of these cases takes --continue-pr, so a refresh of them starts.
caps_continue

# The approval of a refresh of pull request 50 at <head>, as the dispatcher
# records it: refresh_of names the head.
refresh_at() { # <head>
  mkdir -p "$fx/net" "$T/home/tehas/state/cycles"
  issue_is "" NONE
  t_store_db "$T/home/tehas"
  jq -n --arg h "$1" '{links: [], refresh_of: {pr: 50, head_sha: $h, branch: "main"}}' >"$T/refresh-input.json"
  t_approve "$T/home/tehas" 7 alice refresh "$(date -u -d 2026-10-01T00:00:00Z +%s)" \
    --body-sha "$(jq -n '{body: ""}' | tehas_body_sha256)" --input-file "$T/refresh-input.json" \
    --refresh-pr 50 --refresh-head "$1"
  t_started "$T/home/tehas" 7
}

# A branch of the product off main that writes <line> to <file>: the branch's
# head, and the checkout holds it as its own fetch would.
pr_branch() { # <branch> <file> <line>
  git -C "$upstream" checkout -q -b "$1"
  printf '%s\n' "$3" >"$upstream/$2"
  git -C "$upstream" add -f -- "$2"
  git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m "the pull request"
  git -C "$upstream" rev-parse HEAD
  git -C "$upstream" checkout -q main
  git -C "$checkout" fetch -q "$upstream" "+refs/heads/*:refs/remotes/origin/*"
}

# The read of the conflict check is the one call of a start that holds the word
# `merge-tree` before the container is started, so a stub that answers only that
# call ends the read and hands every other call to the host's own git: the clone,
# the checkout's origin and the product's files are the host's own git.
# <status> is what the read answers, <line> the one line it says, and <tree> the
# tree object it wrote.
git_reads() { # <status> <the line git says, or nothing> [<the tree object it wrote>]
  local rc=$1 said=${2:-} tree=${3:-} real_git
  real_git=$(command -pv git)
  t_stub git "case \"\$*\" in
  *merge-tree*)
    [ -z '$tree' ] || printf '%s\\0%s\\0' '$tree' 'one.txt'
    ${said:+printf '%s\\n' '$said' >&2}
    exit $rc ;;
  *) exec $real_git \"\$@\" ;;
esac"
}

note7=$T/home/tehas/state/cycles/7.refresh-conflict

# The workspace holds no such commit: git ran the read, wrote no tree object and
# named the side it could not merge. That is an answer the host can act on, and
# it is no judgement of the merge: the start stops with 78 and says both.
t_case "a refresh whose conflict check named a side it could not merge starts no container: 78"
caps_fresh
caps_continue
head=$(pr_branch feature/none none.txt "a line of its own")
refresh_at "$head"
git_reads 1 "merge-tree: $head - not something we can merge"
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "whether pull request #50 conflicts with origin/main cannot be judged"
t_contains "$t_out" "the read of it names one side of the merge"
t_contains "$t_out" "not something we can merge"
# No note is written for a conflict nobody saw.
t_eq "$(find "$note7" 2>/dev/null)" ""
rm -f "$T/bin/git"
t_store_db "$T/home/tehas"
caps_good

# The read ended at the status of the kill that followed TERM, with what git
# managed to say: the words say the host waited the read out.
t_case "a refresh whose conflict check ran out of time starts no container: 78, with what git said"
caps_fresh
caps_continue
head=$(pr_branch feature/timed timed.txt "a line of its own")
refresh_at "$head"
git_reads 124 "fatal: bad object"
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "whether pull request #50 conflicts with origin/main cannot be judged"
t_contains "$t_out" "git did not answer within 120s: fatal: bad object"
t_eq "$(find "$note7" 2>/dev/null)" ""
rm -f "$T/bin/git"
t_store_db "$T/home/tehas"
caps_good

# A read that wrote a tree object and answered a status that is not an answer
# judged too little to trust: the words say what it answered with.
t_case "a refresh whose conflict check answered a status that is no answer starts no container: 78"
caps_fresh
caps_continue
head=$(pr_branch feature/strange strange.txt "a line of its own")
refresh_at "$head"
git_reads 3 "" "$(printf 'c%.0s' {1..40})"
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "cannot be judged"
t_contains "$t_out" "git wrote a tree object and answered 3 with 1 file(s) named"
t_eq "$(find "$note7" 2>/dev/null)" ""
rm -f "$T/bin/git"
t_store_db "$T/home/tehas"
caps_good

# A git with no such read at all is the shape of the host and no fault of the
# request: the start runs, as before, and the log carries what git said.
t_case "a refresh judged by a git with no such read runs, with git's words in the log"
caps_fresh
caps_continue
head=$(pr_branch feature/old old.txt "a line of its own")
refresh_at "$head"
git_reads 129 "usage: git merge-tree [--write-tree] [<options>] <branch1> <branch2>"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(cycle_args | tr '\n' ' ')" "--continue-pr"
t_contains "$t_out" "usage: git merge-tree"
t_eq "$(find "$note7" 2>/dev/null)" ""
rm -f "$T/bin/git"
t_store_db "$T/home/tehas"
caps_good

# The preflight log of the cycle is where the product's install writes its own
# The start that meets it says so with the name and the fault of the system, and
# is not the 127 of a program that was not there. The name of the log a start
# writes is made of the second it began, so the case names it itself and holds it
# open as a directory, which nothing opens for append: it is the newest name of
# the issue's logs, and the start reads the issue's log from the name the case put
# in its record. The run of the product's install ends 78, names the log and the
# fault of the system in place of the 127 of a program that was not there, and
# tells nothing of a sandbox that refused to push. The push check holds the same
# name and is read before the install; its half is tried beside the code, where
# the name of the log is given to the step.
t_case "an install whose preflight log cannot be opened says that, with the path, and is not 127"
caps_fresh
mkdir -p "$upstream/extension"
echo '{}' >"$upstream/extension/bun.lock"
git -C "$upstream" add -f -- extension/bun.lock
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m lock
git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
# The two runs of the case each read an approval of their own: the ordinary one,
# The name of the log a start writes is made of the second it began, so the fault
# is made at the second run of the issue: the first is an ordinary start, whose
# install wrote its lines beside its own log, and the second is that same run
# again, with its own log's name held open as a directory, which nothing opens for
# append. The install is the first of the two steps to come to it, so the record
# is an ordinary start's and no read of the conflict check is asked of it.
caps_fresh
caps_continue
refresh_at "$(git -C "$upstream" rev-parse main)"
t_run "$work 7"
log=$(printf '%s\n' "$t_out" | sed -n 's/^tehas work: issue 7 started, log //p')
[ -n "$log" ] || t_bad "the run named no log of its own"
[ -f "$log.preflight" ] || t_bad "the run left no preflight log beside its log"
grep -q "^install extension/" "$log.preflight" || t_bad "the install wrote no line of its own"
# The two steps hold the one name, and the check is read first, so the same fault
# is read twice: the check ends the start where the install would have, with the
# name of the log and the fault of the system on it, and neither is told as a
# sandbox that refused to push. The file the check made is put back as the name of
# the install's own: the step is the one that reads back the lines from it, and
# meets the fault of the filesystem in place of the 127 of a program that was not
# there.
caps_fresh
caps_continue
refresh_at "$(git -C "$upstream" rev-parse main)"
rm -f "$log.preflight"
# The name of a start's log is the second it began, so the run before put this
# one's name in the past and the start names the log of its own second and opens
# the preflight beside it. The plant on $log.preflight is read only while the
# clock is still the second of the run before it; a start that names a log of its
# own opens a preflight the case never held and runs as an ordinary start. That is
# the clock of the host, not the fault the case is about, so a directory is held
# on the name of the log of the second the start is made in, and the second after
# it, beside the one the case already holds: the start's own preflight is then a
# directory, whichever of the two seconds it names, and the fault is met.
now=$(date -u +%y%m%d-%H%M%S)
for s in "$now" "$(date -u -d '+1 second' +%y%m%d-%H%M%S)" "$(date -u -d '+2 seconds' +%y%m%d-%H%M%S)" "$log"; do
  mkdir -p "$T/home/tehas/logs/work-7-$s.jsonl.preflight"
done
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "could not be opened for the push check of the sandbox"
# The name in the line is the preflight the start came to, which is the name of
# its own log: read from what the start says of itself, not the one the case held.
reported=$(printf '%s\n' "$t_out" | sed -n "s|^.*the preflight log \(.*\.preflight\) could not be opened.*|\1|p")
[ -d "$reported" ] || t_bad "the fault named no directory the case planted: $reported"
t_lacks "$t_out" "the sandbox cannot push"
t_lacks "$t_out" "install failed"
t_lacks "$t_out" "(exit 127)"
t_eq "$(find "$T/home/tehas/state/push-fault.json" 2>/dev/null | wc -l)" 0
find "$T"/home/tehas/logs -name '*.preflight' -type d -exec rmdir {} + 2>/dev/null
rm -f "$log.preflight"
# The install reads the same name after the check, and its half of the fault is
# tried beside the code, where the name of the log is given to the step: a case
# cannot hold the name open between the two steps of one run
# (console/cycle/src/prepare.rs).
T_ENV=("${env_ok[@]}")

t_done
