#!/usr/bin/env bash
# Tests for the pass that scripts/verify.sh keeps per tree: the same tree in the
# same mode is checked once. Each case runs the gate in a scratch repository
# whose steps are stubs that count their runs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
fresh() {
  rm -rf "$tree" "$T/wt" "$T/calls/suite" "$T/suite_rc" "$T/touch"
  mkdir -p "$tree/scripts"
  cp "$T_ROOT/scripts/verify.sh" "$tree/scripts/"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-generic.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$tree/scripts/check-bash-lines.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'echo "test test/a_test.sh"' >"$tree/scripts/select-tests.sh"
  # The shell suite: counts its runs, edits a file mid-run when told to, and
  # exits as $T/suite_rc says.
  cat >"$tree/scripts/test.sh" <<'EOF'
#!/usr/bin/env bash
echo run >>"$T/calls/suite"
[ ! -f "$T/touch" ] || echo more >>"$(cat "$T/touch")"
exit "$(cat "$T/suite_rc" 2>/dev/null || echo 0)"
EOF
  chmod +x "$tree/scripts/"*.sh
  echo one >"$tree/a.txt"
  g init -q -b main
  g add -A
  g commit -q -m base
  g update-ref refs/remotes/origin/main HEAD
}
runs() { t_calls suite | wc -l | tr -d ' '; }
gate="cd '$tree' && scripts/verify.sh"
t_stub cargo 'exit 0'
t_stub node 'exit 0'

t_case "a second run on the same tree is a kept pass and runs no step"
fresh
t_run "$gate"
t_eq "$t_rc" 0
t_lacks "$t_out" "passed already"
t_run "$gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_contains "$t_out" "note  this tree passed already"
t_contains "$t_out" "ok    shellcheck and the shell tests"

t_case "a changed tracked file runs the gate again"
fresh
t_run "$gate"
echo two >"$tree/a.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a same-size edit in the second of the last index write runs the gate again"
fresh
# git checks a file whose stat is unchanged only when it is not older than
# the index. The times are pinned so the edit lands in the index's second;
# ctime cannot be pinned, so it is not compared.
g config core.trustctime false
index=$tree/.git/index
touch -d '2026-01-01 00:00:00' "$tree/a.txt"
g update-index -q --refresh
t_run "$gate"
echo two >"$tree/a.txt"
touch -d '2026-01-01 00:00:00' "$tree/a.txt" "$index"
# The real index sees the edit; a status that wrote the index would hide it.
t_eq "$(g --no-optional-locks status --porcelain a.txt)" " M a.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a new untracked file runs the gate again"
fresh
t_run "$gate"
echo new >"$tree/b.txt"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a failure is never kept"
fresh
echo 1 >"$T/suite_rc"
t_run "$gate"
t_eq "$t_rc" 1
t_run "$gate"
t_eq "$t_rc" 1
t_eq "$(runs)" 2
t_contains "$t_out" "FAIL  shellcheck and the shell tests"

t_case "another mode runs the gate again"
fresh
t_run "$gate"
t_run "$gate --full"
t_eq "$t_rc" 0
t_eq "$(runs)" 2
t_run "$gate --full"
t_eq "$(runs)" 2

t_case "TEHAS_VERIFY_FRESH=1 runs every step"
fresh
t_run "$gate"
t_run "export TEHAS_VERIFY_FRESH=1; $gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 2
t_lacks "$t_out" "passed already"

t_case "a tree changed during the run is not kept"
fresh
echo "$tree/a.txt" >"$T/touch"
t_run "$gate"
t_eq "$t_rc" 0
rm "$T/touch"
t_run "$gate"
t_eq "$(runs)" 2

t_case "a worktree of the same repository on the same tree uses the kept pass"
fresh
t_run "$gate"
g worktree add -q --detach "$T/wt" HEAD
t_run "cd '$T/wt' && scripts/verify.sh"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_contains "$t_out" "passed already"

t_case "outside a git repository the gate runs every time"
fresh
rm -rf "$tree/.git"
t_run "GIT_CEILING_DIRECTORIES='$T' $gate"
t_run "GIT_CEILING_DIRECTORIES='$T' $gate"
t_eq "$t_rc" 0
t_eq "$(runs)" 2

t_done
