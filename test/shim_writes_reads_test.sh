#!/usr/bin/env bash
# Tests for the gh shim's write log: the calls that are reads, of gh's own
# commands and of the api, add no line at all. The real gh is a stub (t_stub)
# that answers what GH_OUT, GH_ERR and GH_RC name: no network, and no GitHub
# asked of anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1
t_case "reads add no line at all"
reset
# The stub's answer names the call, so one read of the caller's stream says gh
# was reached, and the empty log says nothing was charged for it.
shim issue view 12
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12"
shim api repos/o/r
t_eq "$t_rc" 0
shim pr view 3 --repo o/r
t_eq "$t_rc" 0
shim issue list --repo o/r
t_eq "$t_rc" 0
shim pr status
t_eq "$t_rc" 0
shim label list --repo o/r
t_eq "$t_rc" 0
shim label get tehas:ready
t_eq "$t_rc" 0
shim auth status
t_eq "$t_rc" 0
t_eq "$(lines)" ""

t_done
