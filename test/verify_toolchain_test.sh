#!/usr/bin/env bash
# Tests for the step of .github/workflows/verify.yml and release-gate.yml that
# checks the runner's Rust toolchain before the gate: a runner without cargo
# fails with one error that names what is missing, not with a failure in every
# Rust test. The step's own script runs here with a PATH of stubs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

name='the toolchain the gate builds with'

# check WORKFLOW TOOL...: the step of WORKFLOW, as the runner gets it, with
# only the stubs TOOL... on its PATH. Each stub prints its name and version;
# cargo answers fmt and clippy unless rustfmt or clippy is left out.
check() {
  local workflow=$1 t
  shift
  rm -rf "$T/tools"
  mkdir -p "$T/tools"
  for t in "$@"; do
    case $t in
      cargo)
        # shellcheck disable=SC2016  # the stub's own $1
        printf '#!%s\ncase "$1" in\n' "$T_BASH" >"$T/tools/cargo"
        [[ " $* " == *" rustfmt "* ]] || echo '  fmt) exit 101 ;;' >>"$T/tools/cargo"
        [[ " $* " == *" clippy "* ]] || echo '  clippy) exit 101 ;;' >>"$T/tools/cargo"
        printf '  *) echo "%s 1.0" ;;\nesac\n' "$t" >>"$T/tools/cargo"
        ;;
      rustc) printf '#!%s\necho "rustc 1.0"\n' "$T_BASH" >"$T/tools/rustc" ;;
    esac
  done
  chmod 755 "$T/tools/"* 2>/dev/null
  t_rc=0
  t_out=$(cd "$T" && env -i PATH="$T/tools" RUNNER_NAME=runner-a "$T_BASH" -e "$T/step-${workflow##*/}.sh" 2>&1) || t_rc=$?
}

for workflow in "$T_ROOT/.github/workflows/verify.yml" "$T_ROOT/.github/workflows/release-gate.yml"; do
  w=${workflow##*/}
  awk -v n="      - name: $name" '$0 == n {s = 1; next}
    s && /^      - (name|uses):/ {exit}
    s == 2 {print substr($0, 11)}
    s && /^        run: \|$/ {s = 2}' "$workflow" >"$T/step-$w.sh"

  t_case "$w checks the toolchain just before the gate runs"
  steps=$(grep -E '^      - (name|uses):' "$workflow")
  t_eq "$(grep -A1 -- "- name: $name" <<<"$steps" | tail -n 1)" "      - name: scripts/verify.sh"
  t_lacks "$(sed -n "/- name: $name/,/- name: scripts/p" "$workflow")" 'continue-on-error'

  t_case "$w: a runner with the whole toolchain passes the step"
  check "$w" cargo rustc rustfmt clippy
  t_eq "$t_rc" 0
  t_contains "$t_out" "cargo 1.0"
  t_contains "$t_out" "rustc 1.0"
  t_lacks "$t_out" "::error"

  t_case "$w: a runner without cargo fails with one error that names it"
  check "$w"
  t_eq "$t_rc" 1
  t_eq "$(grep -c '^::error' <<<"$t_out")" 1
  t_contains "$t_out" "::error title=the runner cannot build::runner runner-a has no cargo rustc rustfmt clippy on the job's PATH."
  t_contains "$t_out" "this failure is the runner's, not the change's"

  t_case "$w: a toolchain without rustfmt or clippy names just those"
  check "$w" cargo rustc
  t_eq "$t_rc" 1
  t_contains "$t_out" "has no rustfmt clippy on the job's PATH"
  check "$w" cargo rustc rustfmt
  t_eq "$t_rc" 1
  t_contains "$t_out" "has no clippy on the job's PATH"
done

t_done
