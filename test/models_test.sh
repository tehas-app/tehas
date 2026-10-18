#!/usr/bin/env bash
# Tests for lib/models.sh: the token names models.json may give, which endpoint
# a provider is picked on, and the models.json a container is given. A stub
# curl answers each host from $T/fx/<host>.json and keeps the curl config it
# was handed, where the token is.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

mkdir -p "$T/fx" "$T/home/.pi/agent"
t_stub curl 'while [ "$#" -gt 0 ]; do
  case "$1" in
    -K) cat "$2" >>"$T/calls/curl-config"; shift ;;
    http*) url=$1 ;;
  esac
  shift
done
h=${url#*://}; h=${h%%/*}
cat "$T/fx/$h.json" 2>/dev/null'
lib=". $T_ROOT/lib/config.sh; tehas_load || exit 78; . $T_ROOT/lib/models.sh"
file=$T/home/.pi/agent/models.json
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_MODEL_KEY_MAIN=main-secret TEHAS_MODEL_KEY_FLEET=fleet-secret)

models() { # <test-provider's object as JSON>
  jq -n --argjson p "$1" '{providers: {"test-provider": ($p + {models: [{id: "test-model"}]})}}' >"$file"
}
both='{"baseUrl": "https://main.test/v1", "apiKey": "$TEHAS_MODEL_KEY_MAIN",
  "backupBaseUrl": "https://fleet.test/v1", "backupApiKey": "$TEHAS_MODEL_KEY_FLEET"}'
offers() { echo '{"data":[{"id":"test-model"}]}' >"$T/fx/$1.json"; }
down() { rm -f "$T/fx/$1.json"; }
fresh() { rm -f "$T"/calls/*; }

t_case "an apiKey is a word or a TEHAS_MODEL_KEY_ name that is set"
while IFS='|' read -r key rc; do
  models "{\"baseUrl\": \"https://main.test/v1\", \"apiKey\": \"$key\"}"
  t_run "$lib; tehas_model_keys_ok"
  t_eq "$key:$t_rc" "$key:$rc"
done <<'CASES'
unused|0
$TEHAS_MODEL_KEY_MAIN|0
${TEHAS_MODEL_KEY_MAIN}|0
$TEHAS_MODEL_KEY_UNSET|78
$GH_TOKEN|78
!cat /etc/passwd|78
pre$TEHAS_MODEL_KEY_MAIN|78
CASES
models '{"baseUrl": "https://main.test/v1", "apiKey": "unused", "backupApiKey": "$HOME"}'
t_run "$lib; tehas_model_keys_ok"
t_eq "$t_rc" 78
t_contains "$t_out" "provider test-provider"

t_case "the key names are listed once each"
models "$both"
t_run "$lib; tehas_model_key_names"
t_eq "$t_out" $'TEHAS_MODEL_KEY_FLEET\nTEHAS_MODEL_KEY_MAIN'

t_case "a provider without a backup is on its main endpoint, and nothing is asked"
models '{"baseUrl": "https://main.test/v1", "apiKey": "unused"}'
fresh
t_run "$lib; tehas_endpoint_pick test-provider"
t_eq "$t_out" main
t_eq "$(t_calls curl)" ""

t_case "the main endpoint is picked while it offers the model, with its own token"
models "$both"
offers main.test
offers fleet.test
fresh
t_run "$lib; tehas_endpoint_pick test-provider"
t_eq "$t_out" main
t_contains "$(t_calls curl)" "https://main.test/v1/models"
t_lacks "$(t_calls curl)" "fleet.test"
t_eq "$(cat "$T/calls/curl-config")" 'header = "Authorization: Bearer main-secret"'
t_lacks "$(t_calls curl)" "secret"

t_case "the backup is picked when the main endpoint does not answer or does not offer the model"
for main in down other; do
  models "$both"
  down main.test
  [ "$main" = down ] || echo '{"data":[{"id":"other"}]}' >"$T/fx/main.test.json"
  offers fleet.test
  fresh
  t_run "$lib; tehas_endpoint_pick test-provider"
  t_eq "$main:$t_out" "$main:backup"
  t_contains "$(cat "$T/calls/curl-config")" 'Bearer fleet-secret'
  t_lacks "$(t_calls curl)" "secret"
done

t_case "with neither answering the main endpoint is kept"
models "$both"
down main.test
down fleet.test
t_run "$lib; tehas_endpoint_pick test-provider"
t_eq "$t_out" main

t_case "backupApiKey defaults to apiKey"
models '{"baseUrl": "https://main.test/v1", "apiKey": "$TEHAS_MODEL_KEY_MAIN", "backupBaseUrl": "https://fleet.test/v1"}'
t_run "$lib; tehas_endpoint_key test-provider backup"
t_eq "$t_out" '$TEHAS_MODEL_KEY_MAIN'

t_case "a word as the key sends no token"
models '{"baseUrl": "https://main.test/v1", "apiKey": "unused", "backupBaseUrl": "https://fleet.test/v1"}'
offers main.test
fresh
t_run "$lib; tehas_endpoint_pick test-provider"
t_eq "$t_out" main
t_eq "$(cat "$T/calls/curl-config" 2>/dev/null)" ""

t_case "the resolved models.json is on the picked endpoint, names the key, and has no backup keys"
models "$both"
down main.test
offers fleet.test
t_run "$lib; tehas_models_resolve $T/out.json"
t_eq "$t_rc" 0
t_eq "$t_out" "test-provider backup"
t_eq "$(jq -c '.providers."test-provider" | [.baseUrl, .apiKey, has("backupBaseUrl"), has("backupApiKey"), .models[0].id]' "$T/out.json")" \
  '["https://fleet.test/v1","$TEHAS_MODEL_KEY_FLEET",false,false,"test-model"]'
t_lacks "$(cat "$T/out.json")" "secret"
offers main.test
t_run "$lib; tehas_models_resolve $T/out.json"
t_eq "$t_out" ""
t_eq "$(jq -c '.providers."test-provider" | [.baseUrl, .apiKey, has("backupBaseUrl")]' "$T/out.json")" \
  '["https://main.test/v1","$TEHAS_MODEL_KEY_MAIN",false]'

t_case "loading the instance keeps the model keys and refuses one set in instance.env"
t_run ". $T_ROOT/lib/config.sh; tehas_load; echo \"\$TEHAS_MODEL_KEY_MAIN\""
t_eq "$t_out" main-secret
t_instance "${T_MINIMAL[@]}" TEHAS_MODEL_KEY_MAIN=in-the-repo
t_run ". $T_ROOT/lib/config.sh; tehas_load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MODEL_KEY_"
t_lacks "$t_out" "in-the-repo"

t_done
