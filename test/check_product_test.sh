#!/usr/bin/env bash
# Tests for libexec/check-product: the product checkout against the contract.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

check="$T_ROOT/libexec/check-product"
result=$T/home/tehas/state/check-product.json
g() { git -c user.name=t -c user.email=t@example.invalid -c init.defaultBranch=main "$@"; }

# A product on a scratch origin, with these files committed, and the
# checkout where the settings expect it. Each argument is path=content.
product() {
  local spec
  rm -rf "$T/origin" "$T/origin.git" "$T/home/tehas" "$T"/calls/*
  mkdir -p "$T/origin"
  g -C "$T/origin" init -q
  for spec in "$@"; do
    mkdir -p "$(dirname "$T/origin/${spec%%=*}")"
    printf '%s\n' "${spec#*=}" >"$T/origin/${spec%%=*}"
  done
  g -C "$T/origin" add -A && g -C "$T/origin" commit -q --allow-empty -m one
  g clone -q --bare "$T/origin" "$T/origin.git"
  mkdir -p "$T/home/tehas/repos"
  g clone -q "$T/origin.git" "$T/home/tehas/repos/widget"
  # The origin a settings check expects: a GitHub name, read from the config.
  g -C "$T/home/tehas/repos/widget" config remote.origin.url "https://github.com/acme/widget.git"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  printf '%s\n' '{"providers": {"lab": {"models": [{"id": "big"}]}}}' >"$TEHAS_INSTANCE_DIR/models.json"
}
good=(AGENTS.md=rules ".pi/verify-cmd=# the gate
make test")

t_case "a product that keeps the contract passes, and the result is written"
product "${good[@]}"
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    origin"
t_contains "$t_out" "ok    default  origin/HEAD is origin/main"
t_contains "$t_out" "ok    gate     .pi/verify-cmd: make test"
t_contains "$t_out" "no .pi/settings.json, the instance's model is used"
t_eq "$(jq -c '{ok, failures, sandbox}' "$result")" '{"ok":true,"failures":[],"sandbox":null}'

t_case "settings missing: 78 before any git call"
product "${good[@]}"
t_stub git
t_instance PI_ENSEMBLE_SUBAGENT_MODEL=x
t_run "$check"
t_eq "$t_rc" 78
t_eq "$(t_calls git)" ""
rm -f "$T/bin/git"

t_case "no checkout: 78, and it is named"
product "${good[@]}"
rm -rf "$T/home/tehas/repos/widget"
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  origin   no product checkout at $T/home/tehas/repos/widget"

t_case "a checkout of another repository fails the origin check"
product "${good[@]}"
g -C "$T/home/tehas/repos/widget" config remote.origin.url "https://github.com/acme/other.git"
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  origin   "
t_contains "$t_out" "not of acme/widget"

t_case "no origin/HEAD: named, with the command that sets it"
product "${good[@]}"
g -C "$T/home/tehas/repos/widget" remote set-head origin -d
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  default  origin/HEAD is not set"

t_case "no AGENTS.md and a gate of comments only: each named"
product ".pi/verify-cmd=# nothing yet"
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  agents   no AGENTS.md"
t_contains "$t_out" "FAIL  gate     no command in .pi/verify-cmd"
t_eq "$(jq '.failures | length' "$result")" 2

t_case "an uncommitted file decides nothing"
product ".pi/verify-cmd=make test"
echo rules >"$T/home/tehas/repos/widget/AGENTS.md"
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  agents"

t_case "a product model the instance lists passes; one it does not list fails"
product "${good[@]}" '.pi/settings.json={"defaultProvider": "lab", "defaultModel": "big"}'
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    model    .pi/settings.json names lab/big"
product "${good[@]}" '.pi/settings.json={"defaultProvider": "lab", "defaultModel": "small"}'
t_run "$check"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  model    .pi/settings.json names lab/small, which $TEHAS_INSTANCE_DIR/models.json does not list"

t_case "--sandbox runs setup, the full gate and the smoke in the image, on a clone"
product "${good[@]}" ".pi/verify-cmd-full=make full" ".pi/smoke-cmd=make smoke" ".pi/worktree-setup=true"
t_stub docker 'case $1 in image) exit 0 ;; run) w=""; prev=""; for a in "$@"; do [ "$prev" != -w ] || w=$a; prev=$a; done; ls "$w/.pi" >"$T/calls/tree" 2>&1; exit "${RUN_RC:-0}" ;; esac'
t_run "$check --sandbox"
t_eq "$t_rc" 0
t_contains "$t_out" "ok    sandbox  the gates passed"
run=$(grep '^run ' "$T/calls/docker")
t_contains "$run" "--cgroup-parent tehas.slice"
t_contains "$run" "--user $(id -u):$(id -g)"
t_contains "$run" "sh .pi/worktree-setup && sh -c \"\$1\" && sh -c \"\$2\" gates make full make smoke"
t_contains "$(cat "$T/calls/tree")" "verify-cmd-full"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 | wc -l)" 0
t_eq "$(jq .sandbox "$result")" true

t_case "--sandbox: failing gates are a failure; no image is exit 1"
product "${good[@]}"
T_ENV=(RUN_RC=2)
t_run "$check --sandbox"
t_eq "$t_rc" 78
t_contains "$t_out" "FAIL  sandbox  the gates failed"
t_eq "$(jq .sandbox "$result")" false
t_stub docker 'exit 1'
t_run "$check --sandbox"
t_eq "$t_rc" 1
t_contains "$t_out" "no sandbox image"

t_case "a wrong argument is 64"
t_run "$check --what"
t_eq "$t_rc" 64

t_done
