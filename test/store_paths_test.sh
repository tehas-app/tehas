#!/usr/bin/env bash
# The records on the state store are read and written through lib/store.sh,
# not as files. This test fails when code that is installed or run names the
# old file of a record that moved. The files that stay files, the carriers,
# are signals between two processes, a container's mount or its output: each
# is named only by the code that passes it on. Tests, the manual in host/docs/
# and the records in docs/ may name both.
# shellcheck disable=SC2016  # the patterns and samples are code, not expanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

code() { # every tracked file that is installed or run and could name a record
  git -C "$T_ROOT" ls-files -- bin libexec lib install.sh console/analytics/src
}

# The old files of the records that moved, as extended regular expressions,
# each with a line of code it is there to find. Each group that moves onto the
# store adds its own.
forbidden=()
forbidden_sample=()

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
carrier '\.runner_exit\b' 'runner_exit_file=$TEHAS_HOME/state/cycles/$issue.runner_exit' \
  'lib/cycles.sh libexec/cycle libexec/stop libexec/work' \
  "work tells the cycle how the runner ended"
carrier "$n"'\.status\b' 'status_file=$TEHAS_HOME/state/cycles/$issue.status' \
  'libexec/cycle libexec/work' \
  "work tells the cycle the runner's status word"
carrier '\.oom\b' 'oom_file=${cycles:?}/${issue:?}.oom' \
  "lib/cycles.sh libexec/cycle libexec/stop libexec/work $an/main.rs $an/run.rs $an/tests.rs" \
  "work counts the container's kills for memory for the cycle"
carrier '\.forks\b' 'forks_file=$TEHAS_HOME/state/cycles/$issue.forks' \
  "libexec/cycle libexec/stop libexec/work $an/main.rs $an/run.rs" \
  "work counts the container's refused forks for the cycle"
carrier '\.stalled\b' 'stall_file=${cycles:?}/${issue:?}.stalled' \
  'libexec/cycle libexec/stop libexec/work' \
  "work tells the cycle the container stalled"
carrier '\.timed\b' 'timed_file=${cycles:?}/${issue:?}.timed' \
  'libexec/cycle libexec/stop libexec/work' \
  "work tells the cycle the container ran out of time"
carrier '\.nostart\b' 'nostart_file=$TEHAS_HOME/state/cycles/$issue.nostart' \
  'lib/cycles.sh libexec/cycle libexec/stop libexec/work' \
  "work tells the cycle the runner never started"
carrier '\.result\.json\b' 'result=$TEHAS_HOME/state/cycles/$issue.result.json' \
  "libexec/cycle libexec/export libexec/stop libexec/work $an/run.rs $an/sessions.rs $an/tests.rs" \
  "the container's result, kept to the next start for tehas export"
carrier '\.progress\.json\b' 'progress=$TEHAS_HOME/state/cycles/$issue.progress.json' \
  "lib/cycles.sh libexec/cycle libexec/export libexec/stop libexec/work $an/run.rs $an/tests.rs" \
  "the summary of the container's output, kept to the next start for tehas export"
carrier "$n"'\.done\b' 'echo "$done_why" >"$TEHAS_HOME/state/cycles/$n.done"' \
  'libexec/cycle libexec/dispatch' \
  "the dispatcher tells a running cycle why it is done"
carrier "$n"'\.stopped\b' 'stopped_marker=$state/$n.stopped' \
  'libexec/cycle libexec/stop' \
  "tehas stop tells a running cycle it was stopped"
carrier '\.cid\b' 'cid_file=$TEHAS_HOME/state/cycles/$issue.cid' \
  'libexec/dev libexec/dispatch libexec/stop libexec/work' \
  "the container's id, for docker"
carrier '\.manifest\.json\b' 'kept=$TEHAS_HOME/state/cycles/$issue.manifest.json' \
  'lib/attachments.sh libexec/export libexec/stop libexec/work' \
  "the images a container is given, mounted into it"
carrier '\.images\.md\b' 'cp "$images/x" "$TEHAS_HOME/state/cycles/7.images.md"' \
  'libexec/work' \
  "the list of images a container is given, mounted into it"
carrier '\.retry-context\.md\b' 'cp "$images/x" "$TEHAS_HOME/state/cycles/7.retry-context.md"' \
  'libexec/work' \
  "what a reviewer said before a retry, mounted into the container"
carrier '\$\{?st\}?\.out\b|\.json\.out\b' 'what=$(plan_filed "$st.out")' \
  "lib/planning.sh libexec/dispatch $an/plan.rs $an/tests.rs" \
  "a planning container's output"
carrier '\$\{?st\}?\.rc\b|\.json\.rc\b' 'rc=$(cat "$st.rc" 2>/dev/null)' \
  'lib/planning.sh libexec/dispatch libexec/health' \
  "a planning turn's exit, from the turn to the poll that collects it"
carrier '\$\{?st\}?\.lock\b|\.json\.lock\b' '[ -f "$st.lock" ]' \
  'lib/planning.sh libexec/dispatch libexec/health' \
  "the lock a planning turn holds while it runs"
carrier '\$\{?st\}?\.keep\b|\.json\.keep\b' 'rm -rf "$st.keep"' \
  'lib/planning.sh libexec/dispatch' \
  "the directory a planning container keeps its session in"
carrier 'refined\.jsonl\b' 'ledger=$TEHAS_HOME/state/refined.jsonl' \
  "lib/planning.sh $an/main.rs $an/report.rs $an/sessions.rs $an/tests.rs" \
  "the ledger of refined issues, which the store does not hold"

t_case "no code names the old file of a record on the store"
found=""
while IFS= read -r f; do
  for re in "${forbidden[@]}"; do
    hits=$(grep -nE -- "$re" "$T_ROOT/$f" 2>/dev/null) || continue
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

t_done
