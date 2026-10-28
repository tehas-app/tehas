#!/usr/bin/env bash
# Tests for t_gh_fail in test/lib.sh: the gh of a case can be told to fail one
# chosen call, as GitHub does on a bad minute. The helper names calls by their
# number in the case, counts every call the t_stub gh stub makes, records the
# failed call as it records any other, and lasts only for the case that named
# it.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_case "the named call fails with the defaults, and the others answer as today"
t_stub gh 'echo "answer $*"'
t_gh_fail 2
t_run 'gh one; echo "rc=$?"; gh two; echo "rc=$?"; gh three; echo "rc=$?"'
t_contains "$t_out" "answer one"
t_contains "$t_out" "HTTP 502: Bad Gateway"
t_contains "$t_out" "answer three"
t_eq "$(grep -c '^rc=' <<<"$t_out")" 3
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out" | tr '\n' ' ')" "0 1 0 "
t_eq "$(t_calls gh)" "$(printf '%s\n' one two three)"

t_case "the message and the status are what the case gave"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 1 7 "HTTP 403: Forbidden"
t_run 'gh api user; echo "rc=$?"'
t_eq "$t_out" "HTTP 403: Forbidden
rc=7"
t_eq "$(t_calls gh)" "api user"

t_case "several calls may be named, each with its own message and status"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 1
t_gh_fail 3 3 "HTTP 429: Too Many Requests"
t_run 'for n in one two three four; do gh "$n" 2>/dev/null; printf "%s rc=%s\\n" "$n" "$?"; done'
t_eq "$t_out" "$(printf '%s\n' 'one rc=1' 'two rc=0' 'three rc=3' 'four rc=0')"
t_eq "$(t_calls gh)" "$(printf '%s\n' one two three four)"

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
t_eq "$(t_calls gh)" "one"

t_case "a body that execs away is still counted, and a named call still fails it"
rm -f "$T"/calls/*
t_stub gh 'exec echo execed'
t_gh_fail 2
t_run 'gh one; gh two; echo "rc=$?"'
t_contains "$t_out" "execed"
t_contains "$t_out" "HTTP 502: Bad Gateway"
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out")" 1
t_eq "$(t_calls gh)" "$(printf '%s\n' one two)"

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
t_eq "$(t_calls gh)" "$(printf '%s\n' "issue edit 7 --note-file $T/attach.txt" "issue edit 7 --note-file $T/attach.txt")"

t_case "a call it does not name leaves every call it made answered"
rm -f "$T"/calls/*
t_stub gh
t_gh_fail 4
t_run 'gh one; gh two; gh three; echo "rc=$?"'
t_eq "$(sed -n 's/^rc=//p' <<<"$t_out")" 0
t_eq "$(t_calls gh)" "$(printf '%s\n' one two three)"

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
t_eq "$(t_calls gh)" "one"

t_case "a duplicate that reaches the stub fails the call at once"
rm -f "$T"/calls/*
t_stub gh 'echo ok
echo "the body answered"'
# The answers of a case, written by hand: a duplicate reaches the stub itself,
# which says so and ends the call with no body run and no answer of ok. The
# first status is 3, not 0: the stub reads the duplicate before its status, so
# a 0 here is answered by the status guard of the next case instead.
printf '1\t3\tfirst-word\n1\t4\tsecond-word\n' >"$T/gh-fails"
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "t_gh_fail: call 1 is named twice: 3 then 4
rc=64"
t_lacks "$t_out" "the body answered"
t_eq "$(t_calls gh)" "one"

t_case "a status the helper would refuse is refused by the stub too"
rm -f "$T"/calls/*
t_stub gh 'echo ok
echo "the body answered"'
# The same contract at the reader: a case that writes the answers itself gives
# the status the stub ends with, so a status of 0, of a leading zero, of a sign
# or past 255 is said and the call ends 64 rather than at a status no case named
# - 999 wrapped to 231 by the shell, a blank ended 2, a 0 left the "failed" call
# ended 0. The stub counts the calls of the case, so each entry names the call
# this loop is about to make.
n=0
for bad in '0' '04' '-1' '256' '999' '12345678901234567890' '1;1'; do
  n=$((n + 1))
  printf '%s\t%s\tby hand\n' "$n" "$bad" >"$T/gh-fails"
  t_run 'gh one; echo "rc=$?"'
  t_eq "$t_out" "t_gh_fail: call $n has status $bad, which is no status of 1 to 255
rc=64"
  t_lacks "$t_out" "the body answered"
done
# 255 is the last status of a hand-written record the stub answers with.
n=$((n + 1))
printf '%s\t255\tby hand\n' "$n" >"$T/gh-fails"
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "by hand
rc=255"

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
# a complaint of [ ] on standard error that moves no tally. The pattern bounds
# the digits, not their length, so the two spellings past 3 digits are refused
# by the length test that runs before any [ ] of them: one of them makes [ ]
# complain and abort, which would write the answer and fail nothing. The call
# and the status are separate entries, so each entry is two names given as two
# arguments, as the loops of the two fields below give theirs.
for bad in '01 4' '1 300' '1 256' '1 0' '1* 9' '1e2 9' '1 2*3' '256 1' \
  '12345678901234567890 4' '1 12345678901234567890'; do
  n=${bad% *}
  rc=${bad#* }
  failed_before=$t_failed
  t_gh_fail "$n" "$rc" 2>"$T/bad"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_contains "$(<"$T/bad")" "t_gh_fail: the call and its status are numbers, got '$n' '$rc'"
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

t_case "a message of a tab or a newline is refused, and writes nothing"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
# The record is three fields on one line, so a message of either would put a
# fourth field, or a whole second line, into the file: the extra line is read as
# an entry that names nothing, so the case believes it named one call where the
# file holds two records.
for bad in "$(printf 'msg\t9\tpayload')" "$(printf 'first')
$(printf 'second')"; do
  failed_before=$t_failed
  t_gh_fail 1 4 "$bad" 2>"$T/bad"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_contains "$(<"$T/bad")" "t_gh_fail: the message holds no tab or newline"
  t_eq "$t_failed" "$((failed_before + 1))"
  t_failed=$failed_before
done
t_eq "$([ -s "$T/gh-fails" ] && echo written)" ""
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "ok
rc=0"

t_case "an answers file written by hand with no last newline is read"
rm -f "$T"/calls/*
t_stub gh 'echo ok
echo "the body answered"'
# The documented format is one line per named call. A case that writes it by
# hand and leaves the newline off the last line means it: the reader reads that
# line too, rather than answer the call as an ordinary one with no word said.
printf '1\t4\thand' >"$T/gh-fails"
t_run 'gh one; echo "rc=$?"'
t_eq "$t_out" "hand
rc=4"
t_lacks "$t_out" "the body answered"

t_case "calls made side by side each get a number of their own"
rm -f "$T"/calls/*
t_stub gh 'echo ok'
# The poll makes its gh calls side by side, so the count is taken under a lock.
# Two rounds of 64 calls at once, 128 calls for the case: 64 records a round,
# none of them twice, and the count of the case ending at 128. Each round names
# the 7th call of the case it can reach - 7 of the case for the first round, 71
# of it for the second - and that one call is failed once, wherever in the round
# the scheduler put it. An unguarded count gives one number to several calls and
# ends below them, so a round would answer neither word and a case that named a
# call among concurrent reads would have failed nothing. Both names are given
# before the rounds, since t_case is what clears the answers.
t_gh_fail 7 4 "the named one"
t_gh_fail 71 5 "the second one"
for round in 1 2; do
  # The record of calls is per case, so it is emptied here for the count a round
  # makes of them.
  rm -f "$T"/calls/*
  first=$(( (round - 1) * 64 ))
  seq 1 64 | while read -r n; do printf 'c%s\n' "$(( first + n ))"; done >"$T/names"
  # The snippet is built by this shell and run by t_run's own: the file of names
  # is named here and read there.
  snippet=$(printf %s 'for w in $(cat "'"$T"'/names"); do ( gh "$w" 2>&1 >/dev/null ) & done; wait')
  t_run "$snippet"
  word="the named one"
  [ "$round" = 2 ] && word="the second one"
  t_eq "$(grep -c "$word" <<<"$t_out")" 1
  t_eq "$(grep -c 'rc=' <<<"$t_out")" 0
  t_eq "$(wc -l <"$T/calls/gh")" 64
  t_eq "$(sort -u "$T/calls/gh" | wc -l)" 64
done
t_eq "$(cat "$T/gh-n")" 128
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
t_eq "$(t_calls gh | wc -l)" 255

t_done
