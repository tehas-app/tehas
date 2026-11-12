#!/usr/bin/env bash
# Tests for the gh shim's write log: what each kind of write appends - the two
# words of the kind, the repository and the number of the call - and the
# status a refused call leaves in the line. The real gh is a stub (t_stub)
# that answers what GH_OUT, GH_ERR and GH_RC name: no network, and no GitHub
# asked of anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1
t_case "an api write names the repository and the number of its path"
reset
shim api -X POST repos/o/r/issues/12/labels -f 'labels[]=x'
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "12"
t_eq "$(field 5)" "0"
t_lacks "$(lines)" 'labels[]=x'

t_case "a call with fields and no method is a write, for gh posts it"
reset
shim api repos/o/r/issues/7/comments -f body=hello
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "7"
shim api repos/a/b/git/refs -F name=x
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "a/b"
t_eq "$(field 4)" "-"
# The same path with neither a method nor a field is a read, so the count says
# the read below added nothing to the two writes above.
shim api repos/o/r
t_eq "$(lines | wc -l)" 2

t_case "the -XPOST and --method=POST forms name the same kind"
reset
shim api -XPUT repos/a/b
t_eq "$(field 2)" "api PUT"
shim api --method=PUT repos/a/b
t_eq "$(field 2)" "api PUT"
shim api -Xput repos/a/b
t_eq "$(field 2)" "api PUT"
shim api --method=delete repos/a/b
t_eq "$(field 2)" "api DELETE"
shim api -X DELETE repos/a/b
t_eq "$(field 2)" "api DELETE"

t_case "the kinds are the two words of the call, label's three among them"
reset
shim pr comment 8 -R o/r
t_eq "$(field 2)" "pr comment"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "8"
shim issue create --repo o/r --title T
t_eq "$(field 2)" "issue create"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "-"
shim issue close 3
t_eq "$(field 2)" "issue close"
t_eq "$(field 3)" "-"
t_eq "$(field 4)" "3"
shim pr reopen 11 --repo=o/r
t_eq "$(field 2)" "pr reopen"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "11"
shim pr ready 6
t_eq "$(field 2)" "pr ready"
t_eq "$(field 4)" "6"
shim label create tehas:ready --repo o/r
t_eq "$(field 2)" "label create"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "-"
shim label edit tehas:ready --repo o/r
t_eq "$(field 2)" "label edit"
shim label delete tehas:ready -R o/r
t_eq "$(field 2)" "label delete"
t_eq "$(lines | wc -l)" 8

t_done
