#!/usr/bin/env bash
# Tests for libexec/init-product: the missing contract files, proposed as a
# pull request.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

init="$T_ROOT/libexec/init-product"
origin=$T/acme/widget.git
g() { git -c user.name=t -c user.email=t@example.invalid -c init.defaultBranch=main "$@"; }

# A product on a scratch origin whose path ends in the repository's name, with
# these files committed (path=content), and the checkout the settings expect.
product() {
  local spec
  rm -rf "$T/src" "$T/acme" "$T/home/tehas" "$T"/calls/*
  mkdir -p "$T/src" "$T/acme"
  g -C "$T/src" init -q
  for spec in "$@"; do
    mkdir -p "$(dirname "$T/src/${spec%%=*}")"
    printf '%s\n' "${spec#*=}" >"$T/src/${spec%%=*}"
  done
  g -C "$T/src" add -A && g -C "$T/src" commit -q --allow-empty -m one
  g clone -q --bare "$T/src" "$origin"
  mkdir -p "$T/home/tehas/repos"
  g clone -q "$origin" "$T/home/tehas/repos/widget"
  T_ENV=(GIT_AUTHOR_NAME=bot GIT_AUTHOR_EMAIL=bot@example.invalid GIT_COMMITTER_NAME=bot GIT_COMMITTER_EMAIL=bot@example.invalid)
  t_instance "${T_MINIMAL[@]}"
}
# A file on the branch the run pushed.
pushed() { git -C "$origin" show "tehas/init-product:$1" 2>/dev/null; }
# The gate guessed for a product with these files, from a dry run.
gate_for() {
  product "$@"
  t_run "$init --dry-run"
  sed -n '/^=== .pi\/verify-cmd$/,/^===/p' <<<"$t_out" | sed -n '/^[[:space:]]*#/d; /^===/d; /^$/d; p; q'
}
setup_for() {
  product "$@"
  t_run "$init --dry-run"
  sed -n '/^=== .pi\/worktree-setup$/,/^===/p' <<<"$t_out" | grep -v -e '^===' -e '^tehas init-product:' | tail -n 1
}

t_stub gh 'echo https://github.com/acme/widget/pull/9'

t_case "the missing files go on a branch, and the bot opens a pull request"
product package.json='{"scripts": {"test": "jest"}}' package-lock.json='{}'
t_run "$init"
t_eq "$t_rc" 0
t_contains "$t_out" "stack package.json; proposing AGENTS.md .pi/verify-cmd .pi/worktree-setup"
t_contains "$t_out" "https://github.com/acme/widget/pull/9"
t_contains "$(pushed .pi/verify-cmd)" "A person should"
t_eq "$(pushed .pi/verify-cmd | tail -n 1)" "npm test"
t_eq "$(pushed .pi/worktree-setup | tail -n 1)" "npm ci"
t_eq "$(git -C "$origin" ls-tree "tehas/init-product" .pi/worktree-setup | cut -c1-6)" 100755
t_contains "$(pushed AGENTS.md)" "## Verify"
t_eq "$(git -C "$origin" log --format=%s -1 main)" one
calls=$(t_calls gh)
t_contains "$calls" "pr create --repo acme/widget --base main --head tehas/init-product"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 | wc -l)" 0

t_case "a file the product has is kept; with every file there, nothing is pushed"
product AGENTS.md=ours Cargo.toml='[package]'
t_run "$init"
t_eq "$t_rc" 0
t_eq "$(pushed AGENTS.md)" ours
t_eq "$(git -C "$origin" ls-tree --name-only -r tehas/init-product | LC_ALL=C sort | tr '\n' ' ')" ".pi/verify-cmd .pi/worktree-setup AGENTS.md Cargo.toml "
product AGENTS.md=ours .pi/verify-cmd=make .pi/worktree-setup=true Makefile='test:'
t_run "$init"
t_eq "$t_rc" 0
t_contains "$t_out" "has every file this proposes"
t_eq "$(git -C "$origin" branch --list tehas/init-product)" ""
t_eq "$(t_calls gh)" ""

t_case "--dry-run prints the files and changes nothing"
product go.mod='module x'
t_run "$init --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "=== .pi/verify-cmd"
t_eq "$(git -C "$origin" branch --list tehas/init-product)" ""
t_eq "$(t_calls gh)" ""

t_case "the gate is guessed for each stack"
t_eq "$(gate_for Makefile='check:' package.json='{}')" "make check"
t_eq "$(gate_for package.json='{"scripts": {"build": "tsc"}}' pnpm-lock.yaml=x)" "pnpm run build"
t_eq "$(setup_for package.json='{}' pnpm-lock.yaml=x)" "pnpm install --frozen-lockfile"
t_eq "$(gate_for package.json='{"scripts": {"test": "x"}}' yarn.lock=x)" "yarn test"
t_eq "$(gate_for Cargo.toml='[package]')" "cargo test"
t_eq "$(gate_for go.mod='module x')" "go test ./..."
t_eq "$(setup_for go.mod='module x')" "go mod download"
t_eq "$(gate_for pyproject.toml='[project]')" "python -m pytest"
t_eq "$(gate_for pyproject.toml='[project]' uv.lock=x)" "uv run pytest"
t_eq "$(gate_for .github/workflows/ci.yml='jobs:
  t:
    steps:
      - run: ./configure
      - run: ./run-tests.sh --all')" "./run-tests.sh --all"

t_case "no guess: the gate file holds only a comment, and no setup is written"
product README=x
t_run "$init --dry-run"
t_contains "$t_out" "could not guess a gate"
t_lacks "$t_out" "worktree-setup"

t_case "the branch already on origin is not pushed over"
product go.mod='module x'
git -C "$origin" branch tehas/init-product main
t_run "$init"
t_eq "$t_rc" 1
t_contains "$t_out" "already on origin"
t_eq "$(t_calls gh)" ""

t_case "settings missing, or a checkout of another repository: 78, nothing pushed"
product go.mod='module x'
t_instance PI_ENSEMBLE_SUBAGENT_MODEL=x
t_run "$init"
t_eq "$t_rc" 78
git -C "$T/home/tehas/repos/widget" config remote.origin.url https://github.com/acme/other.git
t_instance "${T_MINIMAL[@]}"
t_run "$init"
t_eq "$t_rc" 78
t_contains "$t_out" "not of acme/widget"
t_eq "$(git -C "$origin" branch --list tehas/init-product)" ""
t_eq "$(t_calls gh)" ""

t_case "a wrong argument is 64"
t_run "$init --force"
t_eq "$t_rc" 64

t_done
