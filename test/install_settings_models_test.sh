#!/usr/bin/env bash
# Tests for install.sh, split from install_settings_test.sh: settings made for
# Pi 1.4, a short model list filled, and a product's own Pi settings.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_case "an instance's settings.json made for Pi 1.4 gets httpIdleTimeoutMs where Pi 1.5 reads it"
fresh
instance
echo '{"retry":{"provider":{"maxRetries":8,"httpIdleTimeoutMs":90000}}}' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.httpIdleTimeoutMs, .retry.provider]' "$home/.pi/agent/settings.json")" '[90000,{"maxRetries":8}]'

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
t_eq "$(jq -cS '.providers["test-provider"].models[0].cost' "$m")" '{"cacheRead":0,"cacheWrite":0,"input":0,"output":0}'
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

t_done
