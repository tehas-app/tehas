#!/usr/bin/env bash
# Tests for install.sh: the previews' network made where there is none, and
# not asked for of a Docker older than 27.1 or one whose version cannot be read.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "a host without the previews' network gets it, internal and with no address for the host"
fresh
instance
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") exit 1 ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "$net_inspect
network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
t_lacks "$t_out" "tehas-previews"
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect"|"network create") exit 1 ;; esac'
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews could not be made"
t_stub docker "$net_good"


t_case "a Docker older than 27.1 is not asked for the network, and the run says why"
while IFS='|' read -r version reason; do
  fresh
  instance
  t_stub docker "case \"\$1 \$2\" in \"version --format\") echo \"$version\" ;; \"network inspect\") exit 1 ;; esac"
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls docker)" "version --format {{.Server.Version}}"
  t_contains "$t_out" "The network tehas-previews was not made or checked: $reason."
done <<'CASES'
26.1.4|Docker 26.1.4 is older than 27.1
27.0.3|Docker 27.0.3 is older than 27.1
|Docker's version could not be read
CASES
t_stub docker 'exit 1'
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews was not made or checked: Docker's version could not be read."
t_eq "$(t_calls docker | grep -c '^network')" 0
t_stub docker "$net_good"


t_done
