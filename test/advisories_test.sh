#!/usr/bin/env bash
# Tests for the advisory check the gate runs: OSV-Scanner's pin in
# host/versions.env, the step of .github/workflows/verify.yml that downloads
# exactly that version, checks the checksum its own release publishes, and
# scans every lockfile this tree tracks, and what osv-scanner.toml must say
# about an advisory it carries. Nothing here reaches the network.
# shellcheck disable=SC2016  # the assertions name the literal text of the workflow
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# shellcheck source=host/versions.env
. "$T_ROOT/host/versions.env"
workflow=$T_ROOT/.github/workflows/verify.yml
config=$T_ROOT/osv-scanner.toml

# The step on its own, from its name to the next one, so a case cannot pass on
# what another step of the job does.
step=$(awk '/- name: the lockfiles against published advisories/{s = 1; next}
  s && /- name:/{exit}
  s' "$workflow")

t_case "OSV-Scanner is pinned to a version and a published checksum"
t_eq "$(grep -cE '^OSV_SCANNER_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^OSV_SCANNER_SHA256=[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1

t_case "the step downloads the pinned version and refuses a binary that is not it"
t_contains "$step" 'curl -fsSL -o "$RUNNER_TEMP/osv-scanner"'
t_contains "$step" 'releases/download/v$OSV_SCANNER_VERSION/osv-scanner_linux_amd64'
t_contains "$step" 'echo "$OSV_SCANNER_SHA256  $RUNNER_TEMP/osv-scanner" | sha256sum -c -'
t_contains "$step" 'osv-scanner version: $OSV_SCANNER_VERSION'

t_case "the scan is bounded, reads this repository's ignore list, and names no registry"
t_contains "$step" 'timeout 300 "$RUNNER_TEMP/osv-scanner" scan source'
t_contains "$step" '--config osv-scanner.toml'
t_lacks "$step" 'continue-on-error'
# npm audit sends the dependency tree to the registry, which is the one thing
# this check may not do.
t_lacks "$step" 'npm audit'
# The sum is taken for the asset the step downloads. Two names in two languages
# for one file is where a pin goes wrong, so they are checked against each other.
t_contains "$(cat "$T_ROOT/console/src/pins/tools.rs")" '"osv-scanner_linux_amd64"'

t_case "every lockfile this tree tracks is one the scan is given"
locks=$(git -C "$T_ROOT" ls-files -- '*Cargo.lock' '*package-lock.json' '*yarn.lock' '*pnpm-lock.yaml' '*bun.lock')
n=0
for f in $locks; do
  t_contains "$step" "-L $f"
  n=$((n + 1))
done
[ "$n" -gt 0 ] || t_bad "no lockfile is tracked, so the case checked nothing"
t_contains "$locks" 'console/Cargo.lock'

t_case "an ignored advisory names its id, its reason and the date to look again"
# One line per IgnoredVulns block that is missing any of the three.
incomplete=$(awk '
  /^\[\[IgnoredVulns\]\]$/ {
    if (n && !(id && reason && until)) print "block " n
    n++; id = 0; reason = 0; until = 0; next
  }
  n && /^id = "[A-Z]+-[0-9]+-[0-9]+"$/ { id = 1 }
  n && /^reason = "[^"]/ { reason = 1 }
  n && /^effectiveUntil = [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/ { until = 1 }
  END { if (n && !(id && reason && until)) print "block " n }' "$config")
t_eq "$incomplete" ""
t_eq "$(grep -cE '^\[\[IgnoredVulns\]\]$' "$config")" \
  "$(grep -cE '^effectiveUntil = ' "$config")"
# An ignore whose date has passed is no longer a decision, it is an accident
# waiting for CI to say so. Say it here too, in the same words.
today=$(date -u +%Y-%m-%d)
today=${today//-/}
# The here-string keeps the loop in this shell: a pipe would run it in a
# subshell, where t_bad records a failure no one counts.
while read -r d; do
  [ -n "$d" ] || continue
  [ "${d//-/}" -gt "$today" ] ||
    t_bad "an ignore expires $d, which is $today or past: fix the advisory or move the date with a reason"
done <<< "$(grep -oE '^effectiveUntil = [0-9-]+$' "$config" | cut -d' ' -f3)"

t_done
