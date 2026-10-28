#!/usr/bin/env bash
# Tests for the log scripts/verify.sh keeps of each run: every step's whole
# output, kept at a dated path, named on a failure. Each case runs the gate in
# a scratch repository whose steps are stubs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
logs=$T/logs
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() {
  rm -rf "$tree" "$logs" "$T/suite_rc"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-generic.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-bash-lines.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'echo "test test/a_test.sh"' >"$tree/scripts/select-tests.sh"
  # The shell suite: 60 numbered lines, more than a failure's tail shows, then
  # the exit $T/suite_rc says.
  cat >"$tree/scripts/test.sh" <<'EOF'
#!/usr/bin/env bash
for i in $(seq 1 60); do echo "suite line $i"; done
exit "$(cat "$T/suite_rc" 2>/dev/null || echo 0)"
EOF
  chmod +x "$tree/scripts/"*.sh
  echo one >"$tree/a.txt"
  g init -q -b main
  g add -A
  g commit -q -m base
  g update-ref refs/remotes/origin/main HEAD
}
gate="cd '$tree' && TEHAS_VERIFY_FRESH=1 TEHAS_VERIFY_LOGS='$logs' scripts/verify.sh"
t_stub cargo 'exit 0'
t_stub node 'exit 0'

t_case "a failing step leaves its whole output in a log the last line names"
fresh
echo 1 >"$T/suite_rc"
t_run "$gate"
t_eq "$t_rc" 1
last=$(tail -n 1 <<<"$t_out")
t_contains "$last" "log: $logs/plain-"
log=${last#log: }
t_eq "$(test -f "$log" && echo kept)" kept
t_contains "$(cat "$log")" "suite line 1"
t_contains "$(cat "$log")" "suite line 60"
t_lacks "$t_out" "suite line 20"

t_case "a run that passes keeps its log and names none"
fresh
t_run "$gate --changed"
t_eq "$t_rc" 0
t_lacks "$t_out" "log: "
t_eq "$(find "$logs" -name 'changed-*.log' | wc -l | tr -d ' ')" 1

t_case "the newest 20 logs are kept"
fresh
mkdir -p "$logs"
for i in $(seq -w 1 25); do touch -d "2026-01-01 00:00:$i" "$logs/plain-old$i.log"; done
t_run "$gate"
t_eq "$(find "$logs" -name '*.log' | wc -l | tr -d ' ')" 20
t_eq "$(test -f "$logs/plain-old01.log" && echo kept)" ""

t_case "without TEHAS_VERIFY_LOGS the log goes under TMPDIR"
fresh
mkdir -p "$T/tmp"
echo 1 >"$T/suite_rc"
t_run "cd '$tree' && TMPDIR='$T/tmp' TEHAS_VERIFY_FRESH=1 scripts/verify.sh"
t_contains "$(tail -n 1 <<<"$t_out")" "log: $T/tmp/tehas-verify/plain-"

t_done
