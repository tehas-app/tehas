#!/usr/bin/env bash
# Tests for the gh shim's write log: the token of the call. With TEHAS_GH_TOKEN
# the shim hands that token to gh and runs no token program; without it the
# program is asked, as it was; and with no console at all the call still
# reaches gh. The real gh is a stub (t_stub) that answers what GH_OUT,
# GH_ERR and GH_RC name: no network, and no GitHub asked of anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1
# The poll's token (#938): the dispatcher gives every gh child of a poll the
# token it took at its start as TEHAS_GH_TOKEN, and the shim hands it to gh as
# GH_TOKEN and runs no tehasbot github-token. The tehasbot of the home says
# on a file of its own that it ran, so a case sees both that gh was given the
# token and that the program was not run for the call.
t_case "with TEHAS_GH_TOKEN set the shim gives gh that token and runs no token program"
reset
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GH_TOKEN=ghs_poll SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12 -R o/r
token=ghs_poll tehas=ghs_poll"
# A read, so no line of its own, and no run of the token program either.
t_eq "$(lines)" ""
t_eq "$(cat "$T/token-runs" 2>/dev/null | wc -c | tr -d '[:space:]' | head -n 1)" 0

t_case "without TEHAS_GH_TOKEN the shim runs the token program, as it did"
reset
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12 -R o/r
token=ghs_token tehas=unset"
# The program ran once for the one call of the shim.
t_eq "$(cat "$T/token-runs" | wc -c | tr -d '[:space:]' | head -n 1)" 1

t_case "an empty TEHAS_GH_TOKEN asks the token program as the name being unset does"
reset
# env leaves an empty value assigned, as it does for the other names of the
# run. What a case can read of such a call is that the program was asked: the
# name itself reaches the stub of gh empty or not as env gave it, and the shim
# asks for a token either way - the two readings together are the case.
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GH_TOKEN= SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_contains "$t_out" "token=ghs_token"
t_eq "$(cat "$T/token-runs" | wc -c | tr -d '[:space:]' | head -n 1)" 1

t_case "with no console the shim still runs gh, with its token, and logs nothing"
reset
# A host that needs repair: the console is gone or cannot be run. Reads and
# writes reach gh as they came, and the token is still given.
rm -f "$T/home/tehas/libexec/console"
T_ENV=(TEHAS_GH_TOKEN=ghs_poll SHOW_TOKEN=1)
shim issue comment 12 -R o/r --body secret
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue comment 12 -R o/r --body secret
token=ghs_poll tehas=ghs_poll"
shim issue view 12
t_eq "$t_rc" 0
t_contains "$t_out" "gh issue view 12"
printf 'not a program\n' >"$T/home/tehas/libexec/console"
chmod 644 "$T/home/tehas/libexec/console"
T_ENV=(GH_RC=3 TEHAS_GH_TOKEN=ghs_poll)
shim pr merge 5 --repo o/r
t_eq "$t_rc" 3
t_eq "$t_out" "gh pr merge 5 --repo o/r"
t_eq "$(lines)" ""

t_done
