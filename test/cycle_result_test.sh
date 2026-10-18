#!/usr/bin/env bash
# Tests for libexec/cycle: a merge, the result it reads, the pull requests of
# the branch, and the workspace it removes or keeps.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

t_case "a merged cycle only loses its running label"
reset
ended merged "done" feature/x
merged_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_lacks "$(t_calls gh)" "--add-label"
t_contains "$(t_calls gh)" "The change was merged: https://github.com/acme/widget/pull/9"

t_case "a merged state with no merged pull request is a claim: parked, and nothing is removed"
reset
ended merged "done" feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
rukas=$(printf 'e%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me --state all"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" 'Tehas cycle stopped: its state says `merged`, but the bot has no merged pull request for this issue on `feature/x`'
t_lacks "$(t_calls gh)" "The change was merged"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
t_eq "$(cat "$park_id")" 4002
rm -f "$fx/image.label"

t_case "a merged state whose branch has only an open pull request is parked, and the comment names it"
reset
ended merged "done" feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "Its branch has an open pull request: https://github.com/acme/widget/pull/9"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "a merged pull request of another issue does not make a merge"
reset
ended merged "done" feature/issue-9-other
merged_pr
echo "Fixes #9" >"$fx/pr-body"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "has a pull request of the bot that does not name this issue"
t_lacks "$(t_calls gh)" "The change was merged"

t_case "a handoff whose branch is another issue's pull request is parked, and nothing is posted on it"
reset
ended handoff commit-pr feature/issue-9-other
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "Fixes #9 and mentions #7" >"$fx/pr-body"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_lacks "$(t_calls gh)" "pr comment"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "The branch \`feature/issue-9-other\` has a pull request of the bot that does not name this issue"
t_lacks "$(t_calls gh)" "Its branch has an open pull request"

t_case "a pull request whose head is the issue's branch names it, whatever its body says"
reset
ended handoff commit-pr feature/issue-7-the-change
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "No keyword here." >"$fx/pr-body"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"

t_case "a body that closes a longer number does not name this issue"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "Closes #70" >"$fx/pr-body"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "a result that is a link is not read"
reset
mkdir -p "$T/elsewhere"
jq -n '{schemaVersion: 4, issues: [{issue: 7, status: "merged", failedStep: "done", branch: "feature/x"}]}' >"$T/elsewhere/7.json"
echo 'ln -sf "$T/elsewhere/7.json" "$TEHAS_HOME/state/cycles/7.result.json"' >"$fx/work.sh"
merged_pr
t_run "$cycle 7"
t_contains "$t_out" "WARN: the result of #7 is a link, it is not read"
t_contains "$(t_calls gh)" 'status `unknown`'
t_lacks "$(t_calls gh)" "pr list"
t_lacks "$(t_calls gh)" "The change was merged"
rm -rf "$T/elsewhere"

t_case "a state file the cycle left in its workspace is not read: only the runner's result is"
reset
mkdir -p "$workdir/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$workdir/.pi/work-state/7.json"
merged_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `unknown`'
t_lacks "$(t_calls gh)" "pr list"

t_case "a result of another schema, or of another issue, is no result"
for change in '.schemaVersion = 3' '.issues[0].issue = 8'; do
  reset
  ended merged "done" feature/x
  jq "$change" "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  merged_pr
  t_run "$cycle 7"
  t_contains "$(t_calls gh)" 'status `unknown`'
done

t_case "a result left by an earlier cycle is not this cycle's"
reset
mkdir -p "$state"
jq -n '{schemaVersion: 4, issues: [{issue: 7, status: "merged", branch: "feature/x"}]}' >"$state/7.result.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `unknown`'
t_eq "$(find "$state/7.result.json" 2>/dev/null | wc -l)" 0

t_case "a pull request a person merged while the cycle ran ends it as merged: no park, no question"
reset
ended running handoff feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
rukas=$(printf 'e%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep '^issue edit')" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_contains "$(t_calls gh)" 'Tehas cycle ended: its pull request https://github.com/acme/widget/pull/9 was merged while the cycle ran (last step `handoff`'
t_contains "$(t_calls gh)" "runner exit 2"
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_lacks "$(t_calls gh)" "issues/7/comments"
t_eq "$(find "$park_id" 2>/dev/null | wc -l)" 0
t_contains "$t_out" "done #7 rc=2 status=merged"
t_contains "$t_out" "pr=https://github.com/acme/widget/pull/9"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 1
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
rm -f "$fx/image.label"

t_case "a pull request closed without a merge parks the issue without the retry boxes"
reset
ended running review feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo CLOSED >"$fx/pr-state"
: >"$T/home/tehas/work/.tehas-workspaces"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas cycle stopped: its pull request https://github.com/acme/widget/pull/9 was closed without being merged"
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"
t_contains "$(t_calls gh)" 'To retry, apply `tehas:ready`.'
t_lacks "$(t_calls gh)" "issues/7/comments"
t_lacks "$(t_calls gh)" "tehas:question"
t_eq "$(find "$park_id" 2>/dev/null | wc -l)" 0
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "an open pull request beside a closed one is the cycle's"
reset
ended handoff commit-pr feature/x
jq -n '[{url: "https://github.com/acme/widget/pull/8", state: "CLOSED", body: "Fixes #7", headRefName: "feature/x"},
        {url: "https://github.com/acme/widget/pull/9", state: "OPEN", body: "Fixes #7", headRefName: "feature/x"}]' >"$fx/pr.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "Pull request waiting for a human: https://github.com/acme/widget/pull/9"

t_case "a state file in the product checkout is not the cycle's: the container never writes there"
reset
mkdir -p "$checkout/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$checkout/.pi/work-state/7.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
rm -rf "$checkout"

t_case "the workspace of a cycle that left a pull request is removed"
reset
ended handoff commit-pr feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "and with it the one kept from an earlier stopped cycle of the issue"
reset
ended handoff commit-pr feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "so is the workspace of a merged cycle"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the workspace of a cycle that stopped is kept for a person to read"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "and so is the one kept from the cycle before it"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
t_run "$cycle 7"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2

# A merge someone else made while the cycle still reviewed is not the cycle's
# own merge: what it had not pushed is only on the host.
t_case "the workspace of a cycle a merge interrupted is kept, and its comment names it"
reset
ended running review feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$(t_calls gh)" "was merged while the cycle ran"
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"
t_lacks "$(t_calls gh)" "$T/home/tehas/work"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "and so is the one kept from an earlier stopped cycle of an interrupted merge"
reset
ended running review feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"

t_case "a cycle the dispatcher stopped for a closed issue names the workspace it kept"
reset
ended running adversarial feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
echo 'mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$(t_calls gh)" "this issue was closed while it ran"
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"

# A run that never cloned a workspace, as one stopped before its images were
# read, has no directory for its comment to name.
t_case "a cycle that made no workspace is not said to have kept one"
reset
ended interrupted review feature/x
rm -rf "$workdir"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "This cycle left no workspace on this host"
t_lacks "$(t_calls gh)" "The workspace is kept at \`$kept_here\`"

# A run that moved an earlier attempt aside and then failed before it cloned
# leaves only that copy, and nothing replaces it: a retry that clones no
# workspace leaves it beside the new one, until a tidy or the poll's sweep.
t_case "a cycle that cloned no workspace names the copy an earlier attempt left"
reset
ended interrupted review feature/x
rm -rf "$workdir"
mkdir -p "$workdir.before-retry"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "The workspace is kept at \`$kept_here.before-retry\`"
t_lacks "$(t_calls gh)" "This cycle left no workspace on this host"

t_case "a directory tehas work did not mark is never removed"
reset
ended merged "done" feature/x
merged_pr
rm -f "$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: $T/home/tehas/work is not marked by tehas work, leaving the workspace $workdir"

t_case "a workspace that cannot be removed is said so"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
chmod a-w "$T/home/tehas/work"
t_run "$cycle 7"
chmod u+w "$T/home/tehas/work"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: could not remove the workspace $workdir"

t_case "another issue's workspace is left alone"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$T/home/tehas/work/widget-9"
t_run "$cycle 7"
t_eq "$(find "$T/home/tehas/work/widget-9" -maxdepth 0 | wc -l)" 1
rm -rf "$T/home/tehas/work"

t_done
