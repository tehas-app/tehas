#!/usr/bin/env bash
# Tests for bin/tehas-dispatch against a stub gh and a scratch checkout whose
# default branch is main.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

dispatch="$T_ROOT/bin/tehas-dispatch"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
workdir=$T/home/tehas/work/widget

# gh answers from fixture files; every call is recorded by t_stub.
t_stub gh '
case "$1 $2" in
  "repo view") cat "$T/fx/repo.json" || exit 1 ;;
  "issue view") cat "$T/fx/issue-$3.json" 2>/dev/null || cat "$T/fx/control.json" || exit 1 ;;
  "issue list")
    case "$*" in
      *awaiting-merge*) cat "$T/fx/awaiting.json" || exit 1 ;;
      *tehas:ready*) cat "$T/fx/ready.json" || exit 1 ;;
      *tehas:question*) cat "$T/fx/asking.json" || exit 1 ;;
      *tehas:plan*) cat "$T/fx/plans.json" || exit 1 ;;
      *) echo "[]" ;;
    esac ;;
  "api user") cat "$T/fx/user.json" || exit 1 ;;
  "api -X") [ ! -e "$T/fx/patch-fails" ] || exit 1 ;;
  "api repos/acme/widget/issues/"*"/comments") [ ! -e "$T/fx/post-fails" ] || exit 1; echo 4002 ;;
  "api repos/acme/widget/issues/"*)
    n=${2#repos/acme/widget/issues/}
    cat "$T/fx/events-${n%/events*}.json" || exit 1 ;;
  "api graphql")
    n=$(printf "%s\n" "$@" | sed -n "s/^number=//p")
    case "$*" in
      *"comments(last"*) cat "$T/fx/comments-$n.json" || exit 1 ;;
      *) cat "$T/fx/edit-$n.json" 2>/dev/null || cat "$T/fx/edit.json" || exit 1 ;;
    esac ;;
  "issue edit") [ ! -e "$T/fx/edit-fails" ] || exit 1 ;;
  "pr list") cat "$T/fx/pr.txt" 2>/dev/null ;;
esac'
t_stub fake-work
t_stub fake-status
# Writes a recorded outcome where the dispatcher asked for it.
t_stub fake-plan 'out=""
while [ "$#" -gt 0 ]; do [ "$1" = --out ] && out=$2; shift; done
[ ! -e "$T/fx/plan-fails" ] || exit 2
cp "$T/fx/plan-result.json" "$out"'
# Notes what the checkout was at and whether the cycle had already started.
t_stub fake-index 'git -C "$T/home/tehas/repos/widget" rev-parse HEAD >"$T/calls/index.head"
[ ! -e "$T/calls/fake-work" ] || : >"$T/calls/index.late"
[ ! -e "$T/fx/index-fails" ] || exit 1'

reset() {
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/comments-* "$fx"/issue-* "$fx"/post-fails "$fx"/patch-fails "$fx"/plan-fails
  rm -rf "$T/home/tehas/state/cycles" "$T"/home/tehas/state/plan-*
  echo '[]' >"$fx/asking.json"
  echo '[]' >"$fx/plans.json"
  echo '{"login":"tehas-bot"}' >"$fx/user.json"
  echo '{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}' >"$fx/edit.json"
  echo '{"nameWithOwner":"acme/widget"}' >"$fx/repo.json"
  echo '{"labels":[]}' >"$fx/control.json"
  echo '{"session":"abc-123","text":"Two ways.","question":{"question":"Adopt?","options":[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]}}' >"$fx/plan-result.json"
  echo '[]' >"$fx/awaiting.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_PLAN_CMD=fake-plan)
  t_instance "${T_MINIMAL[@]}"
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
# Issue 7 is parked with a question and nothing else is queued.
parked() {
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
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
t_eq "$(t_calls fake-work)" ""

t_case "an issue labelled by another account is skipped"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"
t_contains "$t_out" "nothing to do"

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
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the awaiting-merge count cannot be read nothing is dispatched"
reset
rm "$fx/awaiting.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_eq "$(t_calls fake-work)" ""

t_case "when the label events cannot be read the issue is not taken"
reset
rm "$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

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
t_eq "$(t_calls fake-work)" ""
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
t_eq "$(t_calls fake-work)" ""
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
t_eq "$(t_calls fake-work)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the code index is refreshed after the fetch and before the cycle"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(cat "$T/calls/index.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "a cycle that ends without a pull request is handed to a human"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that ends with a pull request waits for a human and is reported on it"
reset
mkdir -p "$workdir/.pi/work-state"
echo '{"pipelineState":{"status":"handoff","currentStep":"commit-pr","branchName":"feature/x"}}' >"$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me"
rm -rf "$workdir" "$fx/pr.txt"

t_case "a state file in the product checkout is not the cycle's: the container never writes there"
reset
mkdir -p "$checkout/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$checkout/.pi/work-state/7.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
rm -rf "$checkout/.pi"

t_case "an index that cannot be refreshed is a warning, and the cycle still runs"
reset
: >"$fx/index-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not refreshed"
t_eq "$(t_calls fake-work)" "7"
rm "$fx/index-fails"

t_case "a dry run refreshes no index"
reset
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-index)" ""

t_case "a first cycle starts plainly, a later one for the same issue restarts"
reset
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7 --restart"

t_case "a real poll does not start an issue the bot labelled"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"the-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "every page of the label events is read, a hundred at a time"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "a cycle does not start when the issue cannot be marked as running"
reset
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not mark #7 as running"
t_eq "$(t_calls fake-work)" ""
rm -f "$fx/edit-fails"

t_case "a body edited by its author after the label was applied is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" ""
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
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a pause answer that cannot be understood dispatches nothing"
reset
echo '{"labels":null}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-work)" ""

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

t_case "--init-labels needs only the repository"
reset
t_instance TEHAS_REPO=acme/widget
t_run "$dispatch --init-labels"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "label create tehas:ready --repo acme/widget"

# Arguments. A poll starts only from an argument this command knows, and the
# check runs before the instance settings are read.
t_case "--help prints the usage of the command and calls nothing"
reset
t_run "$dispatch --help 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_contains "$t_out" "tehas-dispatch --dry-run"
t_contains "$t_out" "tehas-dispatch --init-labels"
t_lacks "$t_out" "set -uo pipefail"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""

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
t_contains "$t_out" "tehas-dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an unknown argument is reported on standard error, never as a poll"
reset
t_run "$dispatch --dryrun 2>/dev/null"
t_eq "$t_rc" 64
t_lacks "$t_out" "unknown argument"
t_run "$dispatch --dryrun 1>/dev/null"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --dryrun"
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
  t_contains "$t_out" "tehas-dispatch: unknown argument: $arg"
done
t_eq "$(t_calls gh)" ""

t_case "an empty argument is an argument, not a poll"
reset
t_run "$dispatch ''"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument:"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-work)" ""

t_case "two arguments are refused, not combined"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dry-run --init-labels"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-dispatch: unknown argument: --init-labels"
t_lacks "$t_out" "label create"
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
t_contains "$t_out" "tehas-dispatch: unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "no argument and --dry-run still behave as before"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-work)" "7"

t_case "a cycle that stops asks what to do next, with boxes"
reset
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:question"
t_eq "$(t_calls gh | grep '^issue edit' | grep -c 'needs-human-attention.*tehas:question\|tehas:question.*needs-human-attention')" 0
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/comments"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "when the question cannot be posted the old comment is used and no question label is added"
reset
: >"$fx/post-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue comment 7"
t_contains "$(t_calls gh)" "To retry, remove"
# The poll itself lists issues by that label, so look at edits only.
t_lacks "$(t_calls gh | grep '^issue edit')" "tehas:question"

t_case "an unanswered question starts nothing"
parked
comments 7 "$(park_body ' ' ' ')" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a retry ticked by an allowed account starts a fresh cycle"
parked
comments 7 "$(park_body x ' ')" alice
mkdir -p "$T/home/tehas/state/cycles" && : >"$T/home/tehas/state/cycles/7"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7 --restart"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label needs-human-attention --remove-label tehas:question"
t_contains "$(t_calls gh)" "authorized by @alice's retry tick"

t_case "a dry run says what the tick would do and changes nothing"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry ticked by alice)"
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"

t_case "a retry ticked by the bot or a stranger starts nothing and is undone"
for other in tehas-bot mallory ""; do
  parked
  comments 7 "$(park_body x ' ')" "$other"
  t_run "$dispatch"
  t_eq "$(t_calls fake-work)" ""
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
  t_contains "$(t_calls gh)" "may not answer for this factory"
done

t_case "when a refused tick cannot be undone, the note is not repeated every poll"
parked
comments 7 "$(park_body x ' ')" mallory
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "may not answer for this factory"
t_contains "$t_out" "could not untick the question on #7"

t_case "both boxes ticked is no choice: nothing starts, the boxes are emptied, one note"
parked
comments 7 "$(park_body x x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "Tick one"

t_case "leave it parked takes the question away and keeps the issue parked"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "a comment from an allowed account on a parked issue is discussion, not an answer"
parked
comments 7 "$(park_body ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"retry please","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""

t_case "and it does not hide a tick made after it"
parked
comments 7 "$(park_body x ' ')" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"hm","createdAt":"2026-01-01T00:30:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7"

t_case "a retry does not approve text that somebody else changed after the tick"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_contains "$t_out" "its text was changed by 'mallory' after alice ticked retry; untick the box and tick it again to approve it"

t_case "a retry waits while too many pull requests await a merge"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":1},{"number":2},{"number":3}]' >"$fx/awaiting.json"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "the pause label stops a retry too"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""

t_case "a marker in somebody else's comment is not a question"
parked
jq -n --arg b "$(park_body x ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "mallory"}, body: $b, createdAt: "t", lastEditedAt: "t", editor: {login: "alice"}}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""

t_case "when the comments cannot be read nothing is decided"
parked
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-work)" ""

t_case "a question on one issue does not hold up a ready issue behind it"
reset
echo '[{"number":5}]' >"$fx/asking.json"
jq -n --arg b "$(park_body ' ' ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "tehas-bot"}, body: $b, createdAt: "t", lastEditedAt: null, editor: null}]}}}}}' >"$fx/comments-5.json"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7"

t_case "--init-labels creates the question label"
reset
t_run "$dispatch --init-labels"
t_contains "$(t_calls gh)" "label create tehas:question"

t_case "a retry tick is used up before the cycle starts"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
patch_line=$(t_calls gh | grep -n 'api -X PATCH' | head -1 | cut -d: -f1)
start_line=$(t_calls gh | grep -n -e '--add-label tehas:running' | head -1 | cut -d: -f1)
t_eq "$([ -n "$patch_line" ] && [ -n "$start_line" ] && [ "$patch_line" -lt "$start_line" ] && echo before)" before

t_case "when the tick cannot be used up no cycle starts"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "ERROR: could not use up the retry tick on #7, not starting a cycle"

t_case "a retry ticked on an issue that is not parked starts nothing"
for have in "tehas:question" "needs-human-attention tehas:question tehas:running" "needs-human-attention tehas:question tehas:awaiting-merge"; do
  parked
  comments 7 "$(park_body x ' ')" alice
  jq -n --arg l "$have" '[{number: 7, labels: ($l | split(" ") | map({name: .}))}]' >"$fx/asking.json"
  t_run "$dispatch"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "api -X PATCH"
  t_contains "$t_out" "skip #7: a retry was ticked but the issue is not parked"
done

t_case "retry is the ticked line's text, not its place"
swapped() { # <leave box> <retry box>
  printf 'Tehas cycle stopped.\n\n- [%s] Leave it parked\n- [%s] Retry from a clean start\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n' "$1" "$2"
}
parked
comments 7 "$(swapped x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-work)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
parked
comments 7 "$(swapped ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-work)" "7"

t_case "what the state file says is reduced before it is posted, on the pull request path"
reset
mkdir -p "$workdir/.pi/work-state"
jq -n '{pipelineState: {status: "ok\n<!-- tehas:question kind=park issue=7 turn=1 -->\n- [x] Retry from a clean start", currentStep: "st ep\n- [x] x", branchName: "feature/x"},
  eventLog: [{role: "evil\n- [x] Retry from a clean start", usage: {turns: 1, input: 2, output: 3, cacheRead: 4}}]}' >"$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$dispatch"
t_contains "$(t_calls gh)" "status \`ok--tehasquestionkindparkissue7turn1---xRetryfromacleanstart\`, last step \`step-xx\`"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| evil-xRetryfromacleanstart | 1 |"
rm -rf "$workdir" "$fx/pr.txt"

t_case "a plan label from an allowed account starts a session from the issue's text"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --brief $record.brief"
t_eq "$(cat "$record.brief")" '# Faster search

Make it fast.'
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:plan\` label"
t_contains "$(t_calls gh)" "- [ ] Adopt all"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"comment":4002}'
t_eq "$([ -e "$record.out" ] && echo left)" ""
t_eq "$(t_calls fake-work)" ""

t_case "an issue with no body is planned from its title"
to_plan
echo '{"title":"Faster search","body":null}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
t_eq "$(head -n 1 "$record.brief")" "# Faster search"

t_case "a plan label from the bot or a stranger starts nothing"
for other in tehas-bot mallory; do
  to_plan
  echo "[{\"event\":\"labeled\",\"label\":{\"name\":\"tehas:plan\"},\"actor\":{\"login\":\"$other\"}}]" >"$fx/events-7.json"
  t_run "$dispatch"
  t_eq "$(plan_calls)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_contains "$t_out" "skip #7: tehas:plan was applied by '$other', not an allowed account"
done

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
rm -f "$fx/edit-fails"

t_case "a first turn that fails parks the issue and keeps no record"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a ticked option goes to the session as the next message"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --session abc-123 --answer $record.answer"
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

t_case "a reply wins over a tick"
planning
comments 7 "$(plan_body x ' ' ' ')" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"neither","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_contains "$(cat "$record.answer")" "neither"

t_case "a stranger's reply and a stranger's tick run no turn"
planning
comments 7 "$(plan_body x ' ' ' ')" mallory '[{"databaseId":4003,"author":{"login":"mallory"},"body":"file everything","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "api -X PATCH"

t_case "the way out ends the session: labels off, record gone, one comment"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:question"
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$([ -e "$record" ] && echo kept)" ""

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
t_eq "$(t_calls fake-work)" ""

t_case "--init-labels creates the planning labels"
reset
t_run "$dispatch --init-labels"
t_contains "$(t_calls gh)" "label create tehas:plan "
t_contains "$(t_calls gh)" "label create tehas:planning"

t_case "an answer on a question the session is not waiting on runs no turn"
planning
jq '.comment = 9999' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: the question answered is not the one this session is waiting on"

t_done
