#!/usr/bin/env bash
# Tests for t_gh_fail in test/lib.sh: the gh of a case can be told to fail one
# chosen call, as GitHub does on a bad minute. The helper names calls by their
# number in the case, counts every call the t_stub gh stub makes, records the
# failed call as it records any other, and lasts only for the case that named
# it.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The calls of a case, each answered as the case before it was.
calls_made() { t_calls gh; }

t_case "the named call fails with the defaults, and the others answer as today"
t_stub gh 'echo "answer $*"'
t_gh_fail 2
t_run 'gh one; echo "rc=$?"; gh two; echo "rc=$?"; gh three; echo "rc=$?"'
t_contains "$t_out" "answer one"
t_contains "$t_out" "HTTP 502: Bad Gateway"
t_contains "$t_out" "answer three"
t_eq "$(grep -c '^rc=' <<<"$t_out")" 3
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out" | tr '\n' ' ')" "0 1 0 "
t_eq "$(calls_made)" "$(printf '%s\n' one two three)"

t_case "the message and the status are what the case gave"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 1 7 "HTTP 403: Forbidden"
t_run 'gh api user; echo "rc=$?"'
t_eq "$t_out" "HTTP 403: Forbidden
rc=7"
t_eq "$(calls_made)" "api user"

t_case "several calls may be named, each with its own message and status"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 1
t_gh_fail 3 3 "HTTP 429: Too Many Requests"
t_run 'for n in one two three four; do gh "$n" 2>/dev/null; printf "%s rc=%s\\n" "$n" "$?"; done'
t_eq "$t_out" "$(printf '%s\n' 'one rc=1' 'two rc=0' 'three rc=3' 'four rc=0')"
t_eq "$(calls_made)" "$(printf '%s\n' one two three four)"

t_case "a call beyond the last made changes nothing"
rm -f "$T"/calls/*
t_stub gh 'echo yes'
t_gh_fail 5
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "yes
rc=0"

t_case "the next t_case clears the failures and the count"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 1
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "HTTP 502: Bad Gateway
rc=1"
t_case "a case after a failing one sees an ordinary gh"
rm -f "$T"/calls/*
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "rc=0"
t_eq "$(calls_made)" "one"

t_case "a body that execs away is still counted, and a named call still fails it"
rm -f "$T"/calls/*
t_stub gh 'exec echo execed'
t_gh_fail 2
t_run 'gh one; gh two; echo "rc=$?"'
t_contains "$t_out" "execed"
t_contains "$t_out" "HTTP 502: Bad Gateway"
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out")" 1
t_eq "$(calls_made)" "$(printf '%s\n' one two)"

t_case "a body of the case still answers the calls it does not name"
rm -f "$T"/calls/*
body_case='case "$1 $2" in
  "issue view") echo "{}" ;;
esac
exit 1'
t_stub gh "$body_case"
t_gh_fail 2 4 "HTTP 502: Bad Gateway"
t_run 'gh issue view 7 >/dev/null 2>&1; echo "rc=$?"; gh issue list >/dev/null 2>&1; echo "rc=$?"'
t_eq "$t_out" "rc=1
rc=4"

t_case "the failed call is counted and recorded as any other is"
rm -f "$T"/calls/*
t_stub gh
printf 'the body' >"$T/attach.txt"
t_gh_fail 2
t_run 'gh issue edit 7 --note-file "$T/attach.txt" >/dev/null; gh issue edit 7 --note-file "$T/attach.txt"; echo "rc=$?"'
t_eq "$t_out" "HTTP 502: Bad Gateway
rc=1"
t_eq "$(calls_made)" "$(printf '%s\n' "issue edit 7 --note-file $T/attach.txt" "issue edit 7 --note-file $T/attach.txt")"

t_case "a call it does not name leaves every call it made answered"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 4
t_run 'gh one; gh two; gh three; echo "rc=$?"'
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out")" 0
t_eq "$(calls_made)" "$(printf '%s\n' one two three)"

t_case "naming one call twice is a failure of the case, not a gh that answers"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
# A case that names one call twice to change its status is a mistake of the
# case: t_bad counts it, the second answer is not written down, and the call the
# first answer named still fails. t_bad moves the tally and writes to standard
# error, so the case reads the count it moved and puts it back, as the guard of
# pack_release_test.sh does: a guard that cannot fail is not a guard.
t_gh_fail 1 4 "first-word"
failed_before=$t_failed
t_gh_fail 1 4 "second-word" 2>"$T/bad"
t_eq "$t_failed" "$((failed_before + 1))"
t_contains "$(<"$T/bad")" "t_gh_fail: call 1 is already named by this case"
t_failed=$failed_before
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "first-word
rc=4"
t_eq "$(calls_made)" "one"

t_case "a duplicate that reaches the stub fails the call at once"
rm -f "$T"/calls/*
t_stub gh 'echo ok
echo "the body answered"'
# The answers of a case, written by hand: a duplicate reaches the stub itself,
# which says so and ends the call with no body run and no answer of ok.
printf '1\t0\tfirst-word\n1\t4\tsecond-word\n' >"$T/gh-fails"
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "t_gh_fail: call 1 is named twice: 0 then 4
rc=64"
t_lacks "$t_out" "the body answered"
t_eq "$(calls_made)" "one"

t_case "t_gh_fail with no call named reports through t_bad"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
# The shell under lib.sh has set -u: the missing argument is reported as a
# failure of the case, not as an unbound variable of lib.sh that ends the file.
# The empty status is refused with it, and the run after either is still an
# ordinary one: the case went on.
failed_before=$t_failed
t_gh_fail 2>"$T/bad"
t_eq "$t_failed" "$((failed_before + 1))"
t_contains "$(<"$T/bad")" "t_gh_fail: the call and its status are numbers, got '' '1'"
t_gh_fail 1 "" "HTTP 502: Bad Gateway" 2>"$T/bad"
t_eq "$t_failed" "$((failed_before + 2))"
t_contains "$(<"$T/bad")" "t_gh_fail: the call and its status are numbers, got '1' ''"
t_failed=$failed_before
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "ok
rc=0"

t_case "a name or a status the stub cannot answer is refused, and writes nothing"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
# Every spelling here would be written down and match no call of the stub, which
# counts 1, 2, 3 and compares character for character: the case would go on
# believing it had failed one. 0 and 256 are refused with them: a call that ends
# 0 has not failed, and 256 ends 0 after the wrap of the shell's status. The
# shape of both fields is settled by pattern, so a value bash reads as neither
# (1*, 1e2, 2*3) is refused through t_bad with the helper's own words and not by
# a complaint of [ ] on standard error that moves no tally. The call and the
# status are separate entries: an entry of two words would be split by the loop
# and the leading blank of ' 1' lost before the helper saw it.
for bad in "01 4" "1 300" "1 256" "1 0" "1* 9" "1e2 9" "1 2*3" "256 1"; do
  failed_before=$t_failed
  # shellcheck disable=SC2086  # the two words of the entry are the two arguments
  t_gh_fail $bad 2>"$T/bad"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_contains "$(<"$T/bad")" "t_gh_fail: the call and its status are numbers, got"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_failed=$failed_before
done
for bad in ' 1' ' 12'; do
  failed_before=$t_failed
  t_gh_fail "$bad" 4 2>"$T/bad"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_contains "$(<"$T/bad")" "t_gh_fail: the call and its status are numbers, got '"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_failed=$failed_before
done
for bad in 0 0002 000000004 '1*'; do
  failed_before=$t_failed
  t_gh_fail 1 "$bad" 2>"$T/bad"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_contains "$(<"$T/bad")" "got '1' '$bad'"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_failed=$failed_before
done
t_eq "$([ -s "$T/gh-fails" ] && echo written)" ""
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "ok
rc=0"

t_case "a name above 255 is refused, and 255 is answered by the stub"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
failed_before=$t_failed
t_gh_fail 256 2>"$T/bad"
t_eq "$t_failed" "$((failed_before + 1))"
t_contains "$(<"$T/bad")" "got '256' '1'"
t_failed=$failed_before
# 255 is the last name the stub can reach by counting: the call is named, fails
# and is recorded as any other is.
t_gh_fail 255 4 "the named one"
t_run 'for i in $(seq 1 255); do gh "c$i" >/dev/null 2>&1 || echo "c$i rc=$?"; done'
t_eq "$t_out" "c255 rc=4"
t_eq "$(calls_made | wc -l)" 255

t_done
