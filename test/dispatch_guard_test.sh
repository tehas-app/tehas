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


t_done
