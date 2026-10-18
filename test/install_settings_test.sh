#!/usr/bin/env bash
# Tests for install.sh: an invalid instance or model list installs
# nothing, and an instance gives Pi its own settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "an instance with a missing setting installs nothing and exits 78"
fresh
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "no instance at all installs nothing and exits 78"
rm -rf "$T/instance"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env in"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a model missing from models.json is a finding"
fresh
instance PI_ENSEMBLE_SUBAGENT_MODEL=other-model
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json does not list model other-model under provider test-provider"

t_case "a role's model missing from models.json is a finding, under the provider it would use"
fresh
instance PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_MODEL_EXPLORE=session-model
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json does not list model big-model under provider test-provider"
t_lacks "$t_out" "model session-model"
t_lacks "$t_out" "model test-model"

t_case "a model token models.json names and the host has not set is a finding; one that is set is not"
fresh
instance
jq '.providers."test-provider" += {apiKey: "$TEHAS_MODEL_KEY_MAIN", backupBaseUrl: "https://fleet.test/v1"}' \
  "$TEHAS_INSTANCE_DIR/models.json" >"$T/m.json" && mv "$T/m.json" "$TEHAS_INSTANCE_DIR/models.json"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json names TEHAS_MODEL_KEY_MAIN and it is not set"
T_ENV+=(TEHAS_MODEL_KEY_MAIN=main-secret)
t_run "$install --check"
t_lacks "$t_out" "TEHAS_MODEL_KEY_MAIN"
t_lacks "$t_out" "main-secret"
T_ENV=("${T_ENV[@]:0:${#T_ENV[@]}-1}")

t_case "a model token in ~/.config/tehas/env is found when the shell running install.sh does not have it"
# A first install has no env.sh, and an update runs under the loader of the
# version before, which may drop the token.
mkdir -p "$T/home/.config/tehas"
printf '%s\n' 'GH_TOKEN=gh-secret' 'export TEHAS_MODEL_KEY_MAIN="main-secret"' >"$T/home/.config/tehas/env"
t_run "$install --check"
t_lacks "$t_out" "TEHAS_MODEL_KEY_MAIN"
t_lacks "$t_out" "secret"
rm -f "$T/home/.config/tehas/env"

t_case "an apiKey that names any other variable or a command is a finding"
fresh
instance
jq '.providers."test-provider".apiKey = "!cat ~/.config/tehas/env"' \
  "$TEHAS_INSTANCE_DIR/models.json" >"$T/m.json" && mv "$T/m.json" "$TEHAS_INSTANCE_DIR/models.json"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "provider test-provider has an apiKey that is neither a word nor"

t_case "a role pi-rukas does not know installs nothing"
fresh
instance PI_ENSEMBLE_MODEL_REVIEWER=test-model
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "PI_ENSEMBLE_MODEL_REVIEWER names no role"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a missing models.json is a finding"
instance
rm "$TEHAS_INSTANCE_DIR/models.json"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "no models.json in"

t_case "--check names a budget setting of an earlier version, and exits 78"
instance TEHAS_WORK_MEMORY=8g
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set"
t_lacks "$t_out" "NOT OK"

t_case "the product's session model must be listed too"
instance
mkdir -p "$th/repos/widget/.pi"
echo '{"defaultProvider":"test-provider","defaultModel":"missing-model"}' >"$th/repos/widget/.pi/settings.json"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "does not list the product's session model test-provider/missing-model"
echo '{"defaultProvider":"test-provider","defaultModel":"session-model"}' >"$th/repos/widget/.pi/settings.json"
t_run "$install"
t_eq "$t_rc" 0

t_case "an instance that asks for database clones installs nothing and exits 78"
fresh
instance TEHAS_CLONES=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CLONES is set and this version of tehas has no database clones"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "an instance that asks for previews without a domain installs nothing and exits 78"
fresh
instance TEHAS_PREVIEWS=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "settings anyone can write are a finding"
instance
chmod o+w "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "is writable by everyone"
chmod o-w "$TEHAS_INSTANCE_DIR/instance.env"

# The check comes before the file is sourced: a file another account can
# write is refused, not run.
t_case "instance.env that anyone can write is never sourced"
instance
printf '%s\n' "touch $T/sourced-marker" >"$TEHAS_INSTANCE_DIR/instance.env"
chmod 666 "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "is writable by everyone"
t_eq "$(find "$T" -name sourced-marker | wc -l)" 0

t_case "settings reached through a symlink are judged by the file they point at"
fresh
instance
mv "$TEHAS_INSTANCE_DIR/instance.env" "$T/real-instance.env"
chmod 600 "$T/real-instance.env"
ln -s "$T/real-instance.env" "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_lacks "$t_out" "INVALID"
t_contains "$t_out" "Instance:     $T/instance OK"
rm "$TEHAS_INSTANCE_DIR/instance.env"

t_case "an instance's own Pi settings are used when it has them"
fresh
instance
echo '{"defaultProvider":"test-provider"}' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$(cat "$home/.pi/agent/settings.json")" '{"defaultProvider":"test-provider"}'


t_done
