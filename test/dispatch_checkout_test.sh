#!/usr/bin/env bash
# Tests for libexec/dispatch: the checkout and the code index, edits after the
# label, pull requests that close the issue, and the branch.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "a real poll puts the checkout on main and fast-forwards it"
reset
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" checkout -q -b feature/left-over
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(git -C "$checkout" branch --show-current)" "main"
t_eq "$(git -C "$checkout" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_lacks "$t_out" "WARN"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "a fetch that hangs is given up after its bound, said, and the cycle still starts"
reset
printf '#!/bin/sh\nexec sleep 30 2>/dev/null\n' >"$T/slow-upload-pack"
chmod +x "$T/slow-upload-pack"
git -C "$checkout" config remote.origin.uploadpack "$T/slow-upload-pack"
T_ENV+=(TEHAS_FETCH_WAIT=1)
began=$(date +%s)
t_run "$dispatch"
git -C "$checkout" config --unset remote.origin.uploadpack
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: could not fast-forward $checkout"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(($(date +%s) - began < 20))" 1

t_case "the code index is refreshed after the fetch and before the cycle"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(cat "$T/calls/index.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "an index that cannot be refreshed is a warning, and the cycle still runs"
reset
: >"$fx/index-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not refreshed"
t_eq "$(t_calls fake-start)" "7"
rm "$fx/index-fails"

t_case "an index that runs as its own unit holds the cycles until the poll after it"
reset
: >"$fx/index-runs"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: the code index is refreshed first, as tehas-index.service; the cycles start on the poll after it"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls fake-index-start)" "$T/home/tehas/logs/dispatch-index.log $T/home/tehas/state/index.rc $T/home/tehas/state/dispatch.again"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
rm "$fx/index-runs"

t_case "the poll after the index run says how it ended and starts the cycles"
base=$(git -C "$checkout" rev-parse origin/main)
mkdir -p "$T/home/tehas/state/index"
: >"$T/home/tehas/state/index/widget.db"
jq -n --arg c "$base" '{commit: $c}' >"$T/home/tehas/state/index.json"
echo 1 >"$T/home/tehas/state/index.rc"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not refreshed, see $T/home/tehas/logs/dispatch-index.log"
t_eq "$(t_calls fake-index)" ""
t_eq "$(t_calls fake-start)" "7"
t_eq "$(find "$T/home/tehas/state/index.rc" 2>/dev/null | wc -l)" 0
rm -rf "$T/home/tehas/state/index" "$T/home/tehas/state/index.json"

t_case "an index run that left its exit status has ended, though systemd still shows its unit"
reset
echo 0 >"$T/home/tehas/state/index.rc"
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo active ;;
esac'
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$t_out" "holding: the cycles wait for the code index"
t_eq "$(t_calls fake-start)" "7"
t_stub systemctl "$systemctl_stub"

t_case "an index the indexer failed at this tip is not run again by a poll"
reset
git -C "$checkout" rev-parse origin/main >"$T/home/tehas/state/index.failed"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-index)" ""
t_eq "$(t_calls fake-start)" "7"
rm -f "$T/home/tehas/state/index.failed"

t_case "while an index run holds its lock no cycle starts, and the poll says why"
reset
exec {ixfd}>>"$T/home/tehas/state/index.lock"
flock -n "$ixfd"
t_run "$dispatch"
exec {ixfd}>&-
t_eq "$t_rc" 0
t_contains "$t_out" "holding: the cycles wait for the code index, which tehas-index.service is refreshing"
t_eq "$(t_calls fake-index)" ""
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a dry run refreshes no index"
reset
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-index)" ""

t_case "a real poll does not start an issue the bot labelled, and takes that label off"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"the-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:ready')" 1
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1

t_case "a cycle does not start when the issue cannot be marked as running"
reset
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not mark #7 as running"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/edit-fails"

t_case "a body edited by its author after the label was applied is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
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

t_case "a title changed after the label by an account with no login is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":null,"created_at":"2026-01-02T10:01:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'unknown'"
t_lacks "$t_out" "would run a cycle"

t_case "a title rename with no time after the label is not approved"
reset
# The one case the poll's own read can differ on, so it is kept here; the full
# matrix of renames is the fixtures of
# console/dispatch/tests/fixtures/approval.json. No edit record either: the
# unstamped rename alone has to hold the start, or the case would pass on a
# body edit and say nothing about the rename.
echo '{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}' >"$fx/edit-7.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'mallory'"
t_lacks "$t_out" "would run a cycle"

t_case "a title rename with no time and no account is not approved"
reset
echo '{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}' >"$fx/edit-7.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":null}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'unknown'"
t_lacks "$t_out" "would run a cycle"

t_case "a title rename before the label is what the label approved"
reset
echo '[{"event":"renamed","actor":{"login":"mallory"},"created_at":"2026-01-02T09:59:00Z"},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7"

# The pull requests GitHub says close an issue: <url> <state>...
closing_prs() { # <issue> [<url> <state>]...
  local n=$1 nodes="[]"
  shift
  while [ "$#" -gt 1 ]; do
    nodes=$(jq -c --arg u "$1" --arg s "$2" '. + [{url: $u, state: $s}]' <<<"$nodes")
    shift 2
  done
  jq -nc --argjson nodes "$nodes" '{data: {repository: {issue: {closedByPullRequestsReferences: {nodes: $nodes}}}}}' >"$fx/prs-$n.json"
}

t_case "an issue with an open pull request that closes it starts nothing, and loses tehas:ready"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: pull request https://github.com/acme/widget/pull/90 is open and closes it"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing: https://github.com/acme/widget/pull/90 is open and closes it."

t_case "a closed or merged pull request that closed it holds nothing back"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 CLOSED https://github.com/acme/widget/pull/91 MERGED
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "closedByPullRequestsReferences"

t_case "an open pull request does not hold up the issue behind it"
reset
ready 7 8
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"

t_case "when the label cannot be taken off, nothing is said, and the next poll tries again"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
echo "remove-label tehas:ready" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "a dry run says it would take the label off, and changes nothing"
reset
closing_prs 7 https://github.com/acme/widget/pull/90 OPEN
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: pull request https://github.com/acme/widget/pull/90 is open and closes it"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "an edited issue does not hold up the one behind it"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
cp "$fx/events-7.json" "$fx/events-8.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #8"
rm -f "$fx/events-8.json"

t_case "a pause answer that cannot be understood dispatches nothing"
reset
echo '{"labels":null}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""

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

t_case "--init-labels is tehas labels now: here it is an unknown argument"
reset
t_run "$dispatch --init-labels"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --init-labels"
t_eq "$(t_calls gh)" ""

t_done
