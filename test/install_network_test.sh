#!/usr/bin/env bash
# Tests for install.sh: the installed console is asked for the previews'
# network and what it says reaches the run, and the network is left alone when
# it does not answer. What the console decides is in console/ops/src/edge.rs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

t_case "the installed console makes the network Docker has none of, and its line reaches the run"
fresh
instance
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") exit 1 ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "$net_inspect
network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
t_lacks "$t_out" "tehas-previews"
fresh
instance
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect"|"network create") exit 1 ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews could not be made"
t_stub docker "$net_good"

t_case "a console that does not answer leaves the network alone, and the run says so"
fresh
instance
t_env CONSOLE_BIN=
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews was not made or checked: the console did not answer."
t_eq "$(t_calls docker)" ""
t_env

t_done
