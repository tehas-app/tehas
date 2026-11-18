#!/usr/bin/env bash
# Tests for .github/workflows/kani.yml: it runs on pull requests into main, on
# pushes to main and by hand, pins Kani, and proves every crate that holds a
# #[cfg(kani)] module.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

workflow=$T_ROOT/.github/workflows/kani.yml
triggers=$(awk '/^on:$/ {s = 1; next} s && /^[^ ]/ {exit} s {print}' "$workflow")

t_case "pull requests run the proofs only when they are into main"
t_eq "$(grep -A1 '^  pull_request:' <<<"$triggers")" "  pull_request:
    branches: [main]"

t_case "pushes run the proofs only on main"
t_eq "$(grep -A1 '^  push:' <<<"$triggers")" "  push:
    branches: [main]"

t_case "a person can run it on any branch, and nothing else starts it"
t_contains "$triggers" '  workflow_dispatch:'
t_eq "$(grep -c '^  [a-z_]*:' <<<"$triggers")" 3

t_case "Kani is installed at a pinned version"
[[ "$(grep 'KANI_VERSION:' "$workflow")" =~ KANI_VERSION:\ [0-9]+\.[0-9]+\.[0-9]+$ ]] || t_bad "KANI_VERSION is not an exact version"
t_contains "$(cat "$workflow")" 'cargo install --locked kani-verifier --version "$KANI_VERSION"'

t_case "every crate with a #[cfg(kani)] module is proved"
crates=$(sed -n 's/^ *KANI_CRATES: //p' "$workflow")
for dir in $(grep -rl --include='*.rs' '#\[cfg(kani)\]' "$T_ROOT/console" | sed "s|$T_ROOT/console/||; s|/src/.*||" | sort -u); do
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$T_ROOT/console/$dir/Cargo.toml" | head -n 1)
  t_contains " $crates " " $name "
done

t_case "every crate it proves declares cfg(kani), so cargo stays quiet"
for name in $crates; do
  manifest=$(grep -l "^name = \"$name\"" "$T_ROOT"/console/*/Cargo.toml | head -n 1)
  t_contains "$(cat "$manifest")" 'check-cfg = ["cfg(kani)"]'
done

t_done
