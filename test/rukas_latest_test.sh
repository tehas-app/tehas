#!/usr/bin/env bash
# Tests for scripts/rukas-latest.sh, against stub gh and curl: it writes the
# newest release's commit and image, changes nothing when a lookup fails,
# refuses a release that lacks what tehas relies on, including the reader of a
# cycle's images, and --check only reports.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

repo=$T/repo
commit=0123456789abcdef0123456789abcdef01234567
digest=sha256:$(printf 'a%.0s' {1..64})

# The release's source: what tehas reads, the setting its commands pass, and
# the file that reads the manifest tehas writes beside a cycle's images. A name
# only a Markdown file mentions does not count.
src=$T/src/up-pi-rukas-0123456
source_tree() {
  rm -rf "$T/src"
  mkdir -p "$src/extension/src"
  printf '%s\n' 'PI_ENSEMBLE_MAX_WORKSTREAMS' 'pipelineState currentStep branchName eventLog' \
    'status: "merged" | "handoff" | "aborted" | "running"' >"$src/extension/driver.ts"
  printf '%s\n' 'export function loadIssueImages() { return [] }' >"$src/extension/src/work-driver-issue-images.ts"
  echo PI_ENSEMBLE_GONE >"$src/CHANGELOG.md"
  tar -czf "$T/src.tgz" -C "$T/src" .
}
source_tree

# A scratch copy, so the script edits files of its own.
fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/host/sandbox" "$repo/lib"
  cp "$T_ROOT/scripts/rukas-latest.sh" "$repo/scripts/"
  echo 'export PI_ENSEMBLE_MAX_WORKSTREAMS=1' >"$repo/lib/config.sh"
  printf '%s\n' '# versions' 'PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git' \
    'PI_RUKAS_VERSION=v0.1.0' "PI_RUKAS_COMMIT=$(printf 'b%.0s' {1..40})" 'NODE_VERSION=1' >"$repo/host/versions.env"
  printf '%s\n' '# base' 'FROM ghcr.io/old/pi-rukas@sha256:old' 'RUN true' >"$repo/host/sandbox/Dockerfile"
  rm -f "$T/calls/"*
}
# The newest release is v0.2.0. GH_RC and CURL_RC make one call fail.
stubs() {
  t_stub gh 'case "$*" in
  *releases*) [ "${GH_RC:-0}" = 0 ] || exit 1; echo v0.2.0 ;;
  *commits/v0.2.0*) echo '"$commit"' ;;
  *tarball/'"$commit"'*) cat '"$T"'/src.tgz ;;
  *) exit 1 ;;
esac'
  t_stub curl 'case "$*" in
  *token*) echo "{\"token\":\"t\"}" ;;
  *manifests/v0.2.0*) [ "${CURL_RC:-0}" = 0 ] || exit 22; printf "HTTP/2 200\r\nDocker-Content-Digest: '"$digest"'\r\n" ;;
  *) exit 22 ;;
esac'
}
snapshot() { cat "$repo/host/versions.env" "$repo/host/sandbox/Dockerfile"; }

t_case "writes the newest release's commit and image"
fresh
stubs
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 0
t_contains "$(cat "$repo/host/versions.env")" "PI_RUKAS_VERSION=v0.2.0"
t_contains "$(cat "$repo/host/versions.env")" "PI_RUKAS_COMMIT=$commit"
t_contains "$(cat "$repo/host/versions.env")" "NODE_VERSION=1"
t_eq "$(sed -n 2p "$repo/host/sandbox/Dockerfile")" "FROM ghcr.io/up-owner/pi-rukas:v0.2.0@$digest"
t_contains "$(t_calls gh)" "repos/Up-Owner/pi-rukas/releases"
t_contains "$(t_calls curl)" "ghcr.io/v2/up-owner/pi-rukas/manifests/v0.2.0"

t_case "a second run finds the pin at the newest release and changes nothing"
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 0
t_contains "$t_out" "newest release, v0.2.0"
t_eq "$(snapshot)" "$before"

t_case "--check reports a newer release and changes nothing"
fresh
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh --check"
t_eq "$t_rc" 1
t_contains "$t_out" "v0.2.0"
t_eq "$(snapshot)" "$before"

t_case "no release found changes nothing"
fresh
before=$(snapshot)
T_ENV=(GH_RC=1)
t_run "$repo/scripts/rukas-latest.sh"
T_ENV=()
t_eq "$t_rc" 2
t_contains "$t_out" "no published release"
t_eq "$(snapshot)" "$before"

t_case "no image for the release changes nothing"
fresh
before=$(snapshot)
T_ENV=(CURL_RC=1)
t_run "$repo/scripts/rukas-latest.sh"
T_ENV=()
t_eq "$t_rc" 2
t_contains "$t_out" "no image"
t_eq "$(snapshot)" "$before"

t_case "a repository that is not on GitHub stops before any call"
fresh
sed -i 's#^PI_RUKAS_REPO=.*#PI_RUKAS_REPO=https://example.org/pi-rukas.git#' "$repo/host/versions.env"
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 2
t_eq "$(t_calls gh)" ""
t_eq "$(snapshot)" "$before"

t_case "a release that lacks a setting tehas passes is refused and nothing changes"
fresh
echo 'x=PI_ENSEMBLE_GONE' >>"$repo/lib/config.sh"
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 3
t_contains "$t_out" "lacks what tehas relies on: PI_ENSEMBLE_GONE"
t_eq "$(snapshot)" "$before"

t_case "a release whose state file has no end tehas acts on is refused"
fresh
sed -i 's/"handoff" | //' "$src/extension/driver.ts"
tar -czf "$T/src.tgz" -C "$T/src" .
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh --check"
t_eq "$t_rc" 3
t_contains "$t_out" '"handoff"'
t_eq "$(snapshot)" "$before"
source_tree

t_case "a release with no reader for the images a cycle is given is refused"
fresh
rm -f "$src/extension/src/work-driver-issue-images.ts"
tar -czf "$T/src.tgz" -C "$T/src" .
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 3
t_contains "$t_out" "lacks what tehas relies on: the issue image reader extension/src/work-driver-issue-images.ts"
t_eq "$(snapshot)" "$before"
source_tree

t_case "no source for the release changes nothing"
fresh
rm "$T/src.tgz"
before=$(snapshot)
t_run "$repo/scripts/rukas-latest.sh"
t_eq "$t_rc" 2
t_contains "$t_out" "no source"
t_eq "$(snapshot)" "$before"
source_tree

t_case "an unknown argument is a usage error"
t_run "$repo/scripts/rukas-latest.sh --force"
t_eq "$t_rc" 64

t_done
