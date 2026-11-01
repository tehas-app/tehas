#!/usr/bin/env bash
# Tests for tehas export: what a person reading a cycle is given, masked, and
# the cursor a program keeps. Which files are copied, and under what names, is
# the question here; what the mask matches is the console's own tests. The
# console is this checkout's own, named by TEHAS_CONSOLE as a checkout has it.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

export_cmd="$T_ROOT/bin/tehas export"
home=$T/home/tehas
cycles=$home/state/cycles
logs=$home/logs
out=$T/archive
t_stub gh
t_stub git

# A token of the shape the mask knows, and what the mask makes of it.
token=ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
masked='ghp_***'
t_console || t_bad "no console: $t_console_why"
console_on() { cp "$T_CONSOLE" "$T/bin/console"; }

T0=1760700000
iso() { date -u -d "@$1" +%FT%TZ; }

reset() {
  rm -rf "$T"/calls/* "$T/instance" "$home" "$out" "$T/home/.pi"
  T_ENV=(TEHAS_CONSOLE="$T/bin/console")
  t_instance "${T_MINIMAL[@]}"
  t_store_home "$home"
  # A case that takes the console away leaves it away, so every case gets it.
  console_on
}
# cycle <issue> <epoch of the end> [fields]: a cycle that ended, with the files
# a cycle leaves behind that an export is meant to carry.
cycle() {
  local n=$1 e=$2 extra=${3-}
  [ -n "$extra" ] || extra='{}'
  mkdir -p "$cycles" "$logs"
  t_history_end "$home" "$n" "$e" "$extra"
  printf '%s\n' '{"issue":12,"result":"merged"}' >"$cycles/$n.result.json"
  printf '%s\n' '{"step":"review"}' >"$cycles/$n.progress.json"
  printf 'plain text %s here\n' "$token" >"$logs/work-$n-1760.jsonl"
  printf '%s\n' 'an error line' >"$logs/work-$n-1760.jsonl.err"
  printf 'the console said %s here\n' "$token" >"$logs/work-$n-1760.jsonl.console"
  printf '%s\n' 'dispatched' >"$logs/dispatch-work-$n.log"
  printf '%s\n' 'a preflight notice' >"$logs/work-$n-1760.jsonl.preflight"
}
# turn <issue> <epoch of the end> [fields]: a planning turn that ended, its
# session on the state store, and no result file at all (t_plan_end).
turn() { t_plan_end "$home" "$@"; }
# run_export <args as words>: tehas export with these arguments. The command it
# built is left in run_line, so a case can wrap it (flock, for one).
run_export() {
  run_line=$export_cmd
  while [ "$#" -gt 0 ]; do
    run_line="$run_line '$1'"
    shift
  done
  t_run "$run_line"
}
# listing <cursor>: the files an export wrote for one cycle, paths relative to
# the cycle's own directory.
listing() {
  [ -d "$out/cycles/$1" ] || {
    echo "missing"
    return
  }
  (cd "$out/cycles/$1" && find . -type f ! -name .stamp | LC_ALL=C sort | sed 's|^\./||')
}

t_case "one cycle: its directory, its files, and the cursor a program keeps"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
t_eq "$(jq -c 'del(.cycles)' <<<"$t_out")" "{\"api\":1,\"next\":\"1760700000-12-1\",\"to\":\"$out\"}"
t_eq "$(listing 1760700000-12-1)" "history.json
logs/dispatch-work.log
logs/progress.json
logs/result.json
logs/work-12-1760.jsonl
logs/work-12-1760.jsonl.console
logs/work-12-1760.jsonl.err
logs/work-12-1760.jsonl.preflight"

t_case "a log is masked whatever bytes it holds, and one stray NUL does not escape the mask"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
printf 'a line with %s in it\n' "$token" >"$logs/work-12-1760.jsonl"
printf '\000\001\002\003\004\005\006\007\010\013\014\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037 %s\n' "$token" >"$logs/work-12-1760.jsonl.err"
printf 'a log with one stray \0 in it and %s\n' "$token" >"$logs/work-12-1760.jsonl.console"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
t_eq "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "a line with $masked in it"
t_eq "$(cmp -s <(printf '\000\001\002\003\004\005\006\007\010\013\014\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037 %s\n' "$masked") \
  "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl.err" && echo masked || echo differs)" masked
t_eq "$(tr -d '\000' <"$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl.console")" "a log with one stray  in it and $masked"
# Bytes that are not UTF-8 in a log are no way past the mask.
printf 'cut short \377\376 and %s\n' "$token" >"$logs/work-12-1760.jsonl"
run_export --json --after $((T0 - 1))-12 "$out"
t_contains "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "and $masked"
t_lacks "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "$token"

t_case "bytes the mask makes nothing of are copied as they came, and an empty file stays empty"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
printf '\001\002\003\004\005\006\007\010\013\014\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037' >"$logs/work-12-1760.jsonl"
: >"$logs/work-12-1760.jsonl.err"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$logs/work-12-1760.jsonl" "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl" &&
  echo same || echo differs)" same
t_eq "$(stat -c %s "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl.err")" 0

t_case "the cycle's own line of history is what tehas history says"
reset
cycle 12 "$T0" '{"kind":"work","rc":10,"status":"handoff","step":"review","parked":true,"pr":"https://github.com/acme/widget/pull/9"}'
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(jq -c . "$out/cycles/1760700000-12-1/history.json")" "{\"cursor\":\"1760700000-12-1\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"started\":\"$(iso $((T0 - 60)))\",\"status\":10,\"outcome\":\"handoff\",\"step\":\"review\",\"provider\":false,\"parked\":true,\"pr\":\"https://github.com/acme/widget/pull/9\",\"how\":\"hand\"}"

t_case "a planning turn brings its session, as the store has it, and no cycle files"
reset
turn 7 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice"}'
run_export --json --after $((T0 - 1))-7 "$out"
t_eq "$t_rc" 0
t_eq "$(listing 1760700000-7-1)" "history.json
plan.json"
t_eq "$(jq -r .kind "$out/cycles/1760700000-7-1/history.json")" plan
t_eq "$(jq -c '[.issue, .session, .turns, .running, .by, .label, (.ended_at | type)]' "$out/cycles/1760700000-7-1/plan.json")" '[7,"abc-123",1,false,"alice","tehas:plan","null"]'
t_eq "$(jq -r .brief "$out/cycles/1760700000-7-1/plan.json")" "# Faster search"

# A turn that made no session ends it, and the session stays on record until
# the next tehas:plan of the issue: the archive of that turn holds it.
t_case "a turn that left no session brings the session it ended"
reset
turn 7 "$T0" '{"kind":"plan","status":"not-started","turn":1,"by":"alice"}'
run_export --json --after $((T0 - 1))-7 "$out"
t_eq "$t_rc" 0
t_eq "$(listing 1760700000-7-1)" "history.json
plan.json"
t_eq "$(jq -c '[.session, .turns, .running, (.ended_at | type)]' "$out/cycles/1760700000-7-1/plan.json")" '[null,1,false,"number"]'

t_case "a turn whose session the store no longer has has no plan.json and is exported all the same"
reset
turn 7 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
# The history of an issue outlives its sessions: a later tehas:plan replaced
# this one, and the store forgets the replaced one.
printf '# Again\n' >"$T/brief-again.md"
TEHAS_HOME=$home "$T_STORE" plan-begin 7 --by alice --label tehas:plan --brief-file "$T/brief-again.md" >/dev/null
TEHAS_HOME=$home "$T_STORE" plan-begin 7 --undo >/dev/null
run_export --json --after $((T0 - 1))-7 "$out"
t_eq "$t_rc" 0
t_eq "$(listing 1760700000-7-1)" "history.json"

t_case "a turn whose session a later tehas:plan replaced has no plan.json: the new session is not its"
reset
turn 7 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice"}'
printf '# Again\n' >"$T/brief-again.md"
TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$((T0 + 600)) "$T_STORE" plan-begin 7 --by bob --label tehas:refine --brief-file "$T/brief-again.md" >/dev/null
run_export --json --after $((T0 - 1))-7 "$out"
t_eq "$t_rc" 0
t_eq "$(listing 1760700000-7-1)" "history.json"

t_case "a planning turn whose session the store cannot give is not written, and the rest are"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
turn 7 "$((T0 + 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
t_store_answers plan-show 1 'store: busy #7: the database is locked'
run_export --json --after $((T0 - 1))-1 "$out"
t_eq "$t_rc" 1
t_contains "$(t_calls store)" "plan-show 7"
t_eq "$(listing 1760700000-12-1 | head -n 1)" "history.json"
t_eq "$([ -e "$out/cycles/1760700060-7-2/plan.json" ] && echo written)" ""
t_store_home "$home"

t_case "the transcripts of the cycle are carried, and the Protolab page with its files"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
mkdir -p "$T/home/.pi/agent/ensemble-runs/issue-12" "$home/protolab/12/files"
printf '%s\n' '{"type":"assistant"}' >"$T/home/.pi/agent/ensemble-runs/issue-12/abc.jsonl"
printf '%s\n' '# the ticket' >"$home/protolab/12/index.md"
printf 'a file with %s\n' "$token" >"$home/protolab/12/files/note.md"
printf '%s\n' 'not for the export' >"$home/protolab/12/.stamp"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(listing 1760700000-12-1 | grep -e '^transcripts/' -e '^protolab/')" "protolab/files/note.md
protolab/index.md
transcripts/abc.jsonl"
t_eq "$(cat "$out/cycles/1760700000-12-1/protolab/files/note.md")" "a file with $masked"

t_case "an image of the Protolab page is copied byte for byte, and its text files are masked"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
mkdir -p "$home/protolab/12/files"
printf '\211PNG\r\n\032\n\377\376\000 %s' "$token" >"$home/protolab/12/files/shot.png"
printf '<p>%s</p>\n' "$token" >"$home/protolab/12/files/page.html"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
t_eq "$(cmp -s "$home/protolab/12/files/shot.png" "$out/cycles/1760700000-12-1/protolab/files/shot.png" &&
  echo same || echo differs)" same
t_eq "$(cat "$out/cycles/1760700000-12-1/protolab/files/page.html")" "<p>$masked</p>"
t_eq "$(stat -c %a "$out/cycles/1760700000-12-1/protolab" "$out/cycles/1760700000-12-1/protolab/files" | uniq)" 755

t_case "a cycle with no logs, no transcripts and no page is exported all the same"
reset
t_history_end "$home" 12 "$T0" '{"rc":0,"status":"merged"}'
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
t_eq "$(listing 1760700000-12-1)" "history.json"

t_case "nothing to copy: exit 0, the object, and no directory of a cycle"
reset
run_export --json --after $((T0 - 1))-1 "$out"
t_eq "$t_rc" 0
t_eq "$(jq -c . <<<"$t_out")" "{\"api\":1,\"cycles\":[],\"next\":\"$((T0 - 1))-1\",\"to\":\"$out\"}"

t_case "without --json, one line for each cycle and one for the directory"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
turn 7 "$((T0 + 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
run_export --after $((T0 - 1))-1 "$out"
t_eq "$t_rc" 0
t_eq "$t_out" "1760700000-12-1  work  #12
1760700060-7-2  plan  #7
exported  2 cycles to $out (next: 1760700060-7-2)"

t_case "an old copy of a cycle is replaced whole"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(jq -r .status "$out/cycles/1760700000-12-1/history.json")" 0
# The same cycle, as it is on record now.
t_store_db "$home"
cycle 12 "$T0" '{"kind":"work","rc":10,"status":"handoff"}'
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(jq -r .status "$out/cycles/1760700000-12-1/history.json")" 10

t_case "a file over 32 MiB is left out, so an export is for reading"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
printf 'x' >"$logs/work-12-1760.jsonl"
truncate -s 33554433 "$logs/work-12-1760.jsonl"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(listing 1760700000-12-1 | grep -c 'work-12-1760.jsonl$')" 0
t_contains "$(listing 1760700000-12-1)" "logs/result.json"

t_case "a file that cannot be read fails its cycle, and the rest of it and the other cycles are copied"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
cycle 13 "$((T0 + 60))" '{"kind":"work","rc":0,"status":"merged"}'
chmod 000 "$logs/work-12-1760.jsonl.err"
run_export --json --after $((T0 - 1))-1 "$out"
chmod 644 "$logs/work-12-1760.jsonl.err"
t_eq "$t_rc" 1
t_eq "$(grep 'could not write' <<<"$t_out")" "tehas export: could not write 1760700000-12-1"
t_eq "$(listing 1760700000-12-1 | grep -c 'jsonl.err$')" 0
t_contains "$(listing 1760700000-12-1)" "logs/work-12-1760.jsonl.console"
t_contains "$(listing 1760700060-13-2)" "logs/work-13-1760.jsonl.err"

t_case "a link is not followed, and only this issue's logs are taken"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
printf '%s\n' 'secret elsewhere' >"$T/outside.jsonl"
ln -s "$T/outside.jsonl" "$logs/work-12-9999.jsonl"
printf '%s\n' "another issue's" >"$logs/work-13-1760.jsonl"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$(listing 1760700000-12-1 | grep -c 9999)" 0
t_eq "$(listing 1760700000-12-1 | grep -c 13)" 0
t_lacks "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "secret elsewhere"

t_case "the directory is made when it is not there, and is readable whatever the umask"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
t_run "umask 077; $export_cmd --json --after $((T0 - 1))-12 '$out/nested/deeper'"
t_eq "$t_rc" 0
t_eq "$(jq -r .to <<<"$t_out")" "$out/nested/deeper"
# The directory export makes, and every level below it: the top of the path a
# caller names belongs to the caller and keeps the caller's own mode.
t_eq "$(stat -c %a "$out/nested/deeper" "$out/nested/deeper/cycles" \
  "$out/nested/deeper/cycles/1760700000-12-1" "$out/nested/deeper/cycles/1760700000-12-1/logs" | uniq)" 755
t_eq "$(stat -c %a "$out/nested/deeper/cycles/1760700000-12-1/history.json")" 644
t_eq "$(stat -c %a "$out/nested/deeper/cycles/1760700000-12-1/logs/result.json")" 644

t_case "an argument it does not know is exit 64 and copies nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
for args in "" "--after $((T0 - 1))-12" "--after $((T0 - 1))-12 $out extra" \
  "--after $((T0 - 1))-12 --after $((T0 - 1))-12 $out" "--after 12 $out" \
  "--after $((T0 - 1))-0 $out" "--after x $out" "$out" "--json $out" \
  "--after $((T0 - 1))-12 ." "--after $((T0 - 1))-12 .." "--after $((T0 - 1))-12 $out/." \
  "--after $((T0 - 1))-12 $home" "--after $((T0 - 1))-12 $home/backup"; do
  t_run "$export_cmd $args"
  t_eq "$t_rc" 64
done
t_eq "$(find "$out" -type f 2>/dev/null | wc -l)" 0

t_case "without settings it is exit 78 and copies nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
rm -rf "$T/instance"
run_export --json --after $((T0 - 1))-12 "$out" 2>/dev/null
t_eq "$t_rc" 78
t_eq "$(find "$out" -type f 2>/dev/null | wc -l)" 0

t_case "without the state store it is exit 78 and copies nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
rm -f "$home/libexec/store"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas export: ERROR: the state store is not installed (run install.sh)"
t_eq "$(find "$out" -type f 2>/dev/null | wc -l)" 0

t_case "a busy store is exit 1 and copies nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
t_store_answers history-show 1 'store: busy: the database is locked'
run_export --json --after $((T0 - 1))-12 "$out" 2>/dev/null
t_eq "$t_rc" 1
t_eq "$(find "$out" -type f 2>/dev/null | wc -l)" 0

t_case "no console means nothing is exported, because nothing could be masked"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
rm -f "$T/bin/console"
run_export --json --after $((T0 - 1))-12 "$out" 2>/dev/null
t_eq "$t_rc" 1
t_eq "$(find "$out" -type f 2>/dev/null | wc -l)" 0

t_case "export changes nothing in the factory"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
turn 7 "$((T0 + 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
mkdir -p "$T/home/.pi/agent/ensemble-runs/issue-12"
printf '%s\n' '{"type":"assistant"}' >"$T/home/.pi/agent/ensemble-runs/issue-12/abc.jsonl"
# The store's own files of its log (-wal, -shm) are SQLite's, not a write.
files() { (cd "$home" && find . ! -name 'tehas.db-wal' ! -name 'tehas.db-shm' | LC_ALL=C sort); }
before=$(files)
before_runs=$(cd "$T/home/.pi" && find . | LC_ALL=C sort)
run_export --json --after $((T0 - 1))-1 "$out"
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 2
t_eq "$(files)" "$before"
t_eq "$(cd "$T/home/.pi" && find . | LC_ALL=C sort)" "$before_runs"

t_case "reading takes no lock a running cycle holds"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
: >"$cycles/12.lock"
run_export --json --after $((T0 - 1))-12 "$out"
t_run "flock '$cycles/12.lock' $run_line"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 1

t_case "every file is masked with the host's names: aliases, each role's endpoints, the email"
reset
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=gpu-one:192.0.2.5,solo-host \
  PI_ENSEMBLE_SUBAGENT_PROVIDER=alpha PI_ENSEMBLE_SUBAGENT_MODEL=m1 \
  PI_ENSEMBLE_PROVIDER_OPS=beta PI_ENSEMBLE_MODEL_OPS=m2
mkdir -p "$T/home/.pi/agent"
printf '%s\n' '{"providers":{"alpha":{"baseUrl":"https://model-main.lan:8086/v1","backupBaseUrl":"http://model-backup.lan/v1"},"beta":{"baseUrl":"http://role-only.lan/v1"},"unused":{"baseUrl":"http://not-used.lan/v1"}}}' \
  >"$T/home/.pi/agent/models.json"
t_stub git 'echo bot@mail.example'
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
names='gpu-one SOLO-HOST model-main.lan Model-Backup.lan role-only.lan bot@mail.example'
printf '%s\n' "$names" >"$logs/work-12-1760.jsonl"
mkdir -p "$home/protolab/12"
printf '%s\n' "$names" >"$home/protolab/12/index.md"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0
six='[private name] [private name] [private name] [private name] [private name] [private name]'
t_eq "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "$six"
t_eq "$(cat "$out/cycles/1760700000-12-1/protolab/index.md")" "$six"
t_contains "$(t_calls git)" "config --global user.email"
printf 'http://not-used.lan/v1\n' >"$logs/work-12-1760.jsonl"
run_export --json --after $((T0 - 1))-12 "$out"
t_contains "$(cat "$out/cycles/1760700000-12-1/logs/work-12-1760.jsonl")" "not-used.lan"
t_stub git

t_case "a file of models that is there and cannot be read is exit 78 and makes nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
mkdir -p "$T/home/.pi/agent"
printf '{"providers":{"alpha":{"baseUrl":"http://model-main.lan/v1"}}}\ntrailing\n' \
  >"$T/home/.pi/agent/models.json"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json"
[ ! -e "$out" ] || t_bad "the export made $out"

t_case "a host with no file of models still exports"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
rm -f "$T/home/.pi/agent/models.json"
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 0

t_case "a repository that is not valid is exit 78 and makes nothing"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
t_instance "${T_MINIMAL[@]}" TEHAS_REPO=not-a-repo
run_export --json --after $((T0 - 1))-12 "$out"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not valid"
t_eq "$([ -e "$out" ] && echo made)" ""

t_case "a link into the home is refused, and makes no directory there"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
ln -s "$home" "$T/to-home"
run_export --after $((T0 - 1))-12 "$T/to-home"
t_eq "$t_rc" 64
run_export --after $((T0 - 1))-12 "$T/to-home/backup/deeper"
t_eq "$t_rc" 64
t_contains "$t_out" "is in the factory's home ($home)"
t_eq "$([ -e "$home/backup" ] && echo made)" ""
t_eq "$([ -e "$home/cycles" ] && echo made)" ""
rm -f "$T/to-home"

t_case "a directory named in bytes that are not text is a bad argument, and a wide cursor one too"
reset
cycle 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
t_run "$export_cmd --after 1-1 \$'$out\\xff'"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas export"
run_export --after 12345678901-1 "$out"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas export"
t_eq "$([ -e "$out" ] && echo made)" ""

t_done
