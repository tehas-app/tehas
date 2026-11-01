#!/usr/bin/env bash
# Tests for the switch of a cycle's report, TEHAS_CYCLE_REPORT: bash by default,
# tehas-cycle report with rust. What the report does is the same in both: the
# files test/cycle_*_rust_test.sh run the cases of the bash report on the Rust.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

home=$T/home/tehas
# seen: tehas-cycle that notes each verb it is asked.
seen() {
  mv "$home/libexec/tehas-cycle" "$home/libexec/tehas-cycle.real"
  printf '%s\n' '#!/usr/bin/env bash' 'echo "$1" >>"$T/calls/verbs"' 'exec "$0.real" "$@"' >"$home/libexec/tehas-cycle"
  chmod +x "$home/libexec/tehas-cycle"
}

t_case "by default the bash reports, and with bash it does too"
for v in "" TEHAS_CYCLE_REPORT=bash; do
  reset
  seen
  t_instance "${T_MINIMAL[@]}" ${v:+"$v"}
  ended aborted develop
  t_run "$cycle 7"
  t_eq "$t_rc" 0
  t_eq "$(tr '\n' ' ' <"$T/calls/verbs")" "cycle runner approved after "
  t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
done

t_case "with rust tehas-cycle reports, nothing is left open in the journal, and the cycle is on record"
reset
seen
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_REPORT=rust
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(tr '\n' ' ' <"$T/calls/verbs")" "cycle "
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$t_out" "done #7 rc=20 status=aborted step=develop"
t_eq "$(TEHAS_HOME=$home "$T_STORE" journal-open | jq '.actions | length')" 0
t_eq "$(t_row cycle-show 7 -- '.cycle.ended_at != null')" true
t_eq "$(t_calls systemctl)" "--user start --no-block tehas-dispatch.service"
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "a report goes by the settings the cycle started with, whatever instance.env says by then"
changes=(
  'echo TEHAS_THEME=nothing-this-version-knows >>"$TEHAS_INSTANCE_DIR/instance.env"'
  'echo "this is ( not shell" >>"$TEHAS_INSTANCE_DIR/instance.env"'
  'echo TEHAS_REPO=other/place >>"$TEHAS_INSTANCE_DIR/instance.env"'
  'rm "$TEHAS_INSTANCE_DIR/instance.env"'
)
for v in "" TEHAS_CYCLE_REPORT=rust; do
  for change in "${changes[@]}"; do
    reset
    t_instance "${T_MINIMAL[@]}" ${v:+"$v"}
    ended aborted develop
    printf '%s\n' "$change" >"$fx/work.sh"
    t_run "$cycle 7"
    t_eq "$t_rc" 0
    t_contains "$t_out" "done #7 rc=20 status=aborted step=develop"
    t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
    t_contains "$(t_calls gh)" "--repo acme/widget"
    t_eq "$(t_calls gh | grep -c other/place)" 0
    t_contains "$(t_calls gh)" "--remove-label tehas:running"
    t_eq "$(t_row cycle-show 7 -- '.cycle.ended_at != null')" true
  done
done

t_case "the switch is read at the start: a line added during a cycle does not move its report"
reset
seen
ended aborted develop
printf '%s\n' 'echo TEHAS_CYCLE_REPORT=rust >>"$TEHAS_INSTANCE_DIR/instance.env"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(tr '\n' ' ' <"$T/calls/verbs")" "cycle runner approved after "

t_case "the bash report does not ask again whether the store is ready: a store gone by then still gets a comment"
reset
ended aborted develop
printf '%s\n' 'rm "$TEHAS_HOME/libexec/store"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 70
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "--remove-label tehas:running"

t_case "what the report runs is not told it is a report"
reset
t_stub fake-status 'echo "ended=${TEHAS_CYCLE_ENDED:-none}" >"$T/calls/status-env"'
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/status-env")" "ended=none"

t_case "a value that is neither stops with 78 before anything runs"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_REPORT=python
t_run "$cycle 7"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CYCLE_REPORT is not valid: expected bash or rust"
t_eq "$(t_calls gh)" ""

t_case "without the program nothing starts: 78, and nothing is asked of GitHub"
reset
rm -f "$home/libexec/tehas-cycle"
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_REPORT=rust
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 78
t_contains "$t_out" "no tehas-cycle at $home/libexec/tehas-cycle"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

t_done
