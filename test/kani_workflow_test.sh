#!/usr/bin/env bash
# Tests for the proofs job of .github/workflows/release-gate.yml: the Kani
# proofs run only in the release gate, on the gated commit, with Kani pinned,
# and on every crate that holds a #[cfg(kani)] module.
# shellcheck disable=SC2016  # the assertions name the literal text of the workflow
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

gate=$T_ROOT/.github/workflows/release-gate.yml
# The proofs job, from its line to the next job.
job=$(awk '/^  proofs:$/ {s = 1; print; next} s && /^  [a-z_]+:$/ {exit} s {print}' "$gate")

t_case "no other workflow runs the proofs"
t_eq "$(grep -l 'cargo kani' "$T_ROOT"/.github/workflows/*.yml)" "$gate"

t_case "the release gate has a proofs job on the gated commit"
t_contains "$job" '  proofs:'
t_contains "$job" 'ref: ${{ inputs.commit }}'
t_contains "$job" 'test "$(git rev-parse HEAD)" = "$COMMIT"'

t_case "Kani is installed at a pinned version"
[[ "$(grep 'KANI_VERSION:' <<<"$job")" =~ KANI_VERSION:\ [0-9]+\.[0-9]+\.[0-9]+$ ]] || t_bad "KANI_VERSION is not an exact version"
t_contains "$job" 'cargo install --locked kani-verifier --version "$KANI_VERSION"'

t_case "every crate with a #[cfg(kani)] module is proved"
crates=$(sed -n 's/^ *KANI_CRATES: //p' <<<"$job")
while read -r dir; do
  [ -n "$dir" ] || continue
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$T_ROOT/console/$dir/Cargo.toml" | head -n 1)
  t_contains " $crates " " $name "
done < <(grep -rl --include='*.rs' '#\[cfg(kani)\]' "$T_ROOT/console" | sed "s|$T_ROOT/console/||; s|/src/.*||" | sort -u)

t_case "every crate it proves declares cfg(kani), so cargo stays quiet"
for name in $crates; do
  manifest=$(grep -l "^name = \"$name\"" "$T_ROOT"/console/*/Cargo.toml | head -n 1)
  t_contains "$(cat "$manifest")" 'check-cfg = ["cfg(kani)"]'
done

t_done
