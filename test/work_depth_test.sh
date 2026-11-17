#!/usr/bin/env bash
# Tests for the history of a cycle's workspace, TEHAS_CLONE_DEPTH: the default
# branch alone at the depth, all of it for 0, and a value that is no depth
# stopping the start before anything is cloned. A refresh whose pull request
# started beyond the depth has its workspace deepened to the commit the branch
# started from, so the conflict check judges the merge and
# `git merge-base --is-ancestor` answers there as in a full clone.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

fresh() { rm -rf "$T"/calls/* "$ws" "$ws.state" "$ws.before-retry"; }
commit() { # <dir> <subject> [<file> <line>]
  [ -z "${3:-}" ] || { printf '%s\n' "$4" >"$1/$3" && git -C "$1" add -f -- "$3"; }
  git -C "$1" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$2"
}
fetched() { git -C "$checkout" fetch -q "$upstream" "+refs/heads/*:refs/remotes/origin/*"; }
note7=$T/home/tehas/state/cycles/7.refresh-conflict

# Twenty commits on main and a branch of the product beside it: the workspace
# of a depth of 3 holds the last three of main and nothing of the branch.
for n in $(seq 1 20); do commit "$upstream" "main $n"; done
git -C "$upstream" branch other HEAD~5
fetched
total=$(git -C "$upstream" rev-list --count main)

t_case "a new cycle's workspace holds the default branch alone, at the depth the instance sets"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=3
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(git -C "$ws" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_eq "$(git -C "$ws" rev-list --count HEAD)" 3
t_eq "$(git -C "$ws" rev-parse --is-shallow-repository)" true
t_eq "$(git -C "$ws" for-each-ref --format='%(refname)' | tr '\n' ' ')" \
  "refs/heads/main refs/remotes/origin/HEAD refs/remotes/origin/main "
t_eq "$(git -C "$ws" tag)" ""

t_case "the default depth is 100 commits"
fresh
t_instance "${T_MINIMAL[@]}"
for n in $(seq 21 110); do commit "$upstream" "main $n"; done
fetched
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(git -C "$ws" rev-list --count HEAD)" 100
total=$(git -C "$upstream" rev-list --count main)

t_case "TEHAS_CLONE_DEPTH=0 gives the workspace full history"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=0
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(git -C "$ws" rev-list --count HEAD)" "$total"
t_eq "$(git -C "$ws" rev-parse --is-shallow-repository)" false

t_case "a depth that is no whole number stops the start with 78 and names it, before anything is cloned"
for bad in -1 full 01; do
  fresh
  t_instance "${T_MINIMAL[@]}" "TEHAS_CLONE_DEPTH=$bad"
  t_run "$work 7"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CLONE_DEPTH is not valid: expected a whole number of 0 or more"
  t_eq "$(find "$ws" -maxdepth 0 2>/dev/null | wc -l)" 0
  t_eq "$(cycle_args)" ""
done

# --- A refresh of a pull request that started beyond the depth ---------------
caps_continue
# The approval of a refresh of pull request 50 at <head>, as the dispatcher
# records it.
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
# A pull request's branch off main of <n> commits, the first writing <file>,
# then main moved on by 8 commits, the last writing <file> too when a fifth
# word says so. Prints the branch's first head, then its last.
old_pr() { # <branch> <file> <n> <line> [conflict]
  git -C "$upstream" checkout -q -b "$1" main
  commit "$upstream" "the pull request" "$2" "$4"
  git -C "$upstream" rev-parse HEAD
  for n in $(seq 2 "$3"); do commit "$upstream" "the pull request $n"; done
  git -C "$upstream" rev-parse HEAD
  git -C "$upstream" checkout -q main
  for n in $(seq 1 7); do commit "$upstream" "main moves $n"; done
  if [ "${5:-}" = conflict ]; then
    commit "$upstream" "main moves 8" "$2" "main moved on"
  else
    commit "$upstream" "main moves 8"
  fi
  fetched
}

t_case "a conflict refresh on a branch that started beyond the depth is judged, and names the file"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=3
heads=$(old_pr feature/far far.txt 2 "the pull request's line" conflict)
head=${heads##*$'\n'}
base=$(git -C "$upstream" merge-base main "$head")
refresh_at "$head"
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "pull request #50 conflicts with origin/main in 1 file(s)"
t_eq "$(jq -c . "$note7")" '{"branch":"main","more":0,"paths":["far.txt"],"pr":50}'
# The workspace holds the start of the branch, and is still shallow below it.
t_eq "$(git -C "$ws" merge-base origin/main "$head")" "$base"
t_eq "$(git -C "$ws" rev-parse --is-shallow-repository)" true
rm -f "$note7"
t_store_db "$T/home/tehas"

t_case "a pull request picked up again: its head reviewed earlier, beyond the depth, is an ancestor of its head"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=3
heads=$(old_pr feature/long long.txt 12 "a file of its own")
first=${heads%%$'\n'*}
head=${heads##*$'\n'}
refresh_at "$head"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(cycle_args | tr '\n' ' ')" "--continue-pr"
git -C "$ws" merge-base --is-ancestor "$first" "$head"
t_eq "$?" 0
t_eq "$(git -C "$ws" merge-base origin/main "$head")" "$(git -C "$upstream" merge-base main "$head")"
t_store_db "$T/home/tehas"

t_case "a refresh whose head the checkout does not hold starts no container: 78, and says why"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=3
refresh_at "$(printf 'b%.0s' {1..40})"
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_contains "$t_out" "the workspace of the refresh of pull request #50 cannot hold the commit its branch started from"
t_store_db "$T/home/tehas"

t_done
