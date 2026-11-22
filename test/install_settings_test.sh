#!/usr/bin/env bash
# Tests for install.sh: an invalid instance or model list installs nothing,
# and an instance gives Pi its own settings. One of four files with
# install_settings_pi, _models and _console.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "an instance with a missing setting installs nothing and exits 78"
fresh
t_instance TEHAS_GITHUB_APP_ID=1 TEHAS_CONTROL_ISSUE=1
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

t_case "an apiKey that is neither a word nor a TEHAS_MODEL_KEY_ name is a finding, as the console's own check"
fresh
instance
for key in '$GH_TOKEN' '!cat /etc/passwd' 'pre$TEHAS_MODEL_KEY_MAIN'; do
  jq --arg k "$key" '.providers."test-provider".backupApiKey = $k' "$TEHAS_INSTANCE_DIR/models.json" >"$T/m.json"
  cp "$TEHAS_INSTANCE_DIR/models.json" "$T/good.json"
  mv "$T/m.json" "$TEHAS_INSTANCE_DIR/models.json"
  t_run "$install --check"
  t_eq "$key:$t_rc" "$key:78"
  t_contains "$t_out" "provider test-provider has an apiKey that is neither a word nor"
  mv "$T/good.json" "$TEHAS_INSTANCE_DIR/models.json"
done

t_case "a model token in ~/.config/tehas/env is found when the shell running install.sh does not have it"
# A first install has no env.sh, and an update's loader may drop the token.
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

t_case "the old app settings stop the install, and each is listed with why"
fresh
instance TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=old-bot[bot]"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_BOT_LOGIN is no longer read: the bot is the GitHub App's login"
t_contains "$t_out" "tehas: TEHAS_GITHUB_APP_INSTALLATION is no longer read: the installation is looked up"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "TEHAS_BOT_LOGIN is no longer read"
t_contains "$t_out" "TEHAS_GITHUB_APP_INSTALLATION is no longer read"

t_case "a 0.30 instance: every line tehas 1.0 renamed or removed is listed, and nothing is installed"
fresh
instance TEHAS_TICKET_CONCURRENCY=3 TEHAS_OWNER=alice TEHAS_EASY_MERGE=0 TEHAS_INDEXING=on
mkdir -p "$home/.config/tehas" && printf 'TEHAS_INDUSTRY_VERSION=v0.9.0\n' >"$home/.config/tehas/env"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "TEHAS_INDEXING is no longer read: tehas 1.0 renamed it TEHAS_SITE_INDEXING"
t_contains "$t_out" "TEHAS_TICKET_CONCURRENCY is no longer read: tehas 1.0 renamed it TEHAS_MAX_CYCLES"
t_contains "$t_out" "TEHAS_OWNER is no longer read: tehas 1.0 renamed it TEHAS_ASSIGNEE"
t_lacks "$t_out" "TEHAS_INDUSTRY_VERSION"
t_lacks "$t_out" "TEHAS_EASY_MERGE"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "renamed it TEHAS_MAX_CYCLES"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f ! -path '*/.config/tehas/env' | wc -l)" 0
rm -f "$home/.config/tehas/env"
instance TEHAS_MAX_CYCLES=3 TEHAS_ASSIGNEE=alice TEHAS_SITE_INDEXING=on
t_run "$install --check"
t_lacks "$t_out" "INVALID"

t_case "a refused token next to the old app settings is named first and installs nothing"
fresh
instance TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=old-bot[bot]"
mkdir -p "$home/.config/tehas" && printf 'GH_CYCLE_TOKEN=ghp_old\n' >"$home/.config/tehas/env"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: GH_CYCLE_TOKEN is no longer used"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f ! -path '*/.config/tehas/env' | wc -l)" 0
rm -f "$home/.config/tehas/env"

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
t_contains "$t_out" "TEHAS_PREVIEWS is on and TEHAS_DOMAIN is not set"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "settings anyone can write are a finding"
instance
chmod o+w "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "is writable by everyone"
chmod o-w "$TEHAS_INSTANCE_DIR/instance.env"

# Checked before it is sourced: refused, not run.
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

t_case "an instance's own Pi settings are used when it has them, and the session model it leaves out comes from the instance"
fresh
instance
echo '{"defaultProvider":"other-provider","hideThinkingBlock":false}' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -cS . "$home/.pi/agent/settings.json")" \
  '{"defaultModel":"test-model","defaultProvider":"other-provider","hideThinkingBlock":false}'

t_case "an instance's Pi settings that are not JSON install nothing and exit 78"
fresh
instance
echo '{"defaultProvider":' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: $TEHAS_INSTANCE_DIR/settings.json is not valid JSON; nothing was installed"
t_eq "$(installed_files | grep -cv '^./tehas/state/')" 0

t_done
