#!/usr/bin/env bash
# Tests for install.sh and outsource: the model proxy, its unit and the
# contractor program are built and installed only while TEHAS_OUTSOURCE is on.
# Outsource is still being built, so a release never turns it on; here the
# test variable does, as every test of a building feature does.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

outsource_files="./.config/systemd/user/tehas-outsource-proxy.service
./tehas/libexec/tehas-contractor
./tehas/libexec/tehas-contractor.stamp
./tehas/libexec/tehas-proxy
./tehas/libexec/tehas-proxy.stamp"

t_case "with outsource off nothing of it is built or installed"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(core_files | grep -e tehas-proxy -e tehas-contractor -e outsource)" ""
t_lacks "$(t_calls cargo)" "tehas-proxy"
t_lacks "$(t_calls cargo)" "tehas-contractor"
t_eq "$(sed -n 2p "$T/calls/systemctl")" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"

t_case "with outsource on the proxy, its unit and the contractor are installed and the unit enabled"
fresh
instance TEHAS_OUTSOURCE=on
t_env TEHAS_TEST_FEATURES=outsource
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(core_files | grep -e tehas-proxy -e tehas-contractor -e outsource)" "$outsource_files"
t_eq "$(cat "$th/libexec/tehas-proxy")" "a tehas-proxy built from source"
t_eq "$(cat "$th/libexec/tehas-contractor")" "a tehas-contractor built from source"
t_eq "$([ -x "$th/libexec/tehas-proxy" ] && [ -x "$th/libexec/tehas-contractor" ] && echo both)" "both"
t_eq "$(cmp -s "$fixture/host/systemd/tehas-outsource-proxy.service" \
  "$home/.config/systemd/user/tehas-outsource-proxy.service" && echo same)" "same"
t_eq "$(t_calls cargo | grep -c -e '-p tehas-proxy$' -e '-p tehas-contractor$')" 2
t_eq "$(sed -n 2p "$T/calls/systemctl")" \
  "--user enable --now tehas-dispatch.timer tehas-dispatch.path tehas-outsource-proxy.service"
# The unit's file asks for the proxy's restart with the rest of the units.
t_contains "$t_out" "tehas-clean.timer tehas-outsource-proxy.service"

t_case "with outsource on a second run changes nothing and --check finds it installed"
t_run "$install"
t_contains "$t_out" "tehas is up to date"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Installed:    OK"

t_case "with outsource on a changed env.sh restarts the proxy with the other units"
echo "# an older env.sh" >>"$th/env.sh"
t_env TEHAS_TEST_FEATURES=outsource
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas-clean.timer tehas-outsource-proxy.service"
t_contains "$(cat "$th/state/update-pending")" "tehas-clean.timer tehas-outsource-proxy.service"
rm -f "$th/state/update-pending"
t_env

t_case "with outsource on a changed proxy is built again and its unit is asked to restart"
rm -f "$T"/calls/*
t_env TEHAS_TEST_FEATURES=outsource CONSOLE_BUILD=a-newer-source
# The stamp names the source the proxy was built from; another one is due.
echo "0000 0000" >"$th/libexec/tehas-proxy.stamp"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(cat "$th/libexec/tehas-proxy")" "a tehas-proxy built from a-newer-source"
t_contains "$t_out" "next: units tehas-outsource-proxy.service"
t_env

t_case "the proxy's unit runs tehasbot outsource-proxy, which runs the installed proxy under the settings"
t_contains "$(cat "$home/.config/systemd/user/tehas-outsource-proxy.service")" \
  'exec %h/tehas/bin/tehasbot outsource-proxy"'
printf '#!/bin/sh\necho "proxy ran: outsource=$TEHAS_OUTSOURCE repo=$TEHAS_REPO args=$#"\n' >"$th/libexec/tehas-proxy"
t_env TEHAS_TEST_FEATURES=outsource
t_run "'$th/bin/tehasbot' outsource-proxy"
t_eq "$t_rc" 0
t_eq "$t_out" "proxy ran: outsource=1 repo=acme/widget args=0"
rm -f "$th/libexec/tehas-proxy"
t_run "'$th/bin/tehasbot' outsource-proxy"
t_eq "$t_rc" 78
t_contains "$t_out" "tehasbot outsource-proxy: no tehas-proxy at $th/libexec/tehas-proxy (run install.sh)"
t_env

t_case "the units step accepts the proxy's unit"
t_run "'$T_CONSOLE' update steps valid 'units tehas-outsource-proxy.service'"
t_eq "$t_rc" 0

t_case "with outsource off a changed env.sh asks for the units without the proxy"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
echo "# an older env.sh" >>"$th/env.sh"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: units "
t_lacks "$t_out" "tehas-outsource-proxy.service"

t_done
