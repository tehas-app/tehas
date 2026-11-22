#!/usr/bin/env bash
# Tests for install.sh, split from install_settings_test.sh: the generic Pi
# settings, and what Pi 1.5.0 leaves is drift until an install.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "with no settings.json in the instance, Pi gets the generic settings and the instance's session model"
fresh
instance
rm -f "$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.defaultProvider, .defaultModel]' "$home/.pi/agent/settings.json")" '["test-provider","test-model"]'
t_eq "$(jq -cS 'del(.defaultProvider, .defaultModel)' "$home/.pi/agent/settings.json")" "$(jq -cS . "$T_ROOT/host/pi/settings.json")"

t_case "a second install changes nothing in Pi's files"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "installed: $home/.pi/agent/"

# test/fixtures/pi-1.5.0: settings.json as Pi 1.5.0 wrote it over this install
# once a person picked another model, and settings-v0.25.2.json over v0.25.2's,
# where httpIdleTimeoutMs was nested and Pi 1.5 wrote it at the top level too.
fx=$T_ROOT/test/fixtures/pi-1.5.0
t_case "--check does not call drift the settings.json and mcp.json Pi 1.5.0 leaves"
fresh
instance
rm -f "$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(stat -c %a "$home/.pi/agent/mcp.json")" 600
cp "$fx/settings.json" "$home/.pi/agent/settings.json"
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"
t_lacks "$t_out" "settings.json"
t_eq "$(cat "$home/.pi/agent/settings.json")" "$(cat "$fx/settings.json")"

t_case "a tehas key changed in the file Pi left is still drift"
jq '.retry.maxRetries = 3' "$fx/settings.json" >"$home/.pi/agent/settings.json"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $home/.pi/agent/settings.json"

t_case "a real install puts the instance's session model back over the one Pi picked"
cp "$fx/settings.json" "$home/.pi/agent/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.defaultProvider, .defaultModel]' "$home/.pi/agent/settings.json")" '["test-provider","test-model"]'

t_case "what v0.25.2 and Pi 1.5.0 left is drift until an install, and passes the check after it"
cp "$fx/settings-v0.25.2.json" "$home/.pi/agent/settings.json"
chmod 644 "$home/.pi/agent/mcp.json"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $home/.pi/agent/settings.json"
t_contains "$t_out" "mode: $home/.pi/agent/mcp.json is 644, wants 600"
t_run "$install"
t_eq "$t_rc" 0
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"
t_eq "$(jq -c '[.httpIdleTimeoutMs, .retry.provider.httpIdleTimeoutMs]' "$home/.pi/agent/settings.json")" '[120000,null]'

t_done
