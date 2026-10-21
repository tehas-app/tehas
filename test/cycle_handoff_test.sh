#!/usr/bin/env bash
# Tests for libexec/cycle: the runner's output, a handoff with or without a
# pull request, the park comment and the usage posted.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

t_case "the runner's output is in the issue's log"
reset
echo 'echo "from the runner"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/logs/dispatch-work-7.log")" "from the runner"

t_case "a cycle that ends without a pull request is handed to a human"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$t_out" "done #7 rc=0 status=unknown"
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "the runner's exit status is in the report"
reset
echo 'exit 3' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "(runner exit 3)"

t_case "a runner that started no cycle over an image (exit 4) is parked with that reason"
reset
echo 'exit 4' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Tehas started no cycle: an image or file the issue links could not be given to it, or the issue could not be read for them (runner exit 4)."
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that never ran is reported from the approval record, and the record goes with the report"
reset
mkdir -p "$state"
jq -n '{at: "2026-01-01T00:00:00Z", by: "alice", how: "label", body_sha256: "0", links: []}' >"$state/7.approved.json"
echo 'exit 4' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "alice approved this start at 2026-01-01T00:00:00Z"
t_contains "$(t_calls gh)" "Nothing of it ran: no workspace, no container, no step, no usage."
t_lacks "$(t_calls gh)" "last step"
t_lacks "$(t_calls gh)" "min,"
t_lacks "$(t_calls gh)" "total, all dispatches"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_eq "$([ -e "$state/7.approved.json" ] && echo left)" ""
t_eq "$(jq -c '[.provider, .retry]' "$state/7.park")" '[false,false]'
t_contains "$(t_calls gh)" "--add-label tehas:question"
# The line of history still names how the start was authorised.
t_eq "$(jq -r '[.rc, .status, .step, .how] | @csv' "$state/7.history.jsonl")" '4,"unknown","-","label"'

t_case "a cycle that never ran and holds no approval record says that, and asks for a fresh one"
reset
echo 'exit 4' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "No approval of this start is recorded on this host: a fresh \`tehas:ready\` is needed."
t_contains "$(t_calls gh)" "runner exit 4"

t_case "the issue's images are gone when the cycle is over, however it ended"
for end in "handoff commit-pr" "aborted develop"; do
  reset
  # shellcheck disable=SC2086  # two words: a status and a step
  ended $end feature/x
  : >"$T/home/tehas/work/.tehas-workspaces"
  mkdir -p "$workdir.images"
  echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
  t_run "$cycle 7"
  t_eq "$(find "$workdir.images" -maxdepth 0 2>/dev/null | wc -l)" 0
done

t_case "an images directory under a directory tehas work did not mark is left alone"
reset
ended merged "done" feature/x
merged_pr
mkdir -p "$workdir.images"
t_run "$cycle 7"
t_eq "$(find "$workdir.images" -maxdepth 0 | wc -l)" 1

t_case "the cycle's own state directory is gone when the cycle is over, however it ended"
for end in "handoff commit-pr" "aborted develop"; do
  reset
  # shellcheck disable=SC2086  # two words: a status and a step
  ended $end feature/x
  : >"$T/home/tehas/work/.tehas-workspaces"
  mkdir -p "$workdir.state/cache" "$workdir.state/memory"
  echo planted >"$workdir.state/memory/note.json"
  [ -s "$workdir.state/memory/note.json" ] || t_bad "the cycle's state contents were not planted"
  echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
  t_run "$cycle 7"
  t_eq "$(find "$workdir.state" -maxdepth 0 2>/dev/null | wc -l)" 0
done

t_case "a state directory under a directory tehas work did not mark is left alone"
reset
ended merged "done" feature/x
mkdir -p "$workdir.state/cache"
t_run "$cycle 7"
t_eq "$(find "$workdir.state" -maxdepth 0 | wc -l)" 1

t_case "a cycle that ends with a pull request waits for a human and is reported on it"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me"
t_contains "$(t_calls gh)" "No token usage was recorded for this cycle."

# #33: only a pull request that waits for nothing but a merge is labelled so.
t_case "a handoff with its pull request open on another cap is parked, and names the cap and the pull request"
reset
ended handoff lens-review feature/x review-incomplete
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" 'Tehas cycle handed off with its pull request open'
t_contains "$(t_calls gh)" 'pi-rukas stopped on the cap `review-incomplete`.'
t_contains "$(t_calls gh)" "Read the pull request before you retry: https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_eq "$(jq -r .cap "$state/7.park")" review-incomplete

t_case "a handoff with its pull request open and no cap is parked"
reset
ended handoff commit-pr feature/x -
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "The result names no cap, so it is not taken for a pull request that waits only for a merge."

t_case "a review whose round cap left findings open is parked, though its cap is a waiting merge"
reset
ended handoff ci feature/x
echo '{"roles":[],"reason":"","reviewHeld":true}' >"$fx/progress.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "Its review ran out of rounds with findings still open"

t_case "a waiting merge with no progress to read is parked"
reset
ended handoff ci feature/x
: >"$fx/no-progress"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"

t_case "an issue that carries needs-human-attention at the end is not also labelled awaiting merge"
reset
ended handoff ci feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo '{"number":7,"state":"open","labels":[{"name":"needs-human-attention"}]}' >"$fx/issue-7.json"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"

t_case "labels that cannot be read do not make a waiting merge"
reset
ended handoff ci feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
# The first ask, what the number is, answers; the label reads after it fail.
cat >"$fx/work.sh" <<'EOS'
: >"$T/fx/issue-fails"
EOS
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$t_out" "WARN: the labels of #7 could not be read"

# The runner is given PI_ENSEMBLE_HANDOFF_CHANNEL=host: it posts no handoff,
# and this is the one comment of the park.
t_case "a park carries pi-rukas's handoff under its marker, cleaned like any agent's words"
reset
ended handoff develop feature/x verify-failed:develop
jq '.issues[0].handoff = {channel: "host", body: "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->\n## Cap hit\n\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->\nask @someone"}' \
  "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
t_run "$cycle 7"
body=$(t_calls gh | sed -n '/issues\/7\/comments -f body=/,$p')
t_contains "$body" "body=<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->
Tehas cycle stopped without a pull request"
t_contains "$body" "The handoff of pi-rukas follows."
t_contains "$body" "## Cap hit"
t_contains "$body" '- \[x] Retry from a clean start'
t_contains "$body" '<\!-- tehas:question kind=park issue=7 turn=1 -->'
t_lacks "$body" "@someone"
t_eq "$(grep -c 'pi-rukas:handoff' <<<"$body")" 1
t_eq "$(grep -c '^- \[ \] Retry from a clean start' <<<"$body")" 1

t_case "a waiting merge posts pi-rukas's handoff on the pull request with the usage"
reset
ended handoff ci feature/x
jq '.issues[0].handoff = {channel: "host", body: "<!-- pi-rukas:handoff issue=7 cap=awaiting-human-merge -->\n## Ready to merge"}' \
  "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9 --body <!-- pi-rukas:handoff issue=7 cap=awaiting-human-merge -->
Tehas cycle for #7"
t_contains "$(t_calls gh)" "## Ready to merge"

t_case "a cycle that reached its deadline says so, and an intent park says its reason"
reset
ended handoff lens-fix feature/x deadline
t_run "$cycle 7"
t_contains "$(t_calls gh)" "It reached its deadline, a quarter of its time limit before the end, and started no new step."
reset
ended handoff explore "" intent-park
jq '.issues[0].parkReason = "too-large"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo 11 >"$fx/rc"
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'pi-rukas stopped on the cap `intent-park` (`too-large`).'

t_case "each end of a cycle leaves a line of history for the fixer: a park and its reason, a waiting merge"
reset
mkdir -p "$state"
echo alice >"$state/7.by"
echo '{"how":"label","by":"alice"}' >"$state/7.approved.json"
ended handoff explore "" intent-park
jq '.issues[0].parkReason = "too-large"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo 11 >"$fx/rc"
t_run "$cycle 7"
t_eq "$(jq -c '[.parked, .status, .step, .cap, .park_reason, .provider, .by, .how, .rc]' "$state/7.history.jsonl")" \
  '[true,"handoff","explore","intent-park","too-large",false,"alice","label",11]'
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(wc -l <"$state/7.history.jsonl")" 2
t_eq "$(tail -n 1 "$state/7.history.jsonl" | jq -c '[.parked, .pr, (.at | test("^[0-9]{4}-"))]')" '[false,"https://github.com/acme/widget/pull/9",true]'

t_case "the token usage per role is posted on the pull request, with the result's totals"
reset
ended handoff commit-pr feature/x
jq '.issues[0].usage = {turns: 4, input: 35, output: 11, cacheRead: 1, dispatches: 3}' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo '{"roles":[{"role":"developer","dispatches":2,"turns":3,"input":30,"output":10,"cacheRead":1}],"reason":""}' >"$fx/progress.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| developer | 2 | 3 | 30 | 10 | 1 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 4 | 35 | 11 | 1 |"

# A refresh's approval record, as the dispatcher writes it before the start.
refresh_started() {
  mkdir -p "$state"
  jq -n '{at: "2026-10-01T00:00:00Z", by: "alice", how: "refresh", body_sha256: ("0" * 64), links: [],
    rests_on: {at: "2026-10-01T00:00:00Z", by: "alice", how: "label", body_sha256: ("0" * 64)},
    refreshes: 1, refresh_of: {pr: 9, head_sha: ("a" * 40), base_sha: ("b" * 40), branch: "main"}}' >"$state/7.approved.json"
}
for outcome in pushed parked failed; do
  t_case "a refresh takes tehas:refreshing off its pull request when it ends: $outcome"
  reset
  refresh_started
  case "$outcome" in
    pushed) ended handoff commit-pr feature/issue-7-x; echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt" ;;
    parked) ended handoff develop feature/issue-7-x verify-failed:develop ;;
    failed) ended crash develop ;;
  esac
  t_run "$cycle 7"
  t_eq "$t_rc" 0
  t_contains "$(t_calls gh)" "pr edit 9 --repo acme/widget --remove-label tehas:refreshing"
  [ "$outcome" != pushed ] || t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"
  [ "$outcome" = pushed ] || t_contains "$(t_calls gh)" "--add-label needs-human-attention"
  t_eq "$(jq -r .how "$state/7.history.jsonl")" refresh
done

t_case "a cycle that is no refresh leaves the labels of pull requests alone"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:refreshing"

t_case "a mark that cannot be taken off is tried three times and said"
reset
refresh_started
: >"$fx/pr-edit-fails"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c 'pr edit 9 --repo acme/widget --remove-label tehas:refreshing')" 3
t_contains "$t_out" "WARN: label change on pull request #9 failed"

t_done
