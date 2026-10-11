#!/usr/bin/env bash
# Tests for the improvement loop: the record a cycle leaves (lib/improve.sh),
# how a whole log is packed, and a run of libexec/improve against a stub
# docker and a stub gh.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

home=$T/home/tehas
base=$home/tmp/improve
fx=$T/fx
REC=7-261010-200140

reset() {
  rm -rf "$fx" "$home" "$T"/calls/*
  mkdir -p "$fx" "$home/logs" "$home/live"
  echo '[]' >"$fx/open.json"
  echo '[]' >"$fx/closed.json"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
}
# lib <snippet>: run it with the settings and the loop's library loaded.
lib() { t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/improve.sh"; tehas_load || exit 78; '"$1"; }

# --- The record of a cycle ---------------------------------------------------
started=$(($(date +%s) - 100))
cycle_log() { # the files a cycle of issue 7 leaves on the host
  {
    echo '{"type":"extension_ui_request","id":"a","method":"setStatus","statusText":"review"}'
    echo '{"type":"extension_ui_request","id":"b","method":"setStatus","statusText":"review"}'
    echo '{"type":"message_update","delta":"x"}'
    echo 'not json'
    jq -n -c --arg t "$(printf 'y%.0s' $(seq 1 2500))" '{type:"message_end",text:$t}'
  } >"$home/logs/work-$REC.jsonl"
  echo "a warning" >"$home/logs/work-$REC.jsonl.err"
  echo "preflight ok" >"$home/logs/work-$REC.jsonl.preflight"
  echo "console text" >"$home/live/console-7.txt"
}

t_case "a cycle's end writes a whole record"
reset
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review https://github.com/acme/widget/pull/9"
t_eq "$t_rc" 0
r=$base/window/$REC
t_eq "$(jq -c '[.issue, .status, .step, .rc, .pr, .log]' "$r/cycle.json")" \
  "[7,\"handoff\",\"review\",0,\"https://github.com/acme/widget/pull/9\",\"work-$REC.jsonl\"]"
t_eq "$(cat "$r/console.txt")" "console text"
t_eq "$(cat "$r/err.txt")" "a warning"
t_eq "$(cat "$r/preflight.txt")" "preflight ok"

t_case "the stream is condensed: no deltas, a repeated status once, long strings cut"
t_eq "$(wc -l <"$r/stream.jsonl")" 2
t_lacks "$(cat "$r/stream.jsonl")" message_update
t_eq "$(sed -n 2p "$r/stream.jsonl" | jq '.text | length')" 2006

t_case "a cycle without a pull request records null"
reset
cycle_log
lib "tehas_improve_record 7 $started 1 aborted handoff"
t_eq "$(jq -c .pr "$base/window/$REC/cycle.json")" null

t_case "with the loop off nothing is written"
reset
t_instance "${T_MINIMAL[@]}"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
[ ! -e "$base" ] || t_bad "$base was made"

t_case "no log since the cycle began: no record, and the caller is told"
reset
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 1
t_contains "$t_out" "no log of issue 7"
[ -z "$(ls -A "$base/window" 2>/dev/null)" ] || t_bad "a record was written"

t_case "an older log of the same issue is not taken for this cycle's"
reset
cycle_log
touch -d '2 hours ago' "$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 1

t_case "two cycles that end at once leave two whole records"
reset
cycle_log
cp "$home/logs/work-$REC.jsonl" "$home/logs/work-8-261010-200150.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review & tehas_improve_record 8 $started 0 handoff review & wait"
t_eq "$(find "$base/window" -name cycle.json | wc -l)" 2
t_eq "$(find "$base" -maxdepth 1 -name '.new.*' | wc -l)" 0

# --- Packing a whole log -----------------------------------------------------
# 300 lines that compress badly, so a small limit forces pieces.
big_log() { for i in $(seq 1 300); do printf '%s\n' "$i" | sha256sum; done >"$1"; }

t_case "a log under the limit is one file"
reset
big_log "$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 1000000'
t_eq "$t_rc" 0
t_eq "$(ls "$fx/gz")" "ticket-7.log.gz"
t_eq "$(zcat "$fx/gz/ticket-7.log.gz" | cksum)" "$(cksum <"$fx/log")"

t_case "a log over the limit is numbered pieces that join back to it"
reset
big_log "$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 3000'
t_eq "$t_rc" 0
n=$(find "$fx/gz" -name 'ticket-7.log.*.gz' | wc -l)
[ "$n" -ge 2 ] || t_bad "expected pieces, got $n"
[ ! -e "$fx/gz/ticket-7.log.gz" ] || t_bad "the whole file was left beside the pieces"
t_eq "$(for i in $(seq 1 "$n"); do zcat "$fx/gz/ticket-7.log.$i.gz"; done | cksum)" "$(cksum <"$fx/log")"
t_eq "$(find "$fx/gz" -name '*.gz' -size +3000c | wc -l)" 0
t_eq "$(find "$fx/gz" -mindepth 1 ! -name '*.gz' | wc -l)" 0

t_case "a line too long for one piece: no pieces are left"
reset
head -c 20000 /dev/urandom | base64 -w 0 >"$fx/log"
echo >>"$fx/log"
echo "second line" >>"$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 3000'
t_eq "$t_rc" 1
t_eq "$(find "$fx/gz" -name '*.gz' | wc -l)" 0

t_case "the shape of a token is seen"
reset
echo "push with ghp_$(printf 'a%.0s' $(seq 1 36)) failed" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0
echo "-----BEGIN OPENSSH PRIVATE KEY-----" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0
echo "{\"apiKey\":\"sk-$(printf 'b%.0s' $(seq 1 24))\"}" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0

t_case "an ordinary log is not taken for a secret"
reset
big_log "$fx/log"
{
  echo "the ghp_ prefix and sk-short are words here"
  echo "check disk-usage-of-the-container-first"
  echo '{"branch":"task-1234-add-the-login-page"}'
} >>"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 1

# --- A run -------------------------------------------------------------------
improve="$T_ROOT/libexec/improve"

# docker leaves $T/fx/findings.json where the analyst would write, and keeps
# its arguments one to a line. $T/fx/docker.rc is an exit status to fail with.
t_stub docker '
printf "%s\n" "$@" >"$T/calls/docker-args"
out=""
for a in "$@"; do case "$a" in *:/improve/out) out=${a%:/improve/out} ;; esac; done
[ -f "$T/fx/docker.rc" ] && exit "$(cat "$T/fx/docker.rc")"
if [ -L "$T/fx/findings.json" ]; then cp -P "$T/fx/findings.json" "$out/findings.json"
elif [ -f "$T/fx/findings.json" ]; then cp "$T/fx/findings.json" "$out/findings.json"; fi
exit 0'

# timeout runs the command at once and keeps what it was asked.
t_stub timeout '[ "$1" = -k ] && shift 2
shift
exec "$@"'

# gh answers from fixtures. A file $T/fx/gh-<what>.rc makes that call fail.
t_stub gh '
case "$1 $2" in
  "issue list")
    [ -f "$T/fx/gh-list.rc" ] && exit 1
    # A cycle that ends while the stack is fetched.
    if [ -f "$T/fx/late" ]; then
      mkdir -p "$T/home/tehas/tmp/improve/window/$(cat "$T/fx/late")"
      echo "{}" >"$T/home/tehas/tmp/improve/window/$(cat "$T/fx/late")/cycle.json"
    fi
    case " $* " in *" --state closed "*) cat "$T/fx/closed.json" ;; *) cat "$T/fx/open.json" ;; esac ;;
  "issue create")
    n=$(($(cat "$T/fx/next" 2>/dev/null || echo 100) + 1))
    echo "$n" >"$T/fx/next"
    [ -f "$T/fx/gh-create.rc" ] && [ "$n" -ge "$(cat "$T/fx/gh-create.rc")" ] && exit 1
    while [ $# -gt 0 ]; do
      case "$1" in
        --body-file) cp "$2" "$T/calls/body-$n" ;;
        --title) printf "%s\n" "$2" >>"$T/calls/titles" ;;
      esac
      shift
    done
    echo "https://github.com/acme/factory/issues/$n" ;;
  "release view") [ -f "$T/fx/release" ]; exit $? ;;
  "release create") [ -f "$T/fx/gh-release.rc" ] && exit 1; : >"$T/fx/release" ;;
  "release upload")
    [ -f "$T/fx/gh-release.rc" ] && exit 1
    for a in "$@"; do case "$a" in *.gz) basename "$a" >>"$T/calls/assets" ;; esac; done ;;
esac'

record() { # <record>: a record in the window and the cycle's log in logs/
  mkdir -p "$base/window/$1"
  echo '{"issue":7}' >"$base/window/$1/cycle.json"
  echo '{"type":"agent_end"}' >"$home/logs/work-$1.jsonl"
}
finding() { # <title> [<jq object to merge in>]
  local more=${2:-}
  [ -n "$more" ] || more='{}'
  jq -n -c --arg t "$1" --arg rec "$REC" --argjson more "$more" '
    {title: $t, category: "cycle", body: "The gate ran twice.", evidence: "gate: ok",
     cycles: [$rec], duplicate_of: null} + $more'
}
findings() { printf '%s\n' "$@" | jq -s '{findings: .}' >"$fx/findings.json"; }
stack() { # <number> <title> [<state>] [<label>]
  jq -n -c --argjson n "$1" --arg t "$2" --arg s "${3:-OPEN}" --arg l "${4:-}" '
    {number: $n, title: $t, state: $s, body: "", labels: (if $l == "" then [] else [{name: $l}] end)}'
}
ready() { # a window with one record and one good finding
  reset
  record "$REC"
  findings "$(finding "The gate runs twice in one cycle")"
}
titles() { cat "$T/calls/titles" 2>/dev/null || true; }

t_case "with the loop off the command says so and calls nothing"
reset
t_instance "${T_MINIMAL[@]}"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "is off"
t_eq "$(t_calls gh)$(t_calls docker)" ""
[ ! -e "$base" ] || t_bad "$base was made"

t_case "a malformed repository exits 78 before any call"
reset
record "$REC"
t_instance "${T_MINIMAL[@]}" "TEHAS_IMPROVE_REPO='not a repo'"
t_run "$improve"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls docker)" ""

t_case "an argument is refused"
reset
t_run "$improve now"
t_eq "$t_rc" 64

t_case "an empty window calls neither docker nor gh"
reset
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=0"
t_eq "$(t_calls gh)$(t_calls docker)" ""

t_case "a run files a finding and leaves nothing behind"
ready
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=1 findings=1 invalid=0 duplicates=0 of=none over=0 filed=1 issues=#101"
t_eq "$(titles)" "The gate runs twice in one cycle"
t_contains "$(t_calls gh)" "--label tehas:improvement"
t_lacks "$(t_calls gh)" "tehas:ready"
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_case "the analyst's container is given exactly this"
args=$(cat "$T/calls/docker-args")
run_dir=$(printf '%s\n' "$args" | sed -n 's|:/improve/in:ro$||p')
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '-v' | grep -v -x -e '-v' -e '--')" \
  "$run_dir:/improve/in:ro
${run_dir%/in}/out:/improve/out
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '-e' | grep -v -x -e '-e' -e '--')" "HOME=/home/vscode"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cpus' | tail -n 1)" 1
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--memory' | tail -n 1)" 2g
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--tools' | tail -n 1)" "read,grep,find,ls,write"
for flag in -p --no-session --no-extensions --no-skills --no-prompt-templates --no-context-files; do
  printf '%s\n' "$args" | grep -q -x -- "$flag" || t_bad "pi was not given $flag"
done
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--model' | tail -n 1)" test-model
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--provider' | tail -n 1)" test-provider
t_lacks "$args" docker.sock
t_lacks "$args" GH_TOKEN
t_lacks "$args" .ssh
t_lacks "$args" "$home/logs"

t_case "the issue body is the script's, with the evidence fenced and the log linked"
body=$(cat "$T/calls/body-101")
t_contains "$body" "improvement loop"
t_contains "$body" "test-model"
t_contains "$body" "**Category:** cycle"
t_contains "$body" "The gate ran twice."
t_contains "$body" '````text'
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_eq "$(cat "$T/calls/assets")" "ticket-$REC.log.gz"

t_case "GitHub does not give the stack: nothing runs and the window is as it was"
ready
: >"$fx/gh-list.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker)" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "ten open improvement issues: the run waits and the records stay"
ready
for i in $(seq 1 10); do stack "$i" "Finding $i" OPEN tehas:improvement; done | jq -s . >"$fx/open.json"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "waiting"
t_eq "$(t_calls docker)" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "eight open improvement issues leave room for two"
ready
for i in $(seq 1 8); do stack "$i" "Finding $i" OPEN tehas:improvement; done | jq -s . >"$fx/open.json"
findings "$(finding One)" "$(finding Two)" "$(finding Three)"
t_run "$improve"
t_eq "$(titles | wc -l)" 2
t_contains "$t_out" "over=1"

t_case "at most three issues a run"
ready
findings "$(finding One)" "$(finding Two)" "$(finding Three)" "$(finding Four)"
t_run "$improve"
t_eq "$(titles | wc -l)" 3
t_contains "$t_out" "filed=3"

t_case "the analyst fails: exit 1 and the records go back to the window"
ready
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "failed"
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"
t_eq "$(find "$base" -maxdepth 1 -name 'run-*' | wc -l)" 0
t_eq "$(titles)" ""

t_case "no findings file files nothing"
ready
rm "$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a findings file that is a link is not read"
ready
finding "From a file of the analyst's choosing" | jq -s '{findings: .}' >"$fx/elsewhere.json"
rm "$fx/findings.json"
ln -s "$fx/elsewhere.json" "$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a findings file over 1 MB is not read"
ready
{
  printf '{"findings":[],"pad":"'
  head -c 1100000 /dev/zero | tr '\0' a
  printf '"}'
} >"$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a file that is not the shape asked for files nothing"
ready
echo '["a list, not an object"]' >"$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "findings of the wrong shape are dropped one by one"
ready
findings "$(finding Good)" \
  "$(finding "$(printf 't%.0s' $(seq 1 101))")" \
  "$(finding "Two
lines")" \
  "$(finding BadCategory '{"category":"product"}')" \
  "$(finding LongBody "$(jq -n -c --arg b "$(printf 'b%.0s' $(seq 1 6001))" '{body:$b}')")" \
  "$(finding UnknownCycle '{"cycles":["9-000000-000000"]}')" \
  "$(finding NoCycles '{"cycles":[]}')" \
  "$(finding BadDuplicate '{"duplicate_of":999}')" \
  '"a string"'
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(titles)" "Good"
t_contains "$t_out" "findings=9 invalid=8"

t_case "a finding the model names a duplicate of is dropped"
ready
stack 4 "Pause and resume" | jq -s . >"$fx/open.json"
findings "$(finding "A pause switch on the host" '{"duplicate_of":4}')"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1 of=#4"

t_case "a title that matches an open issue is dropped, whatever the model says"
ready
stack 4 "the gate runs twice, in one cycle!" | jq -s . >"$fx/open.json"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1"

t_case "a title that matches a closed issue is filed"
ready
stack 4 "The gate runs twice in one cycle" CLOSED | jq -s . >"$fx/closed.json"
t_run "$improve"
t_eq "$(titles)" "The gate runs twice in one cycle"

t_case "the same title twice in one run is filed once"
ready
findings "$(finding "The gate runs twice")" "$(finding "the gate runs twice.")"
t_run "$improve"
t_eq "$(titles)" "The gate runs twice"

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

t_case "a container left by a killed run is removed before the analyst starts"
ready
t_run "$improve"
t_eq "$(t_calls docker | head -n 1)" "rm -f tehas-improve"
t_contains "$(t_calls timeout | head -n 1)" "-k 60 1800 docker run"

t_case "the sweep removes what is over seven days old and nothing newer"
reset
record 3-261001-000000
touch -d '8 days ago' "$base/window/3-261001-000000" "$home/logs/work-3-261001-000000.jsonl"
for f in work-3-261001-000000.jsonl.err dispatch-work-3.log index-261001-000000.log dispatch.log improve.log; do
  touch -d '8 days ago' "$home/logs/$f"
done
touch -d '6 days ago' "$home/logs/work-4-261004-000000.jsonl"
mkdir -p "$base/.new.old" && touch -d '8 days ago' "$base/.new.old"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=0"
t_eq "$(find "$home/logs" -type f -printf '%f\n' | sort | tr '\n' ' ')" "dispatch.log improve.log work-4-261004-000000.jsonl "
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

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
