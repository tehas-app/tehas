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

t_case "a malformed repository name is refused"
reset
t_instance "${T_MINIMAL[@]}" "TEHAS_IMPROVE_REPO='not a repo'"
lib "tehas_improve_valid"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_IMPROVE_REPO"

t_done
