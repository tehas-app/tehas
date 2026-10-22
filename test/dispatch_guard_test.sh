#!/usr/bin/env bash
# Tests for libexec/dispatch: what stops a poll before it starts. The
# repository, the pause label and flag, invalid settings and the arguments.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

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
t_eq "$(t_calls fake-start)" ""
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

t_case "the pause flag stops dispatch, and deciding it asks GitHub nothing"
reset
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}'
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "paused: the flag tehas pause wrote at 2026-10-15T10:00:00Z holds this host"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls fake-start)" ""
# The flag decides on its own: the control issue is not asked about at all.
t_lacks "$(t_calls gh)" "issue view"
t_lacks "$(t_calls gh)" "issues/1/events"

t_case "a flag that cannot be read starts nothing and is said"
reset
pause_flag 'not a flag at all'
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "state/pause.json is not a pause flag: taking it as paused"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a flag written by hand with no reason still holds the host"
reset
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":""}'
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "holds this host"
t_eq "$(t_calls fake-start)" ""

# The label is a stop anyone may apply and only an allowed account may lift.
pause_off() { # <login or "">: the control issue's last tehas:pause event is that account's removal
  local actor
  [ -z "$1" ] && actor="null" || actor="{\"login\":\"$1\"}"
  jq -nc --argjson a "$actor" '[{event: "labeled", label: {name: "tehas:pause"}, actor: {login: "alice"}, created_at: "2026-10-15T09:00:00Z"},
    {event: "unlabeled", label: {name: "tehas:pause"}, actor: $a, created_at: "2026-10-15T10:00:00Z"}]' >"$fx/events-1.json"
}

t_case "a pause the bot removed leaves the factory paused, and says who"
reset
pause_off tehas-bot
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas:pause was taken off #1 by 'tehas-bot'"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a pause removed by an account GitHub does not name leaves the factory paused"
reset
pause_off ''
t_run "$dispatch --dry-run"
t_contains "$t_out" "taken off #1 by 'unknown'"
t_eq "$(t_calls fake-start)" ""

t_case "a pause an allowed account removed resumes the factory"
reset
pause_off alice
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_lacks "$t_out" "taken off #1"

t_case "a control issue that was never paused is dispatched from as usual"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"

t_case "a pause applied again after an allowed removal holds the host again"
reset
pause_off alice
jq -c '. + [{event: "labeled", label: {name: "tehas:pause"}, actor: {login: "mallory"}, created_at: "2026-10-15T11:00:00Z"}]' "$fx/events-1.json" >"$fx/events-1.json.new"
mv "$fx/events-1.json.new" "$fx/events-1.json"
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "paused: issue #1 carries tehas:pause"
t_eq "$(t_calls fake-start)" ""

t_case "a pause whose label was deleted holds the host"
reset
echo '[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"mallory"},"created_at":"2026-10-15T09:00:00Z"}]' >"$fx/events-1.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "its last tehas:pause event only applied it"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls fake-start)" ""

t_case "a pause applied and taken off in the same second holds the host"
reset
jq -nc '[{event: "unlabeled", label: {name: "tehas:pause"}, actor: {login: "alice"}, created_at: "2026-10-15T10:00:00Z"},
  {event: "labeled", label: {name: "tehas:pause"}, actor: {login: "mallory"}, created_at: "2026-10-15T10:00:00Z"}]' >"$fx/events-1.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "only applied it"
t_eq "$(t_calls fake-start)" ""

t_case "events that cannot be read start nothing"
reset
rm -f "$fx/events-1.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "gh api repos/acme/widget/issues/1/events?per_page=100 failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "events that cannot be understood start nothing"
reset
echo 'not a list of events' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "unreadable label events of #1"
t_eq "$(t_calls fake-start)" ""

t_case "an invalid instance makes no call at all and exits 78"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an empty labeller list does not widen who may label"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS=''" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""

# Arguments. A poll starts only from an argument this command knows, and the
# check runs before the instance settings are read.
t_case "--help prints the usage of the command and calls nothing"
reset
t_run "$dispatch --help 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_contains "$t_out" "tehas dispatch --dry-run"
t_lacks "$t_out" "set -uo pipefail"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

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
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an unknown argument is reported on standard error, never as a poll"
reset
t_run "$dispatch --dryrun 2>/dev/null"
t_eq "$t_rc" 64
t_lacks "$t_out" "unknown argument"
t_run "$dispatch --dryrun 1>/dev/null"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
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
  t_contains "$t_out" "tehas dispatch: unknown argument: $arg"
done
t_eq "$(t_calls gh)" ""

t_case "an empty argument is an argument, not a poll"
reset
t_run "$dispatch ''"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument:"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "two arguments are refused, not combined"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dry-run --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --help"
t_lacks "$t_out" "would run a cycle"
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
t_contains "$t_out" "tehas dispatch: unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "no argument and --dry-run still behave as before"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-start)" "7"

t_case "all-hands on the control issue starts nothing and holds nothing"
reset
echo '{"labels":[{"name":"tehas:all-hands"}]}' >"$fx/control.json"
echo '[{"event":"labeled","label":{"name":"tehas:all-hands"},"actor":{"login":"alice"}}]' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"


t_case "a poll runs tidy once, after the cycles it starts"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls fake-tidy | wc -l)" 1
t_eq "$(t_calls fake-tidy)" ""
t_eq "$([ -e "$T/calls/tidy.after-start" ] && echo after)" "after"

t_case "a dry run runs tidy with --dry-run"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-tidy)" "--dry-run"

t_case "a paused poll still tidies"
reset
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-tidy | wc -l)" 1

t_case "a poll that fails after the bot's login still tidies, and keeps its exit status"
reset
rm "$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-tidy | wc -l)" 1

t_case "a failed tidy does not change the poll's exit status"
reset
t_stub fake-tidy 'exit 1'
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_stub fake-tidy '[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/tidy.after-start"'

t_case "a poll that stops before the bot's login runs no tidy"
reset
echo '{"nameWithOwner":"acme/gadget"}' >"$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-tidy)" ""

# The size of one body, as the measure reads it: a path counts when it stands
# between two ticks, so the ticks are built from their octal code (the same one
# lib/sizing.sh uses) and never typed. <word> is the letters of the names, so a
# case can tell one body from another by what it says.
TICK=$(printf '\140')
wide_body() { # <n> [<word>]: a body the measure reads as n files
  local i out='Wide issue, one cycle.'$'\n'
  for i in $(seq 1 "$1"); do
    out+="Change $TICK lib/${2:-aa}$i.sh$TICK.
"
  done
  printf '%s' "$out"
}

# A poll of one issue whose text is the body, labelled tehas:ready by alice.
wide_issue() { # <n of files> [<word>]
  reset
  jq -cn --arg b "$(wide_body "$1" "${2:-}")" \
    '{number: 7, state: "open", title: "Wide", body: $b,
      labels: [{name: "tehas:ready"}]}' >"$fx/work-7.json"
}

t_case "with no TEHAS_SIZE_MAX_FILES a wide issue starts as any other"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "off this issue, put"

t_case "the measure is asked only when the instance names a ceiling"
wide_issue 6
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "an issue over the ceiling is held: its label comes off, needs-human-attention goes on, and the issue says the counts once"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its body names 6 files, more than TEHAS_SIZE_MAX_FILES (3), and one cycle does not make that many changes"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue, put \`needs-human-attention\` on it and started nothing. Its body names 6 files, more than the 3 of \`TEHAS_SIZE_MAX_FILES\`"
t_contains "$(t_calls gh)" "apply \`tehas:ready\` again to start one of them"
# One label change and one comment: the comment of a start is not there.
t_eq "$(grep -c '^issue edit 7 ' <<<"$(t_calls gh)")" 1
t_eq "$(grep -c '^issue comment 7 ' <<<"$(t_calls gh)")" 1

t_case "an issue of exactly the ceiling starts"
wide_issue 3
slots 1 TEHAS_SIZE_MAX_FILES=3
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "off this issue, put"

t_case "and a body with none starts"
wide_issue 0
slots 1 TEHAS_SIZE_MAX_FILES=1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

# A refresh of a wide issue starts: rule 6 holds the `tehas:ready` path, and
# rule 4b takes no path through the hold. The request is the one of rule 4b,
# for pull request 50, which conflicts with main and is the bot's own.
refresh_request() {
  local head base
  head=$(printf 'a%.0s' {1..40})
  base=$(printf 'c%.0s' {1..40})
  mkdir -p "$T/home/tehas/state/refresh" "$T/home/tehas/state/cycles"
  jq -cn --arg h "$head" --arg b "$base" '{at: "2026-10-17T09:00:00Z", pr: 50,
    head_sha: $h, base_sha: $b}' >"$T/home/tehas/state/refresh/7.json"
  jq -cn --arg h "$head" --arg b "$base" '{data: {repository: {pullRequest: {
    state: "OPEN", number: 50, url: "https://github.com/acme/widget/pull/50",
    isDraft: false, isCrossRepository: false, headRefOid: $h, baseRefName: "main",
    baseRefOid: $b, mergeable: "CONFLICTING", author: {login: "tehas-bot"},
    headRepository: {nameWithOwner: "acme/widget"}, labels: {nodes: []},
    closingIssuesReferences: {totalCount: 1, nodes: [{number: 7, state: "OPEN",
      labels: {nodes: [{name: "tehas:awaiting-merge"}]},
      closedByPullRequestsReferences: {nodes: [{number: 50, state: "OPEN"}]}}]}}}}}' >"$fx/pull-50.json"
  echo '{"verbs":{"work":{"flags":["--json","--timeout","--restart","--continue-pr"]}}}' >"$fx/caps.json"
  jq -nc --arg sum "$WSUM" '{at: "2026-10-01T00:00:00Z", by: "alice", how: "label",
    body_sha256: $sum, links: []}' >"$approved_file"
}

t_case "a refresh of a wide issue starts: the hold is never asked of one"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3 TEHAS_REFRESH_CONFLICTS=1
echo '[]' >"$fx/ready.json"
echo '[]' >"$fx/events-7.json"
echo '[{"number":7}]' >"$fx/awaiting.json"
jq -nc '{number: 7, state: "open", labels: [{name: "tehas:awaiting-merge"}], body: "Words."}' >"$fx/work-7.json"
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words."}}}}' >"$fx/edit-7.json"
echo '[]' >"$fx/icomments-7.json"
refresh_request
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (refresh by alice)"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "off this issue, put"

# An issue that carries tehas:plan and no tehas:ready is read by the measure
# and started all the same: the hold is the one a fresh cycle rests on.
t_case "a planning turn on a wide issue is not held"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls | grep -c 7)" 1
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "off this issue, put"

# The hold is asked on the `tehas:ready` path alone, so the three paths that
# start without that label -- a tick on a parked cycle, a cycle a provider
# outage parked, and a refresh -- are not held by it either, and a wide issue on
# any of them starts. The stand-ins for those paths read the model endpoint, so
# the endpoint answers for the whole file: one answer a poll, kept for the rest
# of it.
mesh '{"data":[{"id":"test-model"}]}'
mesh_curl

t_case "a retry tick on a wide parked issue starts the cycle"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
parked
comments 7 "$(park_body x ' ')" alice
jq -cn --arg b "$(wide_body 6)" '{number: 7, state: "open", title: "Wide", body: $b,
  labels: [{name: "needs-human-attention"}, {name: "tehas:question"}]}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"

t_case "a cycle a provider outage parked starts again on a wide issue"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
# Rule 4a: the retry of an outage-parked cycle rests on the account that started
# the parked one, and asks the model endpoint first. The park file is that
# rule's own, and the issue carries no tehas:ready for the hold to read.
parked
comments 7 "$(park_body ' ' ' ')" ""
mesh '{"data":[{"id":"test-model"}]}'
mkdir -p "$cycles_dir"
jq -n --argjson s "$(date -d 2026-01-01T02:00:00Z +%s)" \
  '{cap: "step-failed:adversarial", reason: "Connection error.", provider: true,
    retry: true, started: $s}' >"$cycles_dir/7.park"
echo alice >"$cycles_dir/7.by"
echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
jq -cn --arg b "$(wide_body 6)" '{number: 7, state: "OPEN", title: "Wide", body: $b,
  labels: [{name: "needs-human-attention"}]}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"

# A dry run says the hold and changes nothing, as every label change here does.
t_case "a dry run of a wide issue says it and touches no label"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would take tehas:ready off #7 and add needs-human-attention: its body names 6 files, more than TEHAS_SIZE_MAX_FILES (3)"
t_lacks "$(t_calls gh)" "--remove-label tehas:ready"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "issue comment 7"

# The comment follows the label change, as an epic's does: while the change
# fails the next poll finds the same label and must not say the same once more.
t_case "when the labels cannot be changed nothing is said on the issue, and the poll goes on"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
echo "issue edit 7" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready --add-label needs-human-attention"
t_lacks "$(t_calls gh)" "issue comment 7"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/edit-fails-on"

t_case "an unreadable body ends the poll rather than starting a cycle on a guess"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
# Not a body at all: the measure is given the number 42 and answers nothing that
# reads as a count.
jq '.body = 42' "$fx/work-7.json" >"$fx/work-7.tmp"
mv "$fx/work-7.tmp" "$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the size of #7 could not be read"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"

# The hold is asked on the `tehas:ready` path alone: an issue the poll holds
# back for another reason is never measured, and asks nothing for it.
t_case "a wide issue the poll would not start anyway is not held, and says nothing"
wide_issue 6
slots 1 TEHAS_SIZE_MAX_FILES=3
# Its tehas:ready came from an account outside the list, which the poll takes
# off before the size is ever asked.
label_events 7 tehas:ready=bob
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "clear #7: tehas:ready off, applied by 'bob'"
t_lacks "$t_out" "TEHAS_SIZE_MAX_FILES"
t_lacks "$(t_calls gh)" "--add-label needs-human-attention"

t_done


