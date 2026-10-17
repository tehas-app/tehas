#!/usr/bin/env bash
# Tests for install.sh: a previews' network that does not keep the host
# out is left, and the run says why and that it has to be remade.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

t_case "a network that does not keep the host out is left, and the run says why and that it has to be remade"
while IFS='|' read -r answer reason; do
  fresh
  instance
  t_stub docker "case \"\$1 \$2\" in \"version --format\") echo 27.1.0 ;; \"network inspect\") echo \"$answer\" ;; esac"
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls docker)" "$net_inspect"
  t_contains "$t_out" "The network tehas-previews does not keep the host out ($reason). It has to be remade"
  t_contains "$t_out" "troubleshooting"
done <<'CASES'
true bridge false |it is not isolated from the host
true bridge false nat|it is not isolated from the host
false bridge false isolated|it is not internal
true bridge true isolated|it has IPv6
true ipvlan false isolated|its driver is not bridge
CASES
t_stub docker "$net_good"


t_done
