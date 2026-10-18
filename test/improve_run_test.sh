#!/usr/bin/env bash
# Tests for libexec/improve: a run, what holds or refuses it, and the findings
# file the analyst leaves.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/improve_lib.sh
. "$(dirname "$0")/improve_lib.sh" || exit 1

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
${run_dir%/in}/models.json:/home/vscode/.pi/agent/models.json:ro"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '-e' | grep -v -x -e '-e' -e '--')" "HOME=/home/vscode"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cgroup-parent' | tail -n 1)" tehas.slice
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cpus' | tail -n 1)" 1
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--memory' | tail -n 1)" 2g
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--memory-swap' | tail -n 1)" 2g
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--pids-limit' | tail -n 1)" 1024
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cpu-shares' | tail -n 1)" 1024
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

t_case "the analyst's container gets no option beyond these, and its limits are its own"
# Every option up to the image that is not one this command is known to pass,
# read the way docker reads them. See test/work_lib.sh.
t_eq "$(printf '%s\n' "$args" | awk -v image="tehas/pi-rukas:uid$(id -u)" \
  -v valued=" --name --cgroup-parent --cpus --memory --memory-swap --pids-limit --cpu-shares -e -w -v " -v plain=" --rm --init " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { found = 1; exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    { print }
    END { if (!found) print "no image" }')" ""
# A batch job: a quota of one CPU and 2g whatever the reserve is, and no floor.
t_lacks "$args" "--memory-reservation"
t_eq "$(printf '%s\n' "$args" | grep -c -x -- --init)" 1
t_lacks "$args" "--cidfile"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

t_case "the issue body is the script's, with the evidence fenced and the log linked"
body=$(cat "$T/calls/body-101")
t_contains "$body" "improvement loop"
t_contains "$body" "test-model"
t_contains "$body" "**Category:** cycle"
t_contains "$body" "**Issues:** acme/widget#7"
t_contains "$body" "**Working on it:** This is a problem in the factory. It is fixed in this repository: a cycle builds the fix once a person applies \`tehas:ready\`."
t_contains "$body" "The gate ran twice."
t_contains "$body" '````text'
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_eq "$(cat "$T/calls/assets")" "ticket-$REC.log.gz"

# refused: 78, no GitHub call, no container, and the record still in the window.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(t_calls gh)$(t_calls docker | grep -v '^info ')" ""
  [ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"
  t_eq "$(find "$base" -mindepth 1 -maxdepth 1 ! -name window | wc -l)" 0
}

# held: the run waits on the limit. 0, no GitHub call, no container, and the
# record still in the window.
held() {
  t_eq "$t_rc" 0
  t_eq "$(t_calls gh)$(t_calls docker)" ""
  [ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"
  t_contains "$t_out" "held by the limit"
}

t_case "a limit with planning off holds a run, and the records wait"
ready
mkdir -p "$T/home/tehas/state"
echo '{"v":1,"cycles":3,"plan":false,"until":null,"reason":"quiet"}' >"$T/home/tehas/state/limit.json"
t_run "$improve"
held
t_contains "$t_out" "planning off"

t_case "an expired limit holds a run, and one that cannot be read does too"
ready
mkdir -p "$T/home/tehas/state"
echo '{"v":1,"cycles":3,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}' >"$T/home/tehas/state/limit.json"
t_run "$improve"
held
t_contains "$t_out" "expired"
ready
mkdir -p "$T/home/tehas/state"
echo 'not a limit' >"$T/home/tehas/state/limit.json"
t_run "$improve"
held

t_case "a limit with planning on lets a run go"
ready
mkdir -p "$T/home/tehas/state"
echo '{"v":1,"cycles":0,"plan":true,"until":null,"reason":""}' >"$T/home/tehas/state/limit.json"
t_run "$improve"
t_eq "$t_rc" 0
t_lacks "$t_out" "held by the limit"
t_contains "$(t_calls docker)" "run"

t_case "refuses with 78 without the slice, before any docker run"
ready
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
refused
t_contains "$t_out" "tehas improve: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  ready
  echo "$answer" >"$T/docker.info"
  t_run "$improve"
  refused
  t_contains "$t_out" "tehas improve: "
  t_contains "$t_out" "ocker"
done

t_case "a slice with no limit is refused"
ready
printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
refused
t_contains "$t_out" "tehas improve: tehas.slice has no memory limit"

t_case "with nothing to read the slice is not asked about"
reset
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

t_case "GitHub does not give the stack: nothing runs and the window is as it was"
ready
: >"$fx/gh-list.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "ten open improvement issues: the run waits and the records stay"
ready
for i in $(seq 1 10); do stack "$i" "Finding $i" OPEN tehas:improvement; done | jq -s . >"$fx/open.json"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "waiting"
t_eq "$(t_calls docker | grep -v '^info ')" ""
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

t_case "what a failed analyst printed is kept under the run's stamp, and the line says where"
kept=$(find "$home/logs" -name 'improve-analyst-*.log')
[[ $kept =~ /improve-analyst-[0-9]{6}-[0-9]{6}\.log$ ]] || t_bad "kept as '$kept'"
t_contains "$t_out" "the analyst ended with 3, its output is in $kept"
t_contains "$(cat "$kept")" "No API key found for test-provider."

t_case "a later run, failed or not, leaves the output of a failed one where it is"
ready
echo "from a run that failed" >"$home/logs/improve-analyst-261010-000000.log"
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(cat "$home/logs/improve-analyst-261010-000000.log")" "from a run that failed"
t_eq "$(find "$home/logs" -name 'improve-analyst-*.log' | wc -l)" 2
rm "$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(find "$home/logs" -name 'improve-analyst-*.log' | wc -l)" 2

t_case "a run that works keeps no analyst output of its own"
ready
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(find "$home/logs" -name 'improve-analyst*' | wc -l)" 0

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

t_done
