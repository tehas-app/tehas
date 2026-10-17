#!/usr/bin/env bash
# Tests for install.sh: the code index a real run builds when it finds the
# sandbox image and the product checkout, and never --check.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

# The code index: built by the first real run that finds both the sandbox
# image and the product checkout, and never by --check.

product() {
  mkdir -p "$T/git/acme"
  [ -d "$T/git/acme/widget" ] || {
    git init -q -b main "$T/git/acme/widget"
    git -C "$T/git/acme/widget" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
  }
  git clone -q "$T/git/acme/widget" "$th/repos/widget"
}
index_runs() { t_calls docker | grep -c -e '^run .*index_repository' || true; }
# tehas index starts nothing without the slice: a host that has it.
slice_state 0.00
slice_env="TEHAS_CGROUP_ROOT=$T_CGROUP"
t_stub systemctl '[ "$1" != show ] || printf "%s\n" FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592'
t_stub docker 'case "$1" in
  info) echo "systemd 2" ;;
  version) echo 29.1.3 ;;
  network) echo "true bridge false isolated" ;;
  image) exit "${IMAGE_RC:-0}" ;;
  run) [ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"; echo "{\"status\":\"indexed\",\"nodes\":1,\"edges\":2}" ;;
esac'

t_case "an install without the product checkout names the indexing step"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 0

t_case "an install without the sandbox image names the indexing step"
product
t_env IMAGE_RC=1 "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 0

t_case "--check builds no index"
t_env
t_run "$install --check"
t_eq "$(index_runs)" 0
t_eq "$(find "$th/state/index.json" 2>/dev/null | wc -l)" 0

t_case "a failed indexing is a warning, and the install still succeeds"
t_env RUN_RC=1 "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not built"
t_contains "$t_out" "later: $th/bin/tehas index"

t_case "with the image and the checkout, the install builds the index"
rm -f "$T"/calls/*
t_env "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas index: widget indexed"
t_lacks "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 1
t_eq "$(jq -r .nodes "$th/state/index.json")" 1

t_case "a later install leaves the index alone"
t_run "$install"
t_contains "$t_out" "tehas is up to date"
t_eq "$(index_runs)" 1
t_stub docker "$net_good"
t_stub systemctl
t_env

t_done
