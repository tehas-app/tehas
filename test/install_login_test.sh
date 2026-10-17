#!/usr/bin/env bash
# Tests for install.sh: a stale copy of the command root installed for
# the other accounts is reported and left alone.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

# --- the command root installed for the accounts that are not the tehas user ---
# bootstrap.sh dev copies host/login/tehas to /usr/local/bin/tehas and leaves
# /etc/tehas/manager naming the tehas user. install.sh cannot replace a root's
# file: it reports the difference and names the command that does.
login_dev() { # what bootstrap.sh dev left: the marker and the command
  mkdir -p "$dev_etc/tehas" "$dev_bin"
  echo boss >"$dev_etc/tehas/manager"
  install -m 755 "$T_ROOT/host/login/tehas" "$dev_bin/tehas"
}
login_state() { cksum "$dev_bin/tehas"; }
t_stub sudo

t_case "a host that never ran bootstrap.sh dev reports nothing about it"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_lacks "$t_out" "bootstrap.sh dev"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_eq "$(t_calls sudo)" ""

t_case "install.sh puts nothing there on its own"
t_eq "$([ -e "$dev_bin" ] && echo there)" ""
t_eq "$([ -e "$dev_etc/tehas/manager" ] && echo there)" ""

t_case "a host where it matches this checkout reports nothing"
fresh
instance
login_dev
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_lacks "$t_out" "bootstrap.sh dev"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"

t_case "--check names a stale copy and the command that replaces it, and exits 1"
echo "# the copy an earlier bootstrap.sh dev installed" >>"$dev_bin/tehas"
before=$(login_state)
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Login:        NOT CURRENT"
t_contains "$t_out" "differs: $dev_bin/tehas"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev boss"
# Not drift install.sh can repair, so it is not counted as drift.
t_contains "$t_out" "Installed:    OK"
t_eq "$(login_state)" "$before"
t_eq "$(t_calls sudo)" ""

t_case "a real install names the command, writes nothing there and records no step"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: sudo $T_ROOT/bootstrap.sh dev boss"
# Not a step of tehas update: nothing was recorded as owed.
t_eq "$(cat "$th/state/update-pending" 2>/dev/null)" ""
t_eq "$(login_state)" "$before"
t_eq "$(t_calls sudo)" ""
# The next --check still says it, because nothing here cleared it.
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Login:        NOT CURRENT"

t_case "the command names the tehas user the marker holds"
echo someone >"$dev_etc/tehas/manager"
t_run "$install --check"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev someone"
echo boss >"$dev_etc/tehas/manager"

t_case "a marker that does not hold a plain account name is not carried into the line"
echo 'boss; rm -rf /' >"$dev_etc/tehas/manager"
t_run "$install --check"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev <tehas user>"
t_lacks "$t_out" "rm -rf"
echo boss >"$dev_etc/tehas/manager"

t_case "the marker there and the command gone is named as missing"
rm -f "$dev_bin/tehas"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "missing: $dev_bin/tehas"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev boss"


t_done
