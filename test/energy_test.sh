#!/usr/bin/env bash
# Tests for libexec/energy, the sampler behind the Energy use section. A stub
# curl plays the mesh: it answers with the status and the body the case set.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

energy="$T_ROOT/libexec/energy"
file=$T/home/tehas/live/energy.json
fixture=$T_ROOT/test/fixtures/cluster.json
url=http://mesh.example:8086/v1/cluster
NOW=1791526545 # a multiple of 15: the start of a slot

mkdir -p "$T/mesh"
t_stub curl '[ ! -e "$T/mesh/down" ] || exit 7
cat "$T/mesh/body"
printf "\n%s" "$(cat "$T/mesh/status")"'

# What the mesh answers from now on: a status, and a body or @<file>.
mesh() {
  rm -f "$T/mesh/down"
  printf '%s' "$1" >"$T/mesh/status"
  case "$2" in @*) cp "${2#@}" "$T/mesh/body" ;; *) printf '%s' "$2" >"$T/mesh/body" ;; esac
}
# A host with no energy file, and a mesh that answers with the fixture.
fresh() {
  rm -rf "$T/home/tehas" "$T/calls/curl"
  mesh 200 "@$fixture"
}
# One sample, taken so many seconds after NOW, with these settings.
sample() {
  local after=$1
  shift
  T_ENV=(TEHAS_ENERGY_URL="$url" TEHAS_ENERGY_NOW="$((NOW + after))" "$@")
  t_run "$energy"
}
# t_json <jq filter> <JSON>: what the energy file holds there. A filter that
# fails or an expectation that is not JSON is a failure, not two empty answers.
t_json() {
  local got want
  got=$(jq -cS "$1" "$file" 2>&1) || { t_bad "jq '$1' failed: $got"; return; }
  want=$(jq -cS . <<<"$2" 2>&1) && [ -n "$want" ] || { t_bad "the expectation is not JSON: $2"; return; }
  t_eq "$got" "$want"
}
asked() { t_calls curl | wc -l; }
there() { [ -e "$file" ] && echo there || echo gone; }

t_case "a cluster snapshot becomes the energy file"
fresh
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_json .generatedAt '"2026-10-09T06:15:45Z"'
t_json '[.source, .periodSeconds, .factoryModels]' '["viiwork", 15, ["model-large"]]'
t_json .sourceUrl '"https://github.com/janit/viiwork"'
# Only the alive members that serve a factory model: node-a serves another
# model, node-c serves none and node-d has left. The totals are theirs alone.
t_json .totals '{"watts": 734, "kWh24h": 1.44, "kWh30d": 55.08, "hosts": 1, "reporting": 1, "energyHosts": 1}'
t_json .hosts '[{"name": "node-b", "source": "nvidia-smi", "watts": 734, "kWh24h": 1.44, "kWh30d": 55.08,
  "ramUsedMb": 52624, "ramTotalMb": 253212}]'
# Only the factory's models, though node-b serves model-small as well.
t_json .models '[{"name": "model-large", "hosts": ["node-b"],
  "engines": [{"version": "2.9.0", "engine": "strata", "engineName": null, "engineVersion": null}], "slots": 1, "busy": 1, "queued": 2}]'
t_json .samples "[{\"t\": $NOW, \"w\": {\"node-b\": 734}}]"

t_case "a sample carries each machine's token count for the factory's models"
fresh
# node-b counts tokens for both of its models; only model-large is the
# factory's. node-a serves another model, and a count that is no number is no
# count.
jq '(.members[] | select(.node == "node-b") | .status.models[])
      |= (.tokens_total = (if .name == "model-large" then 620263.6 else 99 end))
  | (.members[] | select(.node == "node-a") | .status.models[]) |= (.tokens_total = "many")' \
  "$fixture" >"$T/counting.json"
mesh 200 "@$T/counting.json"
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_json .samples "[{\"t\": $NOW, \"w\": {\"node-b\": 734}, \"k\": {\"node-b\": 620263}}]"
sample 15 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_json '[.samples[].k]' '[{"node-b": 620263}, {"node-b": 620263}]'

# What serves a model: the mesh's version and the engine, per distinct pair.
both=(PI_ENSEMBLE_SUBAGENT_MODEL=model-large PI_ENSEMBLE_MODEL_REVIEWER=model-small)
# engines <jq filter over the fixture> <model> <expected array>
engines() {
  fresh
  jq "$1" "$fixture" >"$T/engines.json"
  mesh 200 "@$T/engines.json"
  sample 0 "${both[@]}"
  t_eq "$t_rc" 0
  t_json ".models[] | select(.name == \"$2\") | .engines" "$3"
}
member() { printf '(.members[] | select(.node == "%s") | .status)' "$1"; }

t_case "two machines with the same version and engine are one element"
engines . model-small '[{"version": "2.9.0", "engine": "llamacpp", "engineName": null, "engineVersion": null}]'
t_json '.models[] | select(.name == "model-large") | .engines' '[{"version": "2.9.0", "engine": "strata", "engineName": null, "engineVersion": null}]'

t_case "a mesh in the middle of an upgrade gives one element per version, sorted"
want='[{"version": "2.10.0", "engine": "llamacpp", "engineName": null, "engineVersion": null}, {"version": "2.9.0", "engine": "llamacpp", "engineName": null, "engineVersion": null}]'
engines "$(member node-a).ver = \"v2.10.0\"" model-small "$want"
# The same file whatever order the mesh lists its members in.
engines "$(member node-a).ver = \"v2.10.0\" | .members |= reverse" model-small "$want"

t_case "a model served by two engines gives one element per engine"
engines "$(member node-a).models[0].engine = \"vllm\"" model-small \
  '[{"version": "2.9.0", "engine": "llamacpp", "engineName": null, "engineVersion": null}, {"version": "2.9.0", "engine": "vllm", "engineName": null, "engineVersion": null}]'

t_case "a version that is missing or no text is null"
engines "del($(member node-b).ver)" model-large '[{"version": null, "engine": "strata", "engineName": null, "engineVersion": null}]'
engines "$(member node-b).ver = 29" model-large '[{"version": null, "engine": "strata", "engineName": null, "engineVersion": null}]'
engines "$(member node-b).ver = {\"m\": \"LEAKMARK\"}" model-large '[{"version": null, "engine": "strata", "engineName": null, "engineVersion": null}]'
t_lacks "$(cat "$file")" LEAKMARK

t_case "an engine that is missing, empty or no text is null"
engines "del($(member node-b).models[0].engine)" model-large '[{"version": "2.9.0", "engine": null, "engineName": null, "engineVersion": null}]'
engines "$(member node-b).models[0].engine = \"\"" model-large '[{"version": "2.9.0", "engine": null, "engineName": null, "engineVersion": null}]'
engines "$(member node-b).models[0].engine = [\"LEAKMARK\"]" model-large '[{"version": "2.9.0", "engine": null, "engineName": null, "engineVersion": null}]'
t_lacks "$(cat "$file")" LEAKMARK

# From viiwork v2.9.2 a machine also names the engine for people and says
# which version of it runs. node-a is older here and sends neither.
new="$(member node-b).ver = \"v2.9.2\"
  | $(member node-b).models[0] += {\"engine_name\": \"Strata\"}
  | $(member node-b).models[1] += {\"engine_name\": \"llama.cpp\", \"engine_version\": \"b11371\"}"

t_case "the engine's name and its own version are copied where the mesh reports them"
engines "$new" model-small '[
  {"version": "2.9.0", "engine": "llamacpp", "engineName": null, "engineVersion": null},
  {"version": "2.9.2", "engine": "llamacpp", "engineName": "llama.cpp", "engineVersion": "b11371"}]'
# An engine that reports no version of its own.
t_json '.models[] | select(.name == "model-large") | .engines' \
  '[{"version": "2.9.2", "engine": "strata", "engineName": "Strata", "engineVersion": null}]'

t_case "an engine name that is empty or no text is null"
for odd in '""' 7 '["LEAKMARK"]'; do
  engines "$new | $(member node-b).models[0].engine_name = $odd" model-large \
    '[{"version": "2.9.2", "engine": "strata", "engineName": null, "engineVersion": null}]'
  t_lacks "$(cat "$file")" LEAKMARK
done

t_case "an engine version that is not a plain version is null"
# The file can be on the internet: a path, a host name or a long text is not
# copied, whatever the mesh sends.
long=$(printf 'b%.0s' {1..41})
for odd in '""' 7 '{"m": "LEAKMARK"}' '"b1 /opt/LEAKMARK"' '"LEAKMARK@host"' "\"$long\""; do
  engines "$new | $(member node-b).models[0].engine_version = $odd" model-large \
    '[{"version": "2.9.2", "engine": "strata", "engineName": "Strata", "engineVersion": null}]'
  t_lacks "$(cat "$file")" LEAKMARK
done
engines "$new | $(member node-b).models[0].engine_version = \"0.31.0+cu12_x-1\"" model-large \
  '[{"version": "2.9.2", "engine": "strata", "engineName": "Strata", "engineVersion": "0.31.0+cu12_x-1"}]'

t_case "a machine that reports neither adds no element"
engines "del($(member node-b).ver) | del($(member node-b).models[0].engine)" model-large '[]'
# Its own version alone says nothing about what serves the model.
engines "del($(member node-b).ver) | del($(member node-b).models[0].engine)
  | $(member node-b).models[0].engine_version = \"b1\"" model-large '[]'
# The name alone does.
engines "del($(member node-b).ver) | del($(member node-b).models[0].engine)
  | $(member node-b).models[0].engine_name = \"Strata\"" model-large \
  '[{"version": null, "engine": null, "engineName": "Strata", "engineVersion": null}]'

t_case "a machine with no slots for the model adds nothing"
engines "$(member node-c) |= (.ver = \"v3.0.0\"
  | .models = [{\"name\": \"model-small\", \"engine\": \"vllm\", \"slots\": 0}])" \
  model-small '[{"version": "2.9.0", "engine": "llamacpp", "engineName": null, "engineVersion": null}]'
t_json '.models[] | select(.name == "model-small") | .hosts' '["node-a", "node-b"]'

t_case "the file holds no address, node id or GPU"
fresh
sample 0
text=$(cat "$file")
for secret in "192.0.2." viiwork-0000 vram v2.9.0 llamacpp; do t_lacks "$text" "$secret"; done
# With the factory's models named, the version and the engines are published;
# the rest still is not.
sample 0 "${both[@]}"
text=$(cat "$file")
for secret in "192.0.2." viiwork-0000 vram api_port; do t_lacks "$text" "$secret"; done

t_case "one sample per slot, and none older than 30 minutes"
fresh
sample 0
sample 14 # the same slot: replaces the first
t_json '[.samples[].t]' "[$((NOW + 14))]"
sample 15
sample 45 # a slot was missed: nothing is made up for it
t_json '[.samples[].t]' "[$((NOW + 14)), $((NOW + 15)), $((NOW + 45))]"
sample 1815
t_json '[.samples[].t]' "[$((NOW + 45)), $((NOW + 1815))]"

t_case "a sample from the future is dropped when the clock has gone back"
fresh
sample 3600
sample 0
t_json '[.samples[].t]' "[$NOW]"

t_case "a damaged file is replaced, not fatal"
fresh
mkdir -p "$(dirname "$file")"
printf '%s' '{"samples": [{"t": "soon"}, 7, {"t": 5}' >"$file"
sample 0
t_eq "$t_rc" 0
t_json '.samples | [type, length]' '["array", 1]'
printf '%s' "{\"samples\": [{\"t\": \"soon\"}, 7, {\"t\": $((NOW - 15))}, {\"t\": $((NOW - 30)), \"w\": {}}]}" >"$file"
sample 0
t_json '[.samples[].t]' "[$((NOW - 30)), $NOW]"
# Files that parse but are not an energy file, or hold no single value.
for text in '{"samples":5}' $'  \n' '{"samples":[]} {"samples":[]}'; do
  printf '%s' "$text" >"$file"
  sample 0
  t_eq "$t_rc" 0
  t_json '.samples | [type, length]' '["array", 1]'
done

t_case "a mesh that does not answer leaves the file as it is"
fresh
sample 0
before=$(cat "$file")
touch "$T/mesh/down"
sample 15
t_eq "$t_rc" 0
t_eq "$(cat "$file")" "$before"
for status in 502 408 429; do
  mesh "$status" "try later"
  sample 30
  t_eq "$t_rc" 0
  t_eq "$(cat "$file")" "$before"
done

t_case "an answer that is not a cluster snapshot removes the file"
while IFS='|' read -r status body; do
  fresh
  sample 0
  t_eq "$(there)" there
  mesh "$status" "$body"
  sample 0
  t_eq "$t_rc" 0
  t_eq "$(there) after $status $body" "gone after $status $body"
done <<'EOF'
404|404 page not found
200|{"object":"list","data":[]}
200|[]
200|<html>
EOF

t_case "TEHAS_ENERGY=off removes the file and does not ask the mesh"
fresh
sample 0
t_eq "$(asked)" 1
sample 0 TEHAS_ENERGY=off
t_eq "$t_rc" 0
t_eq "$(there)" gone
t_eq "$(asked)" 1

t_case "the mesh address comes from the model provider"
fresh
models=$T/models.json
echo '{"providers": {"fleet": {"baseUrl": "http://mesh.example:8086/v1/"}, "other": {"baseUrl": "http://other.example:9"}}}' >"$models"
T_ENV=(TEHAS_MODELS_FILE="$models")
t_run "$energy"
t_eq "$(there)" there
t_eq "$(t_calls curl | awk '{ print $NF }')" "$url"
T_ENV=(TEHAS_MODELS_FILE="$models" PI_ENSEMBLE_SUBAGENT_PROVIDER=other)
t_run "$energy"
t_eq "$(t_calls curl | awk 'END { print $NF }')" "http://other.example:9/v1/cluster"
# A provider that is not configured, and no model list at all: no file, and
# nothing is asked.
T_ENV=(TEHAS_MODELS_FILE="$models" PI_ENSEMBLE_SUBAGENT_PROVIDER=missing)
t_run "$energy"
t_eq "$(there)" gone
sample 0
T_ENV=(TEHAS_MODELS_FILE="$T/none.json")
t_run "$energy"
t_eq "$t_rc" 0
t_eq "$(there)" gone
t_eq "$(asked)" 3

t_case "a main endpoint that is not a mesh is passed over for its backup, asked with the backup's token"
t_stub curl 'for a; do case "$a" in /dev/fd/*) cat "$a" >>"$T/calls/curl-config" ;; esac; done
case "$*" in
  *main.example*) [ ! -e "$T/mesh/main-down" ] || exit 7; printf "%s\n404" "not found" ;;
  *) cat "$T/mesh/body"; printf "\n%s" "$(cat "$T/mesh/status")" ;;
esac'
fresh
rm -f "$T/calls/curl-config"
jq -n '{providers: {fleet: {baseUrl: "https://main.example/v1", apiKey: "$TEHAS_MODEL_KEY_MAIN",
  backupBaseUrl: "http://mesh.example:8086/v1", backupApiKey: "$TEHAS_MODEL_KEY_FLEET"}}}' >"$models"
T_ENV=(TEHAS_MODELS_FILE="$models" TEHAS_MODEL_KEY_MAIN=main-secret TEHAS_MODEL_KEY_FLEET=fleet-secret TEHAS_ENERGY_NOW="$NOW")
t_run "$energy"
t_eq "$(there)" there
t_eq "$(t_calls curl | awk '{ print $NF }' | tr '\n' ' ')" "https://main.example/v1/cluster $url "
t_eq "$(cat "$T/calls/curl-config")" 'header = "Authorization: Bearer main-secret"
header = "Authorization: Bearer fleet-secret"'
t_lacks "$(t_calls curl)" "secret"
# The backup busy and the main one not a mesh: the file is kept.
mesh 503 "busy"
t_run "$energy"
t_eq "$(there)" there
# Neither a mesh: the file goes.
mesh 404 "not found"
t_run "$energy"
t_eq "$(there)" gone
t_stub curl '[ ! -e "$T/mesh/down" ] || exit 7
cat "$T/mesh/body"
printf "\n%s" "$(cat "$T/mesh/status")"'

t_case "a role's own model is a factory model too"
fresh
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large PI_ENSEMBLE_MODEL_REVIEWER=model-small \
  PI_ENSEMBLE_MODEL_IDLE= PI_ENSEMBLE_MAX_WORKSTREAMS=1
t_json .factoryModels '["model-large", "model-small"]'
# In name order whatever order the mesh lists them in; two hosts of a model are summed.
t_json '[.hosts[].name]' '["node-a", "node-b"]'
t_json '[.models[] | [.name, .slots]]' '[["model-large", 1], ["model-small", 6]]'

t_case "an empty mesh and odd members give a file with nothing in it"
fresh
mesh 200 '{"members": [null, 7, {"node": 5, "state": "alive", "status": {}}, {"node": "node-x", "state": "alive", "status": {}}]}'
sample 0
t_eq "$t_rc" 0
t_json .totals '{"watts": null, "kWh24h": null, "kWh30d": null, "hosts": 0, "reporting": 0, "energyHosts": 0}'
t_json '[.hosts, .models]' '[[], []]'
t_json .samples "[{\"t\": $NOW, \"w\": {}}]"

t_case "objects where text or numbers belong stay out of the file"
fresh
mesh 200 '{"members": [{"node": "node-x", "state": "alive", "status": {
  "power": {"available": true, "watts": 100, "source": {"m": "LEAKMARK"}},
  "host_mem_used_mb": {"m": "LEAKMARK"},
  "host_mem_total_mb": ["LEAKMARK"],
  "models": [{"name": "model-large", "slots": 1}]}}]}'
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_eq "$t_rc" 0
t_lacks "$(cat "$file")" LEAKMARK
t_json .hosts '[{"name": "node-x", "source": null, "watts": 100, "kWh24h": null, "kWh30d": null,
  "ramUsedMb": null, "ramTotalMb": null}]'

t_case "a negative reading is no reading"
fresh
mesh 200 '{"members": [{"node": "node-x", "state": "alive", "status": {
  "power": {"available": true, "watts": -5, "source": "rapl"},
  "models": [{"name": "model-large", "slots": 1}]}}]}'
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_eq "$t_rc" 0
t_json '[.totals.watts, .totals.reporting]' '[null, 0]'
t_json .hosts '[{"name": "node-x", "source": null, "watts": null, "kWh24h": null, "kWh30d": null,
  "ramUsedMb": null, "ramTotalMb": null}]'
t_json .samples "[{\"t\": $NOW, \"w\": {}}]"

t_case "odd models and odd slots do not make a member a factory host; a text power field is no reading"
fresh
mesh 200 '{"members": [
  {"node": "node-x", "state": "alive", "status": {"power": "n/a",
    "models": [7, null, "m", {"name": "model-large", "slots": "many", "busy": "x", "queued": []}]}},
  {"node": "node-y", "state": "alive", "status": {"power": "n/a",
    "models": [{"name": "model-large", "slots": 2, "busy": "x", "queued": {}}]}}]}'
sample 0 PI_ENSEMBLE_SUBAGENT_MODEL=model-large
t_eq "$t_rc" 0
t_json '[.hosts[] | [.name, has("watts"), .watts]]' '[["node-y", true, null]]'
t_json '[.models[] | [.name, .slots, .busy, .queued]]' '[["model-large", 2, 0, 0]]'

t_case "an unknown argument is a usage error"
T_ENV=()
t_run "$energy --loop"
t_eq "$t_rc" 64

t_done
