#!/usr/bin/env bash
# Tests for tehasbot dispatch: the `Spec:` line of an issue body, which the
# spec-pipe feature reads at a start. The feature is still being built, so a
# case turns it on with TEHAS_TEST_FEATURES, as a test build may.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

link=https://github.com/user-attachments/files/12/spec.md
printf '# Faster search\n\nR1: under 100 ms.\n' >"$fx/spec.md"
sum=$(sha256sum "$fx/spec.md" | cut -d' ' -f1)

# curl hands out fx/spec.md for any link, or fails with fx/curl.rc.
t_stub curl '
out=""
while [ $# -gt 0 ]; do
  case "$1" in -o) out=$2; shift ;; esac
  shift
done
[ -f "$T/fx/curl.rc" ] && exit "$(cat "$T/fx/curl.rc")"
cp "$T/fx/spec.md" "$out"
printf text/markdown'

# body <text>: the body of issue 7, ready and labelled by alice.
body() { jq -n --arg b "$1" '{title: "Faster search", body: $b}' >"$fx/issue-7.json"; }
# pipe_on: the instance switches the feature on and the run may use it.
pipe_on() {
  slots 1 TEHAS_SPEC_PIPE=on
  T_ENV+=(TEHAS_TEST_FEATURES=spec-pipe)
}

t_case "with the feature off a body whose Spec: line is malformed starts as before"
reset
body "Make it fast.

Spec: docs/specs/search.md"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "

t_case "with the feature off a Spec: line fetches nothing and pins nothing"
reset
rm -f "$fx/curl.rc"
body "Spec: $link sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(t_calls curl)" ""
t_eq "$(t_row approval-current 7 -- '.current.input.spec')" null

t_case "with the feature on a spec of the bytes its line names starts, and the approval keeps the pin"
reset
pipe_on
body "Make it fast.

Spec: $link sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_contains "$(t_calls curl)" "$link"
t_eq "$(t_row approval-current 7 -- '.current.input.spec | tostring')" "{\"link\":\"$link\",\"sha256\":\"$sum\"}"
t_eq "$(find "$T/home/tehas/tmp" -name 'spec.*' | wc -l)" 0

t_case "with the feature on a spec changed since its line was written starts nothing and says why once"
reset
pipe_on
body "Spec: $link sha256:$(printf '%064d' 0 | tr 0 a)"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "skip #7: the bytes of its spec are not the ones its \`Spec:\` line names: they have sha256 $sum"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing: the bytes of its spec are not the ones its \`Spec:\` line names: they have sha256 $sum, so the spec is not the one that was approved. A person attaches the spec"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_row approval-current 7 -- '.current')" null

t_case "with the feature on a spec link GitHub refuses starts nothing"
reset
pipe_on
echo 22 >"$fx/curl.rc"
body "Spec: $link sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "skip #7: GitHub refused the link of its \`Spec:\` line"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"

t_case "with the feature on a spec fetch that fails on the way changes nothing and ends the poll with 1"
reset
pipe_on
echo 7 >"$fx/curl.rc"
body "Spec: $link sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_contains "$t_out" "ERROR: the spec of #7 could not be fetched (curl exited 7), not starting a cycle"
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"
rm -f "$fx/curl.rc"

t_case "with the feature on a body with no Spec: line starts"
reset
pipe_on
body "Make it fast."
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "

t_case "with the feature on a malformed Spec: line starts nothing and says why once"
reset
pipe_on
body "Make it fast.

Spec: docs/specs/search.md"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "skip #7: its \`Spec:\` line is not a link and its sha256"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing: its \`Spec:\` line is not a link and its sha256. A person writes the one line \`Spec: <link of the attached file> sha256:<64 hex digits of its bytes>\`, or takes it out, and applies \`tehas:ready\` again."
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1

t_case "with the feature on a Spec: link that is no file attached on GitHub starts nothing"
reset
pipe_on
body "Spec: https://example.com/spec.md sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "skip #7: the link of its \`Spec:\` line is not a file attached on GitHub"
t_lacks "$(t_calls curl)" "example.com"

t_case "with the feature on two Spec: lines start nothing"
reset
pipe_on
body "Spec: $link sha256:$sum
Spec: $link sha256:$sum"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "skip #7: it has more than one \`Spec:\` line"

t_case "the switch set on without the test variable is refused with 78 before any call"
reset
slots 1 TEHAS_SPEC_PIPE=on
body "Make it fast."
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_SPEC_PIPE is on and spec-pipe is still being built in this release"
t_eq "$(t_calls gh)" ""
t_eq "$(started)" ""

t_done
