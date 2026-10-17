#!/usr/bin/env bash
# Tests for libexec/improve: the body and the log of an issue, the window of
# records, how many a run takes, its container, the sweep and the lock.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/improve_lib.sh
. "$(dirname "$0")/improve_lib.sh"

t_case "a mention does not reach the body and a comment reaches it as text"
ready
findings "$(finding Mention '{"body":"Ask @alice. <!-- hidden --> And ```fence```.","evidence":"a ``` b"}')"
t_run "$improve"
body=$(cat "$T/calls/body-101")
t_lacks "$body" "@alice"
t_lacks "$body" "<!--"
t_contains "$body" "&lt;!-- hidden"
t_lacks "$body" 'a ``` b'

t_case "a log with the shape of a token is not uploaded and the issue says so"
ready
echo "ghp_$(printf 'a%.0s' $(seq 1 36))" >>"$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/assets" 2>/dev/null)" ""
t_contains "$(cat "$T/calls/body-101")" "shaped like a token"
t_contains "$t_out" "secret-shape=$REC"

t_case "a log that is gone: the issue is filed and says so"
ready
rm "$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$(cat "$T/calls/body-101")" "no longer on the host"

t_case "no permission to make a release: the issue is filed without the log"
ready
: >"$fx/gh-release.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$(cat "$T/calls/body-101")" "could not be uploaded"

t_case "a cycle named by two findings is uploaded once"
ready
findings "$(finding One)" "$(finding Two)"
t_run "$improve"
t_eq "$(wc -l <"$T/calls/assets")" 1

t_case "GitHub refuses the second issue: exit 1 and the records are kept"
ready
findings "$(finding One)" "$(finding Two)"
echo 102 >"$fx/gh-create.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" "One"
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "a run directory left by a killed run gives its records back"
reset
findings "$(finding "The gate runs twice in one cycle")"
mkdir -p "$base/run-261010-000000/in/cycles/$REC" "$base/run-261010-000000/out"
echo '{"issue":7}' >"$base/run-261010-000000/in/cycles/$REC/cycle.json"
echo '{"type":"agent_end"}' >"$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=1"
t_eq "$(titles)" "The gate runs twice in one cycle"
t_eq "$(find "$base" -maxdepth 1 -name 'run-*' | wc -l)" 0

t_case "a directory in the window that is not a record is removed, not read"
ready
mkdir -p "$base/window/..odd name"
t_run "$improve"
t_contains "$t_out" "cycles=1"

t_case "a cycle that ends while the stack is fetched is read in this run, not lost"
ready
echo 8-261010-200150 >"$fx/late"
echo '{"type":"agent_end"}' >"$home/logs/work-8-261010-200150.jsonl"
findings "$(finding "The late cycle parked" '{"cycles":["8-261010-200150"]}')"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=2 findings=1 invalid=0"
t_eq "$(titles)" "The late cycle parked"

t_case "the window is never renamed: a run moves its records out of it"
ready
ino=$(stat -c %i "$base/window")
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(stat -c %i "$base/window")" "$ino"

# --- How many records a run takes -------------------------------------------
cap() { cat "$home/state/improve.cap" 2>/dev/null || echo none; }
many() { # <record>...: a window with these records and no findings
  reset
  for r in "$@"; do record "$r"; done
  echo '{"findings": [], "followups": []}' >"$fx/findings.json"
}
taken() { t_calls cycles | tr '\n' ' '; }
left() { find "$base/window" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' '; }

t_case "a run takes the five oldest records, by time and not by issue, and leaves the rest"
many 9-261001-000000 1-261003-000000 2-261002-120000 30-261002-000000 4-261004-000000 5-261005-000000 6-261006-000000
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(taken)" "1-261003-000000 2-261002-120000 30-261002-000000 4-261004-000000 9-261001-000000 "
t_eq "$(left)" "5-261005-000000 6-261006-000000 "
t_contains "$t_out" "cycles=5 "
t_contains "$t_out" "left=2"

t_case "an analyst stopped at its bound halves what the next run takes"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(cap)" 2
t_contains "$t_out" "the next run takes at most 2"
t_eq "$(left)" "1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000 "
t_run "$improve"
t_eq "$(taken)" "1-261001-000000 2-261002-000000 "
t_eq "$(cap)" 1
t_run "$improve"
t_eq "$(cap)" 1

t_case "a client that needed the kill counts as stopped at the bound"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000
echo 137 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 2

t_case "half of what it took, not of the cap: three records stopped at the bound leave one"
many 1-261001-000000 2-261002-000000 3-261003-000000
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 1

t_case "the analyst is told its time and to write its file early"
ready
t_run "$improve"
p=$(sed -n '/^--model$/,$p' "$T/calls/docker-args" | sed 1,2d)
t_contains "$p" "You have 60 minutes"
t_contains "$p" "Write it early"
t_lacks "$p" "@MINUTES@"

t_case "stopped at its bound with a good file: what it holds is filed, and only the records it names are done"
many 1-261001-000000 2-261002-000000 "$REC"
findings "$(finding "The gate runs twice in one cycle")"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(titles)" "The gate runs twice in one cycle"
t_eq "$(left)" "1-261001-000000 2-261002-000000 "
t_contains "$t_out" "cycles=3 findings=1"
t_contains "$t_out" "stopped=124 returned=2"
t_eq "$(cap)" 1
kept=$(find "$home/logs" -name 'improve-analyst-*.log')
[ -n "$kept" ] || t_bad "the output of a stopped analyst was not kept"
t_contains "$t_out" "output=$kept"

t_case "a record named only by a followup is done too"
many 1-261001-000000 "$REC"
jq -n -c --arg rec "$REC" '{findings: [{title: 1, cycles: ["1-261001-000000", 7]}],
  followups: [{cycle: $rec}, "x"]}' >"$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(left)" ""
t_contains "$t_out" "invalid=1"
t_contains "$t_out" "returned=0"

t_case "stopped at its bound with nothing found yet: every record goes back"
many 1-261001-000000 "$REC"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(left)" "1-261001-000000 $REC "
t_contains "$t_out" "returned=2"

t_case "stopped at its bound with a file of the wrong shape: exit 1 and every record goes back"
many 1-261001-000000 "$REC"
echo '{"findings": [' >"$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "at its bound of 60 minutes; the next run takes at most 1; findings.json is not the shape asked for"
t_eq "$(left)" "1-261001-000000 $REC "
t_eq "$(titles)" ""

t_case "stopped at its bound with no file: exit 1 and every record goes back"
many 1-261001-000000 "$REC"
rm "$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "the next run takes at most 1; the analyst left no findings.json"
t_eq "$(left)" "1-261001-000000 $REC "

t_case "an analyst that fails otherwise is not read, whatever file it left"
many "$REC"
findings "$(finding "The gate runs twice in one cycle")"
echo 1 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""
t_eq "$(left)" "$REC "

t_case "a run that finishes in time doubles what the next takes, up to five"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000 7-261007-000000
mkdir -p "$home/state" && echo 1 >"$home/state/improve.cap"
t_run "$improve"
t_eq "$(taken)" "1-261001-000000 "
t_eq "$(cap)" 2
t_run "$improve"
t_eq "$(taken)" "2-261002-000000 3-261003-000000 "
t_eq "$(cap)" 4
t_run "$improve"
t_eq "$(cap)" 5

t_case "an analyst that fails for another reason leaves the cap as it was"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000
mkdir -p "$home/state" && echo 4 >"$home/state/improve.cap"
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 4

t_case "a cap that is not a number from 1 to 5 is taken as 5"
for bad in 0 6 x "" "3; rm -rf /" 12; do
  many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000
  mkdir -p "$home/state" && printf '%s\n' "$bad" >"$home/state/improve.cap"
  t_run "$improve"
  t_eq "$(t_calls cycles | wc -l)" 5
done

t_case "a container left by a killed run is removed before the analyst starts"
ready
t_run "$improve"
t_eq "$(t_calls docker | grep -v '^info ' | head -n 1)" "rm -f tehas-improve"
t_contains "$(t_calls timeout | grep -v ' systemctl \| docker info ' | head -n 1)" "-k 60 3600 docker run"

t_case "an analyst stopped at its bound has its container removed at once"
ready
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -E '^(run|rm) ' | cut -c1-6 | tr '\n' ' ')" "rm -f  run -- rm -f  "

t_case "an analyst that finished leaves no container to remove"
ready
t_run "$improve"
t_eq "$(t_calls docker | grep -c '^rm -f tehas-improve$')" 1

t_case "the sweep removes what is over seven days old and nothing newer"
reset
record 3-261001-000000
touch -d '8 days ago' "$base/window/3-261001-000000" "$home/logs/work-3-261001-000000.jsonl"
for f in work-3-261001-000000.jsonl.err dispatch-work-3.log index-261001-000000.log dispatch.log improve.log \
  improve-analyst-261001-000000.log improve-analyst.log; do
  touch -d '8 days ago' "$home/logs/$f"
done
touch -d '6 days ago' "$home/logs/work-4-261004-000000.jsonl" "$home/logs/improve-analyst-261004-000000.log"
mkdir -p "$base/.new.old" && touch -d '8 days ago' "$base/.new.old"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=0"
t_eq "$(find "$home/logs" -type f -printf '%f\n' | sort | tr '\n' ' ')" "dispatch.log improve-analyst-261004-000000.log improve.log work-4-261004-000000.jsonl "
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_case "the analyst's container is not given the run's lock"
ready
t_run "$improve"
t_eq "$(find "$T/calls/docker-args" | wc -l)" 1
t_eq "$(find "$T/calls/docker-lock" 2>/dev/null | wc -l)" 0

t_case "a second run while one is going does nothing"
ready
mkdir -p "$home/state"
before=$t_failed
(
  flock -n 9 || exit 1
  t_run "$improve"
  t_eq "$t_rc" 0
  t_contains "$t_out" "another run"
  t_eq "$(t_calls docker)" ""
  [ "$t_failed" = "$before" ]
) 9>"$home/state/improve.lock" || t_bad "the run did not stand back"

t_done
