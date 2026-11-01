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

t_case "an instance's own Pi settings are used when it has them, and the session model it leaves out comes from the instance"
fresh
instance
echo '{"defaultProvider":"other-provider","hideThinkingBlock":false}' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -cS . "$home/.pi/agent/settings.json")" \
  '{"defaultModel":"test-model","defaultProvider":"other-provider","hideThinkingBlock":false}'

t_case "with no settings.json in the instance, Pi gets the generic settings and the instance's session model"
fresh
instance
rm -f "$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.defaultProvider, .defaultModel]' "$home/.pi/agent/settings.json")" '["test-provider","test-model"]'
t_eq "$(jq -c 'del(.defaultProvider, .defaultModel)' "$home/.pi/agent/settings.json")" "$(jq -c . "$T_ROOT/host/pi/settings.json")"

t_case "a second install changes nothing in Pi's files"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "installed: $home/.pi/agent/"

t_case "a model list with only id, baseUrl, api and apiKey gets the other keys filled"
fresh
instance
cat >"$TEHAS_INSTANCE_DIR/models.json" <<'JSON'
{"providers": {"test-provider": {"baseUrl": "http://m.test/v1", "api": "openai-completions", "apiKey": "unused",
  "models": [{"id": "test-model"}, {"id": "session-model", "name": "Named", "input": ["text", "image"], "reasoning": true}]}}}
JSON
t_run "$install"
t_eq "$t_rc" 0
m=$home/.pi/agent/models.json
t_eq "$(jq -c '.providers["test-provider"] | [.baseUrl, .api, .apiKey]' "$m")" '["http://m.test/v1","openai-completions","unused"]'
t_eq "$(jq -c '.providers["test-provider"].compat' "$m")" '{"supportsDeveloperRole":false,"supportsReasoningEffort":false}'
t_eq "$(jq -c '.providers["test-provider"].models[0] | [.name, .input, .reasoning]' "$m")" '["test-model",["text"],false]'
t_eq "$(jq -c '.providers["test-provider"].models[0].cost' "$m")" '{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}'
t_case "a key the model list gives is kept"
t_eq "$(jq -c '.providers["test-provider"].models[1] | [.name, .input, .reasoning]' "$m")" '["Named",["text","image"],true]'

t_case "a product's .pi/settings.json is not required, and one that names a listed model still installs"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
mkdir -p "$th/repos/widget/.pi"
echo '{"defaultProvider":"test-provider","defaultModel":"session-model"}' >"$th/repos/widget/.pi/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -r .defaultModel "$home/.pi/agent/settings.json")" test-model

# tehas_load asks a console, and a checkout has none: the tests name one in
# TEHAS_SETTINGS_CMD, and these cases take that name away, as a host has none.
no_hook='unset TEHAS_SETTINGS_CMD; '
real_console

t_case "a real run builds this tree's console before it reads the settings"
fresh
instance
t_run "$no_hook$install"
t_eq "$t_rc" 0
t_contains "$(t_calls cargo | head -n 1)" "-p console"
t_eq "$([ -x "$th/libexec/console" ] && echo installed)" installed

t_case "the built console judges the settings, and a refusal installs nothing"
fresh
instance TEHAS_WORK_MEMORY=8g
t_run "$no_hook$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORK_MEMORY is set"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a console that does not build stops the run before the settings, with nothing installed"
fresh
instance
t_env CARGO_FAIL=console
t_run "$no_hook$install"
t_env
t_eq "$t_rc" 1
t_contains "$t_out" "the console did not build"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "the home's old console does not judge a tree it did not come from"
fresh
instance
mkdir -p "$th/libexec"
printf '#!%s\necho old-console-ran >&2\nexit 2\n' "$T_BASH" >"$th/libexec/console"
chmod +x "$th/libexec/console"
t_run "$no_hook$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "old-console-ran"

t_case "a later command of the run does not keep the console of the build"
t_contains "$(sed -n '/^tehas_load$/,/^instance=/p' "$install")" 'unset TEHAS_SETTINGS_CMD'

t_case "--check builds nothing: with no console anywhere the settings are not judged, and it says so"
fresh
instance
rm -rf "$T/target"
t_run "$no_hook$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "no console at $th/libexec/console to load the settings: run install.sh"
t_eq "$(t_calls cargo)" ""

t_case "--check takes the console an earlier build left"
fresh
instance
mkdir -p "$T/target/release"
cp "$T_CONSOLE" "$T/target/release/console"
t_run "$no_hook$install --check"
t_lacks "$t_out" "INVALID"
t_eq "$(t_calls cargo)" ""

t_case "--check with no build takes the installed console"
fresh
instance
rm -rf "$T/target"
mkdir -p "$th/libexec"
cp "$T_CONSOLE" "$th/libexec/console"
t_run "$no_hook$install --check"
t_lacks "$t_out" "INVALID"
t_eq "$(t_calls cargo)" ""

t_done
