#!/usr/bin/env bash
# Tests for libexec/dispatch: each poll records what it cost in the state store.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

cost() { t_at "$(date +%s)" poll-show | jq -r "$1"; }
# The idle polls of the last hour, as the health check of #744 reads them,
# in the answer's own words.
idle() { t_at "$(date +%s)" poll-idle --since "$1"; }
# The calls the stub recorded: a line each, and the lines of a multi-line
# argument after it start with a space.
calls() { t_calls gh | grep -cE '^(repo|api|issue|pr|label) '; }
log=$T/home/tehas/state/github-writes.log

t_case "a poll records its wall and CPU time, its gh calls and the shim's writes"
reset
: >"$log"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(cost '.last.wall_ms > 0')" true
t_eq "$(cost '.last.cpu_ms > 0')" true
t_eq "$(cost .last.gh)" "$(calls)"
writes=$(wc -l <"$log")
t_eq "$([ "$writes" -gt 0 ] && echo some)" some
t_eq "$(cost .last.writes)" "$writes"
t_eq "$(cost .last.kinds.edit)" "$(grep -c $'\tissue edit\t' "$log")"
t_eq "$(cost .hour.polls)" 1
t_contains "$(cost .line)" "poll      last "
t_lacks "$t_out" "WARN: the cost of the poll"
# One issue picked, so the poll's own row says one start, whatever the late
# cycle's record says after the trap ran.
t_eq "$(cost .last.starts)" 1
t_eq "$(idle 3600 | jq -r .polls)" 0

t_case "a poll that starts no cycle says 0 starts and is one of the idle polls"
reset
# No write log: the host has no GitHub App, so the idle polls have no writes.
rm -f "$log"
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
echo '[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(cost .last.starts)" 0
# The one idle poll of the hour, and no write figure with no write log: the
# figure is left out rather than said as 0.
t_eq "$(idle 3600 | jq -r .polls)" 1
t_eq "$(idle 3600 | jq -r 'has("writes")')" false
t_eq "$(idle 60 | jq -r .polls)" 1
# A window of no seconds holds nothing, and a window that stops before the
# poll ended holds nothing either.
t_eq "$(idle 0 | jq -r .polls)" 0
t_eq "$(t_at 1 poll-idle --since 3600 | jq -r .polls)" 0

t_case "without a write log the writes are not on record, and the calls still are"
reset
rm -f "$log"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(cost .last.writes)" null
t_eq "$(cost .last.gh)" "$(calls)"
# The row of a poll that started cycles is not an idle one.
t_eq "$(idle 3600 | jq -r .polls)" 0

# An installed home for the shim of host/shim/gh, beside the stub of gh that
# stays on PATH: the shim reads $TEHAS_HOME/bin/gh, so a call of it reaches the
# stub through its own path and nothing of the poll's fixtures is disturbed.
install_shim()
{
  local h=$T/home/tehas
  mkdir -p "$h/bin" "$h/shim"
  cp "$T_ROOT/host/shim/gh" "$h/shim/gh"
  chmod +x "$h/shim/gh"
  cp "$T/bin/gh" "$h/bin/gh"
  chmod +x "$h/bin/gh"
}

t_case "the writes of a poll count a graphql mutation and not its queries"
reset
: >"$log"
# both the shim and the gh the poll answers from.
# The shim of host/shim/gh as the host has it, installed beside the stub of gh
# that the poll's own calls answer from: the dispatcher asks GitHub for nothing
# but queries (#792), so none of them belongs in the figure of the poll,
# whatever the word `mutation` does inside one. The stub keeps no line for a
# graphql call of its own, so a line of the log is the shim's alone.
install_shim
# The shape of its own page: the document of -f beside the variables of -F.
t_run "TEHAS_HOME=\"\$T/home/tehas\" TEHAS_GITHUB_WRITES_LOG=$log \"\$T/home/tehas/shim/gh\" api graphql -f 'query=query { viewer { login } }' -F owner=acme -F name=widget"
t_eq "$t_rc" 0
t_run "TEHAS_HOME=\"\$T/home/tehas\" TEHAS_GITHUB_WRITES_LOG=$log \"\$T/home/tehas/shim/gh\" api graphql -F owner=acme -F name=widget -f 'query=query(\$owner: String!, \$name: String!) {
  repository(owner: \$owner, name: \$name) { pullRequests(states: OPEN, first: 100) { nodes { mutation } } } }'"
t_eq "$t_rc" 0
t_eq "$(cat "$log")" ""
# A mutation, named first and named after a name of its own: the two lines the
# figure of the poll counts as other, of the kind no other call of gh gives.
t_run "TEHAS_HOME=\"\$T/home/tehas\" TEHAS_GITHUB_WRITES_LOG=$log \"\$T/home/tehas/shim/gh\" api graphql -f 'query=mutation { addLabels(subjectId: 1) { clientMutationId } }'"
t_eq "$t_rc" 0
t_run "TEHAS_HOME=\"\$T/home/tehas\" TEHAS_GITHUB_WRITES_LOG=$log \"\$T/home/tehas/shim/gh\" api graphql -f 'query=AddLabel mutation(\$n: Int!) { updateLabels(subjectId: \$n) { x } }'"
t_eq "$t_rc" 0
t_eq "$(wc -l <"$log")" 2
t_eq "$(tail -n 1 "$log" | cut -f2)" "api POST"
# The figure the poll records is the log of its window, its own writes included
# (the two cases above prove the figure and its window). What is said here is
# only that the shim leaves no line for the queries the poll asks GitHub, and
# keeps one for each mutation: the four lines of the file are the four calls of
# this case, whatever the poll adds after it. The two queries are asked the two
# ways the poll asks them, the document of -f alone and the document of -f
# beside the variables of -F.
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(cost '.last.writes >= 2')" true

t_case "a dry run records nothing"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(cost .last)" null
t_eq "$(idle 3600 | jq -r .polls)" 0

t_case "a poll that stops early records its cost too"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
echo '[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(cost '.last.gh > 0')" true
t_eq "$(cost .last.starts)" 0

t_done
