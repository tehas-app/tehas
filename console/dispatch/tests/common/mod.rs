//! What the tests of the poll share, as test/dispatch_lib.sh and
//! test/dispatch_procs_lib.sh did: `tehasbot dispatch` through the console,
//! with tehas-dispatch beside it, against a stub gh, the real store and a
//! scratch checkout of acme/widget whose default branch is main.
//!
//! The world starts as reset() made it: #7 labelled tehas:ready by alice, one
//! slot, a slice of 20 GiB with nothing in it and 40 GB of disk. Fixtures are
//! files in `fx/`, named as the stubs read them (the comments of GH and PAGE
//! say which); every stub records its calls in `calls/<name>`.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use tehas_core::testdir::TestDir;
use tehas_core::testing::{copy_exe, write_exe};

/// The answers of the stub gh, run by the wrapper in `gh_stub`. `$T/fx/...`
/// are the fixtures; a call reads the words past `$2` through `rest_verb`
/// and `rest_path`.
const GH: &str = r####"# rest_verb, rest_path: the verb and the path of a call of the form
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
# comments_json <n>: the comments of #n as the GraphQL comment read gives them,
# from the answer a case wrote or from its REST answer.
comments_json() {
  cat "$T/fx/comments-$1.json" 2>/dev/null ||
    jq "{data: {repository: {issue: {comments: {nodes: [.[] | {databaseId: .id, author: ({login: .user.login} + (.user | with_entries(select(.key == \"id\")))),
      body, createdAt: .created_at, lastEditedAt: (if .updated_at == .created_at then null else .updated_at end),
      editor: (if .updated_at == .created_at then null else {login: .user.login} + (.user | with_entries(select(.key == \"id\"))) end)}]}}}}}" "$T/fx/icomments-$1.json"
}
# not_found: say what GitHub says for a path it does not have, and fail.
not_found() { echo "message: Not Found" >&2; exit 1; }
case "$1 $2" in
  "repo view") if [ -e "$T/fx/gh-stderr-fails" ]; then
      cat "$T/fx/gh-stderr" >&2 2>/dev/null; exit 1; fi
    cat "$T/fx/repo.json" || exit 1 ;;
  # The state of an issue, which the poll reads before it posts a planning
  # turn, and the plain read of an issue for every other caller; both reach
  # this arm. The state read asks GitHub for the one field state and is told
  # from the plain read by the position of that word, not a substring test: the
  # page read of the poll is also a gh call whose query text holds the word
  # states, so a substring match would answer that GraphQL read here. Only a
  # call that names state beside --json is the state read. Its answer and the
  # plain read are said below.
  "issue view")
    asks_state=""
    prev=""
    for a in "$@"; do
      if [ "$prev" = --json ] && [ "$a" = state ]; then asks_state=1; fi
      prev=$a
    done
    if [ -n "$asks_state" ]; then
      # The state of an issue, which the poll reads before it posts a planning
      # turn. A case that means a closed issue writes fx/state-<n>.json and that
      # file answers the read; an issue no case closed is open, as an issue of
      # GitHub that no one closed is. The quotes are escaped with a backslash,
      # as the body loses a single quote and a bare backslash on the way here,
      # and no bare $ appears, which the test shell would expand.
      if [ -f "$T/fx/state-$3.json" ]; then
        cat "$T/fx/state-$3.json" || exit 1
      else
        jq -cn "{state: \"OPEN\"}"
      fi
    elif [ -f "$T/fx/issue-$3.json" ]; then
      cat "$T/fx/issue-$3.json" || exit 1
    else
      cat "$T/fx/control.json" || exit 1
    fi ;;
  "issue list")
    # The closed issues console tidy run searches for, when a case names some.
    case "$*" in *"--state closed"*) [ ! -e "$T/fx/closed.json" ] || exec cat "$T/fx/closed.json" ;; esac
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
  # The reviews and inline comments of a pull request, as REST lists them.
  "api repos/acme/widget/pulls/"*"/reviews?per_page=100" | "api repos/acme/widget/pulls/"*"/comments?per_page=100")
    n=${2#repos/acme/widget/pulls/}
    case "$2" in */reviews*) f=reviews ;; *) f=inline ;; esac
    cat "$T/fx/$f-${n%%/*}.json" 2>/dev/null || echo "[]" ;;
  "api repos/acme/widget/pulls/"*) cat "$T/fx/pull-${2##*/}.json" 2>/dev/null || echo "{}" ;;
  # Who may write: everyone, unless a case writes $fx/perm-<login>.json.
  "api repos/acme/widget/collaborators/"*"/permission")
    n=${2#repos/acme/widget/collaborators/}
    cat "$T/fx/perm-${n%%/*}.json" 2>/dev/null || echo "{\"permission\":\"write\",\"role_name\":\"write\"}" ;;
  "api repos/acme/widget/issues/"*"/comments") [ ! -e "$T/fx/post-fails" ] || exit 1; echo 4002 ;;
  "api repos/acme/widget/issues/"*"/comments?per_page=100")
    n=${2#repos/acme/widget/issues/}
    # None written is none at all; $fx/icomments-fails makes the read fail.
    [ ! -e "$T/fx/icomments-fails" ] || exit 1
    cat "$T/fx/icomments-${n%%/*}.json" 2>/dev/null || echo "[]" ;;
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
      # Or the REST answer a case wrote, as GraphQL gives it. The poll asks a
      # chunk of issues in one query, one alias i<n> an issue, and the stub
      # answers them in one answer; a number the case wrote no comments for
      # refuses the whole chunk, as GitHub refuses the one query.
      *"comments(last"*) if [ -n "$n" ]; then comments_json "$n" || exit 1; else
          acc="{}"
          for k in $(printf "%s\n" "$*" | grep -o "i[0-9]*: issue(number:" | sed "s/^i\([0-9]*\):.*/\1/"); do
            one=$(comments_json "$k") || exit 1
            acc=$(jq -c --arg k "i$k" --argjson one "$one" ". + {(\$k): \$one.data.repository.issue}" <<<"$acc") || exit 1
          done
          echo "{\"data\":{\"repository\":$acc}}"
        fi ;;
      # The edit histories of edited comments, by their node ids: the
      # comment-edits.json a case wrote, else none. It is asked before the
      # body histories of the issues below, whose query holds the same words.
      *"nodes(ids:"*) [ ! -e "$T/fx/comment-edits-fail" ] || exit 1
        cat "$T/fx/comment-edits.json" 2>/dev/null || echo "{\"data\":{\"nodes\":[]}}" ;;
      # The body edit histories of a chunk of issues, one alias i<n> an issue:
      # edits-<n>.json when a case wrote it, else the last edit of
      # edit-<n>.json (or edit.json) alone.
      *"userContentEdits(last"*) [ ! -e "$T/fx/edits-fail" ] || exit 1
        acc="{}"
        for k in $(printf "%s\n" "$*" | grep -o "i[0-9]*: issue(number:" | sed "s/^i\([0-9]*\):.*/\1/"); do
          f="$T/fx/edit-$k.json"
          [ -r "$f" ] || f="$T/fx/edit.json"
          one=$(cat "$T/fx/edits-$k.json" 2>/dev/null) ||
            one=$(jq -c ".data.repository.issue | if .lastEditedAt then {totalCount: 1, nodes: [{editedAt: .lastEditedAt, editor}]} else {totalCount: 0, nodes: []} end" "$f") || exit 1
          acc=$(jq -c --arg k "i$k" --argjson one "$one" ". + {(\$k): {userContentEdits: \$one}}" <<<"$acc") || exit 1
        done
        echo "{\"data\":{\"repository\":$acc}}" ;;
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
    # as approval-view shapes it: .approval the one waiting, .current the
    # one of the last start.
    case "$*" in *"--add-label tehas:running"*) TEHAS_HOME=$T/home/tehas "$T/store-bin" approval-current "$3" >"$T/calls/approved.at-mark" 2>/dev/null ;; esac
    [ ! -e "$T/fx/edit-fails" ] || exit 1
    # An edit whose arguments contain the text in edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(<"$T/fx/edit-fails-on")"*) cat "$T/fx/gh-stderr" >&2 2>/dev/null; exit 1 ;; esac
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
esac"####;

/// The open issues as tehas-dispatch's one GraphQL page asks for them, made
/// from the fixtures: the lists name the issues and their labels,
/// events-<n>.json their label events, work-<n>.json and edit-<n>.json (or
/// edit.json) their text, prs-<n>.json (or prs.json) the pull requests that
/// close them, and issue-<n>.json with a state their state and labels. A
/// cycle that holds its lock is marked running by the bot.
const PAGE: &str = r####"#!/usr/bin/env bash
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
  --argjson pulls "$(read_or "$fx/pulls.json" "" null)" \
  --argjson tlabels "$(read_or "$fx/labels.json" "" null)" \
  --argjson running '{"__typename":"LabeledEvent","createdAt":"2026-01-01T00:00:00Z","actor":{"login":"tehas-bot"},"label":{"name":"tehas:running"}}' '
  def labels($n): [$lists[][] | select((.number | tonumber?) == $n) | (.labels // [])[].name]
    + (if $n == 1 then [$control.labels[].name] else [] end)
    # Being on a list is carrying its label, as gh issue list --label has it.
    + [range(7) as $i | select(any($lists[$i][]; (.number | tonumber?) == $n))
       | ["tehas:ready", "tehas:blocked", "tehas:question", "tehas:plan", "tehas:refine",
          "tehas:awaiting-merge", "tehas:running"][$i]] | unique;
  # GraphQL names the user id databaseId; an id the REST fixture gives is kept.
  def actor: if .actor == null then null else {login: .actor.login}
    + (if .actor | has("id") then {databaseId: .actor.id} else {} end) end;
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
        issueDependenciesSummary: (if .w.issue_dependencies_summary then {blockedBy: .w.issue_dependencies_summary.blocked_by} else null end),
        # Parenthesised: jq 1.7, as in the sandbox image, takes no + in an object value.
        timelineItems: {nodes: ([.ev[] | select($dropped | not) | {__typename: (if .event == "labeled" then "LabeledEvent"
            elif .event == "unlabeled" then "UnlabeledEvent" else "RenamedTitleEvent" end),
          createdAt: .created_at, actor: actor, label: .label}] + [select(any($locked[]; . == $i.n) and ($dropped | not)) | $running])},
        # A linked pull request is of the bot on a branch of the repository,
        # unless a case says otherwise.
        closedByPullRequestsReferences: {nodes: ((.p.data.repository.issue.closedByPullRequestsReferences.nodes // [])
          | map({isCrossRepository: false, author: {login: "tehas-bot"}, headRepository: {nameWithOwner: "acme/widget"}} + .))}}]},
    closedRunning: {nodes: [$lists[][] | select(.state == "CLOSED") | {number}]}}
    # The open pull requests, when a case wrote them.
    + (if $pulls == null then {} else {openPulls: {nodes: $pulls}} end)
    # The tehas: labels with their node ids, when a case wrote them.
    + (if $tlabels == null then {} else {tehasLabels: {nodes: $tlabels}} end)}}'
"####;

/// The other stubs, by name: what each does after it records its call.
/// @TIMEOUT@ is the real timeout.
const STUBS: &[(&str, &str)] = &[
    // systemctl --user start tehas-cycle@<n>.service: starts the record on the
    // real store under the lock, as a cycle does; start-fails, start-silent and
    // start-refused say otherwise.
    (
        "fake-start",
        r####"[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ ! -e "$T/fx/start-silent" ] || exit 0
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >>"$T/calls/cycle-start" 2>&1 || exit 0
[ ! -e "$T/fx/start-refused" ] ||
  flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-end "$1" --attempt "$(TEHAS_HOME=$TEHAS_HOME "$T/store-bin" cycle-show "$1" | jq .cycle.attempt)" --refused >/dev/null 2>&1"####,
    ),
    ("fake-status", ""),
    (
        "fake-tidy",
        r####"[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/tidy.after-start""####,
    ),
    // tehasbot health, which no case reads: real_health() names the real one.
    ("fake-health", ""),
    // The wait for a planning turn returns at once; every other sleep is real.
    (
        "sleep",
        r####"[ "$1" = 10 ] || PATH=/usr/bin:/bin exec sleep "$@""####,
    ),
    // A bound around a stub guards nothing: only the names that can hang, and
    // those in fx/timeout-hangs, go through the real timeout.
    (
        "timeout",
        r####"name=${2##*/}
[ "$1" = -k ] && name=${4##*/}
hang=""
[ ! -s "$T/fx/timeout-hangs" ] || hang=$(<"$T/fx/timeout-hangs")
case " df docker systemctl gh git curl sleep gate-run analytics attach  $hang " in
  *" $name "*) exec @TIMEOUT@ "$@" ;;
esac
[ "$1" != -k ] || shift 2
shift
exec "$@""####,
    ),
    // A planning turn: writes the outcome plan-result.json, or fails with
    // plan-fails.
    (
        "fake-plan",
        r####"TEHAS_HOME=$T/home/tehas "$T/store-bin" plan-show "$1" >"$T/calls/record.at-turn" 2>/dev/null
[ ! -e "$T/fx/image.during" ] || mv "$T/fx/image.during" "$T/fx/image.label"
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/plan.fd8"
out=""
keep=""
while [ "$#" -gt 0 ]; do
  [ "$1" = --out ] && out=${2-}
  [ "$1" = --keep ] && keep=${2-}
  shift
done
if [ -e "$T/fx/plan-fails" ]; then
  [ ! -s "$T/fx/plan-fails" ] || cp "$T/fx/plan-fails" "$out"
  exit 2
fi
cp "$T/fx/plan-result.json" "$out"
# As `console plan` does: the outcome that names an update is made from the
# draft the turn kept beside the session, in a directory of workspaces the turn
# made. A result with no update keeps none.
if [ -n "$keep" ] && grep -q update "$T/fx/plan-result.json"; then
  works=${keep%/*}
  mkdir -p "$works" && : >"$works/.tehas-workspaces"
  mkdir -p "$keep"
  jq -c "{title: .update.title, spec: .update.body}" "$T/fx/plan-result.json" >"$keep/draft.json"
fi"####,
    ),
    (
        "fake-plan-start",
        r####"n=$1; shift
[ ! -e "$T/fx/plan-runs" ] || exit 0
"$@""####,
    ),
    (
        "fake-index",
        r####"git -C "$T/home/tehas/repos/widget" rev-parse HEAD >"$T/calls/index.head"
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/index.fd8"
[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/index.late"
[ ! -e "$T/fx/index-fails" ] || exit 1"####,
    ),
    (
        "fake-index-start",
        r####"[ ! -e "$T/fx/index-runs" ] || exit 0
"$@""####,
    ),
    (
        "systemctl",
        r####"case "$1 $2" in
  "show tehas.slice")
    [ ! -e "$T/fx/slice-hangs" ] || exec sleep 20
    cat "$T/fx/slice.show" ;;
  "--user show")
    if [ -e "$T/fx/plan-runs" ] && [ "${6:-}" = "tehas-plan-$(<"$T/fx/plan-runs").service" ]; then echo active; else echo inactive; fi ;;
esac"####,
    ),
    (
        "docker",
        r####"case "$1" in
  info) case "$*" in
      # The two questions the host asks Docker, both answered from
      # $T/fx/docker.info: the slice probe wants the cgroup driver and version,
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
esac"####,
    ),
    // df of the file system its second argument names: fx/free, or
    // fx/free-docker for Docker's own directory; 40 GB without either.
    (
        "df",
        r####"[ ! -e "$T/fx/df-fails" ] || exit 1
[ ! -e "$T/fx/df-fails-of" ] || [ "${2%/}" = "$(<"$T/fx/df-fails-of")" ] || exit 1
docker_dir=$(sed -n 2p <"$T/fx/docker.info" 2>/dev/null)
if [ -e "$T/fx/docker-dir" ]; then docker_dir=""; fi
if [ -e "$T/fx/free" ]; then answer=$(<"$T/fx/free"); else answer=41943040; fi
if [ -n "$docker_dir" ] && [ "${2%/}" = "$docker_dir" ]; then
  answer=$(<"$T/fx/free-docker")
fi
[[ $answer =~ ^(0|[1-9][0-9]{0,17})$ ]] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $answer 1% /""####,
    ),
    ("nproc", "echo 8"),
];

/// The stub curl of mesh_curl(): what it was given goes to calls/curl-config
/// and calls/curl-body; the endpoint answers mesh.json, and a request with an
/// image is answered when reads-images is there.
const CURL: &str = r####"conf=""
case " $* " in *" -K - "*) conf=$(cat) ;; esac
for a; do case "$a" in /dev/fd/*) cat "$a" >>"$T/calls/curl-config" ;; esac; done
if [ -n "$conf" ]; then
  printf "%s\n" "$conf" | grep -v "^data = " >>"$T/calls/curl-config"
  body=$(printf "%s\n" "$conf" | sed -n "s/^data = //p")
  [ -z "$body" ] || jq -r . <<<"$body" >"$T/calls/curl-body"
fi
heard() { [ -n "$conf" ] || cat >"$T/calls/curl-body"; }
case "$*" in
  *"/chat/completions") heard; [ -e "$T/fx/reads-images" ] ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac"####;

/// The values of a minimal valid instance (T_MINIMAL).
pub const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

/// The logins whose user ids install.sh records, as the shell tests list them.
const LOGINS: &[&str] = &[
    "tehas-bot",
    "alice",
    "bob",
    "bob-2",
    "carol",
    "boss",
    "tehas-fixer",
    "tehas-mill",
    "millbot",
    "mill-bot",
    "acme-millwright",
    "acme-app-millwright",
];

/// The programs of the host a poll reaches for, which procs_reset() shims.
const PROC_NAMES: &[&str] = &[
    "awk", "basename", "cat", "chmod", "cmp", "cp", "cut", "date", "dirname", "du", "find", "grep",
    "head", "jq", "ln", "mkdir", "mktemp", "mv", "rm", "sed", "sleep", "sort", "stat", "tail",
    "timeout", "tr", "uname", "wc", "whoami",
];

/// Files of records that live on the store now, as (directory, start, end):
/// no poll leaves one. A start of "1" is any digit 1 to 9.
const OLD_STATE: &[(&str, &str, &str)] = &[
    ("state/cycles", "", ".taken"),
    ("state/cycles", "", ".by"),
    ("state/cycles", "", ".requeued"),
    ("state/cycles", "", ".rukas"),
    ("state/cycles", "", ".image"),
    ("state/cycles", "", ".settings"),
    ("state/cycles", "", ".branch"),
    ("state/cycles", "", ".approved.json"),
    ("state/cycles", "", ".approved.json.new"),
    ("state/cycles", "1", ".history.jsonl"),
    ("state/cycles", "", ".park"),
    ("state/cycles", "", ".park.new"),
    ("state", "park-", ".id"),
    ("state", "park-", ".json"),
    ("state/cycles", "", ".quiet"),
    ("state", "plan-", ".json"),
    ("state", "plan-", ".json.new"),
    ("state", "plan-", ".json.brief"),
    ("state", "plan-", ".json.answer"),
    ("state", "plan-", ".json.answer.new"),
    ("state", "plan-", ".json.keep"),
    ("state", "plan-", ".turn"),
    ("state/cycles", "plan-", ".history.jsonl"),
    ("state", "plan-", ".history.jsonl"),
];

/// The fixtures reset() takes away, by exact name or, ending in `*`, by start.
const RESET_FX: &[&str] = &[
    "pulls.json",
    "pull-*",
    "edit-*",
    "edits-*",
    "start-*",
    "events-*",
    "comments-*",
    "icomments-*",
    "perm-*",
    "reviews-*",
    "inline-*",
    "prs.json",
    "prlist.json",
    "pr-fails",
    "issue-*",
    "work-*",
    "post-fails",
    "patch-fails",
    "plan-fails",
    "plan-runs",
    "image.during",
    "slice-*",
    "orphan-*",
    "prs-*",
    "blockers*",
    "blocked.json",
    "checks*",
    "mark-fails",
    "mark-fails-times",
    "unmark-fails",
    "label-missing",
    "gh-stderr",
    "gh-stderr-fails",
    "closed.json",
    "labels.json",
];

/// The paths under the home reset() takes away, `*` as in RESET_FX.
const RESET_HOME: &[&str] = &[
    "libexec",
    "state/labels",
    "state/cycles",
    "state/plan-*",
    "live",
    "state/pause.json",
    "state/limit.json",
    "state/branch-check",
    "state/github-failures",
    "state/push-fault.json",
    "work/widget-plan-*",
    "tmp/plan-*",
];

pub const GIB: u64 = 1_073_741_824;

/// The checkout of this source: lib/ and bin/ come from it.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The programs of the workspace a poll runs besides tehas-dispatch.
struct Programs {
    store: PathBuf,
    console: PathBuf,
    attach: PathBuf,
}

/// The store, the console and the attachment checker of this build: above the
/// harness when the workspace built them, built once here otherwise.
fn programs() -> Programs {
    fn above(name: &str) -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    }
    fn found() -> Option<Programs> {
        Some(Programs {
            store: above("store")?,
            console: above("console")?,
            attach: above("attach")?,
        })
    }
    if let Some(p) = found() {
        return p;
    }
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
        let out = Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--offline",
                "--quiet",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .args(["-p", "store", "-p", "console", "-p", "attach"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .expect("cargo");
        assert!(
            out.status.success(),
            "the programs did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    found().expect("the programs of this build are above the test binary")
}

/// A program on the PATH of the tests, by its path.
fn which(name: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/usr/bin/{name}"))
}

/// A hard link of a built program, or a copy where the two cannot link.
fn place(from: &Path, to: &Path) {
    let _ = fs::remove_file(to);
    if fs::hard_link(from, to).is_err() {
        copy_exe(from, to).unwrap();
    }
}

/// The end of one run: its exit status and what it printed, both streams.
#[derive(Debug)]
pub struct Run {
    pub rc: i32,
    pub out: String,
}

/// One host, as reset() leaves it.
pub struct World {
    t: TestDir,
    /// `~/tehas` of the host.
    pub home: PathBuf,
    /// The fixtures.
    pub fx: PathBuf,
    bash: String,
    console: PathBuf,
    env: RefCell<Vec<(String, String)>>,
    always: RefCell<Vec<(String, String)>>,
    held: RefCell<Vec<File>>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl World {
    pub fn new(name: &str) -> World {
        let t = TestDir::new(&format!("dispatch-{name}"));
        let home = t.join("home/tehas");
        for d in [
            "bin",
            "calls",
            "deny",
            "fx",
            "tmp",
            "front/bin",
            "front/libexec",
            "dispatch/libexec",
        ] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        fs::create_dir_all(&home).unwrap();
        let p = programs();
        copy_exe(&p.store, t.join("store-bin")).unwrap();
        copy_exe(&p.attach, t.join("attach-bin")).unwrap();
        let lib = root().join("lib");
        for tree in ["front", "dispatch"] {
            std::os::unix::fs::symlink(&lib, t.join(tree).join("lib")).unwrap();
        }
        for verb in ["tehas", "tehasbot"] {
            fs::copy(
                root().join("bin").join(verb),
                t.join("front/bin").join(verb),
            )
            .unwrap();
        }
        place(&p.console, &t.join("front/libexec/console"));
        place(
            Path::new(env!("CARGO_BIN_EXE_tehas-dispatch")),
            &t.join("dispatch/libexec/tehas-dispatch"),
        );
        let w = World {
            fx: t.join("fx"),
            home,
            bash: which("bash"),
            console: t.join("front/libexec/console"),
            t,
            env: RefCell::new(vec![]),
            always: RefCell::new(vec![]),
            held: RefCell::new(vec![]),
        };
        for name in ["docker", "gh", "systemctl", "curl", "ssh"] {
            write_exe(
                w.t.join("deny").join(name),
                format!(
                    "#!{}\necho \"{name} $*\" >>\"$T/denied\"\necho \"no stub stands in for {name}\" >&2\nexit 97\n",
                    w.bash
                ),
            )
            .unwrap();
        }
        w.gh_stub(GH);
        let timeout = which("timeout");
        for (name, body) in STUBS {
            w.stub(name, &body.replace("@TIMEOUT@", &timeout));
        }
        write_exe(w.t.join("page-stub"), PAGE).unwrap();
        w.fs("gh-n.lock", "");
        let upstream = w.upstream();
        fs::create_dir_all(&upstream).unwrap();
        git(&["init", "-q", "-b", "main"], &upstream);
        git(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
            &upstream,
        );
        let out = Command::new("git")
            .args(["clone", "-q"])
            .arg(&upstream)
            .arg(w.checkout())
            .output()
            .unwrap();
        assert!(out.status.success(), "git clone: {out:?}");
        w.reset();
        w
    }

    /// The world of the shell's reset(): the fixtures and the home of a case
    /// before it says anything, a fresh store, and the settings and the stubs
    /// the poll is given.
    pub fn reset(&self) {
        self.clear_calls();
        for name in RESET_FX {
            remove_like(&self.fx, name);
        }
        for path in RESET_HOME {
            let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
            remove_like(&self.home.join(dir), name);
        }
        self.store_home();
        self.slice_show(&(20 * GIB).to_string(), "");
        self.df_reset();
        self.slice_state("0.00", "");
        self.fx("asking.json", "[]");
        self.fx("plans.json", "[]");
        self.fx("refines.json", "[]");
        self.fx("user.json", r#"{"login":"tehas-bot"}"#);
        self.fx(
            "edit.json",
            r#"{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}"#,
        );
        self.fx(
            "prs.json",
            r#"{"data":{"repository":{"issue":{"closedByPullRequestsReferences":{"nodes":[]}}}}}"#,
        );
        self.fx("repo.json", r#"{"nameWithOwner":"acme/widget"}"#);
        self.fx("control.json", r#"{"labels":[]}"#);
        self.fx(
            "plan-result.json",
            r#"{"session":"abc-123","text":"Two ways.","question":{"question":"Adopt?","options":[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]}}"#,
        );
        self.fx("awaiting.json", "[]");
        self.fx("marked.json", "[]");
        self.fx(
            "ready.json",
            r#"[{"number":7,"labels":[{"name":"tehas:ready"}]}]"#,
        );
        self.fx(
            "events-7.json",
            r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]"#,
        );
        self.fx("events-1.json", "[]");
        let cgroup = self.t.join("cgroup").to_string_lossy().into_owned();
        *self.env.borrow_mut() = [
            ("TEHAS_CYCLE_START_CMD", "fake-start"),
            ("TEHAS_CYCLE_START_WAIT", "2"),
            ("TEHAS_STATUS_CMD", "fake-status"),
            ("TEHAS_TIDY_CMD", "fake-tidy"),
            ("TEHAS_INDEX_CMD", "fake-index"),
            ("TEHAS_INDEX_START_CMD", "fake-index-start"),
            ("TEHAS_PLAN_CMD", "fake-plan"),
            ("TEHAS_PLAN_START_CMD", "fake-plan-start"),
            ("TEHAS_CGROUP_ROOT", &cgroup),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let poll = self.t.join("dispatch/libexec/tehas-dispatch");
        *self.always.borrow_mut() = vec![
            (
                "TEHAS_DISPATCH_CMD".into(),
                poll.to_string_lossy().into_owned(),
            ),
            ("TEHAS_HEALTH_CMD".into(), "fake-health".into()),
        ];
        self.slots(1, &[]);
    }

    /// The real store in the home, a fresh database, the attachment checker
    /// beside it and the account ids (t_store_home).
    fn store_home(&self) {
        let libexec = self.home.join("libexec");
        fs::create_dir_all(&libexec).unwrap();
        for f in ["store-answers", "store-answers.n", "calls/store"] {
            let _ = fs::remove_file(self.t.join(f));
        }
        place(&self.t.join("store-bin"), &libexec.join("store"));
        place(&self.t.join("attach-bin"), &libexec.join("attach"));
        let state = self.home.join("state");
        for f in ["tehas.db", "tehas.db-wal", "tehas.db-shm"] {
            let _ = fs::remove_file(state.join(f));
        }
        self.store_ok(&[
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            "0123456789abcdef0123456789abcdef01234567",
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ]);
        let ids: String = LOGINS.iter().map(|l| format!("{l} {}\n", uid(l))).collect();
        fs::write(state.join("account-ids"), ids).unwrap();
    }

    /// The scratch root, `$T` of the stubs.
    pub fn t(&self) -> &Path {
        &self.t
    }

    /// The product's repository, which the checkout cloned.
    pub fn upstream(&self) -> PathBuf {
        self.t.join("git/acme/widget")
    }

    /// The instance's checkout of the product.
    pub fn checkout(&self) -> PathBuf {
        self.home.join("repos/widget")
    }

    /// What every poll writes for tehas status.
    pub fn dispatch_json(&self) -> PathBuf {
        self.home.join("live/dispatch.json")
    }

    /// The demand, written on every poll.
    pub fn demand_file(&self) -> PathBuf {
        self.home.join("live/demand.json")
    }

    /// The limit tehas limit writes.
    pub fn limit_file(&self) -> PathBuf {
        self.home.join("state/limit.json")
    }

    /// Where a cycle's files are kept.
    pub fn cycles_dir(&self) -> PathBuf {
        self.home.join("state/cycles")
    }

    /// The files of #7's planning turn are named after this.
    pub fn record(&self) -> PathBuf {
        self.home.join("state/plan-7.json")
    }

    /// What #7's session carries from one turn to the next.
    pub fn keep(&self) -> PathBuf {
        self.home.join("work/widget-plan-7.keep")
    }

    /// The tree of bin/tehas, bin/tehasbot and the console, as on a host.
    pub fn front(&self) -> PathBuf {
        self.t.join("front")
    }

    /// tehas-dispatch, in a libexec with lib/ beside it.
    pub fn dispatcher(&self) -> PathBuf {
        self.t.join("dispatch/libexec/tehas-dispatch")
    }

    /// The console of this build.
    pub fn console(&self) -> &Path {
        &self.console
    }

    /// A stub program on the PATH of a run that records its arguments, then
    /// runs `body`. A body that starts with `#!` is the whole program.
    pub fn stub(&self, name: &str, body: &str) {
        let text = if body.starts_with("#!") {
            body.to_string()
        } else {
            format!(
                "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
                self.bash
            )
        };
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    /// The stub gh with these answers in place of GH, as `t_stub gh` writes
    /// one: each call is numbered, recorded with a body read from standard
    /// input written out, answered by gh_fail() when it names it, and every
    /// login in its answer is given the test's user id.
    pub fn gh_stub(&self, answers: &str) {
        let text = format!(
            "#!{}\n{GH_HEAD}\nt_answer() {{\n:\n{answers}\n}}\n{GH_TAIL}",
            self.bash
        );
        write_exe(self.t.join("bin/gh"), text).unwrap();
    }

    /// The `n`-th gh call of the case prints `message` on stderr and ends
    /// with `rc`. Several calls may be named.
    pub fn gh_fail(&self, n: u32, rc: u8, message: &str) {
        assert!(n > 0 && rc > 0 && !message.contains(['\t', '\n']));
        let path = self.t.join("gh-fails");
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !text
                .lines()
                .any(|l| l.split('\t').next() == Some(&n.to_string())),
            "call {n} is already named"
        );
        text.push_str(&format!("{n}\t{rc}\t{message}\n"));
        fs::write(path, text).unwrap();
    }

    /// The stub curl of the model endpoint.
    pub fn mesh_curl(&self) {
        self.stub("curl", CURL);
    }

    /// The model endpoint, as models.json names it for the provider, and its
    /// answer to curl when one is given.
    pub fn mesh(&self, answer: Option<&str>) {
        let agent = self.t.join("home/.pi/agent");
        fs::create_dir_all(&agent).unwrap();
        fs::write(
            agent.join("models.json"),
            r#"{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"}}}"#,
        )
        .unwrap();
        self.rm_fx("mesh.json");
        if let Some(a) = answer {
            self.fx("mesh.json", &format!("{a}\n"));
        }
    }

    /// instance.env, as given, and the app's tokens unless they are there
    /// (t_instance).
    pub fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        if !self.home.join("state/github/login").exists() {
            self.app_world("tehas-bot");
        }
    }

    /// The factory as the one GitHub App `slug` is: its login and tokens with
    /// hours left (t_app_world).
    pub fn app_world(&self, slug: &str) {
        let until = tehas_core::time::now() + 86400;
        for (dir, token) in [
            ("github", "app"),
            ("github-cycle", "cycle"),
            ("github-session", "session"),
        ] {
            let d = self.home.join("state").join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("token"), format!("ghs_{token}\n")).unwrap();
            fs::write(d.join("token.expires"), format!("{until}\n")).unwrap();
        }
        fs::write(
            self.home.join("state/github/login"),
            format!("{slug}[bot]\n"),
        )
        .unwrap();
    }

    /// An instance that works on at most `n` issues side by side, with more
    /// settings.
    pub fn slots(&self, n: u32, more: &[&str]) {
        let max = format!("TEHAS_MAX_CYCLES={n}");
        let mut lines: Vec<&str> = MINIMAL.to_vec();
        lines.push(&max);
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    /// A variable of every later run, until reset() (T_ENV+=).
    pub fn env(&self, k: &str, v: &str) {
        self.env.borrow_mut().push((k.into(), v.into()));
    }

    /// A variable of every later run, reset() or not (T_ENV_ALWAYS+=).
    pub fn env_always(&self, k: &str, v: &str) {
        self.always.borrow_mut().push((k.into(), v.into()));
    }

    /// tehasbot health as a host runs it, for a case that reads what it does.
    pub fn real_health(&self) {
        let tree = self.t.join("health-tree");
        fs::create_dir_all(tree.join("libexec")).unwrap();
        fs::create_dir_all(tree.join("bin")).unwrap();
        let _ = std::os::unix::fs::symlink(root().join("lib"), tree.join("lib"));
        fs::copy(root().join("bin/tehasbot"), tree.join("bin/tehasbot")).unwrap();
        place(&self.console, &tree.join("libexec/console"));
        let health = tree.join("libexec/health");
        write_exe(
            &health,
            "#!/usr/bin/env bash\nexec \"$(dirname \"$0\")/../bin/tehasbot\" health\n",
        )
        .unwrap();
        self.env_always("TEHAS_HEALTH_CMD", &health.to_string_lossy());
    }

    /// A fixture file, with these bytes.
    pub fn fx(&self, name: &str, text: &str) {
        fs::write(self.fx.join(name), text).unwrap();
    }

    /// A fixture file, with this JSON.
    pub fn fx_json(&self, name: &str, v: &Value) {
        self.fx(name, &v.to_string());
    }

    /// A fixture file that is there, empty.
    pub fn touch(&self, name: &str) {
        self.fx(name, "");
    }

    /// A fixture file is gone.
    pub fn rm_fx(&self, name: &str) {
        let _ = fs::remove_file(self.fx.join(name));
    }

    /// A file under `$T`, with these bytes.
    pub fn fs(&self, rel: &str, text: &str) {
        let p = self.t.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, text).unwrap();
    }

    /// A file under the home, with these bytes.
    pub fn home_file(&self, rel: &str, text: &str) {
        let p = self.home.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, text).unwrap();
    }

    /// What a file under the home holds, "" when there is none.
    pub fn read_home(&self, rel: &str) -> String {
        fs::read_to_string(self.home.join(rel)).unwrap_or_default()
    }

    /// A JSON file under the home, Null when there is none.
    pub fn json_home(&self, rel: &str) -> Value {
        serde_json::from_str(&self.read_home(rel)).unwrap_or(Value::Null)
    }

    /// live/dispatch.json.
    pub fn dispatch_state(&self) -> Value {
        self.json_home("live/dispatch.json")
    }

    fn command(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        let path = format!(
            "{}:{}:{}",
            self.t.join("bin").display(),
            self.t.join("deny").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        c.env_clear()
            .env("PATH", path)
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.home)
            .env("T", self.t.path())
            .env("T_ROOT", root())
            .env("CARGO_TARGET_DIR", self.t.join("target"))
            .env("TEHAS_SETTINGS_CMD", &self.console)
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TMPDIR", self.t.join("tmp"))
            .env("TEHAS_INSTANCE_DIR", self.t.join("instance"))
            .envs(self.env.borrow().iter().map(|(k, v)| (k, v)))
            .envs(self.always.borrow().iter().map(|(k, v)| (k, v)));
        c
    }

    /// `tehasbot dispatch <words>`, through the console as on a host.
    pub fn dispatch(&self, words: &[&str]) -> Run {
        let mut c = self.command(&self.t.join("front/bin/tehasbot").to_string_lossy());
        c.arg("dispatch").args(words);
        run(c)
    }

    /// One poll: `tehasbot dispatch`.
    pub fn poll(&self) -> Run {
        self.dispatch(&[])
    }

    /// A bash snippet in the environment of a run (t_run).
    pub fn sh(&self, snippet: &str) -> Run {
        let mut c = self.command(&self.bash);
        c.args(["-c", snippet]);
        run(c)
    }

    /// What a stub was called with, one call per line.
    pub fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name)).unwrap_or_default()
    }

    /// The gh calls.
    pub fn gh(&self) -> String {
        self.calls("gh")
    }

    /// The issues a cycle was started for, each followed by a space.
    pub fn started(&self) -> String {
        self.calls("fake-start")
            .lines()
            .map(|l| format!("{l} "))
            .collect()
    }

    /// The planning turns run.
    pub fn plan_calls(&self) -> String {
        self.calls("fake-plan")
    }

    /// The calls are forgotten.
    pub fn clear_calls(&self) {
        let d = self.t.join("calls");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d).unwrap();
    }

    fn store_cmd(&self, args: &[&str], now: Option<i64>, lock: Option<&str>) -> Run {
        let mut c = match lock {
            Some(n) => {
                fs::create_dir_all(self.cycles_dir()).unwrap();
                let mut c = Command::new("flock");
                c.args(["-w", "5"])
                    .arg(self.cycles_dir().join(format!("{n}.lock")))
                    .arg(self.t.join("store-bin"));
                c
            }
            None => Command::new(self.t.join("store-bin")),
        };
        c.env("TEHAS_HOME", &self.home).args(args);
        if let Some(t) = now {
            c.env("TEHAS_STORE_TEST_NOW", t.to_string());
        }
        let out = c.stdin(Stdio::null()).output().unwrap();
        Run {
            rc: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        }
    }

    /// A verb of the real store against this home.
    pub fn store(&self, args: &[&str]) -> Run {
        self.store_cmd(args, None, None)
    }

    /// A verb of the store with its clock at `epoch` (t_at).
    pub fn store_at(&self, epoch: i64, args: &[&str]) -> Run {
        self.store_cmd(args, Some(epoch), None)
    }

    /// A verb of the real store; its answer, or a panic.
    pub fn store_ok(&self, args: &[&str]) -> Value {
        let out = self.store(args);
        assert_eq!(out.rc, 0, "store {args:?}: {}", out.out);
        serde_json::from_str(&out.out).unwrap_or(Value::Null)
    }

    /// What the store has on record, through a read verb (t_row).
    pub fn row(&self, args: &[&str]) -> Value {
        self.store_ok(args)
    }

    fn store_must(&self, args: &[&str], now: Option<i64>, lock: Option<&str>) {
        let r = self.store_cmd(args, now, lock);
        assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
    }

    /// The store of a run answers a call whose arguments are `args`, or begin
    /// with them and a space, with `rc` and `line`, for its first `times`
    /// calls (all for 0), and is the real store otherwise.
    pub fn store_answers(&self, args: &str, rc: i32, line: &str, times: u32) {
        let times = if times == 0 {
            String::new()
        } else {
            format!("|{times}")
        };
        self.fs("store-answers", &format!("{args}|{rc}|{line}{times}\n"));
        let _ = fs::remove_file(self.t.join("store-answers.n"));
    }

    /// store_answers for every call (store_refuses).
    pub fn store_refuses(&self, args: &str, rc: i32, line: &str) {
        self.store_answers(args, rc, line, 0);
    }

    /// How many calls the answer matched.
    pub fn store_answered(&self) -> u64 {
        fs::read_to_string(self.t.join("store-answers.n"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// An approval of issue `n` on record, as the dispatcher records one: the
    /// body of "Words." unless `extra` names one, tehas:ready for label and
    /// fixer (t_approve).
    pub fn t_approve(&self, n: u64, by: &str, how: &str, at: i64, extra: &[&str]) {
        let (n, at, sum) = (n.to_string(), at.to_string(), wsum());
        let mut a = vec!["approval-record", &n, "--at", &at, "--by", by, "--how", how];
        if matches!(how, "label" | "fixer") {
            a.extend(["--label", "tehas:ready"]);
        }
        if !extra.contains(&"--body-sha") {
            a.extend(["--body-sha", &sum]);
        }
        a.extend_from_slice(extra);
        self.store_must(&a, None, None);
    }

    /// A cycle start of issue `n` on record, under its lock, at `now` when
    /// given (t_started).
    pub fn t_started(&self, n: u64, now: Option<i64>, extra: &[&str]) {
        let n = n.to_string();
        let mut a = vec!["cycle-start", &n];
        a.extend_from_slice(extra);
        self.store_must(&a, now, Some(&n));
    }

    /// The attempt on record for issue `n`, 0 for none.
    pub fn attempt(&self, n: u64) -> u64 {
        self.row(&["cycle-show", &n.to_string()])["cycle"]["attempt"]
            .as_u64()
            .unwrap_or(0)
    }

    /// A cycle of issue `n` on record that has ended (cycle-end `args`, status
    /// failed unless given), started on `by`'s approval of `how`, or by hand
    /// for "-" (t_ended).
    pub fn t_ended(&self, n: u64, by: &str, how: &str, args: &[&str]) {
        let a = self.attempt(n);
        if by == "-" {
            self.t_started(n, None, &["--by-hand"]);
        } else {
            self.t_approve(n, by, how, 1_700_000_000 + a as i64, &[]);
            self.t_started(n, None, &[]);
        }
        let (ns, next) = (n.to_string(), (a + 1).to_string());
        let mut e = vec!["cycle-end", &ns, "--attempt", &next];
        if !args.contains(&"--status") {
            e.extend(["--status", "failed"]);
        }
        e.extend_from_slice(args);
        self.store_must(&e, None, Some(&ns));
    }

    /// The cycle of issue `n` on record parked on `reason`, with the park
    /// question `question` unless it is "-" (t_park).
    pub fn t_park(&self, n: u64, reason: &str, question: &str, args: &[&str]) {
        let (ns, a) = (n.to_string(), self.attempt(n).to_string());
        let mut p = vec!["park", &ns, "--attempt", &a, "--reason", reason];
        p.extend_from_slice(args);
        self.store_must(&p, None, None);
        if question != "-" {
            self.store_must(
                &["question-asked", &ns, "--id", question, "--kind", "park"],
                None,
                None,
            );
        }
    }

    /// On a fresh session of issue `n`, the planning session `j` describes in
    /// the shape its record file had (t_plan): by [alice], label
    /// [tehas:plan], turns, session [abc-123], options and filed of the last
    /// turn, failed, pending, comment, running, lost, answers, fixer_refused,
    /// held, auto and startedAt.
    pub fn t_plan(&self, n: u64, j: Value) {
        let ns = n.to_string();
        let brief = self.t.join("plan-brief.md");
        fs::write(&brief, "# Faster search\n\nMake it fast.\n").unwrap();
        let brief = brief.to_string_lossy().into_owned();
        let turns = j["turns"].as_u64().unwrap_or(0);
        let session = match j["session"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => "abc-123".into(),
        };
        let at = j["startedAt"].as_str().map(epoch);
        let flag = |k: &str| j[k].as_bool().unwrap_or(false);
        let s = |a: &[&str]| self.store_must(a, None, None);
        let answer = |t: u64, when: Option<i64>| {
            let a = j["answers"]
                .as_array()
                .and_then(|v| v.iter().find(|a| a["turn"].as_u64() == Some(t)))
                .cloned()
                .unwrap_or(Value::Null);
            let by = a["by"].as_str().unwrap_or("alice").to_string();
            let ts = t.to_string();
            let mut args = vec![
                "plan-answer",
                &ns,
                "--turn",
                &ts,
                "--by",
                &by,
                "--text-file",
                &brief,
            ];
            if a["fixer"].as_bool() == Some(true) {
                args.push("--millwright");
            }
            self.store_must(&args, when, None);
        };
        let by = j["by"].as_str().unwrap_or("alice");
        let label = j["label"].as_str().unwrap_or("tehas:plan");
        let mut begin = vec![
            "plan-begin",
            &ns,
            "--by",
            by,
            "--label",
            label,
            "--brief-file",
            &brief,
        ];
        if flag("auto") {
            begin.push("--auto");
        }
        self.store_must(&begin, if turns == 0 { at } else { None }, None);
        for t in 1..=turns {
            if t != 1 {
                answer(t, None);
            }
            let ts = t.to_string();
            let end = [
                "plan-end",
                &ns,
                "--turn",
                &ts,
                "--outcome",
                "finished",
                "--rc",
                "0",
                "--session",
                &session,
            ];
            if t < turns {
                s(&end);
                s(&["plan-end", &ns, "--posted"]);
            } else if flag("failed") {
                s(&[
                    "plan-end",
                    &ns,
                    "--turn",
                    &ts,
                    "--outcome",
                    "failed",
                    "--rc",
                    "2",
                    "--session",
                    &session,
                ]);
            } else {
                let mut a: Vec<String> = end.iter().map(|w| w.to_string()).collect();
                for (k, flag) in [("options", "--option"), ("filed", "--filed")] {
                    for v in j[k].as_array().into_iter().flatten() {
                        a.push(flag.into());
                        a.push(v.as_str().unwrap_or_default().into());
                    }
                }
                s(&a.iter().map(String::as_str).collect::<Vec<_>>());
            }
        }
        if turns > 0 {
            let comment = text_of(&j["comment"]);
            if !comment.is_empty() {
                s(&["question-asked", &ns, "--id", &comment, "--kind", "plan"]);
            }
            if !flag("pending") {
                s(&["plan-end", &ns, "--posted"]);
            }
            if flag("running") {
                answer(turns + 1, at);
            }
        }
        if flag("lost") {
            let next = (turns + 1).to_string();
            s(&[
                "plan-end",
                &ns,
                "--turn",
                &next,
                "--outcome",
                "lost",
                "--rc",
                "2",
            ]);
        }
        for (k, kind) in [("fixer_refused", "fixer-refused"), ("held", "held")] {
            for v in j[k].as_array().into_iter().flatten() {
                s(&["plan-note", &ns, "--kind", kind, "--id", &text_of(v)]);
            }
        }
    }

    /// A cycle of issue `n` on record that has ended, started on `by`'s
    /// approval of `how` given at `at` (as GitHub writes a time). The status
    /// is handoff unless `args` names one.
    pub fn past(&self, n: u64, by: &str, how: &str, at: &str, args: &[&str]) {
        let a = self.attempt(n);
        self.t_approve(n, by, how, epoch(at), &[]);
        self.t_started(n, None, &[]);
        let (ns, next) = (n.to_string(), (a + 1).to_string());
        let mut e = vec!["cycle-end", &ns, "--attempt", &next];
        if !args.contains(&"--status") {
            e.extend(["--status", "handoff"]);
        }
        e.extend_from_slice(args);
        self.store_must(&e, None, Some(&ns));
    }

    /// A cycle that is running: its lock, held by this test until release().
    pub fn hold(&self, n: u64) {
        fs::create_dir_all(self.cycles_dir()).unwrap();
        let f = File::create(self.cycles_dir().join(format!("{n}.lock"))).unwrap();
        let _ = f.try_lock();
        self.held.borrow_mut().push(f);
    }

    /// Every lock of hold() is let go.
    pub fn release(&self) {
        self.held.borrow_mut().clear();
    }

    /// The queue: each issue labelled tehas:ready by alice, with #7's events.
    pub fn ready(&self, issues: &[u64]) {
        let events = fs::read_to_string(self.fx.join("events-7.json")).ok();
        let list: Vec<Value> = issues
            .iter()
            .map(|n| {
                if let Some(e) = &events {
                    self.fx(&format!("events-{n}.json"), e);
                }
                json!({"number": n, "labels": [{"name": "tehas:ready"}]})
            })
            .collect();
        self.fx_json("ready.json", &Value::Array(list));
    }

    /// The ready list from "<issue> [label...]" words, each issue labelled
    /// ready by alice on 2026-10-01.
    pub fn queue(&self, items: &[&str]) {
        let list: Vec<Value> = items
            .iter()
            .map(|item| {
                let mut w = item.split(' ');
                let n: u64 = w.next().unwrap().parse().unwrap();
                let mut labels = vec![json!({"name": "tehas:ready"})];
                labels.extend(w.map(|l| json!({"name": l})));
                self.fx_json(
                    &format!("events-{n}.json"),
                    &json!([{"event": "labeled", "label": {"name": "tehas:ready"},
                        "actor": {"login": "alice"}, "created_at": "2026-10-01T00:00:00Z"}]),
                );
                json!({"number": n, "labels": labels})
            })
            .collect();
        self.fx_json("ready.json", &Value::Array(list));
    }

    /// tehas:ready by alice, then each "<label>=<login>" applied in turn, a
    /// minute apart; "-<label>=<login>" takes it off, an empty login is none.
    pub fn label_events(&self, n: u64, events: &[&str]) {
        let mut all = vec![json!({"event": "labeled", "label": {"name": "tehas:ready"},
            "actor": {"login": "alice"}, "created_at": "2026-10-01T00:00:00Z"})];
        for (k, e) in events.iter().enumerate() {
            let off = e.starts_with('-');
            let (label, who) = e.trim_start_matches('-').split_once('=').unwrap_or((e, ""));
            all.push(json!({
                "event": if off { "unlabeled" } else { "labeled" },
                "label": {"name": label},
                "actor": if who.is_empty() { Value::Null } else { json!({"login": who}) },
                "created_at": format!("2026-10-01T00:0{}:00Z", k + 1),
            }));
        }
        self.fx_json(&format!("events-{n}.json"), &Value::Array(all));
    }

    /// The comments of an issue: the bot's question (comment 4001), who
    /// edited it last ("" for none), and the comments written after it.
    pub fn comments(&self, n: u64, body: &str, editor: &str, after: Value) {
        let ed = !editor.is_empty();
        let mut nodes = vec![json!({
            "databaseId": 4001,
            "author": {"login": "tehas-bot", "databaseId": uid("tehas-bot")},
            "body": body,
            "createdAt": "2026-01-01T00:00:00Z",
            "lastEditedAt": if ed { json!("2026-01-01T01:00:00Z") } else { Value::Null },
            "editor": if ed { json!({"login": editor}) } else { Value::Null },
            "userContentEdits": if ed {
                json!({"totalCount": 1, "nodes": [{"editor": {"login": editor}}]})
            } else {
                json!({"totalCount": 0, "nodes": []})
            },
        })];
        nodes.extend(after.as_array().cloned().unwrap_or_default());
        self.fx_json(
            &format!("comments-{n}.json"),
            &json!({"data": {"repository": {"issue": {"comments": {"nodes": nodes}}}}}),
        );
    }

    /// The same, the bot's question being the comment `id`.
    pub fn comments_of(&self, n: u64, id: u64, body: &str, editor: &str) {
        self.comments(n, body, editor, json!([]));
        let name = format!("comments-{n}.json");
        let mut v: Value =
            serde_json::from_str(&fs::read_to_string(self.fx.join(&name)).unwrap()).unwrap();
        v["data"]["repository"]["issue"]["comments"]["nodes"][0]["databaseId"] = json!(id);
        self.fx_json(&name, &v);
    }

    /// The park of #7's current cycle on record, Null without one.
    pub fn park_of(&self) -> Value {
        self.row(&["park-show", "7"])["park"].clone()
    }

    /// The question of the issue's current park, "none" without one.
    pub fn park_q(&self, n: u64) -> String {
        match &self.row(&["park-show", &n.to_string()])["park"]["question"] {
            Value::Null => "none".into(),
            v => text_of(v),
        }
    }

    /// Issue 7 is parked with a question on record (comment 4001), and nothing
    /// else is queued. Its last cycle started on `by`'s label, or by hand for
    /// "-", and parked on `reason`.
    pub fn parked(&self, reason: &str, by: &str) {
        self.reset();
        self.fx("ready.json", "[]");
        self.fx(
            "asking.json",
            r#"[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]"#,
        );
        self.t_ended(7, by, "label", &["--parked"]);
        self.t_park(7, reason, "4001", &[]);
    }

    /// Issue 7's session as the store has it, in the shape of plan-view; Null
    /// when there is none.
    pub fn rec(&self) -> Value {
        self.row(&["plan-view", "7"])["view"].clone()
    }

    /// "open" while #7 has a planning session open, else "none".
    pub fn rec_open(&self) -> &'static str {
        let p = &self.row(&["plan-show", "7"])["planning"];
        if !p.is_null() && p["ended_at"].is_null() {
            "open"
        } else {
            "none"
        }
    }

    /// The brief of #7's session on record.
    pub fn rec_brief(&self) -> String {
        let p = self.row(&["plan-show", "7"]);
        p["planning"]["brief"]
            .as_str()
            .unwrap_or("")
            .trim_end_matches('\n')
            .to_string()
    }

    /// Issue 7 is labelled for planning by alice and nothing else is queued.
    pub fn to_plan(&self) {
        self.reset();
        self.fx("ready.json", "[]");
        self.fx(
            "plans.json",
            r#"[{"number":7,"labels":[{"name":"tehas:plan"}]}]"#,
        );
        self.fx(
            "events-7.json",
            r#"[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]"#,
        );
        self.fx(
            "issue-7.json",
            r#"{"title":"Faster search","body":"Make it fast."}"#,
        );
    }

    /// Issue 7 carries tehas:refine from alice.
    pub fn to_refine(&self) {
        self.to_plan();
        self.fx("plans.json", "[]");
        self.fx(
            "refines.json",
            r#"[{"number":7,"labels":[{"name":"tehas:refine"}]}]"#,
        );
        self.fx(
            "events-7.json",
            r#"[{"event":"labeled","label":{"name":"tehas:refine"},"actor":{"login":"alice"}}]"#,
        );
    }

    /// Issue 7 is parked with its boxes unticked, and asks for a planning
    /// session ("plan" or "refine") by `by` on top.
    pub fn parked_plan(&self, ask: &str, by: &str) {
        let l = if ask == "plan" {
            "tehas:plan"
        } else {
            "tehas:refine"
        };
        self.parked("failed", "alice");
        self.fx_json(
            "asking.json",
            &json!([{"number": 7, "labels": [{"name": "needs-human-attention"},
                {"name": "tehas:question"}, {"name": l}]}]),
        );
        self.fx_json(
            "events-7.json",
            &json!([{"event": "labeled", "label": {"name": l}, "actor": {"login": by}}]),
        );
        for f in ["ready.json", "plans.json", "refines.json"] {
            self.fx(f, "[]");
        }
        let list = json!([{"number": 7, "labels": [{"name": l},
            {"name": "needs-human-attention"}, {"name": "tehas:question"}]}]);
        self.fx_json(
            if ask == "plan" {
                "plans.json"
            } else {
                "refines.json"
            },
            &list,
        );
        self.fx(
            "issue-7.json",
            r#"{"title":"Faster search","body":"Make it fast."}"#,
        );
        self.comments(7, &park_body(&[" ", " ", " "]), "", json!([]));
    }

    /// Issue 7 has a planning question open after one turn.
    pub fn planning(&self) {
        self.reset();
        self.fx("ready.json", "[]");
        self.fx("asking.json", r#"[{"number":7}]"#);
        self.t_plan(
            7,
            json!({"session": "abc-123", "turns": 1, "options": ["Adopt all", "Change some"],
                "pending": false, "failed": false, "comment": 4001}),
        );
    }

    /// The outcome the fake runner writes for the turn: its words and its
    /// question, and the issues the turn filed.
    pub fn plan_files(&self, urls: &[&str]) {
        self.rm_fx("plan-fails");
        let mut v = json!({"session": "abc-123", "text": "Two ways.",
            "question": {"question": "Adopt?", "options": [
                {"label": "Adopt all", "description": "Apply all five"},
                {"label": "Change some", "description": ""}]}});
        if !urls.is_empty() {
            v["filed"] = json!(urls);
        }
        self.fx_json("plan-result.json", &v);
    }

    /// The approval that waited on record when the issue was marked running,
    /// as the shell's at_mark shaped it, through `filter`.
    pub fn at_mark(&self, filter: &str) -> String {
        let f = format!(
            "(.approval | if . == null then null else {{at: (.at | todate), by, how, label, body_sha256, title_sha256, \
             comment_id, refresh_pr, refresh_head}} + (.input // {{}}) end) | {filter}"
        );
        let text = fs::read_to_string(self.t.join("calls/approved.at-mark")).unwrap_or_default();
        jq_with(&["-crS", &f], &text)
    }

    /// What the store has of issue `n`'s approvals, in the shape of
    /// approval-view, through `filter`; "none" when it has none.
    pub fn approval(&self, filter: &str, n: u64) -> String {
        let r = Command::new(self.home.join("libexec/store"))
            .args(["approval-view", &n.to_string()])
            .env("TEHAS_HOME", &self.home)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let v: Value = serde_json::from_slice(&r.stdout).unwrap_or(Value::Null);
        if v["view"].is_null() {
            return "none".into();
        }
        jq_with(&["-cr", filter], &v["view"].to_string())
    }

    /// The flag tehas pause writes, with this JSON.
    pub fn pause_flag(&self, text: &str) {
        self.home_file("state/pause.json", &format!("{text}\n"));
    }

    /// The limit tehas limit writes: `cycles`, whether planning runs, until
    /// when ("" for none) and why.
    pub fn limit(&self, cycles: u64, plan: bool, until: &str, reason: &str) {
        let until = if until.is_empty() {
            Value::Null
        } else {
            json!(until)
        };
        let v = json!({"v": 1, "cycles": cycles, "plan": plan, "until": until, "reason": reason});
        self.home_file("state/limit.json", &format!("{v}\n"));
    }

    /// The open planning sessions are read once, and every read after that
    /// answers busy. With a session on #9 whose issue no longer asks, the poll
    /// ends it.
    pub fn open_read_fails_again(&self) {
        self.t_plan(9, json!({"turns": 1, "comment": 4009}));
        let bin = self.home.join("libexec/store");
        let _ = fs::remove_file(&bin);
        let mark = self.t.join("calls/open-read");
        write_exe(
            &bin,
            format!(
                "#!/usr/bin/env bash\nif [ \"$*\" = 'plan-show --open' ] && [ -e '{m}' ]; then\n  echo 'store: busy #0: the database is locked' >&2; exit 1\nfi\n[ \"$*\" != 'plan-show --open' ] || : >'{m}'\nexec '{s}' \"$@\"\n",
                m = mark.display(),
                s = self.t.join("store-bin").display()
            ),
        )
        .unwrap();
    }

    /// A slice loaded from its file, of `max` bytes, `state` [active].
    pub fn slice_show(&self, max: &str, state: &str) {
        let state = if state.is_empty() { "active" } else { state };
        self.fx(
            "slice.show",
            &format!(
                "FragmentPath=/etc/systemd/system/tehas.slice\nActiveState={state}\nMemoryMax={max}\n"
            ),
        );
    }

    /// A host where the root step was never run: the slice is only a name.
    pub fn no_slice(&self) {
        self.fx(
            "slice.show",
            "FragmentPath=\nActiveState=inactive\nMemoryMax=infinity\n",
        );
    }

    /// The scratch cgroup tree begins again, with the pressure of the slice:
    /// `full` and `some` [0.00] avg60.
    pub fn slice_state(&self, full: &str, some: &str) {
        let root = self.t.join("cgroup");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tehas.slice")).unwrap();
        fs::write(
            root.join("cgroup.controllers"),
            "cpuset cpu io memory hugetlb pids rdma misc\n",
        )
        .unwrap();
        let some = if some.is_empty() { "0.00" } else { some };
        fs::write(
            root.join("tehas.slice/memory.pressure"),
            format!(
                "some avg10=0.00 avg60={some} avg300=0.00 total=0\nfull avg10=0.00 avg60={full} avg300=0.00 total=0\n"
            ),
        )
        .unwrap();
    }

    /// One container in the slice: its anon, reclaimable slab, floor (a
    /// number or "max") and kills.
    pub fn slice_child(&self, id: &str, anon: u64, slab: u64, low: &str, oom_kill: u64) {
        let dir = self
            .t
            .join("cgroup/tehas.slice")
            .join(format!("docker-{id}.scope"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("memory.stat"),
            format!(
                "anon {anon}\nfile 0\nkernel {slab}\nkernel_stack 0\nshmem 0\nslab_reclaimable {slab}\nslab_unreclaimable 0\n"
            ),
        )
        .unwrap();
        fs::write(dir.join("memory.low"), format!("{low}\n")).unwrap();
        fs::write(
            dir.join("memory.events"),
            format!("low 0\nhigh 0\nmax 0\noom 0\noom_kill {oom_kill}\noom_group_kill 0\n"),
        )
        .unwrap();
    }

    /// A container in the slice that is no cycle of this poll.
    pub fn other(&self, bytes: u64) {
        self.slice_child(&slice_id('d'), bytes, 0, &(2 * GIB).to_string(), 0);
    }

    /// The container of a running cycle: its cid file and its place in the
    /// slice.
    pub fn cid(&self, n: u64, digit: char, bytes: u64) {
        fs::create_dir_all(self.cycles_dir()).unwrap();
        fs::write(
            self.cycles_dir().join(format!("{n}.cid")),
            format!("{}\n", slice_id(digit)),
        )
        .unwrap();
        self.slice_child(&slice_id(digit), bytes, 0, &(2 * GIB).to_string(), 0);
    }

    /// The figures of df go back to 40 GB everywhere, Docker on the same file
    /// system.
    pub fn df_reset(&self) {
        for f in [
            "free",
            "free-docker",
            "df-fails",
            "df-fails-of",
            "docker-dir",
            "docker-down",
        ] {
            self.rm_fx(f);
        }
        self.fx("docker.info", "systemd 2\n");
    }

    /// The same free GiB on every file system.
    pub fn df_free(&self, gib: u64) {
        self.df_reset();
        self.fx("free", &format!("{}\n", gib * 1_048_576));
    }

    /// ~/tehas and Docker's data, at /srv/docker, on file systems of their
    /// own.
    pub fn df_free_of(&self, home: u64, docker: u64) {
        self.df_reset();
        self.fx("docker.info", "systemd 2\n/srv/docker\n");
        self.fx("free", &format!("{}\n", home * 1_048_576));
        self.fx("free-docker", &format!("{}\n", docker * 1_048_576));
    }

    /// The counter of procs_*() begins again and every name of the host has
    /// its shim, the store among them.
    pub fn procs_reset(&self) {
        let calls = self.t.join("calls/procs");
        fs::write(&calls, "").unwrap();
        self.stub_procs(PROC_NAMES);
        let bin = self.home.join("libexec/store");
        let real = self.t.join("store-real");
        if !bin.is_file() {
            return;
        }
        if !real.is_file() {
            let head = fs::read(&bin).unwrap_or_default();
            if head.starts_with(b"#!") {
                return;
            }
            fs::rename(&bin, &real).unwrap();
        }
        let c = calls.display();
        write_exe(
            &bin,
            format!(
                "#!{}\nfor p in store timeout jq; do printf 'procs\\t%s/%s %s\\n' \"$PWD\" \"$p\" \"$*\" >>'{c}'; done\nexec '{}' \"$@\"\n",
                self.bash,
                real.display()
            ),
        )
        .unwrap();
    }

    /// For each name not stubbed, a shim that counts the call and runs the
    /// host's program.
    pub fn stub_procs(&self, names: &[&str]) {
        let bin = self.t.join("bin");
        for name in names {
            if bin.join(name).exists() {
                continue;
            }
            write_exe(
                bin.join(name),
                format!(
                    "#!{}\nprintf '%s\\t%s\\n' procs \"$0 $*\" >>\"$T/calls/procs\"\nreal=$(PATH=${{PATH#\"$T/bin:\"}} command -v -- \"${{0##*/}}\") || exit 127\nexec \"$real\" \"$@\"\n",
                    self.bash
                ),
            )
            .unwrap();
        }
    }

    /// The programs counted since procs_reset(), one per line: what the shim
    /// of each was called by.
    fn procs_lines(&self) -> Vec<String> {
        self.calls("procs")
            .lines()
            .filter_map(|l| l.strip_prefix("procs\t"))
            .map(|l| {
                let path = l.split(' ').next().unwrap_or("");
                path.rsplit('/').next().unwrap_or(path).to_string()
            })
            .collect()
    }

    /// How many programs the poll started.
    pub fn procs(&self) -> usize {
        self.calls("procs")
            .lines()
            .filter(|l| !l.is_empty())
            .count()
    }

    /// The programs started, the most often called first, with their counts.
    pub fn procs_list(&self) -> Vec<(usize, String)> {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for p in self.procs_lines() {
            *counts.entry(p).or_default() += 1;
        }
        let mut v: Vec<(usize, String)> = counts.into_iter().map(|(p, n)| (n, p)).collect();
        v.sort_by(|a, b| b.cmp(a));
        v
    }

    /// How many calls of one program were counted.
    pub fn procs_of(&self, name: &str) -> usize {
        self.procs_lines().iter().filter(|p| *p == name).count()
    }

    /// How many calls the stub gh had.
    pub fn gh_reads(&self) -> usize {
        self.gh().lines().filter(|l| !l.is_empty()).count()
    }

    /// The writes the host's shim of gh logged.
    pub fn writes(&self) -> usize {
        self.read_home("state/github-writes.log")
            .lines()
            .filter(|l| !l.is_empty())
            .count()
    }

    /// The names of the files in a directory, sorted.
    pub fn files(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .map(|d| {
                d.map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.release();
        if std::thread::panicking() {
            return;
        }
        let denied = fs::read_to_string(self.t.join("denied")).unwrap_or_default();
        assert!(denied.is_empty(), "no stub stands in for:\n{denied}");
        let mut left = vec![];
        for (dir, start, end) in OLD_STATE {
            for f in self.files(&self.home.join(dir)) {
                let starts = match *start {
                    "1" => f.starts_with(|c: char| ('1'..='9').contains(&c)),
                    s => f.starts_with(s),
                };
                if starts && f.ends_with(end) {
                    left.push(format!("{dir}/{f}"));
                }
            }
        }
        assert!(left.is_empty(), "an old record file is there: {left:?}");
    }
}

/// The start of the stub gh: the call numbered and recorded, and the answer
/// gh_fail() named for it.
const GH_HEAD: &str = r####"exec 6>>"$T/gh-n.lock" || exit 64
command -p flock 6
raw=$(command -p cat "$T/gh-n" 2>/dev/null)
case $raw in '' | *[!0-9]*) raw=0 ;; esac
n=$((raw + 1))
echo "$n" >"$T/gh-n"
exec 6<&-
call="$*"
case " $* " in
  *" --body-file - "*) body=$(cat); call=${call/--body-file -/--body $body} ;;
  *" --input - "*) body=$(cat); body=$(jq -r '.body // empty' <<<"$body" 2>/dev/null); call=${call/--input -/-f body=$body} ;;
esac
printf '%s\n' "$call" >>"$T/calls/gh"
f_rc=
f_answer=
while IFS=$'\t' read -r f_n f_w f_msg || [ -n "$f_n" ]; do
  [ "$f_n" = "$n" ] || continue
  f_rc=$f_w
  f_answer=$f_msg
done < <(command -p cat "$T/gh-fails" 2>/dev/null)
if [ -n "$f_rc" ]; then
  printf '%s\n' "$f_answer" >&2
  exit "$f_rc"
fi"####;

/// The end of the stub gh: the answer run in the background, so a signal
/// ends it at once, and every login in it given its user id.
const GH_TAIL: &str = r####"t_got=$T/gh-out.$$
# In the background, so a signal to this stub ends it at once, as it did when a
# body that execs was the stub itself, and writing to files, so a body killed
# with the stub holds no pipe of the caller open.
exec 3<&0
( t_answer "$@" ) <&3 3<&- >"$t_got" 2>"$t_got.e" &
t_pid=$!
exec 3<&-
trap 'kill -TERM "$t_pid" 2>/dev/null; command -p cat "$t_got.e" >&2; command -p rm -f "$t_got" "$t_got.e"; exit 143' TERM INT
wait "$t_pid"
t_rc=$?
command -p cat "$t_got.e" >&2
if [ "$t_rc" = 0 ] && [ ! -s "$t_got" ]; then
  case "$1 $2 $3 $4" in
    "api users/"*" --jq .id") jq -rn --arg l "${2#users/}" '$l | sub("\\[bot\\]$"; "") | explode | add' >"$t_got" ;;
    # The node id tehas labels pins a label to (src/labels.rs): LA_ and the
    # label's name after tehas:, unless the body answers it.
    "api repos/"*/labels/*" --jq .node_id") t_l=${2##*/}; echo "LA_${t_l#tehas:}" >"$t_got" ;;
  esac
fi
if command -p grep -q '"login"' "$t_got" &&
  jq -c 'walk(if type == "object" and has("login") and (has("id") or has("databaseId") | not)
    then . + {id: (.login | tostring | sub("\\[bot\\]$"; "") | explode | add)} else . end)' "$t_got" >"$t_got.j" 2>/dev/null; then
  command -p cat "$t_got.j"
else
  command -p cat "$t_got"
fi
command -p rm -f "$t_got" "$t_got.j" "$t_got.e"
exit "$t_rc"
"####;

fn git(args: &[&str], dir: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// Remove the entries of `dir` named `pattern`, or starting with it when it
/// ends in `*`.
fn remove_like(dir: &Path, pattern: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let hit = match pattern.strip_suffix('*') {
            Some(start) => name.starts_with(start),
            None => name == pattern,
        };
        if hit {
            let p = e.path();
            if p.is_dir() && !p.is_symlink() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

/// A string as itself, any other value as its JSON, null as "".
fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

#[allow(dead_code)] // each test file uses a part of this module
/// The epoch of a time `date -d` reads.
pub fn epoch(when: &str) -> i64 {
    let out = Command::new("date")
        .args(["-u", "-d", when, "+%s"])
        .output()
        .unwrap();
    assert!(out.status.success(), "date -d {when}");
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

#[allow(dead_code)] // each test file uses a part of this module
/// jq -c `filter` of `input`, its output without the last newline.
pub fn jq(filter: &str, input: &str) -> String {
    jq_with(&["-c", filter], input)
}

fn jq_with(args: &[&str], input: &str) -> String {
    use std::io::Write;
    let mut child = Command::new("jq")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim_end_matches('\n').to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The sha256 of the body `Words.`, which an approval is recorded against.
pub fn wsum() -> String {
    format!("{:x}", Sha256::digest(b"Words."))
}

#[allow(dead_code)] // each test file uses a part of this module
/// The GitHub user id of a login in a test: the sum of its characters.
pub fn uid(login: &str) -> u64 {
    login
        .trim_end_matches("[bot]")
        .chars()
        .map(|c| c as u64)
        .sum()
}

#[allow(dead_code)] // each test file uses a part of this module
/// A container id: the digit 64 times.
pub fn slice_id(digit: char) -> String {
    std::iter::repeat_n(digit, 64).collect()
}

#[allow(dead_code)] // each test file uses a part of this module
/// One line of a park's body, its box ticked for "x" alone.
pub fn park_box(mark: &str, text: &str) -> String {
    format!("- [{}] {text}\n", if mark == "x" { "x" } else { " " })
}

#[allow(dead_code)] // each test file uses a part of this module
/// The body of a park comment, as q_render writes it. Three boxes: the
/// comments retry, the text-only retry and the one to leave it. Two: the
/// shape before the text-only box, the comments retry and the leave box.
/// One: the comments retry alone, with an unticked leave box.
pub fn park_body(boxes: &[&str]) -> String {
    let at = |i: usize| boxes.get(i).copied().unwrap_or("");
    let (clean, alone, leave) = if boxes.len() <= 2 {
        (at(0), "", at(1))
    } else {
        (at(0), at(1), at(2))
    };
    let mut b = String::from("Tehas cycle stopped.\n\n");
    if alone.is_empty() {
        b += &park_box(clean, "Retry from a clean start");
    } else {
        b += &park_box(clean, "Retry with the issue and all its comments");
        b += &park_box(alone, "Retry with the issue text only");
    }
    b += &park_box(leave, "Leave it parked");
    b += "\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n";
    b
}

#[allow(dead_code)] // each test file uses a part of this module
/// The body of a planning question of #7: its two option boxes, the box to
/// end it, its turn [1] and a line of the host's own before the boxes.
pub fn plan_body(b1: &str, b2: &str, end: &str, turn: u64, extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("{extra}\n\n")
    };
    let turn = turn.max(1);
    format!(
        "Two ways.\n\nAdopt?\n\n{extra}- [{b1}] Adopt all\n  Apply all five\n- [{b2}] Change some\n- [{end}] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn={turn} -->\n"
    )
}

#[allow(dead_code)] // each test file uses a part of this module
/// The question of a session whose last turn filed `urls`, as the host
/// posted it.
pub fn plan_filed_body(turn: u64, urls: &[&str]) -> String {
    let extra = if urls.is_empty() {
        String::new()
    } else {
        format!("Filed in this turn: {}", urls.join(", "))
    };
    plan_body(" ", "x", " ", turn, &extra)
}

#[allow(dead_code)] // each test file uses a part of this module
/// One comment as REST lists it.
pub fn icomment(id: u64, who: &str, created: &str, updated: &str, body: &str) -> Value {
    json!({"id": id, "user": {"login": who}, "created_at": created, "updated_at": updated, "body": body})
}

#[allow(dead_code)] // each test file uses a part of this module
/// A program run to its end, its two streams as one.
pub fn run(mut c: Command) -> Run {
    let (mut r, w) = std::io::pipe().unwrap();
    c.stdin(Stdio::null())
        .stdout(w.try_clone().unwrap())
        .stderr(w);
    let mut child = c.spawn().unwrap();
    drop(c);
    let mut out = String::new();
    std::io::Read::read_to_string(&mut r, &mut out).unwrap();
    let status = child.wait().unwrap();
    Run {
        rc: status.code().unwrap_or(-1),
        out: out.trim_end_matches('\n').to_string(),
    }
}

#[allow(dead_code)] // each test file uses a part of this module
/// `want` is in `got`.
#[track_caller]
pub fn has(got: &str, want: &str) {
    assert!(got.contains(want), "{want:?} is not in:\n{got}");
}

#[allow(dead_code)] // each test file uses a part of this module
/// `want` is not in `got`.
#[track_caller]
pub fn lacks(got: &str, want: &str) {
    assert!(!got.contains(want), "{want:?} should not be in:\n{got}");
}

#[allow(dead_code)] // each test file uses a part of this module
/// The lines of `text` that start with `prefix`.
pub fn count_lines(text: &str, prefix: &str) -> usize {
    text.lines().filter(|l| l.starts_with(prefix)).count()
}
