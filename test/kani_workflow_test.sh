#!/usr/bin/env bash
# Tests for the proofs job of .github/workflows/release-gate.yml: the Kani
# proofs run only in the release gate, on a runner of their own, on the
# gated commit, with Kani pinned,
# and on every crate that holds code under cfg(kani).
# shellcheck disable=SC2016  # the assertions name the literal text of the workflow
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

gate=$T_ROOT/.github/workflows/release-gate.yml
# The proofs job, from its line to the next job.
job=$(awk '/^  proofs:$/ {s = 1; print; next} s && /^  [a-z_]+:$/ {exit} s {print}' "$gate")

# The Rust files under $1 with a cfg or cfg_attr that names kani, alone or
# inside all(), any() or not().
kani_files() {
  grep -rlE --include='*.rs' '#!?\[cfg(_attr)?\(([^]]*[^a-z_0-9])?kani([^a-z_0-9]|$)' "$1"
}

t_case "no other workflow runs the proofs"
t_eq "$(grep -l 'cargo kani' "$T_ROOT"/.github/workflows/*.yml)" "$gate"

t_case "the release gate has a proofs job on the gated commit"
t_contains "$job" '  proofs:'
t_contains "$job" 'ref: ${{ inputs.commit }}'
t_contains "$job" 'test "$(git rev-parse HEAD)" = "$COMMIT"'

t_case "the proofs have a runner of their own"
t_contains "$job" '    runs-on: [self-hosted, kani]'
t_lacks "$job" 'speedlane'

t_case "Kani is installed at a pinned version"
[[ "$(grep 'KANI_VERSION:' <<<"$job")" =~ KANI_VERSION:\ [0-9]+\.[0-9]+\.[0-9]+$ ]] || t_bad "KANI_VERSION is not an exact version"
t_contains "$job" 'cargo install --locked kani-verifier --version "$KANI_VERSION"'
t_contains "$job" 'grep -qx "Kani Rust Verifier $KANI_VERSION (cargo plugin)"'

t_case "the Kani bundle is checked against a pinned sha256 before setup"
[[ "$(grep 'KANI_BUNDLE_SHA256:' <<<"$job")" =~ KANI_BUNDLE_SHA256:\ [0-9a-f]{64}$ ]] || t_bad "KANI_BUNDLE_SHA256 is not a sha256"
t_contains "$job" 'echo "$KANI_BUNDLE_SHA256  $bundle" | sha256sum -c -'
t_contains "$job" 'cargo kani setup --use-local-bundle "$bundle"'

t_case "the proofs build every feature and deal every harness to one shard"
t_contains "$job" 'args=(--all-features)'
t_contains "$job" "        shard: [$(seq -s ', ' "$(sed -n 's/^ *SHARDS: //p' <<<"$job")")]"
t_contains "$job" 'kani-shard kani-list.json "$SHARD" "$SHARDS")'
t_contains "$job" 'cargo kani "${args[@]}" --exact "${filters[@]}"'

t_case "code under cfg(kani) is found in every form of the attribute"
mkdir -p "$T/kani"
attrs=('#[cfg(kani)]' '#[cfg(all(kani, test))]' '#[cfg(all(test, kani))]' \
  '#[cfg(any(kani, test))]' '#[cfg_attr(kani, kani::unwind(8))]' '#![cfg(kani)]')
for n in 1 2 3 4 5 6; do printf '%s\nmod proofs {}\n' "${attrs[n - 1]}" >"$T/kani/found$n.rs"; done
printf '%s\n' '#[cfg(kanim)]' '#[cfg(feature = "makani")]' '// kani' >"$T/kani/none.rs"
t_eq "$(kani_files "$T/kani" | sed "s|$T/kani/||" | sort | tr '\n' ' ')" \
  "found1.rs found2.rs found3.rs found4.rs found5.rs found6.rs "

t_case "every crate with code under cfg(kani) is proved"
crates=$(sed -n 's/^ *KANI_CRATES: //p' <<<"$job")
while read -r dir; do
  [ -n "$dir" ] || continue
  name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$T_ROOT/console/$dir/Cargo.toml" | head -n 1)
  t_contains " $crates " " $name "
done < <(kani_files "$T_ROOT/console" | sed "s|$T_ROOT/console/||; s|/src/.*||" | sort -u)

t_case "every crate it proves declares cfg(kani), so cargo stays quiet"
for name in $crates; do
  manifest=$(grep -l "^name = \"$name\"" "$T_ROOT"/console/*/Cargo.toml | head -n 1)
  t_contains "$(cat "$manifest")" 'check-cfg = ["cfg(kani)"]'
done

t_done
