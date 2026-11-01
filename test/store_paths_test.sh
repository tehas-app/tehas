#!/usr/bin/env bash
# The records on the state store are read and written through lib/store.sh,
# not as files. This test fails when code that is installed or run names the
# old file of a record that moved. The files that stay files, the carriers,
# are signals between two processes, a container's mount or its output: each
# is named only by the code that passes it on. Tests, the manual in host/docs/
# and the records in docs/ may name both.
#
# What stays a file, and why (the plan's decision 4; the inventory of
# docs/specs/2026-10-21-state-store.md sections 1.3 and 1.4):
# - a cycle's .lock and a planning turn's .json.lock: liveness is the flock;
# - work's .runner_exit .status .oom .forks .stalled .timed .nostart: how the
#   container ended, from work to the cycle;
# - .result.json and .progress.json: the container's output, kept to the next
#   start for tehas export;
# - the dispatcher's .done and stop's .stopped: a signal to a running cycle;
# - .cid .manifest.json .images.md .retry-context.md: what a container is given;
# - planning's .out and .rc, and the .keep directory beside the workspace;
# - state/refined.jsonl, the ledger the store does not hold (D9);
# - state/dispatch.*: the poll's own lock and notes.
# Every other record moved. install.sh imports the old files (store import),
# and the poll stops at 78 while one is left (test/dispatch_guard_test.sh).
# install.sh may name the old files, on the lines `allow` lists. The export copy
# an ended session left, state/plan-<n>.turn, is neither imported nor guarded:
# nothing reads it.
# shellcheck disable=SC2016  # the patterns and samples are code, not expanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

code() { # every tracked file that is installed or run and could name a record
  git -C "$T_ROOT" ls-files -- bin libexec lib install.sh console/analytics/src console/src/export.rs console/src/stop.rs
}

# The old files of the records that moved, as extended regular expressions,
# each with a line of code it is there to find. Each group that moves onto the
# store adds its own.
forbidden=()
forbidden_sample=()
# forbid <pattern> <sample>
forbid() {
  forbidden+=("$1")
  forbidden_sample+=("$2")
}

# allow <file> <line pattern> <why>: lines of code that name an old file
# because they move it away or stop on it.
a_file=()
a_re=()
allow() {
  a_file+=("$1")
  a_re+=("$2")
}
allow install.sh 'plan-(<n>|\[0-9\]\*)\.json\.keep|%\.json\.keep' \
  "install.sh moves a planning session's kept directory out of state/"

# carrier <pattern> <sample> <files that may name it> <why it stays a file>
c_re=()
c_sample=()
c_files=()
c_why=()
carrier() {
  c_re+=("$1")
  c_sample+=("$2")
  c_files+=("$3")
  c_why+=("$4")
}
# The name of a cycle's file after its issue, as the shell spells it.
n='\$\{?(issue|n|1)(:\?)?\}?'
# The analytics program still reads some of these files itself.
an=console/analytics/src
# A cycle's file as the code names it: after its issue, after <issue> in a
# comment, or in a glob. Not after "plan-": those are the planning session's.
f='(^|[^-])(\$\{?(issue|n|1|owner)(:\?)?\}?|<issue>|<n>|\*)'
# A planning session's file as the code names it: plan- and its issue, as a
# variable, <issue>, <n> or a glob.
p='plan-(\$\{?(issue|n|1|plan_n)(:\?)?\}?|<issue>|<n>|\*)'

# The cycle's records: its start, the approval it used, its image and
# settings, its branch and its history.
forbid "$f"'\.taken\b' ': >"$cycles/$issue.taken"'
forbid "$f"'\.by\b' 'printf x >"$TEHAS_HOME/state/cycles/$n.by"'
forbid "$f"'\.requeued\b' 'rm -f "${cycles:?}/${issue:?}.requeued"'
forbid "$f"'\.rukas\b' 'printf x >"$cycles/$issue.rukas"'
forbid "$f"'\.image\b' 'image_file=${cycles:?}/${issue:?}.image'
forbid "$f"'\.settings\b' 'settings_file=$TEHAS_HOME/state/cycles/$issue.settings'
forbid "$f"'\.branch\b' 'read -r b <"${cycles:?}/${issue:?}.branch"'
forbid "$f"'\.approved\.json\b' 'grep -l x "$TEHAS_HOME"/state/cycles/*.approved.json'
forbid "$f"'\.history\.jsonl\b' '# state/cycles/<issue>.history.jsonl, one line per end'
forbid 'tehas_cycle_history_add\b' 'tehas_cycle_history_add "$issue" "$line"'
# The park, its question on record and the body the question was posted from.
forbid "$f"'\.park\b' 'pf=$TEHAS_HOME/state/cycles/$1.park'
forbid 'state/park-' 'printf x >"$TEHAS_HOME/state/park-$issue.json"'
forbid '\bpark_id\b' 'qid=$(cat "$(park_id "$n")")'
# The quiet-cycle episode: what the poll last asked of a stuck cycle's unit.
forbid "$f"'\.quiet\b' 'note=$TEHAS_HOME/state/cycles/$n.quiet'
forbid 'tehas_cycle_note_files\b' 'for f in $(tehas_cycle_note_files quiet); do'
# The planning session: its record, read or written whole, its brief, its
# answer, the copy of it a turn left, the directory it kept in state/, its
# history and the helpers that read and wrote them. The files a turn's unit
# passes on keep their names after the record's (.out, .rc, .lock).
forbid "$p"'\.json($|[^.a-z])' 'for st in "$TEHAS_HOME"/state/plan-*.json; do'
forbid '"\$\{?st\}?(\.new)?"' 'jq -c '"'"'.pending = false'"'"' "$st" >"$st.new"'
forbid '\$\{?st\}?\.(brief|answer|turn|keep)\b|\.json\.(brief|answer|keep)\b' 'printf x >"$st.answer.new"'
forbid "$p"'\.turn\b' 'copy_in "$TEHAS_HOME/state/plan-$issue.turn" "$out" plan.json'
forbid "$p"'\.history\.jsonl\b' 'files=("$TEHAS_HOME/state/cycles/plan-$issue.history.jsonl")'
forbid '\bplan_record_number\b' 'plan_record_number "$st" || continue'
forbid '\btehas_plan_history_add\b' 'tehas_plan_history_add "$n" "$line"'
carrier '\.runner_exit\b' 'runner_exit_file=$TEHAS_HOME/state/cycles/$issue.runner_exit' \
  'lib/cycles.sh libexec/cycle console/src/stop.rs libexec/work' \
  "work tells the cycle how the runner ended"
carrier "$n"'\.status\b' 'status_file=$TEHAS_HOME/state/cycles/$issue.status' \
  'libexec/cycle' \
  "work tells the cycle the runner's status word"
carrier '\.oom\b' 'oom_file=${cycles:?}/${issue:?}.oom' \
  "lib/cycles.sh libexec/cycle console/src/stop.rs libexec/work $an/main.rs $an/run.rs $an/tests.rs" \
  "work counts the container's kills for memory for the cycle"
carrier '\.forks\b' 'forks_file=$TEHAS_HOME/state/cycles/$issue.forks' \
  "libexec/cycle console/src/stop.rs libexec/work $an/main.rs $an/run.rs" \
  "work counts the container's refused forks for the cycle"
carrier '\.stalled\b' 'stall_file=${cycles:?}/${issue:?}.stalled' \
  'libexec/cycle console/src/stop.rs libexec/work' \
  "work tells the cycle the container stalled"
carrier '\.timed\b' 'timed_file=${cycles:?}/${issue:?}.timed' \
  'libexec/cycle console/src/stop.rs libexec/work' \
  "work tells the cycle the container ran out of time"
carrier '\.nostart\b' 'nostart_file=$TEHAS_HOME/state/cycles/$issue.nostart' \
  'lib/cycles.sh libexec/cycle console/src/stop.rs libexec/work' \
  "work tells the cycle the runner never started"
carrier '\.result\.json\b' 'result=$TEHAS_HOME/state/cycles/$issue.result.json' \
  "libexec/cycle libexec/export console/src/export.rs console/src/stop.rs libexec/work $an/run.rs $an/sessions.rs $an/tests.rs" \
  "the container's result, kept to the next start for tehas export"
carrier '\.progress\.json\b' 'progress=$TEHAS_HOME/state/cycles/$issue.progress.json' \
  "lib/cycles.sh libexec/cycle libexec/export console/src/export.rs console/src/stop.rs libexec/work $an/run.rs $an/tests.rs" \
  "the summary of the container's output, kept to the next start for tehas export"
carrier "$n"'\.done\b' 'echo "$done_why" >"$TEHAS_HOME/state/cycles/$n.done"' \
  'libexec/cycle' \
  "the dispatcher tells a running cycle why it is done"
carrier "$n"'\.stopped\b' 'stopped_marker=$state/$n.stopped' \
  'libexec/cycle console/src/stop.rs' \
  "tehas stop tells a running cycle it was stopped"
carrier '\.cid\b' 'cid_file=$TEHAS_HOME/state/cycles/$issue.cid' \
  'console/src/dev.rs console/src/stop.rs libexec/work' \
  "the container's id, for docker"
carrier '\.manifest\.json\b' 'kept=$TEHAS_HOME/state/cycles/$issue.manifest.json' \
  'console/src/export.rs console/src/stop.rs libexec/work' \
  "the images a container is given, mounted into it"
carrier '\.images\.md\b' 'cp "$images/x" "$TEHAS_HOME/state/cycles/7.images.md"' \
  'libexec/work' \
  "the list of images a container is given, mounted into it"
carrier '\.retry-context\.md\b' 'cp "$images/x" "$TEHAS_HOME/state/cycles/7.retry-context.md"' \
  'libexec/work' \
  "what a reviewer said before a retry, mounted into the container"
carrier '\$\{?st\}?\.rc\b|\.json\.rc\b' 'rc=$(cat "$st.rc" 2>/dev/null)' \
  'console/src/health.rs' \
  "a planning turn's exit, from the turn to the poll that collects it"
carrier '\$\{?st\}?\.lock\b|\.json\.lock\b' '[ -f "$st.lock" ]' \
  'install.sh console/src/health.rs' \
  "the lock a planning turn holds while it runs; install.sh moves no .keep while it is held"
carrier '\$\{?st\}?\.out\b|\.json\.out\b' 'what=$(cat "$st.out")' \
  "$an/plan.rs $an/tests.rs" \
  "a planning container's output"
carrier 'refined\.jsonl\b' 'ledger=$TEHAS_HOME/state/refined.jsonl' \
  "$an/main.rs $an/report.rs $an/sessions.rs $an/tests.rs" \
  "the ledger of refined issues, which the store does not hold"

t_case "no code names the old file of a record on the store"
found=""
while IFS= read -r f; do
  for re in "${forbidden[@]}"; do
    hits=$(grep -nE -- "$re" "$T_ROOT/$f" 2>/dev/null) || continue
    for i in "${!a_file[@]}"; do
      [ "${a_file[i]}" = "$f" ] || continue
      hits=$(grep -vE -- "^[0-9]+:.*(${a_re[i]})" <<<"$hits") || continue 2
    done
    found+="$f: $hits"$'\n'
  done
done < <(code)
t_eq "$found" ""

t_case "a carrier is named only by the code that passes it on"
for i in "${!c_re[@]}"; do
  while IFS= read -r f; do
    grep -qE -- "${c_re[i]}" "$T_ROOT/$f" 2>/dev/null || continue
    case " ${c_files[i]} " in
      *" $f "*) ;;
      *) t_bad "$f names ${c_re[i]}, which only ${c_files[i]} may: ${c_why[i]}" ;;
    esac
  done < <(code)
done

t_case "each carrier is still named where the list says"
for i in "${!c_re[@]}"; do
  for f in ${c_files[i]}; do
    grep -qE -- "${c_re[i]}" "$T_ROOT/$f" 2>/dev/null || t_bad "$f does not name ${c_re[i]}: take it off the list"
  done
done

t_case "each pattern finds what it is for"
for i in "${!forbidden[@]}"; do
  grep -qE -- "${forbidden[i]}" <<<"${forbidden_sample[i]}" || t_bad "the pattern ${forbidden[i]} finds nothing in its sample"
done
for i in "${!c_re[@]}"; do
  grep -qE -- "${c_re[i]}" <<<"${c_sample[i]}" || t_bad "the pattern ${c_re[i]} finds nothing in its sample"
done

t_case "no forbidden pattern takes a JSON field, a label or a kept name for a file"
for line in 'jq -r .by <<<"$base"' '.branch // ""' 'tehas.rukas' '"$TEHAS_HOME/state/dispatch.taken"' \
  '$images/.taken' '.images.md' '"$st.out"' '"$st.rc"' '"$st.lock"' 'state/plan-<issue>.json.out' \
  '.planning.answer // ""' \
  '.planning.brief' "tehas_store_get '.planning.turn'" '"$keep/answer.md"' \
  "q=\$(tehas_store_get '.park.question // empty')" ".park // \"\" | strings" \
  'park_holds "$n"' 'park-show "$n"' '<!-- tehas:park issue=7 -->' \
  'systemd-run --user --quiet --collect' 'quiet_running=$(tehas_cycles_running)' '.cycle.quiet'; do
  for re in "${forbidden[@]}"; do
    grep -qE -- "$re" <<<"$line" && t_bad "the pattern $re finds: $line"
  done
done

t_case "no carrier's pattern takes a JSON field or a struct's for a file"
for line in 'jq -r .status' '$last.status' '.tick.status' 'run.status = x' '.stopped | length' \
  'jq .out' 'given.rc.parse()' '.done // false'; do
  for re in "${c_re[@]}"; do
    grep -qE -- "$re" <<<"$line" && t_bad "the pattern $re finds: $line"
  done
done

t_case "the list covers the code that is installed and run"
t_contains "$(code)" "libexec/work"
t_contains "$(code)" "libexec/dispatch"
t_contains "$(code)" "lib/store.sh"
t_contains "$(code)" "console/analytics/src/run.rs"
t_contains "$(code)" "install.sh"

t_case "each allowed line is still there"
for i in "${!a_file[@]}"; do
  grep -qE -- "${a_re[i]}" "$T_ROOT/${a_file[i]}" || t_bad "${a_file[i]} has no line ${a_re[i]}: take it off the list"
done

t_case "every file the importer reads is an old record the tests look for, and stops the poll"
imp=$T_ROOT/console/store/src/import.rs
names=()
while IFS= read -r sfx; do names+=("state/cycles/5$sfx"); done < <(
  sed -n '/^const CYCLE_SUFFIXES/,/^];/p' "$imp" | sed -nE 's/^ *\("(\.[a-z.]+)".*/\1/p')
while IFS=' ' read -r pre sfx; do names+=("state/${pre}5$sfx"); done < <(
  sed -n '/^const STATE_NAMES/,/^];/p' "$imp" | sed -nE 's/^ *\("([a-z]+-)", "([a-z.]+)".*/\1 \2/p')
names+=(state/cycles/plan-5.history.jsonl)
t_eq "${#names[@]}" 19
# The samples test/dispatch_guard_test.sh stops a poll with, one per line.
guard=$(sed -n '/^old_records=(/,/)$/p' "$T_ROOT/test/dispatch_guard_test.sh" | tr -d '()' | tr ' ' '\n' |
  sed -e 's/^old_records=//' -e 's|/$||' | grep .)
for name in "${names[@]}"; do
  hit=no
  for g in "${t_old_state[@]}"; do
    # shellcheck disable=SC2053  # the right side is a glob on purpose
    [[ $name == $g ]] && hit=yes
  done
  t_eq "$name: $hit" "$name: yes"
  t_eq "$name: $(grep -cxF -- "$name" <<<"$guard")" "$name: 1"
done

t_done
